//! Explicit v1 asset subset. Unsupported data is refused, never silently dropped.
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, VertexAttributeValues},
    mesh::UvChannel,
    prelude::*,
    render::render_resource::{Extent3d, Face, PrimitiveTopology, TextureDimension, TextureFormat},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MeshDocument {
    pub positions: Vec<[f32; 3]>,
    normals: Option<Vec<[f32; 3]>>,
    uv0: Option<Vec<[f32; 2]>>,
    uv1: Option<Vec<[f32; 2]>>,
    tangents: Option<Vec<[f32; 4]>>,
    colors: Option<Vec<[f32; 4]>>,
    indices: IndexDocument,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
enum IndexDocument {
    None,
    U16(Vec<u16>),
    U32(Vec<u32>),
}
impl MeshDocument {
    pub(crate) fn has_uv0(&self) -> bool {
        self.uv0.as_ref().is_some_and(|v| !v.is_empty())
    }
    pub(crate) fn same_sculpt_surface(&self, other: &Self) -> bool {
        // Chunk merge order is a runtime HashMap detail. Compare oriented corner
        // position/UV triangles independently of emitted render vertex order.
        // Normals remain lossless in BOTH representations; they are not geometry.
        fn triangles(m: &MeshDocument) -> Vec<[[u32; 5]; 3]> {
            let indices: Vec<usize> = match &m.indices {
                IndexDocument::None => (0..m.positions.len()).collect(),
                IndexDocument::U16(v) => v.iter().map(|v| *v as usize).collect(),
                IndexDocument::U32(v) => v.iter().map(|v| *v as usize).collect(),
            };
            let mut triangles = Vec::new();
            for ids in indices.chunks_exact(3) {
                let mut t = [[0; 5]; 3];
                for (corner, &id) in ids.iter().enumerate() {
                    let p = m.positions[id];
                    let uv = m.uv0.as_ref().map_or([0.; 2], |v| v[id]);
                    t[corner] = [
                        p[0].to_bits(),
                        p[1].to_bits(),
                        p[2].to_bits(),
                        uv[0].to_bits(),
                        uv[1].to_bits(),
                    ];
                }
                let mut best = t;
                for _ in 0..2 {
                    t.rotate_left(1);
                    best = best.min(t);
                }
                triangles.push(best);
            }
            triangles.sort_unstable();
            triangles
        }
        self.uv1.is_none()
            && self.tangents.is_none()
            && self.colors.is_none()
            && triangles(self) == triangles(other)
    }
    pub(crate) fn matches(&self, mesh: &Mesh) -> bool {
        Self::capture(mesh)
            .ok()
            .is_some_and(|rhs| serde_json::to_vec(&rhs).ok() == serde_json::to_vec(self).ok())
    }
    pub fn capture(mesh: &Mesh) -> Result<Self, String> {
        if mesh.primitive_topology() != PrimitiveTopology::TriangleList
            || mesh.morph_targets().is_some()
        {
            return Err("Project v1 supports triangle meshes without morph targets".into());
        }
        let supported = [
            Mesh::ATTRIBUTE_POSITION.id,
            Mesh::ATTRIBUTE_NORMAL.id,
            Mesh::ATTRIBUTE_UV_0.id,
            Mesh::ATTRIBUTE_UV_1.id,
            Mesh::ATTRIBUTE_TANGENT.id,
            Mesh::ATTRIBUTE_COLOR.id,
        ];
        if mesh.attributes().any(|(a, _)| !supported.contains(&a.id)) {
            return Err("Project v1 does not support this mesh attribute".into());
        }
        let f3 = |a| match mesh.attribute(a) {
            None => Ok(None),
            Some(VertexAttributeValues::Float32x3(v)) => Ok(Some(v.clone())),
            _ => Err("Unsupported mesh attribute format".to_string()),
        };
        let f2 = |a| match mesh.attribute(a) {
            None => Ok(None),
            Some(VertexAttributeValues::Float32x2(v)) => Ok(Some(v.clone())),
            _ => Err("Unsupported mesh attribute format".to_string()),
        };
        let f4 = |a| match mesh.attribute(a) {
            None => Ok(None),
            Some(VertexAttributeValues::Float32x4(v)) => Ok(Some(v.clone())),
            _ => Err("Unsupported mesh attribute format".to_string()),
        };
        let out = Self {
            positions: f3(Mesh::ATTRIBUTE_POSITION)?.ok_or("Missing mesh positions")?,
            normals: f3(Mesh::ATTRIBUTE_NORMAL)?,
            uv0: f2(Mesh::ATTRIBUTE_UV_0)?,
            uv1: f2(Mesh::ATTRIBUTE_UV_1)?,
            tangents: f4(Mesh::ATTRIBUTE_TANGENT)?,
            colors: f4(Mesh::ATTRIBUTE_COLOR)?,
            indices: match mesh.indices() {
                None => IndexDocument::None,
                Some(Indices::U16(v)) => IndexDocument::U16(v.clone()),
                Some(Indices::U32(v)) => IndexDocument::U32(v.clone()),
            },
        };
        out.validate()?;
        Ok(out)
    }
    pub fn validate(&self) -> Result<(), String> {
        let n = self.positions.len();
        let lengths = [
            self.normals.as_ref().map(Vec::len),
            self.uv0.as_ref().map(Vec::len),
            self.uv1.as_ref().map(Vec::len),
            self.tangents.as_ref().map(Vec::len),
            self.colors.as_ref().map(Vec::len),
        ];
        let finite = self
            .positions
            .iter()
            .flatten()
            .chain(self.normals.iter().flat_map(|v| v.iter().flatten()))
            .chain(self.uv0.iter().flat_map(|v| v.iter().flatten()))
            .chain(self.uv1.iter().flat_map(|v| v.iter().flatten()))
            .chain(self.tangents.iter().flat_map(|v| v.iter().flatten()))
            .chain(self.colors.iter().flat_map(|v| v.iter().flatten()))
            .all(|v| v.is_finite());
        let (count, valid) = match &self.indices {
            IndexDocument::None => (n, true),
            IndexDocument::U16(v) => (v.len(), v.iter().all(|&i| (i as usize) < n)),
            IndexDocument::U32(v) => (v.len(), v.iter().all(|&i| (i as usize) < n)),
        };
        if n == 0
            || n > 250_000
            || count > 750_000
            || count == 0
            || count % 3 != 0
            || !valid
            || !finite
            || lengths.into_iter().flatten().any(|v| v != n)
        {
            return Err("Invalid or over-limit project mesh arrays/indices".into());
        }
        Ok(())
    }
    pub fn restore(self) -> Mesh {
        let mut m = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        m.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        if let Some(v) = self.normals {
            m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, v);
        }
        if let Some(v) = self.uv0 {
            m.insert_attribute(Mesh::ATTRIBUTE_UV_0, v);
        }
        if let Some(v) = self.uv1 {
            m.insert_attribute(Mesh::ATTRIBUTE_UV_1, v);
        }
        if let Some(v) = self.tangents {
            m.insert_attribute(Mesh::ATTRIBUTE_TANGENT, v);
        }
        if let Some(v) = self.colors {
            m.insert_attribute(Mesh::ATTRIBUTE_COLOR, v);
        }
        match self.indices {
            IndexDocument::None => {}
            IndexDocument::U16(v) => m.insert_indices(Indices::U16(v)),
            IndexDocument::U32(v) => m.insert_indices(Indices::U32(v)),
        };
        m
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ImageDocument {
    width: u32,
    height: u32,
    format: ImageFormat,
    data: Vec<u8>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
enum ImageFormat {
    Rgba8Srgb,
    Rgba8,
    Bgra8Srgb,
    Bgra8,
}
impl ImageDocument {
    pub(crate) fn validate_direct_uv(&self, width: u32, height: u32) -> Result<(), String> {
        if self.width != width
            || self.height != height
            || !matches!(self.format, ImageFormat::Rgba8Srgb)
        {
            return Err("DirectUV requires a same-size RGBA8 sRGB original image".into());
        }
        self.validate()
    }
    pub fn capture(image: &Image) -> Result<Self, String> {
        if image.texture_descriptor.dimension != TextureDimension::D2
            || image.texture_descriptor.size.depth_or_array_layers != 1
            || image.texture_descriptor.mip_level_count != 1
            || image.texture_view_descriptor.is_some()
            || !matches!(image.sampler, bevy::image::ImageSampler::Default)
        {
            return Err("Project v1 requires single-level 2D images with default sampler".into());
        }
        let format = match image.texture_descriptor.format {
            TextureFormat::Rgba8UnormSrgb => ImageFormat::Rgba8Srgb,
            TextureFormat::Rgba8Unorm => ImageFormat::Rgba8,
            TextureFormat::Bgra8UnormSrgb => ImageFormat::Bgra8Srgb,
            TextureFormat::Bgra8Unorm => ImageFormat::Bgra8,
            _ => return Err("Unsupported project image format".into()),
        };
        let out = Self {
            width: image.width(),
            height: image.height(),
            format,
            data: image
                .data
                .clone()
                .ok_or("Missing CPU-readable project image asset")?,
        };
        out.validate()?;
        Ok(out)
    }
    pub fn byte_count(&self) -> usize {
        self.data.len()
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.width == 0
            || self.height == 0
            || self.width > 4096
            || self.height > 4096
            || (self.width as usize)
                .checked_mul(self.height as usize)
                .and_then(|v| v.checked_mul(4))
                != Some(self.data.len())
        {
            return Err("Invalid or over-limit project image".into());
        }
        Ok(())
    }
    pub fn restore(self) -> Image {
        Image::new(
            Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            self.data,
            match self.format {
                ImageFormat::Rgba8Srgb => TextureFormat::Rgba8UnormSrgb,
                ImageFormat::Rgba8 => TextureFormat::Rgba8Unorm,
                ImageFormat::Bgra8Srgb => TextureFormat::Bgra8UnormSrgb,
                ImageFormat::Bgra8 => TextureFormat::Bgra8Unorm,
            },
            RenderAssetUsages::default(),
        )
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
enum ColorDocument {
    Srgb([f32; 4]),
    Linear([f32; 4]),
}
impl ColorDocument {
    fn capture(c: Color) -> Result<Self, String> {
        match c {
            Color::Srgba(c) => Ok(Self::Srgb(c.to_f32_array())),
            Color::LinearRgba(c) => Ok(Self::Linear(c.to_f32_array())),
            _ => Err("Unsupported project material color space".into()),
        }
    }
    fn restore(&self) -> Color {
        match self {
            Self::Srgb(v) => Color::srgba(v[0], v[1], v[2], v[3]),
            Self::Linear(v) => Color::linear_rgba(v[0], v[1], v[2], v[3]),
        }
    }
    fn finite(&self) -> bool {
        match self {
            Self::Srgb(v) | Self::Linear(v) => v.iter().all(|v| v.is_finite()),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
enum AlphaDocument {
    Opaque,
    Mask(f32),
    Blend,
    Premultiplied,
    AlphaToCoverage,
    Add,
    Multiply,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MaterialDocument {
    color: ColorDocument,
    pub texture: Option<ImageDocument>,
    roughness: f32,
    metallic: f32,
    reflectance: f32,
    double_sided: bool,
    cull: Option<bool>,
    unlit: bool,
    fog: bool,
    alpha: AlphaDocument,
    depth_bias: f32,
}
impl MaterialDocument {
    pub fn capture(m: &StandardMaterial, images: &Assets<Image>) -> Result<Self, String> {
        let d = StandardMaterial::default();
        macro_rules! require_default {($($field:ident),*)=>{if false $(|| m.$field!=d.$field)* {return Err("Unsupported project v1 material feature".into());}}}
        require_default!(
            base_color_channel,
            emissive,
            emissive_exposure_weight,
            emissive_channel,
            emissive_texture,
            metallic_roughness_channel,
            metallic_roughness_texture,
            specular_tint,
            diffuse_transmission,
            specular_transmission,
            thickness,
            ior,
            attenuation_distance,
            attenuation_color,
            normal_map_channel,
            normal_map_texture,
            flip_normal_map_y,
            occlusion_channel,
            occlusion_texture,
            clearcoat,
            clearcoat_perceptual_roughness,
            anisotropy_strength,
            anisotropy_rotation,
            depth_map,
            parallax_depth_scale,
            parallax_mapping_method,
            max_parallax_layer_count,
            lightmap_exposure,
            opaque_render_method,
            deferred_lighting_pass_id,
            uv_transform
        );
        let out = Self {
            color: ColorDocument::capture(m.base_color)?,
            texture: m
                .base_color_texture
                .as_ref()
                .map(|h| {
                    ImageDocument::capture(
                        images
                            .get(h)
                            .ok_or("Missing project material image asset")?,
                    )
                })
                .transpose()?,
            roughness: m.perceptual_roughness,
            metallic: m.metallic,
            reflectance: m.reflectance,
            double_sided: m.double_sided,
            cull: m.cull_mode.map(|f| f == Face::Back),
            unlit: m.unlit,
            fog: m.fog_enabled,
            alpha: match m.alpha_mode {
                AlphaMode::Opaque => AlphaDocument::Opaque,
                AlphaMode::Mask(v) => AlphaDocument::Mask(v),
                AlphaMode::Blend => AlphaDocument::Blend,
                AlphaMode::Premultiplied => AlphaDocument::Premultiplied,
                AlphaMode::AlphaToCoverage => AlphaDocument::AlphaToCoverage,
                AlphaMode::Add => AlphaDocument::Add,
                AlphaMode::Multiply => AlphaDocument::Multiply,
            },
            depth_bias: m.depth_bias,
        };
        out.validate()?;
        Ok(out)
    }
    pub fn validate(&self) -> Result<(), String> {
        if !self.color.finite()
            || ![
                self.roughness,
                self.metallic,
                self.reflectance,
                self.depth_bias,
            ]
            .iter()
            .all(|v| v.is_finite())
            || matches!(self.alpha,AlphaDocument::Mask(v) if !v.is_finite())
        {
            return Err("Invalid project material numbers".into());
        }
        if let Some(t) = &self.texture {
            t.validate()?;
        }
        Ok(())
    }
    pub fn restore(self, images: &mut Assets<Image>) -> StandardMaterial {
        StandardMaterial {
            base_color: self.color.restore(),
            base_color_channel: UvChannel::Uv0,
            base_color_texture: self.texture.map(|i| images.add(i.restore())),
            perceptual_roughness: self.roughness,
            metallic: self.metallic,
            reflectance: self.reflectance,
            double_sided: self.double_sided,
            cull_mode: self.cull.map(|b| if b { Face::Back } else { Face::Front }),
            unlit: self.unlit,
            fog_enabled: self.fog,
            alpha_mode: match self.alpha {
                AlphaDocument::Opaque => AlphaMode::Opaque,
                AlphaDocument::Mask(v) => AlphaMode::Mask(v),
                AlphaDocument::Blend => AlphaMode::Blend,
                AlphaDocument::Premultiplied => AlphaMode::Premultiplied,
                AlphaDocument::AlphaToCoverage => AlphaMode::AlphaToCoverage,
                AlphaDocument::Add => AlphaMode::Add,
                AlphaDocument::Multiply => AlphaMode::Multiply,
            },
            depth_bias: self.depth_bias,
            ..default()
        }
    }
}
