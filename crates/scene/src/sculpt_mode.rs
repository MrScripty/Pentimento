//! Sculpt mode for 3D mesh sculpting with dynamic tessellation
//!
//! Provides sculpting functionality:
//! - Ctrl+Tab to enter/exit sculpt mode (requires mesh selected)
//! - Brush-based deformation (Push, Pull, Smooth, etc.)
//! - Screen-space adaptive tessellation
//! - Mesh chunking for optimized GPU updates

use bevy::ecs::message::Message;
use bevy::input::mouse::MouseButton;
use bevy::math::{Affine3A, Isometry3d};
use bevy::mesh::{Indices, VertexAttributeValues};
use bevy::prelude::*;
use bevy::window::{CursorMoved, PrimaryWindow};
use painting::half_edge::HalfEdgeMesh;
use pentimento_ipc::{
    BevyToUi, EditMode, SculptBrushSettings, SculptCommand, SculptFalloff, SculptTool,
};
use sculpting::{
    BrushInput, BrushPreset, ChunkConfig, ChunkedMesh, DeformationType, FalloffCurve,
    PipelineConfig, ScreenSpaceConfig, SculptingPipeline, TessellationConfig, TessellationMode,
    partition_mesh,
};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::OutboundUiMessages;
use crate::camera::MainCamera;
use crate::edit_mode::EditModeState;
use crate::frontend_input::FrontendInputBlockState;
use crate::paint_mode::StrokeIdGenerator;
use crate::pixel_coverage::{PixelCoverageState, estimate_pixel_coverage_cpu};
use crate::render_camera::{ActiveRenderCamera, RenderCamera};
#[cfg(feature = "selection")]
use crate::selection::Selected;

/// Mode for interactive brush adjustment (Blender-style F key)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BrushAdjustMode {
    /// Not adjusting
    #[default]
    None,
    /// Adjusting brush radius (F key)
    Radius,
    /// Adjusting brush strength (Shift+F key)
    Strength,
}

/// Resource tracking sculpt mode state
#[derive(Resource)]
pub struct SculptState {
    /// Whether sculpt mode is currently active
    pub active: bool,
    /// Entity currently being sculpted
    pub target_entity: Option<Entity>,
    /// Current deformation type
    pub deformation_type: DeformationType,
    /// Brush radius in mesh-local units
    pub brush_radius: f32,
    /// Brush strength (0.0 - 1.0)
    pub brush_strength: f32,
    /// Brush hardness (0.0 - 1.0). Defines the inner zone of full strength.
    pub brush_hardness: f32,
    /// Falloff curve type for the brush
    pub brush_falloff: FalloffCurve,
    /// Tessellation configuration
    pub tessellation_config: TessellationConfig,
    /// Chunk sizing configuration
    pub chunk_config: ChunkConfig,
    /// Current stroke ID (if stroke in progress)
    pub current_stroke_id: Option<u64>,
    /// Last world position for stroke direction calculation
    pub last_world_pos: Option<Vec3>,
    /// Last frame time for timing
    pub last_time: f64,
    /// Current brush adjustment mode (F key interaction)
    pub adjust_mode: BrushAdjustMode,
    /// Starting cursor position when adjustment began
    pub adjust_start_cursor: Option<Vec2>,
    /// Starting value when adjustment began
    pub adjust_start_value: f32,
    /// A brush-adjustment confirmation owns its whole left-button press.
    pub suppress_left_until_release: bool,
}

impl Default for SculptState {
    fn default() -> Self {
        Self {
            active: false,
            target_entity: None,
            deformation_type: DeformationType::Push,
            brush_radius: 0.5,
            brush_strength: 1.0,
            brush_hardness: 0.5,
            brush_falloff: FalloffCurve::Smooth,
            tessellation_config: TessellationConfig::default(),
            chunk_config: ChunkConfig::default(),
            current_stroke_id: None,
            last_world_pos: None,
            last_time: 0.0,
            adjust_mode: BrushAdjustMode::None,
            adjust_start_cursor: None,
            adjust_start_value: 0.0,
            suppress_left_until_release: false,
        }
    }
}

/// Resource holding the active sculpting data
#[derive(Resource, Default)]
pub struct SculptingData {
    /// The chunked mesh being sculpted
    pub chunked_mesh: Option<ChunkedMesh>,
    /// The sculpting pipeline
    pub pipeline: Option<SculptingPipeline>,
    /// Chunk entities spawned for visualization
    pub chunk_entities: Vec<Entity>,
    /// Original mesh handle for restoration
    pub original_mesh_handle: Option<Handle<Mesh>>,
    /// Mesh ID for stroke tracking
    pub mesh_id: u32,
    /// Inverse transform for world-to-local conversion
    pub inverse_transform: Option<Affine3A>,
    /// Transform for local-to-world conversion (for normals)
    pub transform_rotation: Option<Quat>,
    /// Model matrix (local-to-world) for screen-space tessellation.
    /// Vertex positions in HalfEdgeMesh are in local space; this matrix
    /// is needed to correctly compute screen-space edge lengths.
    pub model_matrix: Option<Mat4>,
    /// Original topological vertex → every emitted render vertex (UV corners).
    /// Used for position-only GPU updates without splitting sculpt seams.
    pub cached_vertex_mapping:
        Option<std::collections::HashMap<painting::half_edge::VertexId, Vec<usize>>>,
}

/// Message for sculpt mode events
#[derive(Message, Debug, Clone)]
pub enum SculptEvent {
    /// Enter sculpt mode for the specified entity
    Enter { entity: Entity },
    /// Exit sculpt mode
    Exit,
    /// Set the deformation type
    SetDeformationType(DeformationType),
    /// Set brush radius
    SetBrushRadius(f32),
    /// Set brush strength
    SetBrushStrength(f32),
    /// Start a sculpt stroke
    StrokeStart {
        /// World-space position where stroke started
        world_pos: Vec3,
        /// Surface normal at hit point
        normal: Vec3,
        /// Unique stroke ID
        stroke_id: u64,
    },
    /// Continue a sculpt stroke
    StrokeMove {
        /// World-space position
        world_pos: Vec3,
        /// Surface normal at hit point
        normal: Vec3,
        /// Pressure value (0.0-1.0)
        pressure: f32,
    },
    /// End a sculpt stroke
    StrokeEnd,
    /// Cancel a sculpt stroke
    StrokeCancel,
}

/// Plugin for sculpt mode functionality
pub struct SculptModePlugin;

impl Plugin for SculptModePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SculptState>()
            .init_resource::<SculptingData>()
            .add_message::<SculptEvent>()
            .add_systems(
                Update,
                (
                    update_sculpt_screen_config,
                    update_sculpt_budget,
                    handle_sculpt_mode_hotkey,
                    handle_brush_adjustment,
                    handle_sculpt_input,
                    handle_sculpt_events,
                    sync_sculpt_chunks_to_gpu,
                    render_sculpt_brush_gizmo,
                )
                    .chain(),
            );
    }
}

/// Update the pipeline's screen-space configuration from the camera.
///
/// This is essential for tessellation to work correctly - without valid
/// camera data, edge length calculations will be wrong.
fn update_sculpt_screen_config(
    sculpt_state: Res<SculptState>,
    mut sculpting_data: ResMut<SculptingData>,
    camera_query: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    if !sculpt_state.active {
        return;
    }

    let Ok((camera, camera_transform)) = camera_query.single() else {
        warn!("update_sculpt_screen_config: no camera found");
        return;
    };

    let Ok(window) = windows.single() else {
        warn!("update_sculpt_screen_config: no window found");
        return;
    };

    // Extract model matrix before mutable borrow of pipeline
    let model_matrix = sculpting_data.model_matrix.unwrap_or(Mat4::IDENTITY);

    if let Some(pipeline) = &mut sculpting_data.pipeline {
        // Build view-projection matrix
        let view_matrix = camera_transform.affine().inverse();
        let projection = camera.clip_from_view();
        let view_projection = projection * view_matrix;

        // Log the first update to verify values are reasonable
        static LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if !LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            info!(
                "update_sculpt_screen_config: viewport={}x{}, camera_pos={:?}",
                window.width(),
                window.height(),
                camera_transform.translation()
            );
        }

        // Use the model matrix (local-to-world) so tessellation correctly
        // evaluates screen-space edge lengths from local-space vertex positions.
        let screen_config = ScreenSpaceConfig::with_model_matrix(
            view_projection,
            model_matrix,
            window.width(),
            window.height(),
        );

        pipeline.update_screen_config(screen_config);
    }
}

