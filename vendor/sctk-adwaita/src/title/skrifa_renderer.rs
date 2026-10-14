//! Title renderer using skrifa.
//!
//! Requires no dynamically linked dependencies.
//!
//! Can fallback to an embedded Cantarell-Regular.ttf font (SIL Open Font Licence v1.1)
//! if the system font doesn't work.
use crate::title::{config, font_preference::FontPreference};
use skrifa::raw::{tables::kern::SubtableKind, types::GlyphId, TableProvider};
use skrifa::{
    instance::{Location, LocationRef, Size},
    outline::{DrawSettings, HintingInstance, HintingOptions, OutlinePen},
    MetadataProvider, Tag,
};
use std::{fs::File, process::Command};
use tiny_skia::{Color, FillRule, Paint, Pixmap, Transform};

const CANTARELL: &[u8] = include_bytes!("Cantarell-Regular.ttf");

#[derive(Debug)]
pub struct SkrifaTitleText {
    title: String,
    font: Option<(memmap2::Mmap, FontPreference)>,
    original_px_size: f32,
    px_size: f32,
    color: Color,
    pixmap: Option<Pixmap>,
}

impl SkrifaTitleText {
    pub fn new(color: Color) -> Self {
        let font_pref = config::titlebar_font().unwrap_or_default();
        let font_pref_pt_size = font_pref.pt_size;
        let font = font_file_matching(&font_pref)
            .and_then(|f| mmap(&f))
            .map(|mmap| (mmap, font_pref));

        // Convert point size to pixel size.
        // 1pt = 1/72in, 1px = 1/96in, so px = pt * (96/72)
        let px_size = font_pref_pt_size * (96.0 / 72.0);

        Self {
            title: <_>::default(),
            font,
            original_px_size: px_size,
            px_size,
            color,
            pixmap: None,
        }
    }

    pub fn update_scale(&mut self, scale: u32) {
        let new_size = self.original_px_size * scale as f32;
        if (self.px_size - new_size).abs() > f32::EPSILON {
            self.px_size = new_size;
            self.pixmap = self.render();
        }
    }

    pub fn update_title(&mut self, title: impl Into<String>) {
        let new_title = title.into();
        if new_title != self.title {
            self.title = new_title;
            self.pixmap = self.render();
        }
    }

    pub fn update_color(&mut self, color: Color) {
        if color != self.color {
            self.color = color;
            self.pixmap = self.render();
        }
    }

    pub fn pixmap(&self) -> Option<&Pixmap> {
        self.pixmap.as_ref()
    }

