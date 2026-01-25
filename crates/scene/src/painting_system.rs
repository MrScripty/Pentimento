//! Painting system for Bevy integration
//!
//! This module connects PaintEvent messages to the painting pipeline
//! and handles GPU texture upload for dirty tiles.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::{
    extract_resource::ExtractResource,
    render_asset::RenderAssets,
    render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
    renderer::RenderQueue,
    texture::GpuImage,
    Render, RenderApp, RenderSystems,
};
use std::collections::HashMap;

use painting::{BrushPreset, PaintingPipeline};

use crate::canvas_plane::{ActiveCanvasPlane, CanvasPlane};
use crate::paint_mode::PaintEvent;

/// Resource holding painting pipelines for each canvas plane
///
/// Each canvas plane gets its own painting pipeline, indexed by plane_id.
#[derive(Resource)]
pub struct PaintingResource {
    /// Mapping from plane_id to pipeline
    pipelines: HashMap<u32, PaintingPipeline>,
    /// Current brush color
    pub brush_color: [f32; 4],
    /// Current brush preset
    pub brush_preset: BrushPreset,
}

impl Default for PaintingResource {
    fn default() -> Self {
        Self::new()
    }
}

impl PaintingResource {
    /// Create a new painting resource
    pub fn new() -> Self {
        Self {
            pipelines: HashMap::new(),
            brush_color: [0.0, 0.0, 0.0, 1.0], // Default black
            brush_preset: BrushPreset::default(),
        }
    }

    /// Get or create a pipeline for a canvas plane
    pub fn get_or_create_pipeline(&mut self, plane_id: u32, width: u32, height: u32) -> &mut PaintingPipeline {
        self.pipelines.entry(plane_id).or_insert_with(|| {
            let mut pipeline = PaintingPipeline::new(width, height);
            pipeline.set_color(self.brush_color);
            pipeline.set_brush(self.brush_preset.clone());
            // Clear to white by default
            pipeline.clear([1.0, 1.0, 1.0, 1.0]);
            pipeline
        })
    }

    /// Get a pipeline for a canvas plane
    pub fn get_pipeline(&self, plane_id: u32) -> Option<&PaintingPipeline> {
        self.pipelines.get(&plane_id)
    }

    /// Get a mutable pipeline for a canvas plane
    pub fn get_pipeline_mut(&mut self, plane_id: u32) -> Option<&mut PaintingPipeline> {
        self.pipelines.get_mut(&plane_id)
    }

    /// Set brush color for all pipelines
    pub fn set_brush_color(&mut self, color: [f32; 4]) {
        self.brush_color = color;
        for pipeline in self.pipelines.values_mut() {
            pipeline.set_color(color);
        }
    }

    /// Set brush preset for all pipelines
    pub fn set_brush_preset(&mut self, preset: BrushPreset) {
        self.brush_preset = preset.clone();
        for pipeline in self.pipelines.values_mut() {
            pipeline.set_brush(preset.clone());
        }
    }
}

/// Component linking a CanvasPlane to its GPU texture
#[derive(Component)]
pub struct CanvasTexture {
    /// Handle to the Bevy Image asset
    pub image_handle: Handle<Image>,
    /// Whether this is the first frame (needs full upload)
    pub needs_full_upload: bool,
}

/// Single tile upload request for partial GPU texture updates
#[derive(Clone)]
pub struct DirtyTileUpload {
    /// Pixel offset in texture (x, y)
    pub offset: (u32, u32),
    /// Tile dimensions (width, height)
    pub size: (u32, u32),
    /// RGBA8 pixel data (row-major, size.0 * size.1 * 4 bytes)
    pub data: Vec<u8>,
}

/// Per-canvas dirty tile buffer
#[derive(Clone)]
pub struct CanvasDirtyTileBuffer {
    /// Asset ID of the canvas texture (for GpuImage lookup)
    pub image_id: AssetId<Image>,
    /// Pending tile uploads
    pub tiles: Vec<DirtyTileUpload>,
}

