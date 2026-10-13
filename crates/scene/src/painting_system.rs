//! Painting system for Bevy integration
//!
//! This module connects PaintEvent messages to the painting pipeline
//! and handles GPU texture upload for dirty tiles.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::{
    Render, RenderApp, RenderSystems,
    extract_resource::ExtractResource,
    render_asset::RenderAssets,
    render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
    renderer::RenderQueue,
    texture::GpuImage,
};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use painting::{BlendMode, BrushPreset, PaintingPipeline};
use pentimento_ipc::{BevyToUi, BlendMode as IpcBlendMode, LayerInfo};

use crate::canvas_plane::{ActiveCanvasPlane, CanvasPlane};
use crate::paint_mode::PaintEvent;

#[cfg(test)]
#[path = "paint_input_batch_tests.rs"]
mod paint_input_batch_tests;

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
    /// Current blend mode
    pub blend_mode: BlendMode,
}

impl Default for PaintingResource {
    fn default() -> Self {
        Self::new()
    }
}

impl PaintingResource {
    pub(crate) fn restore_document_layers(
        &mut self,
        id: u32,
        layers: painting::layer::LayerStackDocument,
    ) -> Result<(), String> {
        let width = layers.width;
        let height = layers.height;
        let restored = layers.restore()?;
        let mut pipeline = PaintingPipeline::new(width, height);
        pipeline.set_brush(self.brush_preset.clone());
        pipeline.set_color(self.brush_color);
        pipeline.set_blend_mode(self.blend_mode);
        pipeline.layers = restored;
        self.pipelines.insert(id, pipeline);
        Ok(())
    }
    /// Sample straight RGB synchronously at the ordered input press. This read
    /// never creates a pipeline, dirties tiles, logs a packet or changes history.
    pub fn sample_brush_color(
        &mut self,
        canvas: &CanvasPlane,
        uv: Vec2,
        source: pentimento_ipc::ColorSampleSource,
    ) -> bool {
        if self.has_active_stroke()
            || canvas.width == 0
            || canvas.height == 0
            || !uv.is_finite()
            || !(0.0..=1.0).contains(&uv.x)
            || !(0.0..=1.0).contains(&uv.y)
        {
            return false;
        }
        let x = ((uv.x * canvas.width as f32).floor() as u32).min(canvas.width - 1);
        let y = ((uv.y * canvas.height as f32).floor() as u32).min(canvas.height - 1);
        let rgb = self.get_pipeline(canvas.plane_id).and_then(|p| {
            p.layers.sample_color(
                x,
                y,
                source == pentimento_ipc::ColorSampleSource::VisibleLayers,
            )
        });
        if let Some(rgb) = rgb {
            self.set_brush_color([rgb[0], rgb[1], rgb[2], self.brush_color[3]]);
            true
        } else {
            false
        }
    }

    pub(crate) fn has_active_stroke(&self) -> bool {
        self.pipelines.values().any(PaintingPipeline::is_stroking)
    }
    /// Native mode changes may exempt only their verified active plane.
    pub(crate) fn has_unowned_active_stroke(&self, native_plane: Option<u32>) -> bool {
        self.pipelines
            .iter()
            .any(|(id, pipeline)| pipeline.is_stroking() && Some(*id) != native_plane)
    }
    /// Create a new painting resource
    pub fn new() -> Self {
        Self {
            pipelines: HashMap::new(),
            brush_color: [0.0, 0.0, 0.0, 1.0], // Default black
            brush_preset: BrushPreset::default(),
            blend_mode: BlendMode::Normal,
        }
    }

