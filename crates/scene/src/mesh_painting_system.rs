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
    /// Handle to the Bevy Image asset (composited paint + original)
    pub image_handle: Handle<Image>,
    /// Handle to original material texture (if any) for compositing
    pub original_texture: Option<Handle<Image>>,
    /// Original base color from material (for meshes without texture)
    pub original_base_color: [f32; 4],
    /// Whether this needs full upload
    pub needs_full_upload: bool,
    /// Whether any paint has been applied (don't touch material until painting)
    pub has_paint: bool,
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
    materials: Res<Assets<StandardMaterial>>,
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

        // Extract original texture and color from material (don't modify material yet)
        let (original_texture, original_base_color) = if let Some(material_ref) = material_handle {
            if let Some(material) = materials.get(&material_ref.0) {
                let color = material.base_color.to_linear();
                (
                    material.base_color_texture.clone(),
                    [color.red, color.green, color.blue, color.alpha],
                )
            } else {
                (None, [0.8, 0.8, 0.8, 1.0])
            }
        } else {
            (None, [0.8, 0.8, 0.8, 1.0])
        };

        commands.entity(entity).insert(MeshPaintTexture {
            image_handle,
            original_texture,
            original_base_color,
            needs_full_upload: false, // Don't upload until we have paint
            has_paint: false,
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
                speed: _,
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

/// Upload dirty tiles to GPU for UV surfaces, compositing paint over original texture.
fn upload_mesh_dirty_tiles(
    painting_res: Res<MeshPaintingResource>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut query: Query<(
        &PaintableMesh,
        &mut MeshPaintTexture,
        Option<&MeshMaterial3d<StandardMaterial>>,
    )>,
) {
    for (paintable, mut paint_texture, material_handle) in query.iter_mut() {
        match paintable.storage_mode {
            MeshStorageMode::UvAtlas { .. } => {
                if let Some(surface) = painting_res.get_uv_surface(paintable.mesh_id) {
                    if !surface.has_dirty_tiles() && !paint_texture.needs_full_upload {
                        continue;
                    }

                    let cpu_surface = surface.surface().surface();
                    let width = cpu_surface.width as usize;
                    let height = cpu_surface.height as usize;

                    // Get original texture data for compositing
                    let original_data: Option<Vec<u8>> =
                        paint_texture.original_texture.as_ref().and_then(|handle| {
                            images.get(handle).and_then(|img| img.data.clone())
                        });

                    // Composite paint over original
                    let mut data = vec![0u8; width * height * 4];
                    for y in 0..height {
                        for x in 0..width {
                            let idx = (y * width + x) * 4;

                            // Get original pixel (from texture or base color)
                            let (orig_r, orig_g, orig_b, orig_a) =
                                if let Some(ref orig) = original_data {
                                    if idx + 3 < orig.len() {
                                        (orig[idx], orig[idx + 1], orig[idx + 2], orig[idx + 3])
                                    } else {
                                        color_to_srgb_u8(paint_texture.original_base_color)
                                    }
                                } else {
                                    color_to_srgb_u8(paint_texture.original_base_color)
                                };

                            // Get paint pixel
                            if let Some(paint_pixel) = cpu_surface.get_pixel(x as u32, y as u32) {
                                let paint_alpha = paint_pixel[3];

                                if paint_alpha > 0.001 {
                                    // Alpha blend paint over original
                                    let paint_r = linear_to_srgb_u8(paint_pixel[0]);
                                    let paint_g = linear_to_srgb_u8(paint_pixel[1]);
                                    let paint_b = linear_to_srgb_u8(paint_pixel[2]);
                                    let paint_a = (paint_alpha * 255.0) as u8;

                                    let alpha = paint_a as f32 / 255.0;
                                    let inv_alpha = 1.0 - alpha;

                                    data[idx] =
                                        (paint_r as f32 * alpha + orig_r as f32 * inv_alpha) as u8;
                                    data[idx + 1] =
                                        (paint_g as f32 * alpha + orig_g as f32 * inv_alpha) as u8;
                                    data[idx + 2] =
                                        (paint_b as f32 * alpha + orig_b as f32 * inv_alpha) as u8;
                                    data[idx + 3] = orig_a.max(paint_a);
                                } else {
                                    // No paint, use original
                                    data[idx] = orig_r;
                                    data[idx + 1] = orig_g;
                                    data[idx + 2] = orig_b;
                                    data[idx + 3] = orig_a;
                                }
                            } else {
                                // No paint data, use original
                                data[idx] = orig_r;
                                data[idx + 1] = orig_g;
                                data[idx + 2] = orig_b;
                                data[idx + 3] = orig_a;
                            }
                        }
                    }

                    // Upload to GPU texture
                    if let Some(image) = images.get_mut(&paint_texture.image_handle) {
                        image.data = Some(data);
                    }

                    // Apply texture to material on first paint
                    if !paint_texture.has_paint {
                        if let Some(material_ref) = material_handle {
                            if let Some(material) = materials.get_mut(&material_ref.0) {
                                material.base_color_texture =
                                    Some(paint_texture.image_handle.clone());
                                material.base_color = Color::WHITE;
                            }
                        }
                        paint_texture.has_paint = true;
                    }

                    paint_texture.needs_full_upload = false;
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

/// Convert linear [f32; 4] color to sRGB (u8, u8, u8, u8).
fn color_to_srgb_u8(color: [f32; 4]) -> (u8, u8, u8, u8) {
    (
        linear_to_srgb_u8(color[0]),
        linear_to_srgb_u8(color[1]),
        linear_to_srgb_u8(color[2]),
        (color[3] * 255.0) as u8,
    )
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
