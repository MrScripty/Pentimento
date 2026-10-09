//! DirectUV extension of the canonical project schema; derived images are rebuilt.
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DirectUvDocument {
    pub width: u32,
    pub height: u32,
    pub seam_padding: u32,
    pub pixels: Vec<[f32; 4]>,
    pub display_bound: bool,
    pub original_linear_color: [f32; 4],
}
impl DirectUvDocument {
    pub fn validate(&self, resolution: (u32, u32)) -> Result<(), String> {
        if (self.width, self.height) != resolution
            || self.width == 0
            || self.height == 0
            || self.width > 1048
            || self.height > 1048
            || self.seam_padding > self.width.min(self.height)
            || self.pixels.len() != self.width as usize * self.height as usize
            || (!self.display_bound && self.pixels.iter().flatten().any(|v| v.to_bits() != 0))
            || self
                .pixels
                .iter()
                .flatten()
                .chain(&self.original_linear_color)
                .any(|p| !p.is_finite() || !(0.0..=1.0).contains(p))
        {
            return Err("Invalid/over-limit DirectUV authoring surface".into());
        }
        Ok(())
    }
}

#[cfg(feature = "mesh_painting")]
pub(crate) fn capture(
    world: &World,
    entity: Entity,
    current: &StandardMaterial,
) -> Result<Option<(DirectUvDocument, StandardMaterial)>, String> {
    capture_impl(world, entity, current, false)
}
#[cfg(feature = "mesh_painting")]
pub(crate) fn capture_layer_migration(
    world: &World,
    entity: Entity,
    current: &StandardMaterial,
) -> Result<Option<(DirectUvDocument, StandardMaterial)>, String> {
    capture_impl(world, entity, current, true)
}
#[cfg(feature = "mesh_painting")]
fn capture_impl(
    world: &World,
    entity: Entity,
    current: &StandardMaterial,
    allow_projection: bool,
) -> Result<Option<(DirectUvDocument, StandardMaterial)>, String> {
    let Some(p) = world.get::<crate::PaintableMesh>(entity) else {
        return Ok(None);
    };
    let resource = world.get_resource::<crate::MeshPaintingResource>();
    let has_ptex = resource
        .and_then(|r| r.get_ptex_surface(p.mesh_id))
        .is_some_and(|s| s.face_count() > 0);
    let painting::MeshStorageMode::UvAtlas { resolution } = p.storage_mode else {
        if has_ptex {
            return Err("Painted PTex cannot be saved: per-face rendering/upload is not implemented; no project file was changed.".into());
        }
        if resource
            .and_then(|r| r.get_uv_surface(p.mesh_id))
            .is_some_and(|s| {
                s.atlas
                    .surface()
                    .pixels()
                    .iter()
                    .any(|p| p.iter().any(|v| v.to_bits() != 0))
            })
            || world
                .get::<crate::MeshPaintTexture>(entity)
                .is_some_and(|t| t.has_paint)
        {
            return Err(
                "PTex storage has retained DirectUV work; refusing to drop a previous paint owner"
                    .into(),
            );
        }
        return Ok(None);
    };
    if has_ptex {
        return Err(
            "DirectUV storage has retained PTex faces; refusing to drop a previous paint owner"
                .into(),
        );
    }
    let surface = resource
        .and_then(|r| r.get_uv_surface(p.mesh_id))
        .ok_or("DirectUV setup is pending; wait for scene settlement before saving")?;
    let texture = world
        .get::<crate::MeshPaintTexture>(entity)
        .ok_or("Missing owned DirectUV display texture")?;
    if texture.has_paint
        && (current.base_color != Color::WHITE
            || current.base_color_texture.as_ref() != Some(&texture.image_handle))
    {
        return Err("DirectUV material has another paint owner or was edited externally; refusing mixed projection/direct data".into());
    }
    if !texture.has_paint
        && (current.base_color != texture.original_color_source
            || current.base_color_texture != texture.original_texture)
    {
        return Err("DirectUV original appearance changed outside its owner".into());
    }
    let meaningful = surface
        .atlas
        .surface()
        .pixels()
        .iter()
        .any(|p| p.iter().any(|v| v.to_bits() != 0));
    if meaningful
        && !allow_projection
        && world
            .get_resource::<crate::ProjectionTargets>()
            .is_some_and(|targets| !targets.document_layers(entity).is_empty())
    {
        return Err("Simultaneous DirectUV and canvas-projected paint require a shared material owner; refusing to omit either".into());
    }
    let images = world
        .get_resource::<Assets<Image>>()
        .ok_or("Image assets unavailable")?;
    crate::project_assets::ImageDocument::capture(
        images
            .get(&texture.image_handle)
            .ok_or("Missing owned DirectUV image")?,
    )?
    .validate_direct_uv(resolution.0, resolution.1)?;
    if let Some(handle) = &texture.original_texture {
        let image = images
            .get(handle)
            .ok_or("Missing original DirectUV image")?;
        if image.texture_descriptor.size.width != resolution.0
            || image.texture_descriptor.size.height != resolution.1
            || image.texture_descriptor.format
                != bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb
        {
            return Err("DirectUV currently requires a same-size RGBA8 sRGB original image".into());
        }
    }
    let document = DirectUvDocument {
        width: resolution.0,
        height: resolution.1,
        seam_padding: surface.seam_padding,
        pixels: surface.atlas.surface().pixels().to_vec(),
        display_bound: texture.has_paint,
        original_linear_color: texture.original_base_color,
    };
    document.validate(resolution)?;
    if texture.has_paint {
        let expected = crate::mesh_painting_system::composite_uv(
            surface,
            texture.original_base_color,
            texture
                .original_texture
                .as_ref()
                .and_then(|h| images.get(h))
                .and_then(|i| i.data.as_deref()),
        );
        if images
            .get(&texture.image_handle)
            .and_then(|i| i.data.as_deref())
            != Some(expected.as_slice())
        {
            return Err(
                "DirectUV display is stale or externally changed; wait for upload before saving"
                    .into(),
            );
        }
    } else if meaningful {
        return Err("DirectUV display upload is pending; wait before saving".into());
    }
    let mut original = current.clone();
    original.base_color = texture.original_color_source;
    original.base_color_texture = texture.original_texture.clone();
    Ok(Some((document, original)))
}