    /// Get or create a pipeline for a canvas plane
    pub fn get_or_create_pipeline(
        &mut self,
        plane_id: u32,
        width: u32,
        height: u32,
    ) -> &mut PaintingPipeline {
        let brush_color = self.brush_color;
        let brush_preset = self.brush_preset.clone();
        let blend_mode = self.blend_mode;
        self.pipelines.entry(plane_id).or_insert_with(|| {
            let mut pipeline = PaintingPipeline::new(width, height);
            pipeline.set_color(brush_color);
            pipeline.set_brush(brush_preset);
            pipeline.set_blend_mode(blend_mode);
            // Clear to transparent by default (glass effect - see scene behind)
            pipeline.clear([0.0, 0.0, 0.0, 0.0]);
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

    /// Set blend mode for all pipelines
    pub fn set_blend_mode(&mut self, mode: BlendMode) {
        self.blend_mode = mode;
        for pipeline in self.pipelines.values_mut() {
            pipeline.set_blend_mode(mode);
        }
    }

    /// Set blend mode from IPC type (converts to painting::BlendMode internally)
    pub fn set_blend_mode_ipc(&mut self, mode: IpcBlendMode) {
        let paint_mode = match mode {
            IpcBlendMode::Normal => BlendMode::Normal,
            IpcBlendMode::Erase => BlendMode::Erase,
        };
        self.set_blend_mode(paint_mode);
    }

    /// Undo the last stroke on a specific pipeline
    pub fn undo(&mut self, plane_id: u32) -> bool {
        if let Some(pipeline) = self.pipelines.get_mut(&plane_id) {
            pipeline.undo()
        } else {
            false
        }
    }

    /// Redo the last undone stroke on the requested canvas only.
    pub fn redo(&mut self, plane_id: u32) -> bool {
        self.pipelines
            .get_mut(&plane_id)
            .is_some_and(|pipeline| pipeline.redo())
    }

    /// Undo the last stroke on any pipeline that has undo available
    /// Returns true if an undo was performed
    pub fn undo_any(&mut self) -> bool {
        // Try to undo on each pipeline, starting with those that have undo available
        for pipeline in self.pipelines.values_mut() {
            if pipeline.can_undo() {
                return pipeline.undo();
            }
        }
        false
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
    pub data: Arc<[u8]>,
}

/// Per-canvas dirty tile buffer
#[derive(Clone)]
pub struct CanvasDirtyTileBuffer {
    /// Asset ID of the canvas texture (for GpuImage lookup)
    pub image_id: AssetId<Image>,
    /// Revision acknowledged only after all of this batch reaches the queue.
    revision: u64,
    /// Pending tile uploads
    pub tiles: Vec<DirtyTileUpload>,
}

/// Resource for buffering dirty tile uploads to be extracted to render world
#[derive(Resource, Default, Clone, ExtractResource)]
#[extract_app(bevy::render::RenderApp)]
pub struct DirtyTileUploadBuffer {
    /// Per-canvas upload buffers
    pub canvases: Vec<CanvasDirtyTileBuffer>,
    next_revision: u64,
    /// Extracted clones share only acknowledgements, never mutable pixel data.
    acknowledged: Arc<Mutex<HashMap<AssetId<Image>, u64>>>,
}

impl DirtyTileUploadBuffer {
    fn acknowledge(&self, image: AssetId<Image>, revision: u64) {
        let mut acknowledged = self
            .acknowledged
            .lock()
            .expect("canvas acknowledgement lock");
        acknowledged
            .entry(image)
            .and_modify(|old| *old = (*old).max(revision))
            .or_insert(revision);
    }

    fn settle(&mut self, live_images: &HashSet<AssetId<Image>>) {
        let acknowledged = std::mem::take(
            &mut *self
                .acknowledged
                .lock()
                .expect("canvas acknowledgement lock"),
        );
        self.canvases.retain(|canvas| {
            live_images.contains(&canvas.image_id)
                && acknowledged
                    .get(&canvas.image_id)
                    .is_none_or(|revision| *revision != canvas.revision)
        });
    }

    fn merge(&mut self, image_id: AssetId<Image>, tiles: Vec<DirtyTileUpload>) {
        if tiles.is_empty() {
            return;
        }
        self.next_revision = self
            .next_revision
            .checked_add(1)
            .expect("canvas upload revision exhausted");
        let revision = self.next_revision;
        if let Some(pending) = self
            .canvases
            .iter_mut()
            .find(|canvas| canvas.image_id == image_id)
        {
            pending.revision = revision;
            for tile in tiles {
                if let Some(old) = pending
                    .tiles
                    .iter_mut()
                    .find(|old| old.offset == tile.offset)
                {
                    *old = tile;
                } else {
                    pending.tiles.push(tile);
                }
            }
        } else {
            self.canvases.push(CanvasDirtyTileBuffer {
                image_id,
                revision,
                tiles,
            });
        }
    }
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
            .add_plugins(bevy::render::extract_resource::ExtractResourcePlugin::<
                DirtyTileUploadBuffer,
            >::default())
            .add_systems(
                Update,
                (
                    setup_canvas_textures,
                    process_paint_events.after(crate::paint_mode::handle_paint_input),
                    extract_dirty_tiles,
                )
                    .chain(),
            );

        // Render world system for GPU tile uploads
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            warn!("PaintingSystemPlugin: RenderApp not available, skipping render world setup");
            return;
        };

        render_app.add_systems(
            Render,
            upload_dirty_tiles_to_gpu.in_set(RenderSystems::Prepare),
        );
    }
}

/// Setup textures for newly created canvas planes
pub(crate) fn setup_canvas_textures(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    query: Query<(Entity, &CanvasPlane, &MeshMaterial3d<StandardMaterial>), Without<CanvasTexture>>,
    mut painting_res: ResMut<PaintingResource>,
    mut outbound: ResMut<crate::OutboundUiMessages>,
) {
    for (entity, canvas_plane, material_handle) in query.iter() {
        let width = canvas_plane.width;
        let height = canvas_plane.height;

        // Create the painting pipeline for this plane
        let pipeline = painting_res.get_or_create_pipeline(canvas_plane.plane_id, width, height);

        // Send initial layer state to UI
        let layers = pipeline
            .layers
            .layer_info()
            .into_iter()
            .map(|l| LayerInfo {
                id: l.id,
                name: l.name,
                visible: l.visible,
                opacity: l.opacity,
                is_active: l.is_active,
            })
            .collect();
        outbound.send(BevyToUi::LayerStateChanged { layers });

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
        if let Some(mut material) = materials.get_mut(&material_handle.0) {
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
pub(crate) fn process_paint_events(world: &mut World) {
    world
        .run_system_cached(apply_paint_events)
        .expect("Canvas owner resources available");
}

fn apply_paint_events(
    mut paint_events: MessageReader<PaintEvent>,
    mut painting_res: ResMut<PaintingResource>,
    canvas_query: Query<&CanvasPlane>,
    active_plane: Res<ActiveCanvasPlane>,
) {
    // Get the active plane info if available
    let active_plane_info = active_plane.entity.and_then(|e| {
        canvas_query
            .get(e)
            .ok()
            .map(|cp| (cp.plane_id, cp.width, cp.height))
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
                        info!(
                            "process_paint_events: StrokeMove pixel=({:.1}, {:.1}), pressure={}, is_stroking={}",
                            x,
                            y,
                            pressure,
                            pipeline.is_stroking()
                        );
                        pipeline.stroke_to(x, y, *pressure);
                    } else {
                        info!(
                            "process_paint_events: StrokeMove but no pipeline for plane_id={}",
                            plane_id
                        );
                    }
                } else {
                    info!(
                        "process_paint_events: StrokeMove but active_plane_info is None (entity={:?})",
                        active_plane.entity
                    );
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
pub(crate) fn extract_dirty_tiles(
    mut painting_res: ResMut<PaintingResource>,
    canvas_query: Query<(&CanvasPlane, &CanvasTexture)>,
    mut buffer: ResMut<DirtyTileUploadBuffer>,
    mut images: Option<ResMut<Assets<Image>>>,
) {
    let live_images = canvas_query
        .iter()
        .map(|(_, texture)| texture.image_handle.id())
        .collect();
    buffer.settle(&live_images);

    for (canvas_plane, canvas_texture) in canvas_query.iter() {
        let Some(pipeline) = painting_res.get_pipeline_mut(canvas_plane.plane_id) else {
            continue;
        };

        let dirty_tiles = pipeline.take_dirty_tiles();
        if dirty_tiles.is_empty() {
            continue;
        }

        let mut tiles = Vec::with_capacity(dirty_tiles.len());

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

            tiles.push(DirtyTileUpload {
                offset: (x, y),
                size: (w, h),
                data: rgba8.into(),
            });
        }

        buffer.merge(canvas_texture.image_handle.id(), tiles);
    }
    // Keep a CPU recovery copy without scheduling redundant whole-image
    // uploads. Retry pending patches too if an absent image is reinserted.
    if let Some(images) = images.as_mut() {
        for canvas in &buffer.canvases {
            if let Some(image) = images
                .bypass_change_detection()
                .get_mut_untracked(canvas.image_id)
            {
                for tile in &canvas.tiles {
                    patch_canvas_backing(image, tile);
                }
            }
        }
    }
}

fn patch_canvas_backing(image: &mut Image, tile: &DirtyTileUpload) {
    let size = image.texture_descriptor.size;
    if image.texture_descriptor.format != TextureFormat::Rgba8UnormSrgb
        || size.depth_or_array_layers != 1
        || tile
            .offset
            .0
            .checked_add(tile.size.0)
            .is_none_or(|end| end > size.width)
        || tile
            .offset
            .1
            .checked_add(tile.size.1)
            .is_none_or(|end| end > size.height)
        || tile.data.len() != tile.size.0 as usize * tile.size.1 as usize * 4
    {
        return;
    }
    let Some(bytes) = image.data.as_mut() else {
        return;
    };
    let width = size.width as usize;
    if bytes.len() != width * size.height as usize * 4 {
        return;
    }
    for row in 0..tile.size.1 as usize {
        let destination = ((tile.offset.1 as usize + row) * width + tile.offset.0 as usize) * 4;
        let source = row * tile.size.0 as usize * 4;
        let count = tile.size.0 as usize * 4;
        bytes[destination..destination + count].copy_from_slice(&tile.data[source..source + count]);
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
        buffer.acknowledge(canvas.image_id, canvas.revision);
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

#[cfg(test)]
mod upload_retry_tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    fn fixture() -> (World, Entity, Handle<Image>) {
        let mut world = World::new();
        world.init_resource::<PaintingResource>();
        world.init_resource::<DirtyTileUploadBuffer>();
        world.init_resource::<Assets<Image>>();
        let image = Image::new_fill(
            Extent3d {
                width: 256,
                height: 256,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[0, 0, 0, 0],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        let handle = world.resource_mut::<Assets<Image>>().add(image);
        world
            .resource_mut::<PaintingResource>()
            .get_or_create_pipeline(7, 256, 256);
        let entity = world
            .spawn((
                CanvasPlane::new(7, 256, 256, 1., 1.),
                CanvasTexture {
                    image_handle: handle.clone(),
                    needs_full_upload: true,
                },
            ))
            .id();
        (world, entity, handle)
    }

    #[test]
    fn unavailable_render_retries_same_bytes_then_acknowledged_idle_is_empty() {
        let (mut world, _, handle) = fixture();
        world.run_system_once(extract_dirty_tiles).unwrap();
        let extracted = world.resource::<DirtyTileUploadBuffer>().clone();
        assert_eq!(extracted.canvases[0].tiles.len(), 4);
        for _ in 0..3 {
            // The production uploader returns without an acknowledgement when
            // queue/GpuImage resources are absent, exactly as on startup.
            world.run_system_once(upload_dirty_tiles_to_gpu).unwrap();
            world.run_system_once(extract_dirty_tiles).unwrap();
            let pending = &world.resource::<DirtyTileUploadBuffer>().canvases[0];
            assert_eq!(pending.revision, extracted.canvases[0].revision);
            assert!(Arc::ptr_eq(
                &pending.tiles[0].data,
                &extracted.canvases[0].tiles[0].data
            ));
        }
        extracted.acknowledge(handle.id(), extracted.canvases[0].revision);
        world.run_system_once(extract_dirty_tiles).unwrap();
        assert!(
            world
                .resource::<DirtyTileUploadBuffer>()
                .canvases
                .is_empty()
        );
        world.run_system_once(extract_dirty_tiles).unwrap();
        assert!(
            world
                .resource::<DirtyTileUploadBuffer>()
                .canvases
                .is_empty()
        );
    }

    #[test]
    fn pending_pixels_patch_reinserted_cpu_image_without_new_stroke() {
        let (mut world, _, handle) = fixture();
        let saved = world
            .resource_mut::<Assets<Image>>()
            .remove(handle.id())
            .unwrap();
        world
            .resource_mut::<PaintingResource>()
            .get_pipeline_mut(7)
            .unwrap()
            .clear([0.2, 0.4, 0.1, 1.]);
        world.run_system_once(extract_dirty_tiles).unwrap();
        let pending = world.resource::<DirtyTileUploadBuffer>().clone();
        world
            .resource_mut::<Assets<Image>>()
            .insert(handle.id(), saved)
            .unwrap();
        world.run_system_once(extract_dirty_tiles).unwrap();
        assert_eq!(
            world.resource::<DirtyTileUploadBuffer>().canvases[0].revision,
            pending.canvases[0].revision
        );
        let bytes = world
            .resource::<Assets<Image>>()
            .get(&handle)
            .unwrap()
            .data
            .as_ref()
            .unwrap();
        assert!(bytes.chunks_exact(4).all(|p| p
            == [
                linear_to_srgb_u8(0.2),
                linear_to_srgb_u8(0.4),
                linear_to_srgb_u8(0.1),
                255
            ]));
    }

    #[test]
    fn cancel_and_undo_supersede_pending_pixels_without_stale_ack_loss() {
        let (mut world, entity, handle) = fixture();
        world.run_system_once(extract_dirty_tiles).unwrap();
        let initial = world.resource::<DirtyTileUploadBuffer>().clone();
        initial.acknowledge(handle.id(), initial.canvases[0].revision);
        world.run_system_once(extract_dirty_tiles).unwrap();
        let baseline = world
            .resource::<Assets<Image>>()
            .get(&handle)
            .unwrap()
            .data
            .clone();
        for cancel in [true, false] {
            let mut painting = world.resource_mut::<PaintingResource>();
            let pipeline = painting.get_pipeline_mut(7).unwrap();
            pipeline.set_color([0.6, 0.2, 0.1, 1.]);
            pipeline.begin_stroke(7, if cancel { 1 } else { 2 }, 0);
            pipeline.stroke_to(64., 64., 1.);
            world.run_system_once(extract_dirty_tiles).unwrap();
            let drawn = world.resource::<DirtyTileUploadBuffer>().clone();
            assert!(!drawn.canvases[0].tiles.is_empty());
            assert_ne!(
                world.resource::<Assets<Image>>().get(&handle).unwrap().data,
                baseline
            );
            let mut painting = world.resource_mut::<PaintingResource>();
            let pipeline = painting.get_pipeline_mut(7).unwrap();
            if cancel {
                pipeline.cancel_stroke();
            } else {
                pipeline.end_stroke();
                pipeline.undo();
            }
            world.run_system_once(extract_dirty_tiles).unwrap();
            let restored = world.resource::<DirtyTileUploadBuffer>().clone();
            assert!(restored.canvases[0].revision > drawn.canvases[0].revision);
            assert_eq!(
                world.resource::<Assets<Image>>().get(&handle).unwrap().data,
                baseline
            );
            drawn.acknowledge(handle.id(), drawn.canvases[0].revision);
            world.run_system_once(extract_dirty_tiles).unwrap();
            assert_eq!(
                world.resource::<DirtyTileUploadBuffer>().canvases[0].revision,
                restored.canvases[0].revision
            );
            restored.acknowledge(handle.id(), restored.canvases[0].revision);
            world.run_system_once(extract_dirty_tiles).unwrap();
            assert!(
                world
                    .resource::<DirtyTileUploadBuffer>()
                    .canvases
                    .is_empty()
            );
        }
        world
            .resource_mut::<PaintingResource>()
            .get_pipeline_mut(7)
            .unwrap()
            .clear([0.2, 0.1, 0.3, 1.]);
        world.run_system_once(extract_dirty_tiles).unwrap();
        world.despawn(entity);
        world.run_system_once(extract_dirty_tiles).unwrap();
        assert!(
            world
                .resource::<DirtyTileUploadBuffer>()
                .canvases
                .is_empty(),
            "deleted canvas drops pending bytes"
        );
    }
}