    /// Render returning the new `Pixmap`.
    fn render(&self) -> Option<Pixmap> {
        if self.title.is_empty() {
            return None;
        }

        let font = parse_font(&self.font);
        let font_pref = self.font.as_ref().map(|(_, pref)| pref);
        if !self.px_size.is_finite() || self.px_size <= 0.0 {
            return None;
        }
        let size = Size::new(self.px_size);
        // Failed system fonts fall back to regular, matching the 0.10.1 backend.
        let is_bold = self
            .font
            .as_ref()
            .is_some_and(|(data, _)| valid_font(data).is_some())
            && font_pref
                .and_then(|p| p.style.as_deref())
                .is_some_and(|s| s.eq_ignore_ascii_case("bold"));
        let location_storage = if is_bold {
            font.axes().location([(Tag::new(b"wght"), 700.0f32)])
        } else {
            Location::default()
        };
        let location = LocationRef::from(&location_storage);

        let metrics = font.metrics(size, location);
        // tiny-skia represents dimensions with signed 32-bit bounds. Reject
        // impossible font/scale dimensions before hinting or allocating paths.
        let line_height = metrics.ascent - metrics.descent;
        if metrics.units_per_em == 0
            || !line_height.is_finite()
            || line_height <= 0.0
            || line_height >= i32::MAX as f32
        {
            return None;
        }
        let glyph_metrics = font.glyph_metrics(size, location);
        let charmap = font.charmap();
        let outlines = font.outline_glyphs();

        // Create a hinting instance for better glyph quality at small sizes.
        // Falls back to unhinted rendering if the font doesn't support hinting.
        let hinting =
            HintingInstance::new(&outlines, size, location, HintingOptions::default()).ok();

        // Layout glyphs and collect paths
        let mut paths: Vec<(tiny_skia::Path, f32, f32)> = Vec::new();
        let mut caret: f32 = 0.0;
        let ascent = metrics.ascent;

        let mut last_glyph_id = None;
        let mut glyph_count = 0;
        // Keep advance/line metrics plus actual ink bounds and a 1px guard.
        let mut min_x: f32 = 0.0;
        let mut max_x: f32 = 0.0;
        let mut min_y: f32 = 0.0;
        let mut max_y = metrics.ascent - metrics.descent;
        for c in self.title.chars() {
            if c.is_control() {
                continue;
            }

            let glyph_id = charmap.map(c).unwrap_or_default();

            if let Some(previous) = last_glyph_id {
                caret += horizontal_kerning(&font, previous, glyph_id) * self.px_size
                    / f32::from(metrics.units_per_em);
            }
            glyph_count += 1;

            let mut pb = tiny_skia::PathBuilder::new();
            let mut pen = PathPen(&mut pb);

            if let Some(outline) = outlines.get(glyph_id) {
                let settings = match &hinting {
                    Some(h) => DrawSettings::from(h),
                    None => DrawSettings::unhinted(size, location),
                };
                if outline.draw(settings, &mut pen).is_err() {
                    log::warn!("Cannot draw title glyph outline");
                    pb = tiny_skia::PathBuilder::new();
                }
            }

            if let Some(path) = pb.finish() {
                let bounds = path.bounds();
                min_x = min_x.min(caret + bounds.left());
                max_x = max_x.max(caret + bounds.right());
                min_y = min_y.min(ascent + bounds.top());
                max_y = max_y.max(ascent + bounds.bottom());
                paths.push((path, caret, ascent));
            }

            caret += glyph_metrics.advance_width(glyph_id).unwrap_or(0.0);
            last_glyph_id = Some(glyph_id);
        }

        if glyph_count == 0 {
            return None;
        }

        // Expand for overhangs/combining marks instead of cropping negative bearings.
        // The old renderer allocated advance+2 by line-height+2; preserve that
        // nominal box for ordinary titles, expanding only when ink needs it.
        max_x = max_x.max(caret);
        let left = min_x.floor();
        let top = min_y.floor();
        let total_width = ((max_x.ceil() - left) as u32).checked_add(2)?;
        let total_height = ((max_y.ceil() - top) as u32).checked_add(2)?;
        let mut pixmap = Pixmap::new(total_width, total_height)?;
        let mut paint = Paint::default();
        paint.set_color(self.color);
        paint.anti_alias = true;

        for (path, x, y) in &paths {
            let transform = Transform::from_translate(*x - left + 1.0, *y - top + 1.0);
            pixmap.fill_path(path, &paint, FillRule::Winding, transform, None);
        }

        Some(pixmap)
    }
}

/// Parse the memmapped system font or fallback to built-in Cantarell.
fn parse_font<'a>(sys_font: &'a Option<(memmap2::Mmap, FontPreference)>) -> skrifa::FontRef<'a> {
    sys_font
        .as_ref()
        .and_then(|(data, _)| valid_font(data))
        .unwrap_or_else(|| {
            // Bundled upstream font is validated by regression tests.
            skrifa::FontRef::from_index(CANTARELL, 0).expect("valid bundled Cantarell")
        })
}

/// Reject incomplete/unusable fonts before selecting the system backend.
fn valid_font(data: &[u8]) -> Option<skrifa::FontRef<'_>> {
    let font = skrifa::FontRef::from_index(data, 0).ok()?;
    let metrics = font.metrics(Size::unscaled(), LocationRef::default());
    (metrics.units_per_em > 0
        && metrics.glyph_count > 0
        && metrics.ascent > metrics.descent
        && font.cmap().is_ok()
        && font.outline_glyphs().iter().next().is_some())
    .then_some(font)
}