/// Update the pipeline's vertex budget from the render camera's pixel coverage.
///
/// When in `BudgetCurvature` tessellation mode, this system:
/// 1. Reads the render camera's transform and projection
/// 2. Projects all mesh triangles through the render camera
/// 3. Counts pixel coverage (with backface culling + frustum clipping)
/// 4. Updates the pipeline's vertex budget
fn update_sculpt_budget(
    sculpt_state: Res<SculptState>,
    mut sculpting_data: ResMut<SculptingData>,
    mut coverage_state: ResMut<PixelCoverageState>,
    render_camera_query: Query<(&RenderCamera, &GlobalTransform)>,
    active_render_camera: Res<ActiveRenderCamera>,
) {
    if !sculpt_state.active {
        return;
    }

    // Only run for BudgetCurvature mode
    if sculpt_state.tessellation_config.mode != TessellationMode::BudgetCurvature {
        return;
    }

    // Need an active render camera
    let Some(render_cam_entity) = active_render_camera.entity else {
        return;
    };

    let Ok((render_camera, render_cam_transform)) = render_camera_query.get(render_cam_entity)
    else {
        return;
    };

    let model_matrix = sculpting_data.model_matrix.unwrap_or(Mat4::IDENTITY);
    let view_projection = render_camera.view_projection(render_cam_transform);

    // Compute pixel coverage across all chunks
    let mut total_coverage: u32 = 0;

    if let Some(chunked_mesh) = &sculpting_data.chunked_mesh {
        for chunk in chunked_mesh.chunks.values() {
            // Extract positions and build index list from chunk mesh
            let positions: Vec<Vec3> = chunk.mesh.vertices().iter().map(|v| v.position).collect();
            let mut indices: Vec<u32> = Vec::new();
            for face in chunk.mesh.faces() {
                let verts = chunk.mesh.get_face_vertices(face.id);
                if verts.len() >= 3 {
                    for i in 1..(verts.len() - 1) {
                        indices.push(verts[0].0);
                        indices.push(verts[i].0);
                        indices.push(verts[i + 1].0);
                    }
                }
            }

            total_coverage += estimate_pixel_coverage_cpu(
                &positions,
                &indices,
                &model_matrix,
                &view_projection,
                render_camera.resolution,
            );
        }

        // Clamp to max possible pixels
        let max_pixels = render_camera.total_pixels();
        total_coverage = total_coverage.min(max_pixels);
    }

    // Update coverage state resource
    coverage_state.pixel_count = total_coverage;
    coverage_state.max_vertices = render_camera.max_vertices_for_mesh(total_coverage as f32);
    coverage_state.stale = false;

    // Update pipeline budget
    if let Some(pipeline) = &mut sculpting_data.pipeline {
        pipeline.update_budget_from_coverage(total_coverage);
    }
}

/// Handle Ctrl+Tab to toggle sculpt mode
///
/// Ctrl+Tab enters sculpt mode when a mesh is selected.
/// If already in sculpt mode, Ctrl+Tab exits.
#[cfg(feature = "selection")]
fn handle_sculpt_mode_hotkey(
    key_input: Res<ButtonInput<KeyCode>>,
    edit_mode: Res<EditModeState>,
    selected_meshes: Query<Entity, (With<Selected>, With<Mesh3d>)>,
    input_blocks: Res<FrontendInputBlockState>,
    mut events: MessageWriter<SculptEvent>,
) {
    if input_blocks.blocks_keyboard() {
        return;
    }

    // Check for Ctrl modifier
    let ctrl = key_input.pressed(KeyCode::ControlLeft) || key_input.pressed(KeyCode::ControlRight);
    let tab = key_input.just_pressed(KeyCode::Tab);

    if !ctrl || !tab {
        return;
    }

    // If already in sculpt mode, exit
    if edit_mode.mode == EditMode::Sculpt {
        events.write(SculptEvent::Exit);
        return;
    }

    // If we have a mesh selected, enter sculpt mode
    if let Ok(entity) = selected_meshes.single() {
        events.write(SculptEvent::Enter { entity });
    }
}

/// Stub for non-selection builds
#[cfg(not(feature = "selection"))]
fn handle_sculpt_mode_hotkey() {}

/// Handle Blender-style brush adjustment (F for radius, Shift+F for strength)
///
/// - Press F: Begin radius adjustment, drag mouse horizontally to resize
/// - Press Shift+F: Begin strength adjustment, drag mouse horizontally to change
/// - Click or press Enter: Confirm adjustment
/// - Press Escape: Cancel adjustment and restore original value
fn handle_brush_adjustment(
    key_input: Res<ButtonInput<KeyCode>>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut sculpt_state: ResMut<SculptState>,
    mut sculpting_data: ResMut<SculptingData>,
    input_blocks: Res<FrontendInputBlockState>,
) {
    if input_blocks.blocks_keyboard() || input_blocks.blocks_pointer() {
        return;
    }

    // Only active in sculpt mode
    if !sculpt_state.active {
        return;
    }

    let Ok(window) = windows.single() else {
        return;
    };

    let cursor_pos = window.cursor_position();
    let shift = key_input.pressed(KeyCode::ShiftLeft) || key_input.pressed(KeyCode::ShiftRight);

    // Check for starting adjustment mode
    if key_input.just_pressed(KeyCode::KeyF) && sculpt_state.adjust_mode == BrushAdjustMode::None {
        if shift {
            // Shift+F: Strength adjustment
            sculpt_state.adjust_mode = BrushAdjustMode::Strength;
            sculpt_state.adjust_start_cursor = cursor_pos;
            sculpt_state.adjust_start_value = sculpt_state.brush_strength;
            info!(
                "Brush strength adjustment: drag horizontally (current: {:.2})",
                sculpt_state.brush_strength
            );
        } else {
            // F: Radius adjustment
            sculpt_state.adjust_mode = BrushAdjustMode::Radius;
            sculpt_state.adjust_start_cursor = cursor_pos;
            sculpt_state.adjust_start_value = sculpt_state.brush_radius;
            info!(
                "Brush radius adjustment: drag horizontally (current: {:.2})",
                sculpt_state.brush_radius
            );
        }
        return;
    }

    // Handle active adjustment
    if sculpt_state.adjust_mode != BrushAdjustMode::None {
        // Cancel with Escape
        if key_input.just_pressed(KeyCode::Escape) {
            match sculpt_state.adjust_mode {
                BrushAdjustMode::Radius => {
                    sculpt_state.brush_radius = sculpt_state.adjust_start_value;
                    // Update pipeline
                    if let Some(pipeline) = &mut sculpting_data.pipeline {
                        let mut preset = pipeline.brush_preset().clone();
                        preset.radius = sculpt_state.brush_radius;
                        pipeline.set_brush_preset(preset);
                    }
                    info!(
                        "Radius adjustment cancelled, restored to {:.2}",
                        sculpt_state.brush_radius
                    );
                }
                BrushAdjustMode::Strength => {
                    sculpt_state.brush_strength = sculpt_state.adjust_start_value;
                    // Update pipeline
                    if let Some(pipeline) = &mut sculpting_data.pipeline {
                        let mut preset = pipeline.brush_preset().clone();
                        preset.strength = sculpt_state.brush_strength;
                        pipeline.set_brush_preset(preset);
                    }
                    info!(
                        "Strength adjustment cancelled, restored to {:.2}",
                        sculpt_state.brush_strength
                    );
                }
                BrushAdjustMode::None => {}
            }
            sculpt_state.adjust_mode = BrushAdjustMode::None;
            sculpt_state.adjust_start_cursor = None;
            return;
        }

        // Confirm with Enter or Left Click
        if key_input.just_pressed(KeyCode::Enter) || mouse_button.just_pressed(MouseButton::Left) {
            // The next chained system sees this same ButtonInput. Keep the
            // confirmation press consumed until release, even after this mode ends.
            sculpt_state.suppress_left_until_release = mouse_button.just_pressed(MouseButton::Left);
            match sculpt_state.adjust_mode {
                BrushAdjustMode::Radius => {
                    info!("Brush radius set to {:.2}", sculpt_state.brush_radius);
                }
                BrushAdjustMode::Strength => {
                    info!("Brush strength set to {:.2}", sculpt_state.brush_strength);
                }
                BrushAdjustMode::None => {}
            }
            sculpt_state.adjust_mode = BrushAdjustMode::None;
            sculpt_state.adjust_start_cursor = None;
            return;
        }

        // Update value based on mouse movement
        if let (Some(start_pos), Some(current_pos)) = (sculpt_state.adjust_start_cursor, cursor_pos)
        {
            let delta_x = current_pos.x - start_pos.x;
            // Scale: 200 pixels = double/halve for radius, 200 pixels = ±0.5 for strength
            let sensitivity = 200.0;

            match sculpt_state.adjust_mode {
                BrushAdjustMode::Radius => {
                    // Exponential scaling for radius (more intuitive)
                    let factor = (delta_x / sensitivity).exp2();
                    let new_radius = (sculpt_state.adjust_start_value * factor)
                        .max(0.01)
                        .min(10.0);
                    sculpt_state.brush_radius = new_radius;

                    // Update pipeline
                    if let Some(pipeline) = &mut sculpting_data.pipeline {
                        let mut preset = pipeline.brush_preset().clone();
                        preset.radius = new_radius;
                        pipeline.set_brush_preset(preset);
                    }
                }
                BrushAdjustMode::Strength => {
                    // Linear scaling for strength
                    let delta = delta_x / sensitivity;
                    let new_strength = (sculpt_state.adjust_start_value + delta).clamp(0.0, 1.0);
                    sculpt_state.brush_strength = new_strength;

                    // Update pipeline
                    if let Some(pipeline) = &mut sculpting_data.pipeline {
                        let mut preset = pipeline.brush_preset().clone();
                        preset.strength = new_strength;
                        pipeline.set_brush_preset(preset);
                    }
                }
                BrushAdjustMode::None => {}
            }
        }
    }
}

