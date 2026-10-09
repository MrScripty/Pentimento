//! Shared UV authoring layers. Pixels are linear premultiplied RGBA; snapshots
//! preserve every float bit. History contains changed pixel payloads and metadata,
//! never geometry or derived RGBA8 images. Blend functions use straight linear RGB
//! only in the overlap term, then return associated source-over output.
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};

pub const UV_HISTORY_BYTES: usize = 64 * 1024 * 1024;
pub const UV_PENDING_BYTES: usize = 32 * 1024 * 1024;
pub const UV_HISTORY_ENTRIES: usize = 128;
pub const UV_MAX_LAYERS: usize = 64;
pub const UV_MAX_PIXELS: usize = 4_194_304;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UvBlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
}
impl UvBlendMode {
    fn is_normal(&self) -> bool {
        *self == Self::Normal
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UvPaintTarget {
    #[default]
    Color,
    Mask,
}
impl UvPaintTarget {
    fn is_color(&self) -> bool {
        *self == Self::Color
    }
}
fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UvLayerMeta {
    pub id: u32,
    pub name: String,
    pub visible: bool,
    pub opacity: f32,
    pub locked: bool,
    #[serde(default, skip_serializing_if = "UvBlendMode::is_normal")]
    pub blend_mode: UvBlendMode,
    #[serde(default, skip_serializing_if = "is_false")]
    pub mask_enabled: bool,
    #[serde(default, skip_serializing_if = "UvPaintTarget::is_color")]
    pub paint_target: UvPaintTarget,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UvLayerDocument {
    pub meta: UvLayerMeta,
    pub pixels: Vec<[f32; 4]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask: Option<Vec<f32>>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UvLayersDocument {
    pub width: u32,
    pub height: u32,
    pub seam_padding: u32,
    pub active_layer: u32,
    pub next_id: u32,
    /// Bottom to top. An explicit policy prevents legacy files being reinterpreted.
    pub compositor: String,
    pub layers: Vec<UvLayerDocument>,
}
pub const UV_COMPOSITOR: &str = "linear-premultiplied-normal-v1";
/// Older Normal-only readers reject this policy rather than reinterpret a stack.
pub const UV_BLEND_COMPOSITOR: &str = "linear-premultiplied-separable-v1";
pub const UV_MASK_COMPOSITOR: &str = "linear-premultiplied-masked-separable-v1";

/// Associated linear brightness; identical gray channels keep their exact bits.
pub fn uv_luminance(p: [f32; 4]) -> f32 {
    if p[0].to_bits() == p[1].to_bits() && p[0].to_bits() == p[2].to_bits() {
        p[0]
    } else {
        0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]
    }
}
/// Existing RGBA erase reveals white in a mask; opaque gray decodes exactly.
pub fn uv_mask_coverage(p: [f32; 4]) -> f32 {
    if p[3] == 1. {
        p[0]
    } else if p[3] == 0. {
        1.
    } else {
        (p[0] + (1. - p[3])).clamp(0., 1.)
    }
}
pub fn same_mask_pixels(a: &[f32], b: &[f32]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| a.to_bits() == b.to_bits())
}
impl UvLayerDocument {
    pub fn working_pixels(&self) -> Vec<[f32; 4]> {
        match self.meta.paint_target {
            UvPaintTarget::Color => self.pixels.clone(),
            UvPaintTarget::Mask => self
                .mask
                .as_ref()
                .expect("validated mask target")
                .iter()
                .map(|&m| [m, m, m, 1.])
                .collect(),
        }
    }
    pub fn working_matches(&self, pixels: &[[f32; 4]]) -> bool {
        match self.meta.paint_target {
            UvPaintTarget::Color => same_uv_pixels(&self.pixels, pixels),
            UvPaintTarget::Mask => self.mask.as_ref().is_some_and(|m| {
                m.len() == pixels.len()
                    && m.iter()
                        .zip(pixels)
                        .all(|(&m, p)| same_uv_pixels(&[[m, m, m, 1.]], &[*p]))
            }),
        }
    }
}
pub fn same_uv_pixels(a: &[[f32; 4]], b: &[[f32; 4]]) -> bool {
    a.len() == b.len()
        && a.iter()
            .flatten()
            .zip(b.iter().flatten())
            .all(|(a, b)| a.to_bits() == b.to_bits())
}
pub fn valid_uv_pixels(pixels: &[[f32; 4]], count: usize) -> bool {
    pixels.len() == count
        && pixels
            .iter()
            .all(|p| p[3] == 0. || p[..3].iter().all(|v| *v <= p[3]))
        && pixels
            .iter()
            .flatten()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
}
/// Associated source-over. Transparent hidden RGB stays in authoring data but
/// contributes no light to the composite; no conversion mutates those raw bits.
pub fn uv_over(src: [f32; 4], dst: [f32; 4]) -> [f32; 4] {
    if src[3] == 0. {
        return dst;
    }
    let dst = if dst[3] == 0. { [0.; 4] } else { dst };
    let inv = 1. - src[3];
    [
        src[0] + dst[0] * inv,
        src[1] + dst[1] * inv,
        src[2] + dst[2] * inv,
        src[3] + dst[3] * inv,
    ]
}
/// Separable blend followed by source-over, in the existing linear working space.
/// Inputs are associated RGBA, with opacity already applied to the source once.
/// C = (1-as)*Pb + (1-ab)*Ps + as*ab*B(Cb,Cs); a = as + ab*(1-as).
/// Transparent hidden RGB is never divided or introduced into visible output.
/// Normal retains the original arithmetic, including its exact rounding behavior.
pub fn uv_blend_over(src: [f32; 4], dst: [f32; 4], mode: UvBlendMode) -> [f32; 4] {
    if mode == UvBlendMode::Normal || src[3] == 0. || dst[3] == 0. {
        return uv_over(src, dst);
    }
    let a = src[3];
    let b = dst[3];
    let mut out = [0.; 4];
    for c in 0..3 {
        let cs = (src[c] / a).clamp(0., 1.);
        let cb = (dst[c] / b).clamp(0., 1.);
        let blend = match mode {
            UvBlendMode::Normal => unreachable!(),
            UvBlendMode::Multiply => cb * cs,
            UvBlendMode::Screen => cb + cs - cb * cs,
            UvBlendMode::Overlay if cb <= 0.5 => 2. * cb * cs,
            UvBlendMode::Overlay => 1. - 2. * (1. - cb) * (1. - cs),
        };
        out[c] = (1. - a) * dst[c] + (1. - b) * src[c] + a * b * blend;
    }
    out[3] = a + b * (1. - a);
    // Only roundoff can leave the associated unit gamut for valid inputs.
    for c in 0..3 {
        out[c] = out[c].clamp(0., out[3]);
    }
    out
}
impl UvLayersDocument {
    pub fn validate(&self) -> Result<(), String> {
        let count = (self.width as usize)
            .checked_mul(self.height as usize)
            .ok_or("UV layer dimensions overflow")?;
        if self.width == 0
            || self.height == 0
            || self.width > 1048
            || self.height > 1048
            || self.seam_padding > self.width.min(self.height)
            || self.layers.is_empty()
            || self.layers.len() > UV_MAX_LAYERS
            || self.sample_count() > UV_MAX_PIXELS
            || ![UV_COMPOSITOR, UV_BLEND_COMPOSITOR, UV_MASK_COMPOSITOR]
                .contains(&self.compositor.as_str())
            || (self.compositor != UV_MASK_COMPOSITOR
                && self.layers.iter().any(|l| l.mask.is_some()))
            || (self.compositor == UV_COMPOSITOR
                && self
                    .layers
                    .iter()
                    .any(|l| l.meta.blend_mode != UvBlendMode::Normal))
            || self.next_id == u32::MAX
        {
            return Err("Invalid or over-limit UV layer dimensions/count/compositor".into());
        }
        let mut ids = HashSet::new();
        for l in &self.layers {
            if !ids.insert(l.meta.id)
                || l.meta.id >= self.next_id
                || l.meta.name.is_empty()
                || l.meta.name.len() > 256
                || !l.meta.opacity.is_finite()
                || !(0.0..=1.0).contains(&l.meta.opacity)
                || !valid_uv_pixels(&l.pixels, count)
                || l.mask.as_ref().is_some_and(|m| {
                    m.len() != count || !m.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v))
                })
                || (l.meta.mask_enabled && l.mask.is_none())
                || (l.meta.paint_target == UvPaintTarget::Mask
                    && (!l.meta.mask_enabled || l.mask.is_none()))
            {
                return Err("Invalid UV layer identity, metadata or pixels".into());
            }
        }
        if !ids.contains(&self.active_layer) {
            return Err("Missing active UV layer".into());
        }
        Ok(())
    }
    pub fn sample_count(&self) -> usize {
        self.layers
            .iter()
            .map(|l| {
                l.pixels
                    .len()
                    .saturating_add(l.mask.as_ref().map_or(0, Vec::len))
            })
            .sum()
    }
    pub fn composite(&self) -> Vec<[f32; 4]> {
        self.composite_active(None)
    }
    pub fn composite_active(&self, active: Option<&[[f32; 4]]>) -> Vec<[f32; 4]> {
        let mut output = vec![[0.; 4]; self.width as usize * self.height as usize];
        for l in &self.layers {
            if !l.meta.visible || l.meta.opacity == 0. {
                continue;
            }
            let pixels =
                if l.meta.id == self.active_layer && l.meta.paint_target == UvPaintTarget::Color {
                    active.unwrap_or(&l.pixels)
                } else {
                    &l.pixels
                };
            for (i, (dst, src)) in output.iter_mut().zip(pixels).enumerate() {
                let coverage = if l.meta.mask_enabled {
                    if l.meta.id == self.active_layer && l.meta.paint_target == UvPaintTarget::Mask
                    {
                        active
                            .map_or_else(|| l.mask.as_ref().unwrap()[i], |p| uv_mask_coverage(p[i]))
                    } else {
                        l.mask.as_ref().unwrap()[i]
                    }
                } else {
                    1.
                };
                // Retain the exact existing arithmetic for unmasked layers.
                let source = if l.meta.mask_enabled {
                    src.map(|v| v * coverage)
                } else {
                    *src
                };
                *dst = uv_blend_over(source.map(|v| v * l.meta.opacity), *dst, l.meta.blend_mode);
            }
        }
        output
    }
}

