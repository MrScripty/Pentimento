//! Painting system for Bevy integration
//!
//! This module connects PaintEvent messages to the painting pipeline
//! and handles GPU texture upload for dirty tiles.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use std::collections::HashMap;

use painting::{BrushPreset, PaintingPipeline};

use crate::canvas_plane::{ActiveCanvasPlane, CanvasPlane};
use crate::paint_mode::PaintEvent;

/// Resource holding painting pipelines for each canvas plane
///
/// Each canvas plane gets its own painting pipeline, indexed by plane_id.
#[derive(Resource, Default)]
pub struct PaintingResource {
    /// Mapping from plane_id to pipeline
    pipelines: HashMap<u32, PaintingPipeline>,
    /// Current brush color
    pub brush_color: [f32; 4],
    /// Current brush preset
    pub brush_preset: BrushPreset,
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

/// Plugin for the painting system
pub struct PaintingSystemPlugin;

impl Plugin for PaintingSystemPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PaintingResource>()
            .add_systems(
                Update,
                (
                    setup_canvas_textures,
                    process_paint_events,
                    upload_dirty_tiles,
                )
                    .chain(),
            );
    }
}

/// Setup textures for newly created canvas planes
fn setup_canvas_textures(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    query: Query<(Entity, &CanvasPlane), Without<CanvasTexture>>,
    mut painting_res: ResMut<PaintingResource>,
) {
    for (entity, canvas_plane) in query.iter() {
        let width = canvas_plane.width;
        let height = canvas_plane.height;

        // Create the painting pipeline for this plane
        let pipeline = painting_res.get_or_create_pipeline(canvas_plane.plane_id, width, height);

        // Get the surface data to initialize the image
        let surface_bytes = pipeline.surface_as_bytes();

        // Create Bevy Image with Rgba32Float format
        // This matches our [f32; 4] pixel format directly
        let mut image = Image::new_fill(
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            // Initialize with white (matches pipeline.clear([1.0, 1.0, 1.0, 1.0]))
            bytemuck::bytes_of(&[1.0f32, 1.0f32, 1.0f32, 1.0f32]),
            TextureFormat::Rgba32Float,
            RenderAssetUsages::all(),
        );

        // Set texture usages for painting
        image.texture_descriptor.usage =
            TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST;

        // Set the image data to the full surface data
        // In Bevy 0.18, image.data is Option<Vec<u8>>
        image.data = Some(surface_bytes.to_vec());

        let handle = images.add(image);

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
                    let pipeline = painting_res.get_or_create_pipeline(
                        canvas_plane.plane_id,
                        canvas_plane.width,
                        canvas_plane.height,
                    );

                    // Convert UV to pixel coordinates
                    let x = uv_pos.x * canvas_plane.width as f32;
                    let y = uv_pos.y * canvas_plane.height as f32;

                    pipeline.begin_stroke(*space_id, *stroke_id, 0);
                    pipeline.stroke_to(x, y, 1.0); // First point
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

/// Upload dirty tiles to GPU
fn upload_dirty_tiles(
    mut painting_res: ResMut<PaintingResource>,
    mut images: ResMut<Assets<Image>>,
    canvas_query: Query<(&CanvasPlane, &CanvasTexture)>,
) {
    for (canvas_plane, canvas_texture) in canvas_query.iter() {
        let Some(pipeline) = painting_res.get_pipeline_mut(canvas_plane.plane_id) else {
            continue;
        };

        // Check if we need full upload
        if canvas_texture.needs_full_upload {
            if let Some(image) = images.get_mut(&canvas_texture.image_handle) {
                let surface_bytes = pipeline.surface_as_bytes();
                image.data = Some(surface_bytes.to_vec());
            }
            continue;
        }

        // Get dirty tiles
        let dirty_tiles = pipeline.take_dirty_tiles();
        if dirty_tiles.is_empty() {
            continue;
        }

        // Update each dirty tile in the image
        if let Some(image) = images.get_mut(&canvas_texture.image_handle) {
            let Some(ref mut image_data) = image.data else {
                // Initialize image data if it's None
                let surface_bytes = pipeline.surface_as_bytes();
                image.data = Some(surface_bytes.to_vec());
                continue;
            };

            let surface_width = canvas_plane.width;
            let bytes_per_pixel = 16; // 4 f32s * 4 bytes

            for tile_coord in dirty_tiles {
                let tile_data = pipeline.get_tile_data(tile_coord);
                let (tile_x, tile_y, tile_w, tile_h) = pipeline.get_tile_bounds(tile_coord);

                // Copy tile data to image
                // Image data is stored row by row
                for local_y in 0..tile_h {
                    let global_y = tile_y + local_y;
                    let src_start = (local_y * tile_w) as usize;
                    let src_end = src_start + tile_w as usize;

                    if src_end <= tile_data.len() {
                        let src_slice = &tile_data[src_start..src_end];
                        let src_bytes: &[u8] = bytemuck::cast_slice(src_slice);

                        let dst_start =
                            ((global_y * surface_width + tile_x) as usize) * bytes_per_pixel;
                        let dst_end = dst_start + (tile_w as usize * bytes_per_pixel);

                        if dst_end <= image_data.len() {
                            image_data[dst_start..dst_end].copy_from_slice(src_bytes);
                        }
                    }
                }
            }
        }
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
