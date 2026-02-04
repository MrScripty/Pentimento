//! Sculpt mode for 3D mesh sculpting with dynamic tessellation
//!
//! Provides sculpting functionality:
//! - Ctrl+Tab to enter/exit sculpt mode (requires mesh selected)
//! - Brush-based deformation (Push, Pull, Smooth, etc.)
//! - Screen-space adaptive tessellation
//! - Mesh chunking for optimized GPU updates

use bevy::ecs::message::Message;
use bevy::input::mouse::MouseButton;
use bevy::math::Affine3A;
use bevy::mesh::{Indices, Meshable, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use bevy::window::{CursorMoved, PrimaryWindow};
use painting::half_edge::HalfEdgeMesh;
use pentimento_ipc::{BevyToUi, EditMode};
use sculpting::{
    partition_mesh, BrushInput, BrushPreset, ChunkConfig, ChunkedMesh, DeformationType,
    PipelineConfig, ScreenSpaceConfig, SculptingPipeline, TessellationConfig, TessellationMode,
};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::camera::MainCamera;
use crate::edit_mode::EditModeState;
use crate::paint_mode::StrokeIdGenerator;
use crate::pixel_coverage::{estimate_pixel_coverage_cpu, PixelCoverageState};
use crate::render_camera::{ActiveRenderCamera, RenderCamera};
use crate::OutboundUiMessages;
#[cfg(feature = "selection")]
use crate::selection::Selected;

/// Resource tracking sculpt mode state
#[derive(Resource)]
pub struct SculptState {
    /// Whether sculpt mode is currently active
    pub active: bool,
    /// Entity currently being sculpted
    pub target_entity: Option<Entity>,
    /// Current deformation type
    pub deformation_type: DeformationType,
    /// Brush radius in world units
    pub brush_radius: f32,
    /// Brush strength (0.0 - 1.0)
    pub brush_strength: f32,
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
}

impl Default for SculptState {
    fn default() -> Self {
        Self {
            active: false,
            target_entity: None,
            deformation_type: DeformationType::Push,
            brush_radius: 0.5,
            brush_strength: 1.0,
            tessellation_config: TessellationConfig::default(),
            chunk_config: ChunkConfig::default(),
            current_stroke_id: None,
            last_world_pos: None,
            last_time: 0.0,
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
    /// Cached vertex mapping from merge (original_id → unified_id).
    /// Used for position-only GPU updates without full re-merge.
    pub cached_vertex_mapping: Option<std::collections::HashMap<painting::half_edge::VertexId, painting::half_edge::VertexId>>,
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

/// Component for the brush visualization sphere
#[derive(Component)]
pub struct SculptBrushIndicator;

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
                    handle_sculpt_input,
                    handle_sculpt_events,
                    sync_sculpt_chunks_to_gpu,
                    update_brush_indicator,
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

/// Command to spawn brush indicator with proper assets
struct SpawnBrushIndicatorCommand;

impl Command for SpawnBrushIndicatorCommand {
    fn apply(self, world: &mut World) {
        // Create sphere mesh for brush indicator
        let sphere = Sphere::new(0.5);
        let mesh = sphere.mesh().build();
        let mesh_handle = world.resource_mut::<Assets<Mesh>>().add(mesh);

        // Create semi-transparent material with higher visibility
        let material = StandardMaterial {
            base_color: Color::srgba(0.3, 0.7, 1.0, 0.5),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            cull_mode: None, // Show both sides
            depth_bias: 1.0, // Push forward to avoid z-fighting
            ..default()
        };
        let material_handle = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(material);

        // Spawn the brush indicator entity
        world.spawn((
            Mesh3d(mesh_handle),
            MeshMaterial3d(material_handle),
            Transform::default(),
            Visibility::Hidden,
            SculptBrushIndicator,
        ));
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
    mut events: MessageWriter<SculptEvent>,
) {
    // Check for Ctrl modifier
    let ctrl = key_input.pressed(KeyCode::ControlLeft)
        || key_input.pressed(KeyCode::ControlRight);
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

/// Handle mouse input for sculpting
fn handle_sculpt_input(
    mouse_button: Res<ButtonInput<MouseButton>>,
    windows: Query<(Entity, &Window), With<PrimaryWindow>>,
    mut cursor_events: MessageReader<CursorMoved>,
    camera_query: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    mesh_query: Query<(&Mesh3d, &GlobalTransform)>,
    meshes: Res<Assets<Mesh>>,
    sculpt_state: Res<SculptState>,
    mut stroke_id_gen: ResMut<StrokeIdGenerator>,
    mut sculpt_events: MessageWriter<SculptEvent>,
    time: Res<Time>,
) {
    // Only process if sculpt mode is active
    if !sculpt_state.active {
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
        let cursor_pos = cursor_positions.last().copied().or_else(|| window.cursor_position());
        if let Some(cursor_pos) = cursor_pos {
            if let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_pos) {
                if let Some((world_pos, normal)) = ray_mesh_intersection_simple(&ray, mesh, mesh_transform) {
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
                if let Some((world_pos, normal)) = ray_mesh_intersection_simple(&ray, mesh, mesh_transform) {
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
        if let Some((t, u, v)) = ray_triangle_intersection(local_ray_origin, local_ray_dir, v0, v1, v2) {
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
                let local_normal = (n0 * barycentric.x + n1 * barycentric.y + n2 * barycentric.z).normalize();
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

    if t > EPSILON {
        Some((t, u, v))
    } else {
        None
    }
}

/// Handle sculpt mode events
fn handle_sculpt_events(
    mut events: MessageReader<SculptEvent>,
    mut edit_mode: ResMut<EditModeState>,
    mut sculpt_state: ResMut<SculptState>,
    mut sculpting_data: ResMut<SculptingData>,
    mut outbound: ResMut<OutboundUiMessages>,
    mesh_query: Query<(&Mesh3d, &GlobalTransform)>,
    meshes: Res<Assets<Mesh>>,
    material_query: Query<&MeshMaterial3d<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    brush_indicators: Query<Entity, With<SculptBrushIndicator>>,
    mut commands: Commands,
    time: Res<Time>,
) {
    for event in events.read() {
        match event {
            SculptEvent::Enter { entity } => {
                // Enter sculpt mode
                edit_mode.mode = EditMode::Sculpt;
                edit_mode.target_entity = Some(*entity);
                sculpt_state.active = true;
                sculpt_state.target_entity = Some(*entity);
                sculpt_state.deformation_type = DeformationType::Push;

                info!("Entered sculpt mode for entity {:?}", entity);

                // Enable double-sided rendering for the sculpted mesh
                if let Ok(material_handle) = material_query.get(*entity) {
                    if let Some(material) = materials.get_mut(&material_handle.0) {
                        material.double_sided = true;
                        material.cull_mode = None;
                        info!("Enabled double-sided rendering for sculpt target");
                    }
                }

                // Initialize chunked mesh from entity
                if let Ok((mesh_handle, global_transform)) = mesh_query.get(*entity) {
                    // Store transforms for coordinate conversion
                    let affine = global_transform.affine();
                    sculpting_data.inverse_transform = Some(affine.inverse());
                    sculpting_data.transform_rotation = Some(global_transform.rotation());
                    sculpting_data.model_matrix = Some(global_transform.to_matrix());

                    if let Some(bevy_mesh) = meshes.get(&mesh_handle.0) {
                        match HalfEdgeMesh::from_bevy_mesh(bevy_mesh) {
                            Ok(he_mesh) => {
                                // Partition into chunks
                                let partition_config = sculpting::PartitionConfig::from(&sculpt_state.chunk_config);
                                let chunked_mesh = partition_mesh(&he_mesh, &partition_config);

                                info!(
                                    "Created {} chunks from mesh with {} faces",
                                    chunked_mesh.chunk_count(),
                                    chunked_mesh.total_face_count()
                                );

                                // Create pipeline
                                let mut preset = BrushPreset::push();
                                preset.radius = sculpt_state.brush_radius;
                                preset.strength = sculpt_state.brush_strength;

                                // Enable tessellation for dynamic topology
                                let pipeline_config = PipelineConfig {
                                    tessellation_enabled: true,
                                    tessellation_config: sculpt_state.tessellation_config.clone(),
                                    chunk_config: sculpt_state.chunk_config.clone(),
                                    rebalance_after_stroke: true,
                                };

                                let pipeline = SculptingPipeline::with_config(preset, pipeline_config);

                                sculpting_data.chunked_mesh = Some(chunked_mesh);
                                sculpting_data.pipeline = Some(pipeline);
                                sculpting_data.original_mesh_handle = Some(mesh_handle.0.clone());
                                sculpting_data.mesh_id = entity.index().index();
                            }
                            Err(e) => {
                                warn!("Failed to convert mesh to half-edge: {:?}", e);
                            }
                        }
                    }
                }

                // Spawn brush indicator with actual mesh and material
                // The actual mesh/material creation is handled in a separate startup system
                // Here we just mark that we need to spawn one
                commands.queue(SpawnBrushIndicatorCommand);

                // Notify UI
                outbound.send(BevyToUi::EditModeChanged {
                    mode: EditMode::Sculpt,
                });
            }
            SculptEvent::Exit => {
                info!("Exited sculpt mode");

                // Merge chunks back and update original mesh
                if let Some(chunked_mesh) = sculpting_data.chunked_mesh.take() {
                    let merged = sculpting::merge_chunks(&chunked_mesh);
                    info!("Merged {} chunks back into single mesh with {} faces",
                          chunked_mesh.chunk_count(),
                          merged.mesh.face_count());
                    // TODO: Update the original mesh entity with merged.mesh
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

                // Despawn brush indicator
                for indicator in brush_indicators.iter() {
                    commands.entity(indicator).despawn();
                }

                edit_mode.mode = EditMode::None;
                edit_mode.target_entity = None;
                sculpt_state.active = false;
                sculpt_state.target_entity = None;
                sculpt_state.current_stroke_id = None;
                sculpt_state.last_world_pos = None;

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
                if let (Some(pipeline), Some(chunked_mesh)) = (pipeline.as_mut(), chunked_mesh.as_mut()) {
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
        let merged = sculpting::merge_chunks(chunked_mesh);

        if let Some(bevy_mesh) = meshes.get_mut(&original_handle) {
            if let Some(new_mesh) = half_edge_to_bevy_mesh(&merged.mesh) {
                *bevy_mesh = new_mesh;
            }
        }

        // Cache the vertex mapping for future position-only updates
        *cached_vertex_mapping = Some(merged.vertex_mapping);
    } else if let Some(mapping) = cached_vertex_mapping {
        // Position-only path: patch vertex buffers in-place
        if let Some(bevy_mesh) = meshes.get_mut(&original_handle) {
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
                            if let Some(&unified_id) = mapping.get(&original_id) {
                                let idx = unified_id.0 as usize;
                                if idx < num_positions {
                                    positions[idx] = vertex.position.to_array();
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
                            if let Some(&unified_id) = mapping.get(&original_id) {
                                let idx = unified_id.0 as usize;
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

    // Clear dirty flags
    for chunk_id in dirty_chunks {
        if let Some(chunk) = chunked_mesh.get_chunk_mut(chunk_id) {
            chunk.clear_dirty();
        }
    }
}

/// Convert a HalfEdgeMesh to a Bevy Mesh
fn half_edge_to_bevy_mesh(he_mesh: &HalfEdgeMesh) -> Option<Mesh> {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    // Validate: check that vertex IDs match array indices
    #[cfg(debug_assertions)]
    for (idx, vertex) in he_mesh.vertices().iter().enumerate() {
        if vertex.id.0 as usize != idx {
            warn!(
                "half_edge_to_bevy_mesh: vertex at index {} has id {:?} (expected {}). \
                 This will cause incorrect face indices!",
                idx, vertex.id, idx
            );
        }
    }

    // Build vertex arrays
    for vertex in he_mesh.vertices() {
        positions.push(vertex.position.to_array());
        normals.push(vertex.normal.to_array());
    }

    let num_positions = positions.len() as u32;

    // Build index array from faces
    let mut skipped_faces = 0;
    for face in he_mesh.faces() {
        let verts = he_mesh.get_face_vertices(face.id);
        if verts.len() >= 3 {
            // Validate: check that all vertex IDs are valid indices
            let mut valid = true;
            for v in &verts {
                if v.0 >= num_positions {
                    warn!(
                        "half_edge_to_bevy_mesh: face {:?} references vertex {:?} \
                         but only {} positions exist. Face will be skipped.",
                        face.id, v, num_positions
                    );
                    valid = false;
                    break;
                }
            }

            if valid {
                // Triangulate the face (assuming convex)
                for i in 1..(verts.len() - 1) {
                    indices.push(verts[0].0);
                    indices.push(verts[i].0);
                    indices.push(verts[i + 1].0);
                }
            } else {
                skipped_faces += 1;
            }
        } else {
            skipped_faces += 1;
            trace!(
                "half_edge_to_bevy_mesh: face {:?} has {} vertices, skipping",
                face.id,
                verts.len()
            );
        }
    }

    if skipped_faces > 0 {
        warn!(
            "half_edge_to_bevy_mesh: skipped {} faces due to invalid vertices or insufficient vertex count",
            skipped_faces
        );
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        bevy::asset::RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_indices(Indices::U32(indices));

    Some(mesh)
}

/// Update brush indicator position and visibility
fn update_brush_indicator(
    sculpt_state: Res<SculptState>,
    mut indicator_query: Query<(&mut Transform, &mut Visibility), With<SculptBrushIndicator>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mesh_query: Query<(&Mesh3d, &GlobalTransform)>,
    meshes: Res<Assets<Mesh>>,
) {
    let Ok((mut transform, mut visibility)) = indicator_query.single_mut() else {
        return;
    };

    if !sculpt_state.active {
        *visibility = Visibility::Hidden;
        return;
    }

    let Some(target_entity) = sculpt_state.target_entity else {
        *visibility = Visibility::Hidden;
        return;
    };

    let Ok((camera, camera_transform)) = camera_query.single() else {
        *visibility = Visibility::Hidden;
        return;
    };

    let Ok(window) = windows.single() else {
        *visibility = Visibility::Hidden;
        return;
    };

    let Some(cursor_pos) = window.cursor_position() else {
        *visibility = Visibility::Hidden;
        return;
    };

    let Ok((mesh_handle, mesh_transform)) = mesh_query.get(target_entity) else {
        *visibility = Visibility::Hidden;
        return;
    };

    let Some(mesh) = meshes.get(&mesh_handle.0) else {
        *visibility = Visibility::Hidden;
        return;
    };

    // Raycast to find position
    if let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_pos) {
        if let Some((world_pos, normal)) = ray_mesh_intersection_simple(&ray, mesh, mesh_transform) {
            *visibility = Visibility::Visible;
            // Offset slightly along normal to avoid z-fighting
            let offset = normal * (sculpt_state.brush_radius * 0.1);
            transform.translation = world_pos + offset;
            transform.scale = Vec3::splat(sculpt_state.brush_radius * 2.0);
        } else {
            *visibility = Visibility::Hidden;
        }
    } else {
        *visibility = Visibility::Hidden;
    }
}