/// Handle mouse input for sculpting
fn handle_sculpt_input(
    mouse_button: Res<ButtonInput<MouseButton>>,
    windows: Query<(Entity, &Window), With<PrimaryWindow>>,
    mut cursor_events: MessageReader<CursorMoved>,
    camera_query: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    mesh_query: Query<(&Mesh3d, &GlobalTransform)>,
    meshes: Res<Assets<Mesh>>,
    mut sculpt_state: ResMut<SculptState>,
    mut stroke_id_gen: ResMut<StrokeIdGenerator>,
    mut sculpt_events: MessageWriter<SculptEvent>,
    time: Res<Time>,
    input_blocks: Res<FrontendInputBlockState>,
) {
    if sculpt_state.suppress_left_until_release {
        if sculpt_state.current_stroke_id.is_some() {
            sculpt_events.write(SculptEvent::StrokeEnd);
        }
        if !mouse_button.pressed(MouseButton::Left) {
            sculpt_state.suppress_left_until_release = false;
        }
        cursor_events.clear();
        return;
    }
    if input_blocks.blocks_pointer() {
        if sculpt_state.current_stroke_id.is_some() {
            sculpt_events.write(SculptEvent::StrokeEnd);
        }
        cursor_events.clear();
        return;
    }

    // Only process if sculpt mode is active
    if !sculpt_state.active {
        return;
    }

    // Don't process sculpting while adjusting brush parameters
    if sculpt_state.adjust_mode != BrushAdjustMode::None {
        // Consume cursor events to prevent them from accumulating
        cursor_events.read().for_each(drop);
        return;
    }

    let Some(target_entity) = sculpt_state.target_entity else {
        return;
    };

    // Get camera for ray casting
    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };

    // Get window for cursor position
    let Ok((window_entity, window)) = windows.single() else {
        return;
    };

    // Get target mesh
    let Ok((mesh_handle, mesh_transform)) = mesh_query.get(target_entity) else {
        return;
    };

    let Some(mesh) = meshes.get(&mesh_handle.0) else {
        return;
    };

    // Collect cursor positions from this frame
    let cursor_positions: Vec<Vec2> = cursor_events
        .read()
        .filter(|e| e.window == window_entity)
        .map(|e| e.position)
        .collect();

    let _current_time = time.elapsed_secs_f64();

    // Handle stroke start
    if mouse_button.just_pressed(MouseButton::Left) {
        let cursor_pos = cursor_positions
            .last()
            .copied()
            .or_else(|| window.cursor_position());
        if let Some(cursor_pos) = cursor_pos {
            if let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_pos) {
                if let Some((world_pos, normal)) =
                    ray_mesh_intersection_simple(&ray, mesh, mesh_transform)
                {
                    let stroke_id = stroke_id_gen.next();
                    sculpt_events.write(SculptEvent::StrokeStart {
                        world_pos,
                        normal,
                        stroke_id,
                    });
                }
            }
        }
    } else if mouse_button.pressed(MouseButton::Left) && sculpt_state.current_stroke_id.is_some() {
        // Continue stroke
        let positions_to_process: Vec<Vec2> = if !cursor_positions.is_empty() {
            cursor_positions
        } else if let Some(pos) = window.cursor_position() {
            vec![pos]
        } else {
            vec![]
        };

        for cursor_pos in positions_to_process {
            if let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_pos) {
                if let Some((world_pos, normal)) =
                    ray_mesh_intersection_simple(&ray, mesh, mesh_transform)
                {
                    sculpt_events.write(SculptEvent::StrokeMove {
                        world_pos,
                        normal,
                        pressure: 1.0, // No pressure sensitivity for mouse
                    });
                }
            }
        }
    } else if mouse_button.just_released(MouseButton::Left) {
        if sculpt_state.current_stroke_id.is_some() {
            sculpt_events.write(SculptEvent::StrokeEnd);
        }
    }
}

/// Simple ray-mesh intersection returning world position and normal
fn ray_mesh_intersection_simple(
    ray: &Ray3d,
    mesh: &Mesh,
    transform: &GlobalTransform,
) -> Option<(Vec3, Vec3)> {
    // Get vertex positions
    let positions = match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
        Some(VertexAttributeValues::Float32x3(v)) => v,
        _ => return None,
    };

    // Get indices
    let indices = match mesh.indices() {
        Some(Indices::U32(i)) => i.iter().map(|&x| x as usize).collect::<Vec<_>>(),
        Some(Indices::U16(i)) => i.iter().map(|&x| x as usize).collect::<Vec<_>>(),
        None => return None,
    };

    // Get optional normals
    let normals = match mesh.attribute(Mesh::ATTRIBUTE_NORMAL) {
        Some(VertexAttributeValues::Float32x3(v)) => Some(v),
        _ => None,
    };

    // Transform ray to local space
    let inv_transform = transform.affine().inverse();
    let local_ray_origin = inv_transform.transform_point3(ray.origin);
    let local_ray_dir = inv_transform.transform_vector3(*ray.direction).normalize();

    let mut closest_hit: Option<(f32, Vec3, Vec3)> = None; // (t, local_pos, barycentric)

    // Iterate through triangles
    for triangle in indices.chunks(3) {
        if triangle.len() != 3 {
            continue;
        }

        let i0 = triangle[0];
        let i1 = triangle[1];
        let i2 = triangle[2];

        let v0 = Vec3::from(positions[i0]);
        let v1 = Vec3::from(positions[i1]);
        let v2 = Vec3::from(positions[i2]);

        // Möller–Trumbore intersection
        if let Some((t, u, v)) =
            ray_triangle_intersection(local_ray_origin, local_ray_dir, v0, v1, v2)
        {
            if t > 0.0 && (closest_hit.is_none() || t < closest_hit.as_ref().unwrap().0) {
                let w = 1.0 - u - v;
                let local_pos = v0 * w + v1 * u + v2 * v;
                closest_hit = Some((t, local_pos, Vec3::new(w, u, v)));
            }
        }
    }

    let (_t, local_pos, barycentric) = closest_hit?;

    // Transform position to world space
    let world_pos = transform.transform_point(local_pos);

    // Get normal - find the triangle again to interpolate normal
    let mut normal = Vec3::Y;
    for triangle in indices.chunks(3) {
        if triangle.len() != 3 {
            continue;
        }

        let i0 = triangle[0];
        let i1 = triangle[1];
        let i2 = triangle[2];

        let v0 = Vec3::from(positions[i0]);
        let v1 = Vec3::from(positions[i1]);
        let v2 = Vec3::from(positions[i2]);

        // Check if this is the triangle we hit
        let test_pos = v0 * barycentric.x + v1 * barycentric.y + v2 * barycentric.z;
        if test_pos.distance(local_pos) < 0.001 {
            if let Some(normals) = normals {
                let n0 = Vec3::from(normals[i0]);
                let n1 = Vec3::from(normals[i1]);
                let n2 = Vec3::from(normals[i2]);
                let local_normal =
                    (n0 * barycentric.x + n1 * barycentric.y + n2 * barycentric.z).normalize();
                normal = (transform.rotation() * local_normal).normalize();
            } else {
                let edge1 = v1 - v0;
                let edge2 = v2 - v0;
                let local_normal = edge1.cross(edge2).normalize();
                normal = (transform.rotation() * local_normal).normalize();
            }
            break;
        }
    }

    Some((world_pos, normal))
}

