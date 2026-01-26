//! Mesh painting system for Bevy integration
//!
//! This module connects MeshPaintEvent messages to mesh painting surfaces
//! and handles GPU texture upload for painted meshes.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use std::collections::HashMap;

use painting::mesh_surface::{MeshPtexSurface, MeshUvSurface};
use painting::types::{BlendMode, MeshHit, MeshStorageMode};
use painting::BrushPreset;

use crate::mesh_paint_mode::{MeshPaintEvent, PaintableMesh};

/// Resource holding painting surfaces for each paintable mesh
#[derive(Resource)]
pub struct MeshPaintingResource {
    /// UV-based surfaces indexed by mesh_id
    uv_surfaces: HashMap<u32, MeshUvSurface>,
    /// Ptex-based surfaces indexed by mesh_id
    ptex_surfaces: HashMap<u32, MeshPtexSurface>,
    /// Current brush color
    pub brush_color: [f32; 4],
    /// Current brush preset
    pub brush_preset: BrushPreset,
    /// Current blend mode
    pub blend_mode: BlendMode,
}

impl Default for MeshPaintingResource {
    fn default() -> Self {
        Self::new()
    }
}

impl MeshPaintingResource {
    /// Create a new mesh painting resource.
    pub fn new() -> Self {
        Self {
            uv_surfaces: HashMap::new(),
            ptex_surfaces: HashMap::new(),
            brush_color: [0.0, 0.0, 0.0, 1.0],
            brush_preset: BrushPreset::default(),
            blend_mode: BlendMode::Normal,
        }
    }

    /// Get or create a UV surface for a mesh.
    pub fn get_or_create_uv_surface(
        &mut self,
        mesh_id: u32,
        width: u32,
        height: u32,
    ) -> &mut MeshUvSurface {
        self.uv_surfaces
            .entry(mesh_id)
            .or_insert_with(|| MeshUvSurface::new(mesh_id, width, height, 2))
    }

    /// Get or create a Ptex surface for a mesh.
    pub fn get_or_create_ptex_surface(
        &mut self,
        mesh_id: u32,
        face_resolution: u32,
    ) -> &mut MeshPtexSurface {
        self.ptex_surfaces
            .entry(mesh_id)
            .or_insert_with(|| MeshPtexSurface::new(mesh_id, face_resolution))
    }

    /// Get a UV surface by mesh_id.
    pub fn get_uv_surface(&self, mesh_id: u32) -> Option<&MeshUvSurface> {
        self.uv_surfaces.get(&mesh_id)
    }

    /// Get a mutable UV surface by mesh_id.
    pub fn get_uv_surface_mut(&mut self, mesh_id: u32) -> Option<&mut MeshUvSurface> {
        self.uv_surfaces.get_mut(&mesh_id)
    }

    /// Get a Ptex surface by mesh_id.
    pub fn get_ptex_surface(&self, mesh_id: u32) -> Option<&MeshPtexSurface> {
        self.ptex_surfaces.get(&mesh_id)
    }

    /// Get a mutable Ptex surface by mesh_id.
    pub fn get_ptex_surface_mut(&mut self, mesh_id: u32) -> Option<&mut MeshPtexSurface> {
        self.ptex_surfaces.get_mut(&mesh_id)
    }

    /// Set brush color.
    pub fn set_brush_color(&mut self, color: [f32; 4]) {
        self.brush_color = color;
    }

    /// Set brush preset.
    pub fn set_brush_preset(&mut self, preset: BrushPreset) {
        self.brush_preset = preset;
    }

    /// Set blend mode.
    pub fn set_blend_mode(&mut self, mode: BlendMode) {
        self.blend_mode = mode;
    }
}

/// Component linking a PaintableMesh to its GPU texture.
#[derive(Component)]
pub struct MeshPaintTexture {
    /// Handle to the Bevy Image asset
    pub image_handle: Handle<Image>,
    /// Whether this needs full upload
    pub needs_full_upload: bool,
}

/// Plugin for mesh painting system.
pub struct MeshPaintingSystemPlugin;

impl Plugin for MeshPaintingSystemPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MeshPaintingResource>()
            .add_systems(
                Update,
                (
                    setup_mesh_paint_textures,
                    process_mesh_paint_events,
                    upload_mesh_dirty_tiles,
                )
                    .chain(),
            );
    }
}