#[cfg(feature = "mesh_painting")]
pub(crate) fn install(
    world: &mut World,
    entity: Entity,
    id: u32,
    document: DirectUvDocument,
    material: Handle<StandardMaterial>,
) {
    use bevy::asset::RenderAssetUsages;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
    let original = world
        .resource::<Assets<StandardMaterial>>()
        .get(&material)
        .unwrap()
        .clone();
    let mut surface =
        painting::MeshUvSurface::new(id, document.width, document.height, document.seam_padding);
    surface
        .atlas
        .surface_mut()
        .pixels_mut()
        .copy_from_slice(&document.pixels);
    let data = crate::mesh_painting_system::composite_uv(
        &surface,
        document.original_linear_color,
        original
            .base_color_texture
            .as_ref()
            .and_then(|h| world.resource::<Assets<Image>>().get(h))
            .and_then(|i| i.data.as_deref()),
    );
    let image = Image::new(
        Extent3d {
            width: document.width,
            height: document.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data.clone(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    let image_handle = world.resource_mut::<Assets<Image>>().add(image);
    let original_bytes = original
        .base_color_texture
        .as_ref()
        .and_then(|h| world.resource::<Assets<Image>>().get(h))
        .and_then(|i| i.data.clone());
    let mesh = world.get::<Mesh3d>(entity).unwrap().0.id();
    let mut resource = world.resource_mut::<crate::MeshPaintingResource>();
    resource.record_installed_owner(
        id,
        entity,
        mesh,
        painting::MeshStorageMode::UvAtlas {
            resolution: (document.width, document.height),
        },
        image_handle.id(),
        original.base_color_texture.as_ref().map(|h| h.id()),
    );
    resource.restore_uv_surface(surface);
    resource.shared_display_bytes.insert(id, data);
    resource.shared_original_bytes.insert(id, original_bytes);
    drop(resource);
    if document.display_bound {
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        let material = materials.get_mut(&material).unwrap();
        material.base_color = Color::WHITE;
        material.base_color_texture = Some(image_handle.clone());
    }
    world.entity_mut(entity).insert(crate::MeshPaintTexture {
        image_handle,
        original_texture: original.base_color_texture,
        original_base_color: document.original_linear_color,
        original_color_source: original.base_color,
        needs_full_upload: false,
        has_paint: document.display_bound,
    });
}