/// Resource for buffering dirty tile uploads to be extracted to render world
#[derive(Resource, Default, Clone, ExtractResource)]
pub struct DirtyTileUploadBuffer {
    /// Per-canvas upload buffers
    pub canvases: Vec<CanvasDirtyTileBuffer>,
}

/// Convert f32 RGBA surface data to u8 RGBA for GPU upload
/// Input: &[u8] containing [f32; 4] per pixel (from CpuSurface::as_bytes)
/// Output: Vec<u8> containing [u8; 4] per pixel
fn surface_to_rgba8(surface_bytes: &[u8]) -> Vec<u8> {
    // surface_bytes is &[u8] but contains f32 data
    let f32_slice: &[[f32; 4]] = bytemuck::cast_slice(surface_bytes);
    let mut output = Vec::with_capacity(f32_slice.len() * 4);

    for pixel in f32_slice {
        // Convert f32 (0.0-1.0) to u8 (0-255) with sRGB gamma
        // Apply gamma correction: linear to sRGB
        output.push(linear_to_srgb_u8(pixel[0]));
        output.push(linear_to_srgb_u8(pixel[1]));
        output.push(linear_to_srgb_u8(pixel[2]));
        output.push((pixel[3].clamp(0.0, 1.0) * 255.0) as u8); // Alpha stays linear
    }

    output
}

/// Convert tile f32 RGBA data to u8 RGBA for partial GPU upload
/// Input: &[[f32; 4]] per pixel (from TiledSurface::get_tile_data)
/// Output: Vec<u8> containing [u8; 4] per pixel
fn tile_data_to_rgba8(tile_data: &[[f32; 4]]) -> Vec<u8> {
    let mut output = Vec::with_capacity(tile_data.len() * 4);

    for pixel in tile_data {
        // Convert f32 (0.0-1.0) to u8 (0-255) with sRGB gamma
        output.push(linear_to_srgb_u8(pixel[0]));
        output.push(linear_to_srgb_u8(pixel[1]));
        output.push(linear_to_srgb_u8(pixel[2]));
        output.push((pixel[3].clamp(0.0, 1.0) * 255.0) as u8); // Alpha stays linear
    }

    output
}

/// Convert linear float to sRGB u8
#[inline]
fn linear_to_srgb_u8(linear: f32) -> u8 {
    let linear = linear.clamp(0.0, 1.0);
    let srgb = if linear <= 0.0031308 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    };
    (srgb * 255.0) as u8
}

/// Plugin for the painting system
pub struct PaintingSystemPlugin;

impl Plugin for PaintingSystemPlugin {
    fn build(&self, app: &mut App) {
        // Main world resources and systems
        app.init_resource::<PaintingResource>()
            .init_resource::<DirtyTileUploadBuffer>()
            // ExtractResourcePlugin must be added to main app, not render_app
            .add_plugins(
                bevy::render::extract_resource::ExtractResourcePlugin::<DirtyTileUploadBuffer>::default(),
            )
            .add_systems(
                Update,
                (
                    setup_canvas_textures,
                    process_paint_events,
                    extract_dirty_tiles,
                )
                    .chain(),
            );

        // Render world system for GPU tile uploads
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            warn!("PaintingSystemPlugin: RenderApp not available, skipping render world setup");
            return;
        };

        render_app.add_systems(Render, upload_dirty_tiles_to_gpu.in_set(RenderSystems::Prepare));
    }
}