/// Set up paint textures for newly added PaintableMesh entities.
fn setup_mesh_paint_textures(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut painting_res: ResMut<MeshPaintingResource>,
    query: Query<
        (Entity, &PaintableMesh, Option<&MeshMaterial3d<StandardMaterial>>),
        Without<MeshPaintTexture>,
    >,
) {
    for (entity, paintable, material_handle) in query.iter() {
        let (width, height) = match paintable.storage_mode {
            MeshStorageMode::UvAtlas { resolution } => resolution,
            MeshStorageMode::Ptex { face_resolution } => {
                // For Ptex, we create a placeholder texture
                // Actual per-face textures are managed separately
                (face_resolution * 16, face_resolution * 16)
            }
        };

        // Create the paint texture image
        let mut image = Image::new_fill(
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[0, 0, 0, 0], // Transparent
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
        );

        image.texture_descriptor.usage =
            TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;

        let image_handle = images.add(image);

        // Initialize the surface
        match paintable.storage_mode {
            MeshStorageMode::UvAtlas { resolution } => {
                let surface =
                    painting_res.get_or_create_uv_surface(paintable.mesh_id, resolution.0, resolution.1);
                surface.surface_mut().surface_mut().clear([0.0, 0.0, 0.0, 0.0]);
            }
            MeshStorageMode::Ptex { face_resolution } => {
                painting_res.get_or_create_ptex_surface(paintable.mesh_id, face_resolution);
            }
        }

        // Apply texture to material if available
        if let Some(material_ref) = material_handle {
            if let Some(material) = materials.get_mut(&material_ref.0) {
                material.base_color_texture = Some(image_handle.clone());
                material.base_color = Color::WHITE;
                material.alpha_mode = AlphaMode::Blend;
                material.unlit = true;
                material.double_sided = true;
            }
        }

        commands.entity(entity).insert(MeshPaintTexture {
            image_handle,
            needs_full_upload: true,
        });

        info!(
            "Set up paint texture for mesh_id={} ({}x{})",
            paintable.mesh_id, width, height
        );
    }
}

/// Process mesh paint events and apply dabs to surfaces.
fn process_mesh_paint_events(
    mut mesh_paint_events: MessageReader<MeshPaintEvent>,
    mut painting_res: ResMut<MeshPaintingResource>,
    mesh_query: Query<&PaintableMesh>,
) {
    for event in mesh_paint_events.read() {
        match event {
            MeshPaintEvent::StrokeStart {
                mesh_entity,
                mesh_id,
                hit,
                stroke_id,
            } => {
                info!("Mesh stroke start: mesh_id={}, stroke_id={}", mesh_id, stroke_id);
                apply_dab_to_mesh(&mut painting_res, &mesh_query, *mesh_entity, hit);
            }
            MeshPaintEvent::StrokeMove {
                hit,
                pressure,
                speed,
            } => {
                // We need to know which mesh - get from hit's context
                // For now, iterate and find matching mesh by checking if surface exists
                // In a real implementation, we'd track the active mesh entity
                for (_, paintable) in mesh_query.iter().enumerate() {
                    if painting_res.get_uv_surface(paintable.mesh_id).is_some()
                        || painting_res.get_ptex_surface(paintable.mesh_id).is_some()
                    {
                        apply_dab_for_move(&mut painting_res, paintable.mesh_id, hit, *pressure);
                        break;
                    }
                }
            }
            MeshPaintEvent::StrokeEnd => {
                info!("Mesh stroke end");
            }
            MeshPaintEvent::StrokeCancel => {
                info!("Mesh stroke cancelled");
            }
        }
    }
}