/// Möller–Trumbore ray-triangle intersection
fn ray_triangle_intersection(
    ray_origin: Vec3,
    ray_dir: Vec3,
    v0: Vec3,
    v1: Vec3,
    v2: Vec3,
) -> Option<(f32, f32, f32)> {
    const EPSILON: f32 = 1e-8;

    let edge1 = v1 - v0;
    let edge2 = v2 - v0;

    let h = ray_dir.cross(edge2);
    let a = edge1.dot(h);

    if a.abs() < EPSILON {
        return None;
    }

    let f = 1.0 / a;
    let s = ray_origin - v0;
    let u = f * s.dot(h);

    if !(0.0..=1.0).contains(&u) {
        return None;
    }

    let q = s.cross(edge1);
    let v = f * ray_dir.dot(q);

    if v < 0.0 || u + v > 1.0 {
        return None;
    }

    let t = f * edge2.dot(q);

    if t > EPSILON { Some((t, u, v)) } else { None }
}

/// Handle sculpt mode events
fn handle_sculpt_events(
    mut events: MessageReader<SculptEvent>,
    mut edit_mode: ResMut<EditModeState>,
    mut sculpt_state: ResMut<SculptState>,
    mut sculpting_data: ResMut<SculptingData>,
    mut paint_mode: ResMut<crate::PaintMode>,
    mut paint_events: MessageWriter<crate::PaintEvent>,
    mut active_canvas: ResMut<crate::ActiveCanvasPlane>,
    mut outbound: ResMut<OutboundUiMessages>,
    mesh_query: Query<(&Mesh3d, &GlobalTransform)>,
    mut meshes: ResMut<Assets<Mesh>>,
    material_query: Query<&MeshMaterial3d<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
    time: Res<Time>,
) {
    for event in events.read() {
        match event {
            SculptEvent::Enter { entity } => {
                // Validate and prepare the entire target before changing either
                // brush owner. A rejected import must not abandon paint or claim
                // that an unusable mesh is actively being sculpted.
                let prepared = (|| -> Result<_, String> {
                    if sculpt_state.active {
                        return Err(
                            "Exit the current sculpt session before entering another.".into()
                        );
                    }
                    let (mesh_handle, global_transform) =
                        mesh_query.get(*entity).map_err(|_| {
                            "The sculpt target has no available mesh transform.".to_string()
                        })?;
                    let mesh = meshes
                        .get(&mesh_handle.0)
                        .ok_or_else(|| "The sculpt target mesh is not loaded.".to_string())?;
                    let affine = global_transform.affine();
                    if !affine.is_finite() || affine.matrix3.determinant().abs() < 1e-10 {
                        return Err("The sculpt target has a non-invertible transform.".into());
                    }
                    let he_mesh = HalfEdgeMesh::from_bevy_mesh_welded(mesh)
                        .or_else(|error| {
                            warn!("Sculpt seam weld rejected; preserving indexed connectivity: {error}");
                            HalfEdgeMesh::from_bevy_mesh(mesh)
                        })
                        .map_err(|error| format!("Cannot sculpt this mesh: {error}"))?;
                    let partition_config =
                        sculpting::PartitionConfig::from(&sculpt_state.chunk_config);
                    Ok((
                        mesh_handle.0.clone(),
                        *global_transform,
                        partition_mesh(&he_mesh, &partition_config),
                    ))
                })();
                let (mesh_handle, global_transform, chunked_mesh) = match prepared {
                    Ok(prepared) => prepared,
                    Err(message) => {
                        warn!("{message}");
                        outbound.send(BevyToUi::Error {
                            code: "sculpt_target_unavailable".into(),
                            message,
                        });
                        continue;
                    }
                };

                let mut preset = sculpt_preset(sculpt_state.deformation_type);
                preset.radius = sculpt_state.brush_radius;
                preset.strength = sculpt_state.brush_strength;
                preset.hardness = sculpt_state.brush_hardness;
                preset.falloff = sculpt_state.brush_falloff;
                let pipeline_config = PipelineConfig {
                    tessellation_enabled: true,
                    tessellation_config: sculpt_state.tessellation_config.clone(),
                    chunk_config: sculpt_state.chunk_config.clone(),
                    rebalance_after_stroke: true,
                };
                let pipeline = SculptingPipeline::with_config(preset, pipeline_config);

                // Only one viewport brush can own a stroke at a time.
                paint_mode.active = false;
                if paint_mode.current_stroke.take().is_some() {
                    paint_events.write(crate::PaintEvent::StrokeCancel);
                }
                active_canvas.camera_locked = false;
                edit_mode.mode = EditMode::Sculpt;
                edit_mode.target_entity = Some(*entity);
                sculpt_state.active = true;
                sculpt_state.target_entity = Some(*entity);
                sculpt_state.current_stroke_id = None;
                sculpt_state.last_world_pos = None;
                sculpt_state.adjust_mode = BrushAdjustMode::None;
                sculpt_state.adjust_start_cursor = None;
                sculpt_state.suppress_left_until_release = false;

                // Preserve the selected tool and customization when re-entering.
                sculpting_data.inverse_transform = Some(global_transform.affine().inverse());
                sculpting_data.transform_rotation = Some(global_transform.rotation());
                sculpting_data.model_matrix = Some(global_transform.to_matrix());
                sculpting_data.chunked_mesh = Some(chunked_mesh);
                sculpting_data.pipeline = Some(pipeline);
                sculpting_data.original_mesh_handle = Some(mesh_handle);
                sculpting_data.mesh_id = entity.index().index();
                sculpting_data.cached_vertex_mapping = None;

                if let Ok(material_handle) = material_query.get(*entity) {
                    if let Some(material) = materials.get_mut(&material_handle.0) {
                        material.double_sided = true;
                        material.cull_mode = None;
                    }
                }
                info!("Entered sculpt mode for entity {:?}", entity);
                outbound.send(BevyToUi::EditModeChanged {
                    mode: EditMode::Sculpt,
                });
            }
            SculptEvent::Exit => {
                info!("Exited sculpt mode");

                // Merge chunks back and update original mesh
                if let Some(chunked_mesh) = sculpting_data.chunked_mesh.take() {
                    let merged = sculpting::merge_chunks(&chunked_mesh);
                    info!(
                        "Merged {} chunks back into single mesh with {} faces",
                        chunked_mesh.chunk_count(),
                        merged.mesh.face_count()
                    );
                    // Commit the final dirty stroke even when Exit and StrokeEnd arrive together.
                    if let Some(handle) = sculpting_data.original_mesh_handle.as_ref() {
                        if let Some((new_mesh, _)) = half_edge_to_bevy_mesh(&merged.mesh) {
                            if let Some(original) = meshes.get_mut(handle) {
                                *original = new_mesh;
                            }
                        }
                    }
                }

                // Cleanup
                sculpting_data.pipeline = None;
                sculpting_data.original_mesh_handle = None;
                sculpting_data.inverse_transform = None;
                sculpting_data.transform_rotation = None;
                sculpting_data.model_matrix = None;
                sculpting_data.cached_vertex_mapping = None;

                // Remove chunk entities
                for entity in sculpting_data.chunk_entities.drain(..) {
                    commands.entity(entity).despawn();
                }

                edit_mode.mode = EditMode::None;
                edit_mode.target_entity = None;
                sculpt_state.active = false;
                sculpt_state.target_entity = None;
                sculpt_state.current_stroke_id = None;
                sculpt_state.last_world_pos = None;
                sculpt_state.suppress_left_until_release = false;

                // Notify UI
                outbound.send(BevyToUi::EditModeChanged {
                    mode: EditMode::None,
                });
            }
            SculptEvent::SetDeformationType(deformation_type) => {
                sculpt_state.deformation_type = *deformation_type;

                // Update pipeline preset
                if let Some(pipeline) = &mut sculpting_data.pipeline {
                    let mut preset = pipeline.brush_preset().clone();
                    preset.deformation_type = *deformation_type;
                    pipeline.set_brush_preset(preset);
                }

                info!("Set deformation type to {:?}", deformation_type);
            }
            SculptEvent::SetBrushRadius(radius) => {
                sculpt_state.brush_radius = radius.max(0.01);

                // Update pipeline preset
                if let Some(pipeline) = &mut sculpting_data.pipeline {
                    let mut preset = pipeline.brush_preset().clone();
                    preset.radius = sculpt_state.brush_radius;
                    pipeline.set_brush_preset(preset);
                }

                info!("Set brush radius to {}", sculpt_state.brush_radius);
            }
            SculptEvent::SetBrushStrength(strength) => {
                sculpt_state.brush_strength = strength.clamp(0.0, 1.0);

                // Update pipeline preset
                if let Some(pipeline) = &mut sculpting_data.pipeline {
                    let mut preset = pipeline.brush_preset().clone();
                    preset.strength = sculpt_state.brush_strength;
                    pipeline.set_brush_preset(preset);
                }

                info!("Set brush strength to {}", sculpt_state.brush_strength);
            }
            SculptEvent::StrokeStart {
                world_pos,
                normal,
                stroke_id,
            } => {
                sculpt_state.current_stroke_id = Some(*stroke_id);
                sculpt_state.last_world_pos = Some(*world_pos);
                sculpt_state.last_time = time.elapsed_secs_f64();

                info!(
                    "Sculpt stroke started: id={}, pos={:?}, normal={:?}",
                    stroke_id, world_pos, normal
                );

                // Extract values before mutable borrow
                let inverse_transform = sculpting_data.inverse_transform;
                let transform_rotation = sculpting_data.transform_rotation;
                let mesh_id = sculpting_data.mesh_id;

                // Begin stroke in pipeline with local-space coordinates
                if let Some(pipeline) = &mut sculpting_data.pipeline {
                    // Transform world position to local space
                    let local_pos = if let Some(inv) = &inverse_transform {
                        inv.transform_point3(*world_pos)
                    } else {
                        *world_pos
                    };

                    // Transform normal to local space (inverse transpose of rotation)
                    let local_normal = if let Some(rot) = &transform_rotation {
                        rot.inverse() * *normal
                    } else {
                        *normal
                    };

                    let timestamp_ms = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0);

                    let input = BrushInput {
                        position: local_pos,
                        normal: local_normal.normalize(),
                        pressure: 1.0,
                        timestamp_ms,
                    };

                    pipeline.begin_stroke(mesh_id, input);
                }
            }
            SculptEvent::StrokeMove {
                world_pos,
                normal,
                pressure,
            } => {
                // Destructure to enable split borrowing
                let SculptingData {
                    ref mut pipeline,
                    ref mut chunked_mesh,
                    ref inverse_transform,
                    ref transform_rotation,
                    ..
                } = *sculpting_data;

                // Apply deformation via pipeline with local-space coordinates
                if let (Some(pipeline), Some(chunked_mesh)) =
                    (pipeline.as_mut(), chunked_mesh.as_mut())
                {
                    // Transform world position to local space
                    let local_pos = if let Some(inv) = inverse_transform {
                        inv.transform_point3(*world_pos)
                    } else {
                        *world_pos
                    };

                    // Transform normal to local space
                    let local_normal = if let Some(rot) = transform_rotation {
                        rot.inverse() * *normal
                    } else {
                        *normal
                    };

                    let timestamp_ms = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_millis() as u64)
                        .unwrap_or(0);

                    let input = BrushInput {
                        position: local_pos,
                        normal: local_normal.normalize(),
                        pressure: *pressure,
                        timestamp_ms,
                    };

                    let result = pipeline.process_input(input, chunked_mesh);

                    if result.vertices_modified > 0 {
                        debug!(
                            "Deformed {} vertices in {} chunks",
                            result.vertices_modified,
                            result.chunks_affected.len()
                        );
                    }
                }

                sculpt_state.last_world_pos = Some(*world_pos);
                sculpt_state.last_time = time.elapsed_secs_f64();
            }
            SculptEvent::StrokeEnd => {
                if let Some(stroke_id) = sculpt_state.current_stroke_id.take() {
                    info!("Sculpt stroke ended: id={}", stroke_id);

                    // Destructure to enable split borrowing
                    let SculptingData {
                        ref mut pipeline,
                        ref mut chunked_mesh,
                        ..
                    } = *sculpting_data;

                    // End stroke in pipeline (triggers rebalancing)
                    if let (Some(pipeline), Some(chunked_mesh)) =
                        (pipeline.as_mut(), chunked_mesh.as_mut())
                    {
                        let result = pipeline.end_stroke(chunked_mesh);
                        if result.chunks_split > 0 || result.chunks_merged > 0 {
                            info!(
                                "Rebalanced: {} chunks split, {} chunks merged",
                                result.chunks_split, result.chunks_merged
                            );
                        }
                    }
                }

                sculpt_state.last_world_pos = None;
            }
            SculptEvent::StrokeCancel => {
                if let Some(stroke_id) = sculpt_state.current_stroke_id.take() {
                    info!("Sculpt stroke cancelled: id={}", stroke_id);

                    // Cancel stroke in pipeline
                    if let Some(pipeline) = &mut sculpting_data.pipeline {
                        pipeline.cancel_stroke();
                    }
                }

                sculpt_state.last_world_pos = None;
            }
        }
    }
}

