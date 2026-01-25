//! Paint mode state and input handling
//!
//! This module provides the paint mode resource and handles input for
//! stroke creation. When paint mode is active and a canvas plane is selected,
//! left mouse button starts/continues a stroke, generating PaintEvents.
//!
//! The actual dab generation is handled elsewhere (Phase 3) - this module
//! just emits PaintEvents with world-space positions.

use bevy::ecs::message::Message;
use bevy::input::mouse::MouseButton;
use bevy::prelude::*;
use bevy::window::{CursorMoved, PrimaryWindow};

use crate::camera::MainCamera;
use crate::canvas_plane::{ActiveCanvasPlane, CanvasPlane};

/// Resource tracking paint tool state
#[derive(Resource, Default)]
pub struct PaintMode {
    /// Whether paint mode is currently active
    pub active: bool,
    /// Current stroke state, if a stroke is in progress
    pub current_stroke: Option<StrokeState>,
}

/// State for an in-progress stroke
pub struct StrokeState {
    /// Unique stroke identifier
    pub stroke_id: u64,
    /// Space ID (plane_id) this stroke is targeting
    pub space_id: u32,
    /// Timestamp when stroke started (milliseconds)
    pub start_time: u64,
    /// Last world-space position for delta calculation
    pub last_world_pos: Option<Vec3>,
    /// Last frame time for speed calculation
    pub last_time: f64,
}

/// Resource for generating unique stroke IDs
#[derive(Resource, Default)]
pub struct StrokeIdGenerator {
    next_id: u64,
}

impl StrokeIdGenerator {
    /// Generate the next unique stroke ID
    pub fn next(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

/// Message for painting actions
#[derive(Message, Debug, Clone)]
pub enum PaintEvent {
    /// A stroke has started on a plane
    StrokeStart {
        /// The canvas plane entity
        plane_entity: Entity,
        /// World-space position where stroke started
        world_pos: Vec3,
        /// UV position on the plane (0-1 range)
        uv_pos: Vec2,
        /// Unique stroke ID
        stroke_id: u64,
        /// Space ID (plane_id)
        space_id: u32,
    },
    /// Stroke continues with a new position
    StrokeMove {
        /// World-space position
        world_pos: Vec3,
        /// UV position on the plane (0-1 range)
        uv_pos: Vec2,
        /// Pressure value (0.0-1.0, defaults to 1.0 for mouse)
        pressure: f32,
        /// Speed in world units per second
        speed: f32,
    },
    /// Stroke has ended normally
    StrokeEnd,
    /// Stroke was cancelled (e.g., Escape key)
    StrokeCancel,
}

/// Plugin for paint mode functionality
pub struct PaintModePlugin;

impl Plugin for PaintModePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PaintMode>()
            .init_resource::<StrokeIdGenerator>()
            .add_message::<PaintEvent>()
            .add_systems(
                Update,
                (
                    handle_paint_mode_toggle,
                    handle_paint_input.after(handle_paint_mode_toggle),
                ),
            );
    }
}

/// Handle paint mode toggle (P key)
fn handle_paint_mode_toggle(
    key_input: Res<ButtonInput<KeyCode>>,
    mut paint_mode: ResMut<PaintMode>,
    mut paint_events: MessageWriter<PaintEvent>,
) {
    if key_input.just_pressed(KeyCode::KeyP) {
        paint_mode.active = !paint_mode.active;
        info!(
            "Paint mode {}",
            if paint_mode.active {
                "enabled"
            } else {
                "disabled"
            }
        );

        // Cancel any in-progress stroke when toggling off
        if !paint_mode.active && paint_mode.current_stroke.is_some() {
            paint_events.write(PaintEvent::StrokeCancel);
            paint_mode.current_stroke = None;
        }
    }

    // Escape cancels current stroke
    if key_input.just_pressed(KeyCode::Escape) && paint_mode.current_stroke.is_some() {
        paint_events.write(PaintEvent::StrokeCancel);
        paint_mode.current_stroke = None;
        info!("Stroke cancelled");
    }
}