/// Apply a dab to a mesh surface based on hit data.
fn apply_dab_to_mesh(
    painting_res: &mut MeshPaintingResource,
    mesh_query: &Query<&PaintableMesh>,
    mesh_entity: Entity,
    hit: &MeshHit,
) {
    let Ok(paintable) = mesh_query.get(mesh_entity) else {
        return;
    };

    let brush_size = painting_res.brush_preset.base_size;
    let color = painting_res.brush_color;
    let opacity = painting_res.brush_preset.opacity;
    let hardness = painting_res.brush_preset.hardness;
    let blend_mode = painting_res.blend_mode;

    match paintable.storage_mode {
        MeshStorageMode::UvAtlas { resolution } => {
            if let Some(uv) = hit.uv {
                let surface = painting_res.get_or_create_uv_surface(
                    paintable.mesh_id,
                    resolution.0,
                    resolution.1,
                );

                // Convert world brush size to texture pixels
                // For simplicity, use a fixed scale based on texture resolution
                let avg_res = (resolution.0 + resolution.1) as f32 / 2.0;
                let texel_radius = brush_size * avg_res / 10.0; // Approximate scaling

                surface.apply_dab(
                    uv,
                    texel_radius,
                    color,
                    opacity,
                    hardness,
                    blend_mode,
                    0.0, // angle
                    1.0, // aspect_ratio (circular)
                );
            }
        }
        MeshStorageMode::Ptex { face_resolution } => {
            let surface =
                painting_res.get_or_create_ptex_surface(paintable.mesh_id, face_resolution);

            // Convert barycentric to face-local coordinates
            let local_coords = Vec2::new(
                hit.barycentric.x * face_resolution as f32,
                hit.barycentric.y * face_resolution as f32,
            );

            surface.apply_dab(
                hit.face_id,
                local_coords,
                brush_size,
                color,
                opacity,
                hardness,
                blend_mode,
            );
        }
    }
}

/// Apply a dab for stroke move event.
fn apply_dab_for_move(
    painting_res: &mut MeshPaintingResource,
    mesh_id: u32,
    hit: &MeshHit,
    pressure: f32,
) {
    let brush_size = painting_res.brush_preset.size_for_pressure(pressure);
    let color = painting_res.brush_color;
    let opacity = painting_res.brush_preset.opacity;
    let hardness = painting_res.brush_preset.hardness;
    let blend_mode = painting_res.blend_mode;

    // Try UV surface first
    if let Some(surface) = painting_res.get_uv_surface_mut(mesh_id) {
        if let Some(uv) = hit.uv {
            let (width, height) = surface.dimensions();
            let avg_res = (width + height) as f32 / 2.0;
            let texel_radius = brush_size * avg_res / 10.0;

            surface.apply_dab(uv, texel_radius, color, opacity, hardness, blend_mode, 0.0, 1.0);
        }
        return;
    }

    // Try Ptex surface
    if let Some(surface) = painting_res.get_ptex_surface_mut(mesh_id) {
        let face_resolution = surface.default_resolution;
        let local_coords = Vec2::new(
            hit.barycentric.x * face_resolution as f32,
            hit.barycentric.y * face_resolution as f32,
        );

        surface.apply_dab(
            hit.face_id,
            local_coords,
            brush_size,
            color,
            opacity,
            hardness,
            blend_mode,
        );
    }
}

/// Upload dirty tiles to GPU for UV surfaces.
fn upload_mesh_dirty_tiles(
    painting_res: Res<MeshPaintingResource>,
    mut images: ResMut<Assets<Image>>,
    query: Query<(&PaintableMesh, &MeshPaintTexture)>,
) {
    for (paintable, paint_texture) in query.iter() {
        match paintable.storage_mode {
            MeshStorageMode::UvAtlas { .. } => {
                if let Some(surface) = painting_res.get_uv_surface(paintable.mesh_id) {
                    if surface.has_dirty_tiles() || paint_texture.needs_full_upload {
                        // Upload entire texture for now
                        // TODO: Implement partial tile upload like canvas planes
                        if let Some(image) = images.get_mut(&paint_texture.image_handle) {
                            let cpu_surface = surface.surface().surface();
                            let width = cpu_surface.width as usize;
                            let height = cpu_surface.height as usize;

                            let mut data = vec![0u8; width * height * 4];
                            for y in 0..height {
                                for x in 0..width {
                                    if let Some(pixel) =
                                        cpu_surface.get_pixel(x as u32, y as u32)
                                    {
                                        let idx = (y * width + x) * 4;
                                        data[idx] = linear_to_srgb_u8(pixel[0]);
                                        data[idx + 1] = linear_to_srgb_u8(pixel[1]);
                                        data[idx + 2] = linear_to_srgb_u8(pixel[2]);
                                        data[idx + 3] = (pixel[3] * 255.0) as u8;
                                    }
                                }
                            }
                            image.data = Some(data);
                        }
                    }
                }
            }
            MeshStorageMode::Ptex { .. } => {
                // Ptex upload would require a different texture format
                // or compositing faces into an atlas
                // For now, this is a placeholder
            }
        }
    }
}

/// Convert linear color to sRGB u8.
fn linear_to_srgb_u8(linear: f32) -> u8 {
    let srgb = if linear <= 0.0031308 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    };
    (srgb.clamp(0.0, 1.0) * 255.0) as u8
}