/// Sync dirty chunks to GPU.
///
/// Two paths:
/// - **Topology changed**: full merge + rebuild Bevy mesh (expensive, O(V+F))
/// - **Position only**: patch vertex positions/normals in-place (cheap, O(dirty vertices))
fn sync_sculpt_chunks_to_gpu(
    sculpt_state: Res<SculptState>,
    mut sculpting_data: ResMut<SculptingData>,
    _mesh_query: Query<&Mesh3d>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    if !sculpt_state.active {
        return;
    }

    let Some(_target_entity) = sculpt_state.target_entity else {
        return;
    };

    // Get the mesh handle first (clone to avoid borrow issues)
    let Some(original_handle) = sculpting_data.original_mesh_handle.clone() else {
        return;
    };

    // Destructure to allow split borrows
    let SculptingData {
        ref mut chunked_mesh,
        ref mut cached_vertex_mapping,
        ..
    } = *sculpting_data;

    let Some(chunked_mesh) = chunked_mesh else {
        return;
    };

    let dirty_chunks = chunked_mesh.dirty_chunks();
    if dirty_chunks.is_empty() {
        return;
    }

    // Check if any chunk has topology changes (splits/collapses/rebalancing)
    let has_topology_change = dirty_chunks.iter().any(|&id| {
        chunked_mesh
            .get_chunk(id)
            .map_or(false, |c| c.topology_changed)
    });

    if has_topology_change || cached_vertex_mapping.is_none() {
        // Full rebuild path: merge all chunks and rebuild Bevy mesh
        debug!(
            "sync_sculpt_to_gpu: merging chunks (topology_changed={}, cached={})",
            has_topology_change,
            cached_vertex_mapping.is_some()
        );
        let merge_start = std::time::Instant::now();
        let merged = sculpting::merge_chunks(chunked_mesh);
        debug!(
            "sync_sculpt_to_gpu: merge done in {:?} ({} faces)",
            merge_start.elapsed(),
            merged.mesh.face_count()
        );

        let Some(bevy_mesh) = meshes.get_mut(&original_handle) else {
            return;
        };
        let Some((new_mesh, render_to_vertex)) = half_edge_to_bevy_mesh(&merged.mesh) else {
            return;
        };
        let mut unified_to_render: std::collections::HashMap<_, Vec<_>> =
            std::collections::HashMap::new();
        for (render_index, vertex) in render_to_vertex.into_iter().enumerate() {
            unified_to_render
                .entry(vertex)
                .or_default()
                .push(render_index);
        }
        let mapping = merged
            .vertex_mapping
            .into_iter()
            .filter_map(|(original, unified)| {
                unified_to_render
                    .remove(&unified)
                    .map(|indices| (original, indices))
            })
            .collect();
        *bevy_mesh = new_mesh;
        *cached_vertex_mapping = Some(mapping);
    } else if let Some(mapping) = cached_vertex_mapping {
        // Position-only path: patch vertex buffers in-place
        let Some(bevy_mesh) = meshes.get_mut(&original_handle) else {
            return;
        };
        {
            // Patch positions
            if let Some(VertexAttributeValues::Float32x3(positions)) =
                bevy_mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
            {
                let num_positions = positions.len();
                for &chunk_id in &dirty_chunks {
                    let Some(chunk) = chunked_mesh.get_chunk(chunk_id) else {
                        continue;
                    };
                    for vertex in chunk.mesh.vertices() {
                        if let Some(&original_id) = chunk.local_to_original.get(&vertex.id) {
                            if let Some(render_indices) = mapping.get(&original_id) {
                                for &idx in render_indices {
                                    if idx < num_positions {
                                        positions[idx] = vertex.position.to_array();
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Patch normals
            if let Some(VertexAttributeValues::Float32x3(normals)) =
                bevy_mesh.attribute_mut(Mesh::ATTRIBUTE_NORMAL)
            {
                let num_normals = normals.len();
                for &chunk_id in &dirty_chunks {
                    let Some(chunk) = chunked_mesh.get_chunk(chunk_id) else {
                        continue;
                    };
                    for vertex in chunk.mesh.vertices() {
                        if let Some(&original_id) = chunk.local_to_original.get(&vertex.id) {
                            if let Some(render_indices) = mapping.get(&original_id) {
                                for &idx in render_indices {
                                    if idx < num_normals {
                                        normals[idx] = vertex.normal.to_array();
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Clear dirty flags
    for chunk_id in dirty_chunks {
        if let Some(chunk) = chunked_mesh.get_chunk_mut(chunk_id) {
            chunk.clear_dirty();
        }
    }
}

/// Convert a HalfEdgeMesh to a Bevy Mesh
fn half_edge_to_bevy_mesh(
    he_mesh: &HalfEdgeMesh,
) -> Option<(Mesh, Vec<painting::half_edge::VertexId>)> {
    if let Err(error) = he_mesh.validate() {
        warn!("Cannot export invalid sculpt geometry: {error}");
        return None;
    }
    Some(he_mesh.to_bevy_mesh_with_vertex_map())
}

/// Render the sculpt brush gizmo: a wire circle at the brush radius aligned to the
/// surface normal, plus a falloff profile curve showing hardness and falloff shape.
fn render_sculpt_brush_gizmo(
    sculpt_state: Res<SculptState>,
    mut gizmos: Gizmos,
    camera_query: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mesh_query: Query<(&Mesh3d, &GlobalTransform)>,
    meshes: Res<Assets<Mesh>>,
) {
    if !sculpt_state.active {
        return;
    }

    let Some(target_entity) = sculpt_state.target_entity else {
        return;
    };

    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };

    let Ok(window) = windows.single() else {
        return;
    };

    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };

    let Ok((mesh_handle, mesh_transform)) = mesh_query.get(target_entity) else {
        return;
    };

    let Some(mesh) = meshes.get(&mesh_handle.0) else {
        return;
    };

    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_pos) else {
        return;
    };

    let Some((world_pos, normal)) = ray_mesh_intersection_simple(&ray, mesh, mesh_transform) else {
        return;
    };

    let radius = sculpt_state.brush_radius;

    // Small offset along normal to prevent z-fighting with the mesh surface
    let offset = normal * (radius * 0.005);
    let center = world_pos + offset;

    // Orient the circle so its plane is perpendicular to the surface normal
    let rotation = Quat::from_rotation_arc(Vec3::Z, normal);

    // --- Outer circle (brush radius) ---
    let circle_color = Color::srgba(0.3, 0.7, 1.0, 0.8);
    gizmos.circle(Isometry3d::new(center, rotation), radius, circle_color);

    // --- Falloff profile curve ---
    // Orient the profile so its "up" direction faces the camera for readability.
    let camera_pos = camera_transform.translation();
    let view_dir = (camera_pos - center).normalize();
    // Project view direction onto the circle plane to get profile "up"
    let projected = view_dir - normal * view_dir.dot(normal);
    let profile_up = if projected.length_squared() > 0.001 {
        projected.normalize()
    } else {
        // Viewing straight along the normal — fall back to an arbitrary in-plane axis
        rotation * Vec3::Y
    };
    // The profile extends horizontally along the perpendicular in-plane axis
    let profile_across = normal.cross(profile_up).normalize();

    let segments = 32u32;
    let max_height = radius * 0.4;
    let profile_color = Color::srgba(0.3, 0.7, 1.0, 0.4);
    let falloff = sculpt_state.brush_falloff;
    let hardness = sculpt_state.brush_hardness;

    let mut prev_point: Option<Vec3> = None;

    for i in 0..=segments {
        let t = i as f32 / segments as f32; // 0.0 to 1.0
        let x = (t * 2.0 - 1.0) * radius; // -radius to +radius
        let d = x.abs() / radius; // normalized distance 0..1

        let strength = falloff.evaluate_with_hardness(d, hardness);
        let y = strength * max_height;

        let point = center + profile_across * x + profile_up * y;

        if let Some(prev) = prev_point {
            gizmos.line(prev, point, profile_color);
        }
        prev_point = Some(point);
    }
}

#[cfg(test)]
mod sculpt_geometry_sync_tests {
    use super::*;
    use bevy::mesh::PrimitiveTopology;
    use painting::half_edge::VertexId;

    #[test]
    fn adjustment_confirmation_consumes_pointer_until_release() {
        use bevy::ecs::system::RunSystemOnce;

        let mut world = World::new();
        world.init_resource::<ButtonInput<KeyCode>>();
        world.init_resource::<ButtonInput<MouseButton>>();
        world.init_resource::<SculptState>();
        world.init_resource::<SculptingData>();
        world.init_resource::<FrontendInputBlockState>();
        world.init_resource::<Assets<Mesh>>();
        world.init_resource::<StrokeIdGenerator>();
        world.init_resource::<Time>();
        world.init_resource::<Messages<CursorMoved>>();
        world.init_resource::<Messages<SculptEvent>>();
        world.spawn((Window::default(), PrimaryWindow));
        {
            let mut state = world.resource_mut::<SculptState>();
            state.active = true;
            state.adjust_mode = BrushAdjustMode::Radius;
            state.brush_radius = 1.25;
            // A stroke interrupted by F must not resume on the confirming press.
            state.current_stroke_id = Some(7);
        }
        world
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        world.run_system_once(handle_brush_adjustment).unwrap();
        assert_eq!(
            world.resource::<SculptState>().adjust_mode,
            BrushAdjustMode::None
        );
        assert!(world.resource::<SculptState>().suppress_left_until_release);
        assert_eq!(world.resource::<SculptState>().brush_radius, 1.25);
        world.run_system_once(handle_sculpt_input).unwrap();
        let events: Vec<_> = world
            .resource_mut::<Messages<SculptEvent>>()
            .drain()
            .collect();
        assert!(matches!(events.as_slice(), [SculptEvent::StrokeEnd]));
        world.resource_mut::<SculptState>().current_stroke_id = None;
        world.resource_mut::<ButtonInput<MouseButton>>().clear();
        // Still held after leaving the adjustment: no new or continued stroke.
        world.run_system_once(handle_sculpt_input).unwrap();
        assert!(world.resource::<SculptState>().suppress_left_until_release);
        assert!(world.resource::<Messages<SculptEvent>>().is_empty());
        // Release is consumed even if it occurs over UI; the following press is free.
        world
            .resource_mut::<FrontendInputBlockState>()
            .block_pointer = true;
        world
            .resource_mut::<ButtonInput<MouseButton>>()
            .release(MouseButton::Left);
        world.run_system_once(handle_sculpt_input).unwrap();
        assert!(!world.resource::<SculptState>().suppress_left_until_release);
        assert!(world.resource::<Messages<SculptEvent>>().is_empty());
    }

    fn sculpt_event_app() -> App {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .init_resource::<EditModeState>()
            .init_resource::<SculptState>()
            .init_resource::<SculptingData>()
            .init_resource::<crate::PaintMode>()
            .init_resource::<crate::ActiveCanvasPlane>()
            .init_resource::<OutboundUiMessages>()
            .init_resource::<Time>()
            .add_message::<SculptEvent>()
            .add_message::<crate::PaintEvent>()
            .add_systems(Update, handle_sculpt_events);
        app
    }

    #[test]
    fn rejected_sculpt_entry_keeps_existing_mode_and_reports_error() {
        let mut app = sculpt_event_app();
        let missing_entity = app.world_mut().spawn_empty().id();
        let missing_asset = app
            .world_mut()
            .spawn((Mesh3d(Handle::default()), GlobalTransform::IDENTITY))
            .id();
        let invalid = Mesh::new(
            PrimitiveTopology::TriangleList,
            bevy::asset::RenderAssetUsages::default(),
        );
        let invalid_handle = app.world_mut().resource_mut::<Assets<Mesh>>().add(invalid);
        let invalid_entity = app
            .world_mut()
            .spawn((Mesh3d(invalid_handle), GlobalTransform::IDENTITY))
            .id();
        let valid_handle = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(Sphere::new(1.).mesh().uv(12, 6));
        let singular_entity = app
            .world_mut()
            .spawn((
                Mesh3d(valid_handle),
                GlobalTransform::from(Transform::from_scale(Vec3::ZERO)),
            ))
            .id();
        {
            let mut paint = app.world_mut().resource_mut::<crate::PaintMode>();
            paint.active = true;
            paint.current_stroke = Some(crate::StrokeState {
                stroke_id: 17,
                space_id: 3,
                start_time: 0,
                last_world_pos: Some(Vec3::ZERO),
                last_time: 0.0,
            });
        }
        app.world_mut()
            .resource_mut::<crate::ActiveCanvasPlane>()
            .camera_locked = true;
        for entity in [
            missing_entity,
            missing_asset,
            invalid_entity,
            singular_entity,
        ] {
            app.world_mut()
                .resource_mut::<OutboundUiMessages>()
                .messages
                .clear();
            app.world_mut().write_message(SculptEvent::Enter { entity });
            app.update();
            assert!(!app.world().resource::<SculptState>().active);
            assert_eq!(app.world().resource::<EditModeState>().mode, EditMode::None);
            let paint = app.world().resource::<crate::PaintMode>();
            assert!(paint.active);
            assert_eq!(paint.current_stroke.as_ref().unwrap().stroke_id, 17);
            assert!(
                app.world()
                    .resource::<crate::ActiveCanvasPlane>()
                    .camera_locked
            );
            assert!(app.world().resource::<SculptingData>().pipeline.is_none());
            let outbound = &app.world().resource::<OutboundUiMessages>().messages;
            assert!(
                matches!(outbound.as_slice(), [BevyToUi::Error { code, .. }] if code == "sculpt_target_unavailable")
            );
        }
    }

    #[test]
    fn exit_commits_dirty_geometry_and_reentry_preserves_brush_and_uv_corners() {
        let mut app = sculpt_event_app();
        let source = Sphere::new(1.).mesh().uv(12, 6);
        let handle = app.world_mut().resource_mut::<Assets<Mesh>>().add(source);
        let entity = app
            .world_mut()
            .spawn((Mesh3d(handle.clone()), GlobalTransform::IDENTITY))
            .id();
        {
            let mut state = app.world_mut().resource_mut::<SculptState>();
            state.deformation_type = DeformationType::Grab;
            state.brush_radius = 1.25;
            state.brush_strength = 0.4;
            state.brush_hardness = 0.7;
            state.brush_falloff = FalloffCurve::Sharp;
        }
        let settings = sculpt_snapshot(app.world().resource::<SculptState>());
        app.world_mut().write_message(SculptEvent::Enter { entity });
        app.update();
        assert!(app.world().resource::<SculptState>().active);

        // Change chunk geometry without running GPU sync. Exit must commit this
        // final dirty update, and must use the UV-corner-preserving mesh export.
        let expected = {
            let mut data = app.world_mut().resource_mut::<SculptingData>();
            let chunks = data.chunked_mesh.as_mut().unwrap();
            for chunk in chunks.chunks.values_mut() {
                let ids: Vec<_> = chunk.mesh.vertices().iter().map(|v| v.id).collect();
                for id in ids {
                    chunk.mesh.vertex_mut(id).unwrap().position.y += 0.2;
                }
                chunk.mark_dirty();
            }
            half_edge_to_bevy_mesh(&sculpting::merge_chunks(chunks).mesh)
                .unwrap()
                .0
        };
        app.world_mut().write_message(SculptEvent::Exit);
        app.update();
        {
            let assets = app.world().resource::<Assets<Mesh>>();
            let committed = assets.get(&handle).unwrap();
            for attribute in [
                Mesh::ATTRIBUTE_POSITION,
                Mesh::ATTRIBUTE_NORMAL,
                Mesh::ATTRIBUTE_UV_0,
            ] {
                assert_eq!(
                    committed.attribute(attribute),
                    expected.attribute(attribute)
                );
            }
            assert_eq!(
                committed.indices().unwrap().iter().collect::<Vec<_>>(),
                expected.indices().unwrap().iter().collect::<Vec<_>>()
            );
        }
        assert!(!app.world().resource::<SculptState>().active);
        assert!(
            app.world()
                .resource::<SculptingData>()
                .chunked_mesh
                .is_none()
        );
        assert!(
            app.world()
                .resource::<SculptingData>()
                .cached_vertex_mapping
                .is_none()
        );

        app.world_mut().write_message(SculptEvent::Enter { entity });
        app.update();
        assert_eq!(
            format!(
                "{:?}",
                sculpt_snapshot(app.world().resource::<SculptState>())
            ),
            format!("{settings:?}")
        );
        let data = app.world().resource::<SculptingData>();
        let merged = sculpting::merge_chunks(data.chunked_mesh.as_ref().unwrap());
        merged.mesh.validate().unwrap();
        assert_eq!(
            merged.mesh.face_count(),
            expected.indices().unwrap().len() / 3
        );
        assert!(
            merged
                .mesh
                .half_edges()
                .iter()
                .all(|edge| edge.twin.is_some())
        );
        assert_eq!(data.pipeline.as_ref().unwrap().brush_preset().radius, 1.25);
    }

    #[test]
    fn gpu_sync_keeps_uv_corners_and_patches_every_render_copy() {
        let source = Sphere::new(1.).mesh().uv(12, 6);
        let imported = HalfEdgeMesh::from_bevy_mesh_welded(&source).unwrap();
        let mut chunks = partition_mesh(&imported, &sculpting::PartitionConfig::default());
        for chunk in chunks.chunks.values_mut() {
            chunk.mark_dirty();
        }
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>();
        let handle = app.world_mut().resource_mut::<Assets<Mesh>>().add(source);
        let entity = app.world_mut().spawn(Mesh3d(handle.clone())).id();
        app.insert_resource(SculptState {
            active: true,
            target_entity: Some(entity),
            ..Default::default()
        });
        app.insert_resource(SculptingData {
            chunked_mesh: Some(chunks),
            original_mesh_handle: Some(handle.clone()),
            ..Default::default()
        });
        app.add_systems(Update, sync_sculpt_chunks_to_gpu);
        app.update();
        let first_uv = app
            .world()
            .resource::<Assets<Mesh>>()
            .get(&handle)
            .unwrap()
            .attribute(Mesh::ATTRIBUTE_UV_0)
            .unwrap()
            .clone();
        let render_indices = app
            .world()
            .resource::<SculptingData>()
            .cached_vertex_mapping
            .as_ref()
            .unwrap()[&VertexId(0)]
            .clone();
        assert!(render_indices.len() > 1);
        let position = Vec3::new(2., 3., 4.);
        {
            let mut data = app.world_mut().resource_mut::<SculptingData>();
            for chunk in data.chunked_mesh.as_mut().unwrap().chunks.values_mut() {
                if let Some(&local) = chunk.original_to_local.get(&VertexId(0)) {
                    chunk.mesh.set_vertex_position(local, position);
                    chunk.mark_dirty();
                }
            }
        }
        app.update();
        let mesh = app.world().resource::<Assets<Mesh>>().get(&handle).unwrap();
        assert_eq!(mesh.attribute(Mesh::ATTRIBUTE_UV_0), Some(&first_uv));
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap()
            .as_float3()
            .unwrap();
        for index in render_indices {
            assert_eq!(positions[index], position.to_array());
        }
        assert!(
            app.world()
                .resource::<SculptingData>()
                .chunked_mesh
                .as_ref()
                .unwrap()
                .dirty_chunks()
                .is_empty()
        );
    }
}

/// Keep UI and keyboard paths backed by the same brush state.
pub(crate) fn sculpt_snapshot(state: &SculptState) -> SculptBrushSettings {
    SculptBrushSettings {
        tool: match state.deformation_type {
            DeformationType::Push => SculptTool::Push,
            DeformationType::Pull => SculptTool::Pull,
            DeformationType::Grab => SculptTool::Grab,
            DeformationType::Smooth => SculptTool::Smooth,
            DeformationType::Flatten => SculptTool::Flatten,
            DeformationType::Inflate => SculptTool::Inflate,
            DeformationType::Pinch => SculptTool::Pinch,
            DeformationType::Crease => SculptTool::Crease,
        },
        radius: state.brush_radius,
        strength: state.brush_strength,
        hardness: state.brush_hardness,
        falloff: match state.brush_falloff {
            FalloffCurve::Linear => SculptFalloff::Linear,
            FalloffCurve::Smooth => SculptFalloff::Smooth,
            FalloffCurve::Sharp => SculptFalloff::Sharp,
            FalloffCurve::Constant => SculptFalloff::Constant,
            FalloffCurve::Sphere => SculptFalloff::Sphere,
        },
    }
}

fn sculpt_preset(tool: DeformationType) -> BrushPreset {
    match tool {
        DeformationType::Push => BrushPreset::push(),
        DeformationType::Pull => BrushPreset::pull(),
        DeformationType::Grab => BrushPreset::grab(),
        DeformationType::Smooth => BrushPreset::smooth(),
        DeformationType::Flatten => BrushPreset::flatten(),
        DeformationType::Inflate => BrushPreset::inflate(),
        DeformationType::Pinch => BrushPreset::pinch(),
        DeformationType::Crease => BrushPreset::crease(),
    }
}

pub(crate) fn apply_sculpt_command(world: &mut World, command: &SculptCommand) {
    let Some(mut state) = world.get_resource_mut::<SculptState>() else {
        return;
    };
    match command {
        SculptCommand::SetTool { tool } => {
            state.deformation_type = match tool {
                SculptTool::Push => DeformationType::Push,
                SculptTool::Pull => DeformationType::Pull,
                SculptTool::Grab => DeformationType::Grab,
                SculptTool::Smooth => DeformationType::Smooth,
                SculptTool::Flatten => DeformationType::Flatten,
                SculptTool::Inflate => DeformationType::Inflate,
                SculptTool::Pinch => DeformationType::Pinch,
                SculptTool::Crease => DeformationType::Crease,
            }
        }
        SculptCommand::SetRadius { radius } if radius.is_finite() => {
            state.brush_radius = radius.clamp(0.01, 10.0)
        }
        SculptCommand::SetStrength { strength } if strength.is_finite() => {
            state.brush_strength = strength.clamp(0.0, 1.0)
        }
        SculptCommand::SetHardness { hardness } if hardness.is_finite() => {
            state.brush_hardness = hardness.clamp(0.0, 1.0)
        }
        SculptCommand::SetFalloff { falloff } => {
            state.brush_falloff = match falloff {
                SculptFalloff::Linear => FalloffCurve::Linear,
                SculptFalloff::Smooth => FalloffCurve::Smooth,
                SculptFalloff::Sharp => FalloffCurve::Sharp,
                SculptFalloff::Constant => FalloffCurve::Constant,
                SculptFalloff::Sphere => FalloffCurve::Sphere,
            }
        }
        _ => return,
    }
    // Tool-specific engine behavior (e.g. continuous Grab) is preserved, while the
    // user's radius/strength/hardness/falloff survive tool and mode switches.
    let mut preset = sculpt_preset(state.deformation_type);
    preset.radius = state.brush_radius;
    preset.strength = state.brush_strength;
    preset.hardness = state.brush_hardness;
    preset.falloff = state.brush_falloff;
    if let Some(mut data) = world.get_resource_mut::<SculptingData>() {
        if let Some(pipeline) = data.pipeline.as_mut() {
            pipeline.set_brush_preset(preset);
        }
    }
}

#[cfg(test)]
mod brush_control_tests {
    use super::*;

    #[test]
    fn sculpt_controls_update_the_active_pipeline_and_preserve_user_settings() {
        let mut world = World::new();
        world.init_resource::<SculptState>();
        world.insert_resource(SculptingData {
            pipeline: Some(SculptingPipeline::new(BrushPreset::push())),
            ..default()
        });
        for command in [
            SculptCommand::SetRadius { radius: 1.2 },
            SculptCommand::SetStrength { strength: 0.25 },
            SculptCommand::SetHardness { hardness: 0.2 },
            SculptCommand::SetFalloff {
                falloff: SculptFalloff::Sharp,
            },
            SculptCommand::SetTool {
                tool: SculptTool::Grab,
            },
        ] {
            apply_sculpt_command(&mut world, &command);
        }
        let data = world.resource::<SculptingData>();
        let preset = data.pipeline.as_ref().unwrap().brush_preset();
        assert_eq!(preset.deformation_type, DeformationType::Grab);
        assert_eq!(preset.radius, 1.2);
        assert_eq!(preset.strength, 0.25);
        assert_eq!(preset.hardness, 0.2);
        assert_eq!(preset.falloff, FalloffCurve::Sharp);
        assert_eq!(
            preset.spacing, 0.0,
            "Grab must keep its continuous engine preset"
        );
        assert_eq!(preset.autosmooth, 0.0);
        assert_eq!(
            sculpt_snapshot(world.resource::<SculptState>()).tool,
            SculptTool::Grab
        );
    }

    #[test]
    fn sculpt_strength_and_falloff_change_real_vertex_displacement() {
        fn displacement(strength: f32, falloff: SculptFalloff, hardness: f32) -> f32 {
            let mut world = World::new();
            world.init_resource::<SculptState>();
            world.init_resource::<SculptingData>();
            for command in [
                SculptCommand::SetRadius { radius: 1.0 },
                SculptCommand::SetStrength { strength },
                SculptCommand::SetHardness { hardness },
                SculptCommand::SetFalloff { falloff },
            ] {
                apply_sculpt_command(&mut world, &command);
            }
            let state = world.resource::<SculptState>();
            let mut mesh =
                HalfEdgeMesh::from_raw(Vec::new(), Vec::new(), Vec::new(), Default::default());
            let vertex = mesh.add_vertex(Vec3::new(0.75, 0.0, 0.0), Vec3::Y, None);
            let dab = sculpting::deformation::DabInfo {
                position: Vec3::ZERO,
                normal: Vec3::Y,
                radius: state.brush_radius,
                strength: state.brush_strength,
                hardness: state.brush_hardness,
            };
            sculpting::deformation::apply_push(&mut mesh, &[vertex], &dab, state.brush_falloff);
            mesh.vertex(vertex).unwrap().position.y
        }
        assert!(
            displacement(1.0, SculptFalloff::Linear, 0.0)
                > displacement(0.2, SculptFalloff::Linear, 0.0)
        );
        assert!(
            displacement(1.0, SculptFalloff::Linear, 0.0)
                > displacement(1.0, SculptFalloff::Sharp, 0.0)
        );
        assert!(
            displacement(1.0, SculptFalloff::Sharp, 0.9)
                > displacement(1.0, SculptFalloff::Sharp, 0.0)
        );
    }
}