/// Preserve 0.10.1's first supported horizontal non-variable kern lookup.
/// Table decoding and bounds checks remain in maintained read-fonts, not here.
fn horizontal_kerning(font: &skrifa::FontRef<'_>, left: GlyphId, right: GlyphId) -> f32 {
    let Ok(kern) = font.kern() else { return 0.0 };
    kerning_from_table(&kern, left, right)
}

fn kerning_from_table(
    kern: &skrifa::raw::tables::kern::Kern<'_>,
    left: GlyphId,
    right: GlyphId,
) -> f32 {
    kern.subtables()
        .filter_map(Result::ok)
        .filter(|table| table.is_horizontal() && !table.is_variable())
        .find_map(|table| match table.kind().ok()? {
            SubtableKind::Format0(t) => t.kerning(left, right),
            SubtableKind::Format2(t) => t.kerning(left, right),
            SubtableKind::Format3(t) => t.kerning(left, right),
            // State-machine kerning was not used by the previous backend.
            SubtableKind::Format1(_) => None,
        })
        .unwrap_or_default() as f32
}

/// Font-config without dynamically linked dependencies
fn font_file_matching(pref: &FontPreference) -> Option<File> {
    let mut pattern = pref.name.clone();
    if let Some(style) = &pref.style {
        pattern.push(':');
        pattern.push_str(style);
    }
    Command::new("fc-match")
        .arg("-f")
        .arg("%{file}")
        .arg(&pattern)
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .and_then(|path| File::open(path.trim()).ok())
}

fn mmap(file: &File) -> Option<memmap2::Mmap> {
    // Safety: System font files are not expected to be mutated during use
    unsafe { memmap2::Mmap::map(file).ok() }
}

/// A pen that draws skrifa glyph outlines into a `tiny_skia::PathBuilder`.
struct PathPen<'a>(&'a mut tiny_skia::PathBuilder);

impl OutlinePen for PathPen<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to(x, -y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to(x, -y);
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0.quad_to(cx0, -cy0, x, -y);
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.cubic_to(cx0, -cy0, cx1, -cy1, x, -y);
    }

    fn close(&mut self) {
        self.0.close();
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// Helper to create a renderer using only the bundled Cantarell font.
    fn new_renderer() -> SkrifaTitleText {
        let mut renderer = SkrifaTitleText {
            title: String::new(),
            font: None, // forces fallback to bundled Cantarell
            original_px_size: 13.3,
            px_size: 13.3,
            color: Color::BLACK,
            pixmap: None,
        };
        renderer.update_title("Hello");
        renderer
    }

    #[test]
    fn renders_non_empty_pixmap() {
        let renderer = new_renderer();
        let pixmap = renderer.pixmap().expect("should produce a pixmap");
        assert!(pixmap.width() > 0);
        assert!(pixmap.height() > 0);
    }

    #[test]
    fn renders_pixels_with_coverage() {
        let renderer = new_renderer();
        let pixmap = renderer.pixmap().expect("should produce a pixmap");
        // At least some pixels should have non-zero alpha (i.e. glyphs were drawn)
        let has_visible = pixmap.pixels().iter().any(|px| px.alpha() > 0);
        assert!(has_visible, "rendered text should have visible pixels");
    }

    #[test]
    fn empty_title_returns_none() {
        let mut renderer = new_renderer();
        renderer.update_title("");
        assert!(renderer.pixmap().is_none());
    }

    #[test]
    fn update_scale_changes_size() {
        let mut renderer = new_renderer();
        let w1 = renderer.pixmap().expect("pixmap").width();
        renderer.update_scale(2);
        let w2 = renderer.pixmap().expect("pixmap after scale").width();
        assert!(w2 > w1, "scaled text should be wider");
    }

    #[test]
    fn update_color_rerenders() {
        let mut renderer = new_renderer();
        let old_pixels: Vec<_> = renderer.pixmap().unwrap().pixels().to_vec();
        renderer.update_color(Color::from_rgba8(255, 0, 0, 255));
        let new_pixels: Vec<_> = renderer.pixmap().unwrap().pixels().to_vec();
        assert_ne!(
            old_pixels, new_pixels,
            "color change should produce different pixels"
        );
    }

    #[test]
    fn parse_bundled_font_succeeds() {
        let font = parse_font(&None);
        let metrics = font.metrics(Size::new(16.0), LocationRef::default());
        assert!(metrics.units_per_em > 0);
        assert!(metrics.ascent > 0.0);
    }
}