/// Setup textures for newly created canvas planes
fn setup_canvas_textures(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    query: Query<(Entity, &CanvasPlane, &MeshMaterial3d<StandardMaterial>), Without<CanvasTexture>>,
    mut painting_res: ResMut<PaintingResource>,
) {
    for (entity, canvas_plane, material_handle) in query.iter() {
        let width = canvas_plane.width;
        let height = canvas_plane.height;

        // Create the painting pipeline for this plane
        let pipeline = painting_res.get_or_create_pipeline(canvas_plane.plane_id, width, height);

        // Get the surface data and convert to RGBA8
        let surface_bytes = pipeline.surface_as_bytes();
        let rgba8_data = surface_to_rgba8(surface_bytes);

        // Create Bevy Image with Rgba8UnormSrgb format
        // This is the standard format with best compatibility
        let mut image = Image::new(
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            rgba8_data,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );

        // Set texture usages for painting
        image.texture_descriptor.usage =
            TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::COPY_SRC;

        let handle = images.add(image);

        // Update the material to use this texture
        if let Some(material) = materials.get_mut(&material_handle.0) {
            material.base_color_texture = Some(handle.clone());
            material.base_color = Color::WHITE; // Full color so texture shows through
            info!(
                "Set texture on material for canvas plane {}",
                canvas_plane.plane_id
            );
        }

        // Insert the CanvasTexture component
        commands.entity(entity).insert(CanvasTexture {
            image_handle: handle.clone(),
            needs_full_upload: false, // Already uploaded initial data
        });

        info!(
            "Created texture for canvas plane {} ({}x{})",
            canvas_plane.plane_id, width, height
        );
    }
}

/// Process paint events and update the pipeline
fn process_paint_events(
    mut paint_events: MessageReader<PaintEvent>,
    mut painting_res: ResMut<PaintingResource>,
    canvas_query: Query<&CanvasPlane>,
    active_plane: Res<ActiveCanvasPlane>,
) {
    // Get the active plane info if available
    let active_plane_info = active_plane.entity.and_then(|e| {
        canvas_query.get(e).ok().map(|cp| (cp.plane_id, cp.width, cp.height))
    });

    for event in paint_events.read() {
        match event {
            PaintEvent::StrokeStart {
                plane_entity,
                world_pos: _,
                uv_pos,
                stroke_id,
                space_id,
            } => {
                // Get canvas plane dimensions
                if let Ok(canvas_plane) = canvas_query.get(*plane_entity) {
                    // Convert UV to pixel coordinates
                    let x = uv_pos.x * canvas_plane.width as f32;
                    let y = uv_pos.y * canvas_plane.height as f32;
                    let brush_color = painting_res.brush_color;
                    let plane_id = canvas_plane.plane_id;

                    info!(
                        "StrokeStart: plane={}, pixel=({:.1}, {:.1}), uv={:?}, color={:?}",
                        plane_id, x, y, uv_pos, brush_color
                    );

                    let pipeline = painting_res.get_or_create_pipeline(
                        plane_id,
                        canvas_plane.width,
                        canvas_plane.height,
                    );

                    pipeline.begin_stroke(*space_id, *stroke_id, 0);
                    pipeline.stroke_to(x, y, 1.0); // First point

                    info!(
                        "  After stroke_to: has_dirty_tiles={}",
                        pipeline.has_dirty_tiles()
                    );
                }
            }
            PaintEvent::StrokeMove {
                world_pos: _,
                uv_pos,
                pressure,
                speed: _,
            } => {
                // Use active plane for stroke continuation
                if let Some((plane_id, width, height)) = active_plane_info {
                    if let Some(pipeline) = painting_res.get_pipeline_mut(plane_id) {
                        let x = uv_pos.x * width as f32;
                        let y = uv_pos.y * height as f32;
                        debug!(
                            "StrokeMove: pixel=({:.1}, {:.1}), pressure={}",
                            x, y, pressure
                        );
                        pipeline.stroke_to(x, y, *pressure);
                    }
                }
            }
            PaintEvent::StrokeEnd => {
                if let Some((plane_id, _, _)) = active_plane_info {
                    if let Some(pipeline) = painting_res.get_pipeline_mut(plane_id) {
                        pipeline.end_stroke();
                    }
                }
            }
            PaintEvent::StrokeCancel => {
                if let Some((plane_id, _, _)) = active_plane_info {
                    if let Some(pipeline) = painting_res.get_pipeline_mut(plane_id) {
                        pipeline.cancel_stroke();
                    }
                }
            }
        }
    }
}