/// Handle paint input (left mouse button for strokes)
fn handle_paint_input(
    mouse_button: Res<ButtonInput<MouseButton>>,
    windows: Query<(Entity, &Window), With<PrimaryWindow>>,
    mut cursor_events: MessageReader<CursorMoved>,
    camera_query: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    plane_query: Query<(&GlobalTransform, &CanvasPlane)>,
    active_plane: Res<ActiveCanvasPlane>,
    mut paint_mode: ResMut<PaintMode>,
    mut stroke_id_gen: ResMut<StrokeIdGenerator>,
    mut paint_events: MessageWriter<PaintEvent>,
    time: Res<Time>,
) {
    // Only process if paint mode is active and a plane is selected
    if !paint_mode.active {
        return;
    }

    let Some(plane_entity) = active_plane.entity else {
        return;
    };

    // Get camera for ray casting
    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };

    // Get plane transform and component
    let Ok((plane_transform, canvas_plane)) = plane_query.get(plane_entity) else {
        return;
    };

    // Get window and its entity for filtering cursor events
    let Ok((window_entity, window)) = windows.single() else {
        return;
    };

    // Collect all cursor positions from this frame for this window
    let cursor_positions: Vec<Vec2> = cursor_events
        .read()
        .filter(|e| e.window == window_entity)
        .map(|e| e.position)
        .collect();

    let current_time = time.elapsed_secs_f64();

    // Handle stroke start (just pressed) - use current cursor position
    if mouse_button.just_pressed(MouseButton::Left) {
        let cursor_pos = cursor_positions.last().copied().or_else(|| window.cursor_position());
        if let Some(cursor_pos) = cursor_pos {
            let ray = camera.viewport_to_world(camera_transform, cursor_pos);
            if let Some(ray) = ray.ok() {
                let plane_intersection = ray_plane_intersection(
                    ray,
                    plane_transform,
                    canvas_plane.world_width,
                    canvas_plane.world_height,
                );

                if let Some((world_pos, uv_pos)) = plane_intersection {
                    // Start new stroke
                    let stroke_id = stroke_id_gen.next();
                    let space_id = canvas_plane.plane_id;

                    paint_mode.current_stroke = Some(StrokeState {
                        stroke_id,
                        space_id,
                        start_time: (current_time * 1000.0) as u64,
                        last_world_pos: Some(world_pos),
                        last_time: current_time,
                    });

                    paint_events.write(PaintEvent::StrokeStart {
                        plane_entity,
                        world_pos,
                        uv_pos,
                        stroke_id,
                        space_id,
                    });

                    info!(
                        "Stroke started: id={}, pos={:?}, uv={:?}",
                        stroke_id, world_pos, uv_pos
                    );
                }
            }
        }
    } else if mouse_button.pressed(MouseButton::Left) {
        // Continue stroke - process ALL cursor events for smooth input
        if let Some(ref mut stroke_state) = paint_mode.current_stroke {
            // If we have cursor events this frame, process each one
            // This gives us sub-frame input resolution for smooth strokes
            let positions_to_process: Vec<Vec2> = if !cursor_positions.is_empty() {
                cursor_positions
            } else if let Some(pos) = window.cursor_position() {
                // Fallback to current position if no events (cursor stationary)
                vec![pos]
            } else {
                vec![]
            };

            for cursor_pos in positions_to_process {
                let ray = camera.viewport_to_world(camera_transform, cursor_pos);
                let Some(ray) = ray.ok() else {
                    continue;
                };

                let plane_intersection = ray_plane_intersection(
                    ray,
                    plane_transform,
                    canvas_plane.world_width,
                    canvas_plane.world_height,
                );

                if let Some((world_pos, uv_pos)) = plane_intersection {
                    // Calculate speed from position delta and time delta
                    let speed = if let Some(last_pos) = stroke_state.last_world_pos {
                        let distance = world_pos.distance(last_pos);
                        let dt = (current_time - stroke_state.last_time) as f32;
                        if dt > 0.0 {
                            distance / dt
                        } else {
                            0.0
                        }
                    } else {
                        0.0
                    };

                    // Update stroke state
                    stroke_state.last_world_pos = Some(world_pos);
                    stroke_state.last_time = current_time;

                    paint_events.write(PaintEvent::StrokeMove {
                        world_pos,
                        uv_pos,
                        pressure: 1.0, // Default pressure for mouse
                        speed,
                    });
                }
            }
        }
    } else if mouse_button.just_released(MouseButton::Left) {
        // End stroke
        if paint_mode.current_stroke.is_some() {
            paint_events.write(PaintEvent::StrokeEnd);
            paint_mode.current_stroke = None;
            info!("Stroke ended");
        }
    }
}

/// Perform ray-plane intersection
///
/// Returns the world-space intersection point and UV coordinates on the plane.
/// The plane is a Rectangle mesh (XY plane in local space, -Z is forward/normal).
fn ray_plane_intersection(
    ray: Ray3d,
    plane_transform: &GlobalTransform,
    world_width: f32,
    world_height: f32,
) -> Option<(Vec3, Vec2)> {
    // Rectangle mesh is in XY plane, facing -Z (toward camera via looking_at)
    // So the plane normal in world space is the plane's forward direction (local -Z)
    let plane_normal = plane_transform.forward();
    let plane_origin = plane_transform.translation();

    // Ray-plane intersection: t = (plane_origin - ray_origin) . normal / (ray_direction . normal)
    let denom = ray.direction.dot(*plane_normal);

    // Check if ray is parallel to plane
    if denom.abs() < 1e-6 {
        return None;
    }

    let t = (plane_origin - ray.origin).dot(*plane_normal) / denom;

    // Check if intersection is in front of ray
    if t < 0.0 {
        return None;
    }

    let world_pos = ray.origin + *ray.direction * t;

    // Convert to local space to get UV coordinates
    let local_pos = plane_transform
        .affine()
        .inverse()
        .transform_point3(world_pos);

    // Rectangle is in XY plane, UV is based on X and Y
    // local_pos ranges from [-world_width/2, world_width/2] and [-world_height/2, world_height/2]
    // Normalize to UV [0, 1] range, with Y inverted for texture coordinates
    let uv = Vec2::new(
        local_pos.x / world_width + 0.5,
        -local_pos.y / world_height + 0.5,
    );

    Some((world_pos, uv))
}