#[derive(Clone, Debug)]
pub enum UvLayerOp {
    Create(String),
    Duplicate(u32),
    Delete(u32),
    Select(u32),
    Rename(u32, String),
    Reorder(u32, usize),
    Visible(u32, bool),
    Opacity(u32, f32),
    Lock(u32, bool),
    BlendMode(u32, UvBlendMode),
    AddMask(u32),
    RemoveMask(u32),
    MaskEnabled(u32, bool),
    PaintTarget(u32, UvPaintTarget),
}
#[derive(Clone)]
struct Layout {
    metas: Vec<UvLayerMeta>,
    active: u32,
    compositor: String,
}
impl Layout {
    fn bytes(&self) -> usize {
        self.metas.iter().map(|m| m.name.len() + 48).sum::<usize>() + 8 + self.compositor.len()
    }
}
struct PixelChange {
    id: u32,
    before: Option<Vec<[f32; 4]>>,
    after: Option<Vec<[f32; 4]>>,
    mask: Option<MaskChange>,
}
struct MaskChange {
    before: Option<Vec<f32>>,
    after: Option<Vec<f32>>,
}
struct Change {
    before: Layout,
    after: Layout,
    pixels: Vec<PixelChange>,
}
impl Change {
    fn bytes(&self) -> usize {
        self.before.bytes()
            + self.after.bytes()
            + self
                .pixels
                .iter()
                .map(|p| {
                    p.before.as_ref().map_or(0, |v| v.len() * 16)
                        + p.after.as_ref().map_or(0, |v| v.len() * 16)
                        + p.mask.as_ref().map_or(0, |m| {
                            m.before.as_ref().map_or(0, |v| v.len() * 4)
                                + m.after.as_ref().map_or(0, |v| v.len() * 4)
                        })
                })
                .sum::<usize>()
    }
}
pub struct UvLayers {
    document: UvLayersDocument,
    undo: VecDeque<Change>,
    redo: VecDeque<Change>,
    evicted: usize,
}
impl UvLayers {
    pub fn new(width: u32, height: u32) -> Result<Self, String> {
        if width == 0 || height == 0 || width > 1048 || height > 1048 {
            return Err("Invalid UV layer dimensions".into());
        }
        Self::restore(UvLayersDocument {
            width,
            height,
            seam_padding: 2.min(width).min(height),
            active_layer: 0,
            next_id: 1,
            compositor: UV_COMPOSITOR.into(),
            layers: vec![UvLayerDocument {
                meta: UvLayerMeta {
                    id: 0,
                    name: "UV Layer 1".into(),
                    visible: true,
                    opacity: 1.,
                    locked: false,
                    blend_mode: UvBlendMode::Normal,
                    mask_enabled: false,
                    paint_target: UvPaintTarget::Color,
                },
                pixels: vec![[0.; 4]; width as usize * height as usize],
                mask: None,
            }],
        })
    }
    pub fn restore(document: UvLayersDocument) -> Result<Self, String> {
        document.validate()?;
        Ok(Self {
            document,
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            evicted: 0,
        })
    }
    pub fn document(&self) -> &UvLayersDocument {
        &self.document
    }
    pub fn active(&self) -> &UvLayerDocument {
        self.document
            .layers
            .iter()
            .find(|l| l.meta.id == self.document.active_layer)
            .unwrap()
    }
    pub fn paintable(&self) -> Result<(), String> {
        if self.active().meta.locked {
            Err("The active UV layer is locked against painting".into())
        } else if !self.active().meta.visible {
            Err("Show the active UV layer before painting".into())
        } else {
            Ok(())
        }
    }
    fn layout(&self) -> Layout {
        Layout {
            metas: self
                .document
                .layers
                .iter()
                .map(|l| l.meta.clone())
                .collect(),
            active: self.document.active_layer,
            compositor: self.document.compositor.clone(),
        }
    }
    pub fn history_bytes(&self) -> usize {
        self.undo.iter().chain(&self.redo).map(Change::bytes).sum()
    }
    pub fn undo_count(&self) -> usize {
        self.undo.len()
    }
    pub fn redo_count(&self) -> usize {
        self.redo.len()
    }
    pub fn evicted(&self) -> usize {
        self.evicted
    }
    pub fn evict_oldest(&mut self) -> bool {
        if self.undo.pop_front().is_some() || self.redo.pop_front().is_some() {
            self.evicted += 1;
            true
        } else {
            false
        }
    }
    pub fn clear_redo(&mut self) {
        self.redo.clear();
    }
    pub fn clear_history(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
    fn record(&mut self, change: Change) -> Result<(), String> {
        let bytes = change.bytes();
        if bytes > UV_HISTORY_BYTES {
            return Err("UV layer change exceeds the retained history payload limit".into());
        }
        self.redo.clear();
        while self.history_bytes() > UV_HISTORY_BYTES - bytes
            || self.undo.len() >= UV_HISTORY_ENTRIES
        {
            self.undo.pop_front();
            self.evicted += 1;
        }
        self.undo.push_back(change);
        Ok(())
    }
    fn apply_layout(&mut self, layout: &Layout, pixels: &[PixelChange], redo: bool) {
        let mut old = std::mem::take(&mut self.document.layers);
        self.document.layers = layout
            .metas
            .iter()
            .map(|m| {
                let replacement = pixels.iter().find(|p| p.id == m.id).and_then(|p| {
                    if redo {
                        p.after.as_ref()
                    } else {
                        p.before.as_ref()
                    }
                });
                let mut old_layer = old
                    .iter()
                    .position(|l| l.meta.id == m.id)
                    .map(|i| old.swap_remove(i));
                let mask = if let Some(change) = pixels
                    .iter()
                    .find(|p| p.id == m.id)
                    .and_then(|p| p.mask.as_ref())
                {
                    if redo {
                        change.after.clone()
                    } else {
                        change.before.clone()
                    }
                } else {
                    old_layer.as_mut().and_then(|l| l.mask.take())
                };
                let data = if let Some(p) = replacement {
                    p.clone()
                } else {
                    old_layer.unwrap().pixels
                };
                UvLayerDocument {
                    meta: m.clone(),
                    pixels: data,
                    mask,
                }
            })
            .collect();
        self.document.active_layer = layout.active;
        self.document.compositor = layout.compositor.clone();
        // next_id is an allocation high-water mark; Undo never reuses identities.
    }
    pub fn edit(&mut self, op: UvLayerOp) -> Result<bool, String> {
        if let UvLayerOp::PaintTarget(id, target) = op {
            let l = self
                .document
                .layers
                .iter_mut()
                .find(|l| l.meta.id == id)
                .ok_or("Unknown UV layer")?;
            if target == UvPaintTarget::Mask && (!l.meta.mask_enabled || l.mask.is_none()) {
                return Err("Add and enable the layer mask before selecting Mask".into());
            }
            let changed = l.meta.paint_target != target;
            l.meta.paint_target = target;
            return Ok(changed);
        }
        if let UvLayerOp::Select(id) = op {
            if !self.document.layers.iter().any(|l| l.meta.id == id) {
                return Err("Unknown UV layer".into());
            }
            let changed = self.document.active_layer != id;
            self.document.active_layer = id;
            return Ok(changed);
        }
        let before = self.layout();
        let mut after = before.clone();
        let mut pixels = Vec::new();
        let mut allocated = None;
        let find = |id: u32, metas: &Vec<UvLayerMeta>| {
            metas
                .iter()
                .position(|m| m.id == id)
                .ok_or_else(|| "Unknown UV layer".to_string())
        };
        match op {
            UvLayerOp::Create(name) => {
                let id = self.document.next_id;
                if id >= u32::MAX - 1
                    || after.metas.len() >= UV_MAX_LAYERS
                    || self.document.sample_count().saturating_add(
                        self.document.width as usize * self.document.height as usize,
                    ) > UV_MAX_PIXELS
                {
                    return Err("UV layer count/pixel/identity limit reached".into());
                }
                let name = if name.is_empty() {
                    format!("UV Layer {}", id + 1)
                } else {
                    name
                };
                if name.len() > 256 {
                    return Err("UV layer name is too long".into());
                }
                let index = find(after.active, &after.metas)? + 1;
                after.metas.insert(
                    index,
                    UvLayerMeta {
                        id,
                        name,
                        visible: true,
                        opacity: 1.,
                        locked: false,
                        blend_mode: UvBlendMode::Normal,
                        mask_enabled: false,
                        paint_target: UvPaintTarget::Color,
                    },
                );
                after.active = id;
                allocated = Some(id);
                pixels.push(PixelChange {
                    id,
                    before: None,
                    after: Some(vec![
                        [0.; 4];
                        self.document.width as usize
                            * self.document.height as usize
                    ]),
                    mask: None,
                });
            }
            UvLayerOp::Duplicate(source) => {
                let index = find(source, &after.metas)?;
                let id = self.document.next_id;
                if id >= u32::MAX - 1
                    || after.metas.len() >= UV_MAX_LAYERS
                    || self.document.sample_count().saturating_add(
                        self.document.layers[index].pixels.len()
                            + self.document.layers[index]
                                .mask
                                .as_ref()
                                .map_or(0, Vec::len),
                    ) > UV_MAX_PIXELS
                {
                    return Err("UV layer count/pixel/identity limit reached".into());
                }
                if self.document.layers[index].pixels.len() * 16
                    + self.document.layers[index]
                        .mask
                        .as_ref()
                        .map_or(0, |m| m.len() * 4)
                    + before.bytes()
                    + after.bytes()
                    + 48
                    + self.document.layers[index].meta.name.len()
                    + 5
                    > UV_HISTORY_BYTES
                {
                    return Err("Duplicate exceeds retained payload limit".into());
                }
                let mut meta = after.metas[index].clone();
                meta.id = id;
                meta.name = format!("{} copy", meta.name);
                if meta.name.len() > 256 {
                    return Err("Duplicate layer name exceeds its limit; shorten it first".into());
                }
                after.metas.insert(index + 1, meta);
                after.active = id;
                allocated = Some(id);
                pixels.push(PixelChange {
                    id,
                    before: None,
                    after: Some(self.document.layers[index].pixels.clone()),
                    mask: Some(MaskChange {
                        before: None,
                        after: self.document.layers[index].mask.clone(),
                    }),
                });
            }
            UvLayerOp::Delete(id) => {
                let index = find(id, &after.metas)?;
                if after.metas.len() == 1 {
                    return Err("Keep at least one UV layer".into());
                }
                if self.document.layers[index].pixels.len() * 16
                    + self.document.layers[index]
                        .mask
                        .as_ref()
                        .map_or(0, |m| m.len() * 4)
                    + before.bytes()
                    + after.bytes()
                    > UV_HISTORY_BYTES
                {
                    return Err("Delete exceeds retained payload limit".into());
                }
                after.metas.remove(index);
                if after.active == id {
                    after.active = after.metas[index.min(after.metas.len() - 1)].id;
                }
                pixels.push(PixelChange {
                    id,
                    before: Some(self.document.layers[index].pixels.clone()),
                    after: None,
                    mask: Some(MaskChange {
                        before: self.document.layers[index].mask.clone(),
                        after: None,
                    }),
                });
            }
            UvLayerOp::Select(id) => {
                find(id, &after.metas)?;
                after.active = id;
            }
            UvLayerOp::Rename(id, name) => {
                if name.is_empty() || name.len() > 256 {
                    return Err("Use a nonempty UV layer name up to 256 bytes".into());
                }
                let i = find(id, &after.metas)?;
                after.metas[i].name = name;
            }
            UvLayerOp::Reorder(id, index) => {
                if index >= after.metas.len() {
                    return Err("Invalid UV layer order".into());
                }
                let i = find(id, &after.metas)?;
                let m = after.metas.remove(i);
                after.metas.insert(index, m);
            }
            UvLayerOp::Visible(id, value) => {
                let i = find(id, &after.metas)?;
                after.metas[i].visible = value;
            }
            UvLayerOp::Opacity(id, value) => {
                if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                    return Err("UV layer opacity must be finite in [0,1]".into());
                }
                let i = find(id, &after.metas)?;
                after.metas[i].opacity = value;
            }
            UvLayerOp::Lock(id, value) => {
                let i = find(id, &after.metas)?;
                after.metas[i].locked = value;
            }
            UvLayerOp::BlendMode(id, mode) => {
                let i = find(id, &after.metas)?;
                after.metas[i].blend_mode = mode;
            }
            UvLayerOp::AddMask(id) => {
                let i = find(id, &after.metas)?;
                if self.document.layers[i].mask.is_some() {
                    return Ok(false);
                }
                let count = self.document.layers[i].pixels.len();
                if self.document.sample_count().saturating_add(count) > UV_MAX_PIXELS
                    || count * 4 > UV_PENDING_BYTES
                    || count * 4 + before.bytes() + after.bytes() > UV_HISTORY_BYTES
                {
                    return Err("Mask exceeds UV sample/history payload limits".into());
                }
                after.metas[i].mask_enabled = true;
                pixels.push(PixelChange {
                    id,
                    before: None,
                    after: None,
                    mask: Some(MaskChange {
                        before: None,
                        after: Some(vec![1.; count]),
                    }),
                });
            }
            UvLayerOp::RemoveMask(id) => {
                let i = find(id, &after.metas)?;
                let Some(mask) = self.document.layers[i].mask.as_ref() else {
                    return Ok(false);
                };
                if mask.len() * 4 + before.bytes() + after.bytes() > UV_HISTORY_BYTES {
                    return Err("Mask exceeds history payload limit".into());
                }
                after.metas[i].mask_enabled = false;
                after.metas[i].paint_target = UvPaintTarget::Color;
                pixels.push(PixelChange {
                    id,
                    before: None,
                    after: None,
                    mask: Some(MaskChange {
                        before: Some(mask.clone()),
                        after: None,
                    }),
                });
            }
            UvLayerOp::MaskEnabled(id, enabled) => {
                let i = find(id, &after.metas)?;
                if self.document.layers[i].mask.is_none() {
                    return Err("Add a layer mask first".into());
                }
                after.metas[i].mask_enabled = enabled;
                if !enabled {
                    after.metas[i].paint_target = UvPaintTarget::Color;
                }
            }
            UvLayerOp::PaintTarget(_, _) => unreachable!(),
        }
        if before.metas == after.metas && before.active == after.active && pixels.is_empty() {
            return Ok(false);
        }
        let has_mask = self.document.layers.iter().any(|l| {
            after.metas.iter().any(|m| m.id == l.meta.id)
                && pixels
                    .iter()
                    .find(|p| p.id == l.meta.id)
                    .and_then(|p| p.mask.as_ref())
                    .map_or(l.mask.is_some(), |m| m.after.is_some())
        }) || pixels
            .iter()
            .any(|p| p.mask.as_ref().is_some_and(|m| m.after.is_some()));
        after.compositor = if has_mask {
            UV_MASK_COMPOSITOR
        } else if after
            .metas
            .iter()
            .any(|m| m.blend_mode != UvBlendMode::Normal)
        {
            UV_BLEND_COMPOSITOR
        } else {
            UV_COMPOSITOR
        }
        .into();
        let change = Change {
            before,
            after,
            pixels,
        };
        if change.bytes() > UV_HISTORY_BYTES {
            return Err("UV layer operation exceeds the history payload limit".into());
        }
        self.apply_layout(&change.after, &change.pixels, true);
        if let Some(id) = allocated {
            self.document.next_id = id + 1;
        }
        self.record(change)?;
        Ok(true)
    }
    /// A complete accepted stroke/project commit to the current selected source.
    pub fn paint(&mut self, id: u32, after: Vec<[f32; 4]>) -> Result<bool, String> {
        self.paint_on(id, self.active().meta.paint_target, after)
    }
    pub fn paint_on(
        &mut self,
        id: u32,
        target: UvPaintTarget,
        after: Vec<[f32; 4]>,
    ) -> Result<bool, String> {
        self.paintable()?;
        if id != self.document.active_layer || target != self.active().meta.paint_target {
            return Err("The UV paint target changed during the transaction".into());
        }
        if target == UvPaintTarget::Mask {
            return self.paint_mask_workspace(id, after);
        }
        let before = &self.active().pixels;
        if !valid_uv_pixels(&after, before.len()) {
            return Err("Invalid UV paint result".into());
        }
        if same_uv_pixels(before, &after) {
            return Ok(false);
        }
        if before.len() * 16 > UV_PENDING_BYTES {
            return Err("UV stroke exceeds the pending baseline payload limit".into());
        }
        let layout = self.layout();
        let change = Change {
            before: layout.clone(),
            after: layout,
            pixels: vec![PixelChange {
                id,
                before: Some(before.clone()),
                after: Some(after),
                mask: None,
            }],
        };
        if change.bytes() > UV_HISTORY_BYTES {
            return Err("UV stroke exceeds the retained payload limit".into());
        }
        self.apply_layout(&change.after, &change.pixels, true);
        self.record(change)?;
        Ok(true)
    }
    fn paint_mask_workspace(&mut self, id: u32, after: Vec<[f32; 4]>) -> Result<bool, String> {
        let before = self.active().mask.as_ref().ok_or("Missing active mask")?;
        if !self.active().meta.mask_enabled
            || !valid_uv_pixels(&after, before.len())
            || after
                .iter()
                .any(|p| p[0].to_bits() != p[1].to_bits() || p[0].to_bits() != p[2].to_bits())
        {
            return Err("Invalid grayscale mask workspace".into());
        }
        if after.len() * 16 > UV_PENDING_BYTES {
            return Err("Mask workspace exceeds pending payload limit".into());
        }
        let result: Vec<f32> = before
            .iter()
            .zip(after)
            .map(|(&m, p)| {
                if same_uv_pixels(&[[m, m, m, 1.]], &[p]) {
                    m
                } else {
                    uv_mask_coverage(p)
                }
            })
            .collect();
        if same_mask_pixels(before, &result) {
            return Ok(false);
        }
        let layout = self.layout();
        let change = Change {
            before: layout.clone(),
            after: layout,
            pixels: vec![PixelChange {
                id,
                before: None,
                after: None,
                mask: Some(MaskChange {
                    before: Some(before.clone()),
                    after: Some(result),
                }),
            }],
        };
        if change.bytes() > UV_HISTORY_BYTES {
            return Err("Mask stroke exceeds retained payload limit".into());
        }
        self.apply_layout(&change.after, &change.pixels, true);
        self.record(change)?;
        Ok(true)
    }
    pub fn projected_workspace(&self, source: &[[f32; 4]]) -> Result<Vec<[f32; 4]>, String> {
        self.paintable()?;
        if !valid_uv_pixels(source, self.active().pixels.len()) {
            return Err("Invalid projection snapshot dimensions/pixels".into());
        }
        let baseline = self.active().working_pixels();
        Ok(source
            .iter()
            .zip(baseline)
            .map(|(src, dst)| {
                if src[3] == 0. {
                    return dst;
                }
                let src = if self.active().meta.paint_target == UvPaintTarget::Mask {
                    let gray = uv_luminance(*src).clamp(0., src[3]);
                    [gray, gray, gray, src[3]]
                } else {
                    *src
                };
                uv_over(src, dst)
            })
            .collect())
    }
    pub fn project(&mut self, source: &[[f32; 4]]) -> Result<bool, String> {
        self.paintable()?;
        if !valid_uv_pixels(source, self.active().pixels.len()) {
            return Err("Invalid projection snapshot dimensions/pixels".into());
        }
        let output = self.projected_workspace(source)?;
        self.paint(self.document.active_layer, output)
    }
    pub fn exchange(&mut self, redo: bool) -> bool {
        let change = if redo {
            self.redo.pop_back()
        } else {
            self.undo.pop_back()
        };
        let Some(change) = change else {
            return false;
        };
        self.apply_layout(
            if redo { &change.after } else { &change.before },
            &change.pixels,
            redo,
        );
        if redo {
            self.undo.push_back(change);
        } else {
            self.redo.push_back(change);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const MODES: [UvBlendMode; 4] = [
        UvBlendMode::Normal,
        UvBlendMode::Multiply,
        UvBlendMode::Screen,
        UvBlendMode::Overlay,
    ];
    fn near(a: [f32; 4], b: [f32; 4]) {
        for c in 0..4 {
            assert!((a[c] - b[c]).abs() < 1e-6, "{a:?} != {b:?}");
        }
    }
    #[test]
    fn blends_known_translucent_linear_reference_vectors() {
        let s = [0.08, 0.28, 0.16, 0.4];
        let b = [0.48, 0.18, 0.3, 0.6];
        for (mode, expected) in [
            (UvBlendMode::Multiply, [0.3584, 0.2704, 0.292, 0.76]),
            (UvBlendMode::Screen, [0.5216, 0.4096, 0.412, 0.76]),
            (UvBlendMode::Overlay, [0.4832, 0.3208, 0.34, 0.76]),
        ] {
            near(uv_blend_over(s, b, mode), expected);
        }
        near(
            uv_blend_over(
                [0.2, 0.7, 0.4, 1.],
                [0.8, 0.3, 0.5, 1.],
                UvBlendMode::Overlay,
            ),
            [0.68, 0.42, 0.4, 1.],
        );
        near(
            uv_blend_over(
                [0.8, 0.3, 0.5, 1.],
                [0.2, 0.7, 0.4, 1.],
                UvBlendMode::Overlay,
            ),
            [0.32, 0.58, 0.4, 1.],
        );
    }
    #[test]
    fn blends_match_two_stage_straight_reference_at_transparent_and_subnormal_edges() {
        // Independent f64 two-stage specification: blend source color into backdrop,
        // then associate and source-over. Production uses the expanded PM equation.
        for mode in MODES {
            for a in [0., f32::from_bits(1), 1e-7, 0.4, 1.] {
                for b in [0., f32::from_bits(1), 0.6, 1.] {
                    for cs in [0., 0.2, 0.5, 1.] {
                        for cb in [0., 0.3, 0.8, 1.] {
                            let src = [cs * a, cs * a, cs * a, a];
                            let dst = [cb * b, cb * b, cb * b, b];
                            let out = uv_blend_over(src, dst, mode);
                            let a = a as f64;
                            let b = b as f64;
                            let cs = if a > 0. { src[0] as f64 / a } else { 0. };
                            let cb = if b > 0. { dst[0] as f64 / b } else { 0. };
                            let blended = match mode {
                                UvBlendMode::Normal => cs,
                                UvBlendMode::Multiply => cb * cs,
                                UvBlendMode::Screen => 1. - (1. - cb) * (1. - cs),
                                UvBlendMode::Overlay => {
                                    if cb <= 0.5 {
                                        2. * cb * cs
                                    } else {
                                        1. - 2. * (1. - cb) * (1. - cs)
                                    }
                                }
                            };
                            let mixed_source = (1. - b) * cs + b * blended;
                            let reference = (a * mixed_source + (1. - a) * b * cb) as f32;
                            near(
                                out,
                                [reference, reference, reference, (a + (1. - a) * b) as f32],
                            );
                            assert!(valid_uv_pixels(&[out], 1));
                            if mode == UvBlendMode::Normal {
                                assert!(same_uv_pixels(&[out], &[uv_over(src, dst)]));
                            }
                        }
                    }
                }
            }
            assert_eq!(
                uv_blend_over([1., 0.8, 0.6, 0.], [0.2, 0.3, 0.1, 0.5], mode),
                [0.2, 0.3, 0.1, 0.5]
            );
            assert_eq!(
                uv_blend_over([0.2, 0.3, 0.1, 0.5], [1., 0.8, 0.6, 0.], mode),
                [0.2, 0.3, 0.1, 0.5]
            );
        }
    }
    #[test]
    fn blends_opacity_visibility_lock_order_and_raw_bits_are_separate() {
        let mut l = layer();
        l.paint(0, vec![[0.48, 0.18, 0.3, 0.6]; 4]).unwrap();
        l.edit(UvLayerOp::Create("top".into())).unwrap();
        let id = l.document.active_layer;
        l.paint(id, vec![[0.16, 0.56, 0.32, 0.8]; 4]).unwrap();
        l.edit(UvLayerOp::Opacity(id, 0.5)).unwrap();
        l.edit(UvLayerOp::BlendMode(id, UvBlendMode::Overlay))
            .unwrap();
        let raw = l.active().pixels.clone();
        near(l.document.composite()[0], [0.4832, 0.3208, 0.34, 0.76]);
        l.edit(UvLayerOp::Lock(id, true)).unwrap();
        near(l.document.composite()[0], [0.4832, 0.3208, 0.34, 0.76]);
        assert!(l.project(&[[0.; 4]; 4]).is_err());
        l.edit(UvLayerOp::Visible(id, false)).unwrap();
        assert_eq!(l.document.composite()[0], [0.48, 0.18, 0.3, 0.6]);
        l.edit(UvLayerOp::Visible(id, true)).unwrap();
        l.edit(UvLayerOp::Opacity(id, 0.)).unwrap();
        assert_eq!(l.document.composite()[0], [0.48, 0.18, 0.3, 0.6]);
        l.edit(UvLayerOp::Opacity(id, 0.5)).unwrap();
        l.edit(UvLayerOp::Reorder(id, 0)).unwrap();
        near(
            l.document.composite()[0],
            uv_over([0.48, 0.18, 0.3, 0.6], [0.08, 0.28, 0.16, 0.4]),
        );
        assert!(same_uv_pixels(&raw, &l.document.layers[0].pixels));
        let replacement = vec![[0.1, 0.2, 0.3, 0.5]; 4];
        near(
            l.document.composite_active(Some(&replacement))[0],
            uv_over([0.48, 0.18, 0.3, 0.6], [0.05, 0.1, 0.15, 0.25]),
        );
    }
    #[test]
    fn blends_mode_history_noop_redo_duplicate_delete_and_policy_are_atomic() {
        let mut l = layer();
        l.paint(0, vec![[0.2, 0.1, 0.3, 0.5]; 4]).unwrap();
        let original = l.document.clone();
        l.edit(UvLayerOp::BlendMode(0, UvBlendMode::Multiply))
            .unwrap();
        let blended = l.document.clone();
        assert_eq!(blended.compositor, UV_BLEND_COMPOSITOR);
        l.exchange(false);
        assert_eq!(l.document, original);
        assert_eq!(l.redo_count(), 1);
        assert!(
            !l.edit(UvLayerOp::BlendMode(0, UvBlendMode::Normal))
                .unwrap()
        );
        assert_eq!(l.redo_count(), 1);
        assert!(
            l.edit(UvLayerOp::BlendMode(999, UvBlendMode::Screen))
                .is_err()
        );
        assert_eq!(l.redo_count(), 1);
        l.exchange(true);
        assert_eq!(l.document, blended);
        l.edit(UvLayerOp::Duplicate(0)).unwrap();
        let copy = l.document.active_layer;
        assert_eq!(l.active().meta.blend_mode, UvBlendMode::Multiply);
        l.edit(UvLayerOp::Delete(0)).unwrap();
        l.edit(UvLayerOp::BlendMode(copy, UvBlendMode::Normal))
            .unwrap();
        assert_eq!(l.document.compositor, UV_COMPOSITOR);
        l.exchange(false);
        assert_eq!(l.document.compositor, UV_BLEND_COMPOSITOR);
        l.edit(UvLayerOp::BlendMode(copy, UvBlendMode::Overlay))
            .unwrap();
        assert_eq!(l.redo_count(), 0);
        assert!(l.history_bytes() <= UV_HISTORY_BYTES);
    }
    #[test]
    fn blends_v3_serialization_defaults_old_mode_and_rejects_unsafe_policies() {
        let mut l = layer();
        let old = serde_json::to_value(l.document()).unwrap();
        assert!(old["layers"][0]["meta"].get("blend_mode").is_none());
        assert_eq!(
            serde_json::from_value::<UvLayersDocument>(old.clone()).unwrap(),
            *l.document()
        );
        for mode in MODES {
            l.edit(UvLayerOp::BlendMode(0, mode)).unwrap();
            let saved = serde_json::to_value(l.document()).unwrap();
            let restored: UvLayersDocument = serde_json::from_value(saved).unwrap();
            restored.validate().unwrap();
            assert_eq!(restored, *l.document());
        }
        let mut unsafe_doc = l.document.clone();
        unsafe_doc.compositor = UV_COMPOSITOR.into();
        assert!(unsafe_doc.validate().is_err());
        let mut unknown = old;
        unknown["layers"][0]["meta"]["blend_mode"] = serde_json::json!("SoftLight");
        assert!(serde_json::from_value::<UvLayersDocument>(unknown).is_err());
    }
    fn layer() -> UvLayers {
        UvLayers::new(2, 2).unwrap()
    }
    #[test]
    fn metadata_duplicate_delete_and_active_order_are_recoverable() {
        let mut l = layer();
        l.paint(0, vec![[0.25, 0., 0., 0.5]; 4]).unwrap();
        l.edit(UvLayerOp::Duplicate(0)).unwrap();
        let id = l.document.active_layer;
        assert!(same_uv_pixels(
            &l.active().pixels,
            &l.document.layers[0].pixels
        ));
        l.edit(UvLayerOp::Rename(id, "Details".into())).unwrap();
        l.edit(UvLayerOp::Opacity(id, 0.3)).unwrap();
        l.edit(UvLayerOp::Reorder(id, 0)).unwrap();
        l.edit(UvLayerOp::Visible(id, false)).unwrap();
        let before = l.document.clone();
        l.edit(UvLayerOp::Delete(id)).unwrap();
        assert_eq!(l.document.active_layer, 0);
        assert!(l.exchange(false));
        assert_eq!(l.document.active_layer, id);
        assert_eq!(l.document.layers[0].meta, before.layers[0].meta);
        assert!(same_uv_pixels(
            &l.document.layers[0].pixels,
            &before.layers[0].pixels
        ));
        assert!(l.exchange(true));
        assert_eq!(l.document.layers.len(), 1);
    }
    #[test]
    fn associated_composition_applies_alpha_opacity_once_and_order_matters() {
        let mut l = layer();
        l.paint(0, vec![[0.5, 0., 0., 0.5]; 4]).unwrap();
        l.edit(UvLayerOp::Create("blue".into())).unwrap();
        let id = l.document.active_layer;
        l.paint(id, vec![[0., 0., 0.5, 0.5]; 4]).unwrap();
        l.edit(UvLayerOp::Opacity(id, 0.5)).unwrap();
        assert_eq!(l.document.composite()[0], [0.375, 0., 0.25, 0.625]);
        l.edit(UvLayerOp::Reorder(id, 0)).unwrap();
        assert_eq!(l.document.composite()[0], [0.5, 0., 0.125, 0.625]);
        l.edit(UvLayerOp::Visible(0, false)).unwrap();
        assert_eq!(l.document.composite()[0], [0., 0., 0.25, 0.25]);
    }
    #[test]
    fn lock_hidden_noop_and_rejected_changes_preserve_redo_and_raw_bits() {
        let mut l = layer();
        l.paint(0, vec![[0.25, 0., 0., 0.5]; 4]).unwrap();
        l.exchange(false);
        let redo = l.redo_count();
        assert!(!l.paint(0, l.active().pixels.clone()).unwrap());
        assert!(l.paint(0, vec![[f32::NAN; 4]; 4]).is_err());
        assert!(l.edit(UvLayerOp::Opacity(0, f32::INFINITY)).is_err());
        assert_eq!(l.redo_count(), redo);
        l.exchange(true);
        l.edit(UvLayerOp::Lock(0, true)).unwrap();
        assert!(l.project(&vec![[0.5; 4]; 4]).is_err());
        l.edit(UvLayerOp::Lock(0, false)).unwrap();
        l.edit(UvLayerOp::Visible(0, false)).unwrap();
        assert!(l.paint(0, vec![[0.5; 4]; 4]).is_err());
    }
    #[test]
    fn explicit_projection_commits_same_selected_pixels_with_single_history_entry() {
        let mut l = layer();
        l.paint(0, vec![[0.5, 0., 0., 0.5]; 4]).unwrap();
        let before = l.active().pixels.clone();
        let count = l.undo_count();
        l.project(&vec![[0., 0., 0.5, 0.5]; 4]).unwrap();
        assert_eq!(l.undo_count(), count + 1);
        assert_eq!(l.active().pixels[0], [0.25, 0., 0.5, 0.75]);
        l.exchange(false);
        assert!(same_uv_pixels(&l.active().pixels, &before));
        l.exchange(true);
        l.edit(UvLayerOp::Create("above".into())).unwrap();
        l.project(&vec![[0., 0.5, 0., 0.5]; 4]).unwrap();
        assert_eq!(l.document.layers[0].pixels[0], [0.25, 0., 0.5, 0.75]);
    }
    #[test]
    fn document_restores_exact_transparent_bits_metadata_selection_and_fresh_history() {
        let mut l = layer();
        let pixels = vec![[-0., 0.25, 0.125, 0.]; 4];
        l.paint(0, pixels.clone()).unwrap();
        l.edit(UvLayerOp::Create("top".into())).unwrap();
        l.edit(UvLayerOp::Lock(1, true)).unwrap();
        let json = serde_json::to_string(l.document()).unwrap();
        let restored = UvLayers::restore(serde_json::from_str(&json).unwrap()).unwrap();
        assert_eq!(restored.document.active_layer, 1);
        assert_eq!(restored.document.layers[1].meta, l.document.layers[1].meta);
        assert!(same_uv_pixels(&restored.document.layers[0].pixels, &pixels));
        assert_eq!(restored.undo_count(), 0);
    }
    #[test]
    fn metadata_history_has_bounded_entries_and_no_pixel_copies() {
        let mut l = UvLayers::new(256, 256).unwrap();
        for i in 0..200 {
            l.edit(UvLayerOp::Rename(0, format!("name{i}"))).unwrap();
        }
        assert_eq!(l.undo_count(), UV_HISTORY_ENTRIES);
        assert_eq!(l.evicted(), 72);
        assert!(l.history_bytes() < 50_000);
        assert!(l.edit(UvLayerOp::Delete(0)).is_err());
        assert_eq!(l.undo_count(), 128);
    }
    #[test]
    fn identities_never_reuse_after_undo_and_accepted_new_edit_clears_redo() {
        let mut l = layer();
        l.edit(UvLayerOp::Create("a".into())).unwrap();
        l.exchange(false);
        assert_eq!(l.redo_count(), 1);
        l.edit(UvLayerOp::Create("b".into())).unwrap();
        assert_eq!(l.document.active_layer, 2);
        assert_eq!(l.redo_count(), 0);
    }
    #[test]
    fn malformed_document_and_pixel_budget_refuse_before_restoration() {
        let mut d = layer().document.clone();
        d.compositor = "unknown".into();
        assert!(UvLayers::restore(d).is_err());
        let mut d = layer().document.clone();
        d.active_layer = 99;
        assert!(d.validate().is_err());
        let mut l = UvLayers::new(1024, 1024).unwrap();
        for _ in 0..3 {
            l.edit(UvLayerOp::Create("x".into())).unwrap();
        }
        let count = l.undo_count();
        assert!(l.edit(UvLayerOp::Create("over".into())).is_err());
        assert_eq!(l.undo_count(), count);
        assert!(l.history_bytes() <= UV_HISTORY_BYTES);
    }
    #[test]
    fn constructor_preflights_before_allocation_and_selection_keeps_redo() {
        assert!(UvLayers::new(u32::MAX, u32::MAX).is_err());
        let mut l = layer();
        l.edit(UvLayerOp::Create("b".into())).unwrap();
        l.paint(1, vec![[0.5; 4]; 4]).unwrap();
        l.exchange(false);
        l.edit(UvLayerOp::Select(0)).unwrap();
        assert_eq!(l.redo_count(), 1);
        assert!(l.exchange(true));
        assert_eq!(l.document.layers[1].pixels[0], [0.5; 4]);
    }
    #[test]
    fn latent_transparent_source_and_destination_do_not_become_visible() {
        assert_eq!(
            uv_over([1., 0., 0., 0.], [0., 0., 0.5, 0.5]),
            [0., 0., 0.5, 0.5]
        );
        assert_eq!(
            uv_over([0., 0., 0.5, 0.5], [1., 0., 0., 0.]),
            [0., 0., 0.5, 0.5]
        );
        let mut l = layer();
        l.paint(0, vec![[1., 0., 0., 0.]; 4]).unwrap();
        l.project(&vec![[0., 0., 0.5, 0.5]; 4]).unwrap();
        assert_eq!(l.active().pixels[0], [0., 0., 0.5, 0.5]);
        l.exchange(false);
        assert_eq!(l.active().pixels[0], [1., 0., 0., 0.]);
    }
}

#[cfg(test)]
mod mask_tests {
    use super::*;
    fn masked() -> UvLayers {
        let mut l = UvLayers::new(2, 2).unwrap();
        l.paint(0, vec![[0.2, 0.3, 0.1, 0.5]; 4]).unwrap();
        l.edit(UvLayerOp::AddMask(0)).unwrap();
        l.edit(UvLayerOp::PaintTarget(0, UvPaintTarget::Mask))
            .unwrap();
        l
    }
    #[test]
    fn mask_compositor_scales_associated_rgba_then_opacity_for_all_modes() {
        let mut l = masked();
        l.edit(UvLayerOp::Create("top".into())).unwrap();
        l.paint(1, vec![[0.08, 0.28, 0.16, 0.4]; 4]).unwrap();
        l.edit(UvLayerOp::AddMask(1)).unwrap();
        l.edit(UvLayerOp::PaintTarget(1, UvPaintTarget::Mask))
            .unwrap();
        l.paint(1, vec![[0.25, 0.25, 0.25, 1.]; 4]).unwrap();
        l.edit(UvLayerOp::Opacity(1, 0.5)).unwrap();
        for mode in [
            UvBlendMode::Normal,
            UvBlendMode::Multiply,
            UvBlendMode::Screen,
            UvBlendMode::Overlay,
        ] {
            l.edit(UvLayerOp::BlendMode(1, mode)).unwrap();
            let expected = uv_blend_over([0.01, 0.035, 0.02, 0.05], [0.2, 0.3, 0.1, 0.5], mode);
            for p in l.document().composite() {
                for c in 0..4 {
                    assert!((p[c] - expected[c]).abs() < 1e-7);
                }
            }
            let zero = vec![[0., 0., 0., 1.]; 4];
            assert_eq!(
                l.document().composite_active(Some(&zero)),
                vec![[0.2, 0.3, 0.1, 0.5]; 4]
            );
            l.edit(UvLayerOp::MaskEnabled(1, false)).unwrap();
            assert_eq!(
                l.document().composite(),
                vec![uv_blend_over([0.04, 0.14, 0.08, 0.2], [0.2, 0.3, 0.1, 0.5], mode); 4]
            );
            l.edit(UvLayerOp::MaskEnabled(1, true)).unwrap();
            l.edit(UvLayerOp::PaintTarget(1, UvPaintTarget::Mask))
                .unwrap();
        }
    }
    #[test]
    fn mask_untouched_bits_noops_erase_and_rejection_preserve_redo() {
        let mut d = masked().document().clone();
        d.layers[0].mask = Some(vec![-0., f32::from_bits(1), f32::from_bits(0x3eaaaaab), 1.]);
        let mut l = UvLayers::restore(d).unwrap();
        let before = l.active().mask.clone().unwrap();
        let mut pixels = l.active().working_pixels();
        assert!(!l.paint(0, pixels.clone()).unwrap());
        pixels[3] = [0., 0., 0., 1.];
        assert!(l.paint(0, pixels).unwrap());
        for i in 0..3 {
            assert_eq!(
                l.active().mask.as_ref().unwrap()[i].to_bits(),
                before[i].to_bits()
            );
        }
        assert!(l.exchange(false));
        assert!(!l.paint(0, l.active().working_pixels()).unwrap());
        let mut invalid = l.active().working_pixels();
        invalid[1] = [1., 0., 0., 1.];
        assert!(l.paint(0, invalid).is_err());
        assert_eq!((l.undo_count(), l.redo_count()), (0, 1));
        assert!(l.exchange(true));
        let mut erased = l.active().working_pixels();
        erased[3] = [0., 0., 0., 0.4];
        l.paint(0, erased).unwrap();
        assert_eq!(l.active().mask.as_ref().unwrap()[3], 0.6);
        assert!(same_uv_pixels(
            &l.active().pixels,
            &vec![[0.2, 0.3, 0.1, 0.5]; 4]
        ));
    }
    #[test]
    fn mask_projection_skips_transparent_latent_rgb_and_uses_linear_brightness() {
        let mut l = masked();
        l.paint(0, vec![[0.25, 0.25, 0.25, 1.]; 4]).unwrap();
        let raw = l.active().pixels.clone();
        let n = l.undo_count();
        assert!(!l.project(&[[1., 0.7, 0.9, 0.]; 4]).unwrap());
        assert_eq!(l.undo_count(), n);
        l.project(&[[0.4, 0.1, 0.2, 0.5]; 4]).unwrap();
        let expected = 0.2126 * 0.4 + 0.7152 * 0.1 + 0.0722 * 0.2 + 0.25 * 0.5;
        for &m in l.active().mask.as_ref().unwrap() {
            assert!((m - expected).abs() < 1e-7);
        }
        assert_eq!(l.active().pixels, raw);
        assert!(l.exchange(false));
        assert_eq!(l.active().mask, Some(vec![0.25; 4]));
    }
    #[test]
    fn mask_lifecycle_selection_and_scalar_history_are_atomic() {
        let mut l = masked();
        let raw_ptr = l.active().pixels.as_ptr();
        let mask_ptr = l.active().mask.as_ref().unwrap().as_ptr();
        l.edit(UvLayerOp::Opacity(0, 0.5)).unwrap();
        assert_eq!(l.active().pixels.as_ptr(), raw_ptr);
        assert_eq!(l.active().mask.as_ref().unwrap().as_ptr(), mask_ptr);
        l.clear_history();
        let layout_bytes = l.layout().bytes() * 2;
        l.paint(0, vec![[0.3, 0.3, 0.3, 1.]; 4]).unwrap();
        assert_eq!(l.history_bytes(), layout_bytes + 4 * 8);
        let painted = l.document().clone();
        assert!(l.exchange(false));
        l.edit(UvLayerOp::PaintTarget(0, UvPaintTarget::Color))
            .unwrap();
        assert_eq!(l.redo_count(), 1);
        assert!(l.exchange(true));
        assert_eq!(l.document(), &painted);
        l.edit(UvLayerOp::Duplicate(0)).unwrap();
        assert_eq!(l.active().mask, Some(vec![0.3; 4]));
        l.edit(UvLayerOp::Delete(1)).unwrap();
        assert!(l.exchange(false));
        assert_eq!(l.active().meta.id, 1);
        let before = l.document().clone();
        l.edit(UvLayerOp::RemoveMask(1)).unwrap();
        assert!(l.active().mask.is_none());
        assert_eq!(l.active().meta.paint_target, UvPaintTarget::Color);
        assert!(l.exchange(false));
        assert_eq!(l.document(), &before);
        l.edit(UvLayerOp::MaskEnabled(1, false)).unwrap();
        assert_eq!(l.active().mask, Some(vec![0.3; 4]));
        assert_eq!(l.active().meta.paint_target, UvPaintTarget::Color);
        assert!(
            l.edit(UvLayerOp::PaintTarget(1, UvPaintTarget::Mask))
                .is_err()
        );
        assert!(l.exchange(false));
        assert_eq!(l.document(), &before);
        assert_eq!(l.redo_count(), 1);
        l.edit(UvLayerOp::Select(0)).unwrap();
        assert_eq!(l.redo_count(), 1);
        l.paint(0, vec![[0.5, 0.5, 0.5, 1.]; 4]).unwrap();
        assert_eq!(l.redo_count(), 0);
    }
    #[test]
    fn mask_documents_require_explicit_policy_and_validate_before_install() {
        let legacy = UvLayers::new(2, 2).unwrap().document().clone();
        let json = serde_json::to_string(&legacy).unwrap();
        assert!(!json.contains("mask"));
        assert!(!json.contains("paint_target"));
        assert_eq!(
            serde_json::from_str::<UvLayersDocument>(&json).unwrap(),
            legacy
        );
        let good = masked().document().clone();
        assert_eq!(good.compositor, UV_MASK_COMPOSITOR);
        let json = serde_json::to_string(&good).unwrap();
        assert_eq!(
            serde_json::from_str::<UvLayersDocument>(&json).unwrap(),
            good
        );
        for kind in 0..6 {
            let mut bad = good.clone();
            match kind {
                0 => bad.compositor = UV_BLEND_COMPOSITOR.into(),
                1 => bad.layers[0]
                    .mask
                    .as_mut()
                    .unwrap()
                    .pop()
                    .map(|_| ())
                    .unwrap(),
                2 => bad.layers[0].mask.as_mut().unwrap()[0] = f32::NAN,
                3 => bad.layers[0].mask.as_mut().unwrap()[0] = 1.1,
                4 => bad.layers[0].mask = None,
                _ => bad.layers[0].meta.mask_enabled = false,
            }
            assert!(UvLayers::restore(bad).is_err());
        }
    }
    #[test]
    fn masks_share_sample_bound_and_history_entry_cap() {
        let mut l = UvLayers::new(1024, 1024).unwrap();
        l.edit(UvLayerOp::AddMask(0)).unwrap();
        l.edit(UvLayerOp::Duplicate(0)).unwrap();
        assert_eq!(l.document().sample_count(), UV_MAX_PIXELS);
        let d = l.document().clone();
        let n = l.undo_count();
        assert!(l.edit(UvLayerOp::Create("overflow".into())).is_err());
        assert!(l.edit(UvLayerOp::Duplicate(0)).is_err());
        assert_eq!(l.document(), &d);
        assert_eq!(l.undo_count(), n);
        assert!(l.history_bytes() <= UV_HISTORY_BYTES);
        let mut small = masked();
        // Use valid opaque grayscale workspaces for retained scalar strokes.
        for n in 0..300 {
            let m = if n % 2 == 0 { 0. } else { 1. };
            small.paint(0, vec![[m, m, m, 1.]; 4]).unwrap();
        }
        assert_eq!(small.undo_count(), UV_HISTORY_ENTRIES);
        assert!(small.evicted() > 0);
        assert!(small.history_bytes() <= UV_HISTORY_BYTES);
    }
}