/// Extract dirty tiles from painting pipelines into the upload buffer
///
/// This system runs in the main world and prepares tile data for upload.
/// The actual GPU upload happens in the render world via `upload_dirty_tiles_to_gpu`.
fn extract_dirty_tiles(
    mut painting_res: ResMut<PaintingResource>,
    canvas_query: Query<(&CanvasPlane, &CanvasTexture)>,
    mut buffer: ResMut<DirtyTileUploadBuffer>,
) {
    buffer.canvases.clear();

    for (canvas_plane, canvas_texture) in canvas_query.iter() {
        let Some(pipeline) = painting_res.get_pipeline_mut(canvas_plane.plane_id) else {
            continue;
        };

        let dirty_tiles = pipeline.take_dirty_tiles();
        if dirty_tiles.is_empty() {
            continue;
        }

        info!(
            "extract_dirty_tiles: plane={}, {} tiles",
            canvas_plane.plane_id,
            dirty_tiles.len()
        );

        let mut canvas_buffer = CanvasDirtyTileBuffer {
            image_id: canvas_texture.image_handle.id(),
            tiles: Vec::with_capacity(dirty_tiles.len()),
        };

        for tile_coord in dirty_tiles {
            let (x, y, w, h) = pipeline.get_tile_bounds(tile_coord);
            let tile_data = pipeline.get_tile_data(tile_coord);
            let rgba8 = tile_data_to_rgba8(&tile_data);

            debug!(
                "  Tile at ({}, {}) {}x{}, {} bytes",
                x,
                y,
                w,
                h,
                rgba8.len()
            );

            canvas_buffer.tiles.push(DirtyTileUpload {
                offset: (x, y),
                size: (w, h),
                data: rgba8,
            });
        }

        buffer.canvases.push(canvas_buffer);
    }
}

/// Upload dirty tiles to GPU using wgpu::Queue::write_texture
///
/// This system runs in the render world and writes tile data directly to GPU textures.
/// This is more efficient than full image replacement as it only uploads changed regions.
///
/// Parameters are optional to handle cases where the render app was not available
/// during plugin initialization (e.g., in headless or test configurations).
fn upload_dirty_tiles_to_gpu(
    buffer: Option<Res<DirtyTileUploadBuffer>>,
    render_queue: Option<Res<RenderQueue>>,
    gpu_images: Option<Res<RenderAssets<GpuImage>>>,
) {
    let (Some(buffer), Some(render_queue), Some(gpu_images)) = (buffer, render_queue, gpu_images)
    else {
        return;
    };

    for canvas in &buffer.canvases {
        let Some(gpu_image) = gpu_images.get(canvas.image_id) else {
            // GpuImage not ready yet (first frame after creation)
            debug!("GpuImage not ready for {:?}", canvas.image_id);
            continue;
        };

        for tile in &canvas.tiles {
            render_queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &gpu_image.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: tile.offset.0,
                        y: tile.offset.1,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &tile.data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(tile.size.0 * 4), // RGBA8 = 4 bytes per pixel
                    rows_per_image: Some(tile.size.1),
                },
                wgpu::Extent3d {
                    width: tile.size.0,
                    height: tile.size.1,
                    depth_or_array_layers: 1,
                },
            );

            debug!(
                "  Uploaded tile at ({}, {}) {}x{}",
                tile.offset.0, tile.offset.1, tile.size.0, tile.size.1
            );
        }

        info!(
            "Uploaded {} tiles for image {:?}",
            canvas.tiles.len(),
            canvas.image_id
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_painting_resource_creation() {
        let mut res = PaintingResource::new();
        let pipeline = res.get_or_create_pipeline(0, 256, 256);
        assert_eq!(pipeline.width(), 256);
        assert_eq!(pipeline.height(), 256);
    }

    #[test]
    fn test_set_brush_color() {
        let mut res = PaintingResource::new();
        res.get_or_create_pipeline(0, 256, 256);
        res.set_brush_color([1.0, 0.0, 0.0, 1.0]);

        assert_eq!(res.brush_color, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(res.get_pipeline(0).unwrap().color(), [1.0, 0.0, 0.0, 1.0]);
    }
}