#[cfg(test)]
mod regression_tests {
    use super::*;
    use skrifa::raw::{tables::kern::Kern, FontData, FontRead};

    fn renderer(data: Option<&[u8]>, style: Option<&str>) -> SkrifaTitleText {
        let font = data.map(|bytes| {
            let mut mapping = memmap2::MmapMut::map_anon(bytes.len()).unwrap();
            mapping.copy_from_slice(bytes);
            (
                mapping.make_read_only().unwrap(),
                FontPreference {
                    name: "fixture".into(),
                    style: style.map(str::to_owned),
                    pt_size: 10.0,
                },
            )
        });
        SkrifaTitleText {
            title: String::new(),
            font,
            original_px_size: 40.0 / 3.0,
            px_size: 40.0 / 3.0,
            color: Color::BLACK,
            pixmap: None,
        }
    }

    #[test]
    fn controls_are_skipped_and_control_only_title_is_absent() {
        let mut a = renderer(None, None);
        a.update_title("A\nV\tT\r");
        let mut b = renderer(None, None);
        b.update_title("AVT");
        assert_eq!(a.pixmap().unwrap().data(), b.pixmap().unwrap().data());
        a.update_title("\n\t\r");
        assert!(a.pixmap().is_none());
    }

    #[test]
    fn whitespace_retains_advance_and_blank_pixmap() {
        let mut r = renderer(None, None);
        r.update_title("   ");
        let p = r.pixmap().unwrap();
        assert!(p.width() > 2);
        assert!(p.pixels().iter().all(|p| p.alpha() == 0));
    }

    #[test]
    fn malformed_and_incomplete_system_font_falls_back_to_regular() {
        let mut expected = renderer(None, None);
        expected.update_title("Pentimento AV é Ω 日本");
        let mut corrupt = CANTARELL.to_vec();
        // Retain an otherwise valid header but truncate its required tables.
        corrupt.truncate(32);
        for data in [b"not a font".as_slice(), corrupt.as_slice()] {
            let mut actual = renderer(Some(data), Some("Bold"));
            actual.update_title("Pentimento AV é Ω 日本");
            assert_eq!(
                actual.pixmap().unwrap().data(),
                expected.pixmap().unwrap().data()
            );
        }
    }

    #[test]
    fn unicode_and_missing_glyphs_render_with_guard_pixels_at_scales() {
        for title in [
            "Pentimento AV To",
            "é Ω Ж",
            "a\u{301} g_j",
            "日本 😀",
            "f\u{301}j",
        ] {
            for scale in [1, 2, 3] {
                let mut r = renderer(None, None);
                r.update_scale(scale);
                r.update_title(title);
                let p = r.pixmap().unwrap();
                let w = p.width() as usize;
                let h = p.height() as usize;
                let pixels = p.pixels();
                assert!(
                    pixels.iter().any(|p| p.alpha() > 0),
                    "{title}, scale {scale}"
                );
                assert!(pixels[..w].iter().all(|p| p.alpha() == 0));
                assert!(pixels[(h - 1) * w..].iter().all(|p| p.alpha() == 0));
                assert!((0..h)
                    .all(|y| pixels[y * w].alpha() == 0 && pixels[y * w + w - 1].alpha() == 0));
            }
        }
    }

