use tiny_skia::{Color, Pixmap};

#[cfg(any(feature = "crossfont", feature = "ab_glyph", feature = "skrifa"))]
mod config;
#[cfg(any(feature = "crossfont", feature = "ab_glyph", feature = "skrifa"))]
mod font_preference;

#[cfg(feature = "crossfont")]
mod crossfont_renderer;

#[cfg(all(
    not(feature = "crossfont"),
    any(feature = "ab_glyph", feature = "skrifa")
))]
mod skrifa_renderer;

#[cfg(all(
    not(feature = "crossfont"),
    not(any(feature = "ab_glyph", feature = "skrifa"))
))]
mod dumb;

#[derive(Debug)]
pub struct TitleText {
    #[cfg(feature = "crossfont")]
    imp: crossfont_renderer::CrossfontTitleText,
    #[cfg(all(
        not(feature = "crossfont"),
        any(feature = "ab_glyph", feature = "skrifa")
    ))]
    imp: skrifa_renderer::SkrifaTitleText,
    #[cfg(all(
        not(feature = "crossfont"),
        not(any(feature = "ab_glyph", feature = "skrifa"))
    ))]
    imp: dumb::DumbTitleText,
}

impl TitleText {
    pub fn new(color: Color) -> Option<Self> {
        #[cfg(feature = "crossfont")]
        return crossfont_renderer::CrossfontTitleText::new(color)
            .ok()
            .map(|imp| Self { imp });

        #[cfg(all(
            not(feature = "crossfont"),
            any(feature = "ab_glyph", feature = "skrifa")
        ))]
        return Some(Self {
            imp: skrifa_renderer::SkrifaTitleText::new(color),
        });

        #[cfg(all(
            not(feature = "crossfont"),
            not(any(feature = "ab_glyph", feature = "skrifa"))
        ))]
        {
            let _ = color;
            return None;
        }
    }

    pub fn update_scale(&mut self, scale: u32) {
        self.imp.update_scale(scale)
    }

    pub fn update_title(&mut self, title: impl Into<String>) {
        self.imp.update_title(title)
    }

    pub fn update_color(&mut self, color: Color) {
        self.imp.update_color(color)
    }

    pub fn pixmap(&self) -> Option<&Pixmap> {
        self.imp.pixmap()
    }
}