    #[test]
    fn scale_and_title_and_color_updates_are_reversible() {
        let mut r = renderer(None, None);
        r.update_title("Pentimento");
        let original = r.pixmap().unwrap().clone();
        r.update_scale(3);
        assert!(r.pixmap().unwrap().width() > original.width() * 2);
        r.update_title("Changed");
        r.update_color(Color::from_rgba8(230, 20, 40, 255));
        r.update_scale(1);
        r.update_title("Pentimento");
        r.update_color(Color::BLACK);
        assert_eq!(r.pixmap().unwrap().data(), original.data());
    }

    #[test]
    fn variable_weight_bold_changes_ink_and_static_bold_is_not_synthesized() {
        let bytes = include_bytes!("../../tests/fonts/Roboto-Variable.ttf");
        let font = valid_font(bytes).unwrap();
        assert!(font.axes().iter().any(|a| a.tag() == Tag::new(b"wght")));
        let mut regular = renderer(Some(bytes), None);
        regular.update_title("Pentimento AV To");
        let mut bold = renderer(Some(bytes), Some("bOlD"));
        bold.update_title("Pentimento AV To");
        assert_ne!(
            regular.pixmap().unwrap().data(),
            bold.pixmap().unwrap().data()
        );
        let mut normal_static = renderer(Some(CANTARELL), None);
        let mut bold_static = renderer(Some(CANTARELL), Some("Bold"));
        normal_static.update_title("Title");
        bold_static.update_title("Title");
        assert_eq!(
            normal_static.pixmap().unwrap().data(),
            bold_static.pixmap().unwrap().data()
        );
    }

    #[test]
    fn maintained_reader_preserves_legacy_kern_pair_and_filtering() {
        // OpenType kern: version 0, one format-0 horizontal subtable, one
        // authoritative pair (glyph 1, glyph 2) with -120 font-unit advance.
        let mut bytes = vec![
            0, 0, 0, 1, 0, 0, 0, 20, 0, 1, 0, 1, 0, 6, 0, 0, 0, 0, 0, 1, 0, 2, 255, 136,
        ];
        let table = Kern::read(FontData::new(&bytes)).unwrap();
        assert_eq!(
            kerning_from_table(&table, GlyphId::new(1), GlyphId::new(2)),
            -120.0
        );
        assert_eq!(
            kerning_from_table(&table, GlyphId::new(1), GlyphId::new(3)),
            0.0
        );
        bytes[9] = 0; // vertical, must not affect horizontal title layout
        let table = Kern::read(FontData::new(&bytes)).unwrap();
        assert_eq!(
            kerning_from_table(&table, GlyphId::new(1), GlyphId::new(2)),
            0.0
        );
    }

    #[test]
    fn fixed_font_dimensions_match_recorded_legacy_renderer() {
        let mut count = 0;
        for row in include_str!("../../tests/title-metrics-baseline.csv")
            .lines()
            .skip(1)
        {
            let fields: Vec<_> = row.split(',').collect();
            let bytes = match fields[0] {
                "Cantarell" => CANTARELL,
                "RobotoVF" => include_bytes!("../../tests/fonts/Roboto-Variable.ttf").as_slice(),
                // System-font cases are supplementary environment-qualified evidence.
                _ => continue,
            };
            let mut r = renderer(Some(bytes), fields[1].contains("Bold").then_some("Bold"));
            r.update_scale(fields[2].parse().unwrap());
            r.update_title(fields[3]);
            let p = r.pixmap().unwrap();
            assert_eq!(
                (p.width(), p.height()),
                (fields[4].parse().unwrap(), fields[5].parse().unwrap()),
                "{row}"
            );
            count += 1;
        }
        assert_eq!(count, 72);
    }

    #[test]
    fn invalid_scale_returns_absent_without_panicking_and_recovers() {
        let mut r = renderer(None, None);
        r.update_title("Title");
        r.update_scale(0);
        assert!(r.pixmap().is_none());
        r.update_scale(1);
        assert!(r.pixmap().is_some());
        r.update_scale(u32::MAX);
        assert!(r.pixmap().is_none());
    }
}
