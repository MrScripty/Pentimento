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
use bevy::window::{PrimaryWindow, WindowEvent};

use crate::camera::MainCamera;
use crate::canvas_plane::{ActiveCanvasPlane, CanvasPlane};
use crate::frontend_input::{FrontendInputBlockState, FrontendScenePointerInput};

/// Resource tracking paint tool state
#[derive(Resource, Default)]
pub struct PaintMode {
    pub target: pentimento_ipc::PaintTarget,
    pub direct_target: Option<Entity>,
    pub target_notice: Option<String>,
    pub direct_source_entity: Option<Entity>,
    pub direct_source_visibility: Option<Visibility>,
    pub direct_camera_locked: Option<bool>,
    /// Whether paint mode is currently active
    pub active: bool,
    /// Current stroke state, if a stroke is in progress
    pub current_stroke: Option<StrokeState>,
    /// One-shot canvas sampler; a press remains consumed until release.
    pub sample_color: bool,
    pub sample_source: pentimento_ipc::ColorSampleSource,
    pub sample_press_owned: bool,
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
    pub(crate) fn document_next_id(&self) -> u64 {
        self.next_id
    }
    pub(crate) fn from_document(next_id: u64) -> Self {
        Self { next_id }
    }
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

/// Handle paint mode toggle (Shift+Tab)
pub(crate) fn handle_paint_mode_toggle(
    key_input: Res<ButtonInput<KeyCode>>,
    mut paint_mode: ResMut<PaintMode>,
    edit_mode: Res<crate::EditModeState>,
    mut paint_events: MessageWriter<PaintEvent>,
    mut outbound: ResMut<crate::OutboundUiMessages>,
    input_blocks: Res<FrontendInputBlockState>,
) {
    if input_blocks.blocks_keyboard() {
        return;
    }

    // Shift+Tab to toggle paint mode
    let shift = key_input.pressed(KeyCode::ShiftLeft) || key_input.pressed(KeyCode::ShiftRight);
    let tab = key_input.just_pressed(KeyCode::Tab);

    if shift
        && tab
        && !matches!(
            edit_mode.mode,
            pentimento_ipc::EditMode::Sculpt | pentimento_ipc::EditMode::MeshEdit
        )
    {
        paint_mode.active = !paint_mode.active;
        info!(
            "Paint mode {}",
            if paint_mode.active {
                "enabled"
            } else {
                "disabled"
            }
        );

        // Notify UI of mode change
        let mode = if paint_mode.active {
            pentimento_ipc::EditMode::Paint
        } else {
            pentimento_ipc::EditMode::None
        };
        outbound.send(pentimento_ipc::BevyToUi::EditModeChanged { mode });

        // Cancel any in-progress stroke when toggling off
        if !paint_mode.active && paint_mode.current_stroke.is_some() {
            paint_events.write(PaintEvent::StrokeCancel);
            paint_mode.current_stroke = None;
        }
        if !paint_mode.active {
            paint_mode.sample_color = false;
        }
    }
}

/// Handle paint input (left mouse button for strokes)
pub(super) fn handle_paint_input(
    mouse_button: Res<ButtonInput<MouseButton>>,
    windows: Query<(Entity, &Window), With<PrimaryWindow>>,
    mut window_events: MessageReader<WindowEvent>,
    mut last_cursor: Local<(Option<Vec2>, u64)>,
    project: Option<Res<crate::project::ProjectState>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    plane_query: Query<(&GlobalTransform, &CanvasPlane)>,
    active_plane: Res<ActiveCanvasPlane>,
    mut paint_mode: ResMut<PaintMode>,
    mut stroke_id_gen: ResMut<StrokeIdGenerator>,
    mut paint_events: MessageWriter<PaintEvent>,
    time: Res<Time>,
    input_blocks: Res<FrontendInputBlockState>,
    scene_input: Option<Res<FrontendScenePointerInput>>,
    mut painting: ResMut<crate::PaintingResource>,
    mut outbound: ResMut<crate::OutboundUiMessages>,
) {
    let generation = project.as_ref().map_or(0, |p| p.generation);
    if last_cursor.1 != generation {
        *last_cursor = (None, generation);
    }
    let Ok((window_entity, window)) = windows.single() else {
        window_events.clear();
        return;
    };
    // Consume every frame, including blocked/inactive frames, to prevent replay.
    let raw_batch: Vec<_> = window_events.read().cloned().collect();
    let arbitrated = scene_input
        .as_ref()
        .and_then(|input| input.events(window_entity));
    let batch = arbitrated.map_or(raw_batch, |events| events.to_vec());
    let mut cursor = last_cursor.0;
    let mut has_movement = false;
    for event in &batch {
        if let WindowEvent::CursorMoved(event) = event {
            if event.window == window_entity {
                last_cursor.0 = Some(event.position);
                has_movement = true;
            }
        }
    }
    if cursor.is_none() && !has_movement {
        cursor = window.cursor_position();
    }
    // Window holds the final batch focus. An ordered focus transition lets us
    // finish earlier valid input before processing the loss itself.
    let first_focus = batch.iter().find_map(|event| match event {
        WindowEvent::WindowFocused(event) if event.window == window_entity => Some(event.focused),
        _ => None,
    });
    if (arbitrated.is_none() && input_blocks.blocks_pointer())
        || (!window.focused && first_focus.is_none())
    {
        if !mouse_button.pressed(MouseButton::Left) {
            paint_mode.sample_press_owned = false;
        }
        if paint_mode.current_stroke.take().is_some() {
            paint_events.write(PaintEvent::StrokeEnd);
        }
        if !window.focused {
            last_cursor.0 = None;
            paint_mode.sample_color = false;
            paint_mode.sample_press_owned = false;
        }
        return;
    }
    if !paint_mode.active || paint_mode.target != pentimento_ipc::PaintTarget::Canvas {
        paint_mode.sample_color = false;
        if !mouse_button.pressed(MouseButton::Left) {
            paint_mode.sample_press_owned = false;
        }
        return;
    }
    let Some(plane_entity) = active_plane.entity else {
        return;
    };
    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };
    let Ok((plane_transform, canvas_plane)) = plane_query.get(plane_entity) else {
        return;
    };
    let hit = |position| {
        camera
            .viewport_to_world(camera_transform, position)
            .ok()
            .and_then(|ray| {
                ray_plane_intersection(
                    ray,
                    plane_transform,
                    canvas_plane.world_width,
                    canvas_plane.world_height,
                )
            })
    };
    let current_time = time.elapsed_secs_f64();
    let mut moved = false;
    let mut began = false;
    let mut focus_lost = first_focus.map_or(!window.focused, |focused| focused);
    for event in batch {
        match event {
            WindowEvent::CursorMoved(event) if event.window == window_entity => {
                cursor = (!focus_lost).then_some(event.position);
                last_cursor.0 = cursor;
                if let Some(state) = paint_mode.current_stroke.as_mut() {
                    if let Some((world_pos, uv_pos)) = hit(event.position) {
                        emit_move(state, world_pos, uv_pos, current_time, &mut paint_events);
                        moved = true;
                    }
                }
            }
            WindowEvent::MouseButtonInput(event)
                if event.window == window_entity && event.button == MouseButton::Left =>
            {
                if event.state == bevy::input::ButtonState::Pressed {
                    if !focus_lost
                        && paint_mode.current_stroke.is_none()
                        && !paint_mode.sample_press_owned
                    {
                        if paint_mode.sample_color {
                            paint_mode.sample_press_owned = true;
                            if let Some((_, uv_pos)) = cursor.and_then(hit) {
                                if painting.sample_brush_color(
                                    canvas_plane,
                                    uv_pos,
                                    paint_mode.sample_source,
                                ) {
                                    paint_mode.sample_color = false;
                                } else {
                                    outbound.send(pentimento_ipc::BevyToUi::Error {
                                        code: "color_sample_rejected".into(),
                                        message: "No painted color at this source pixel. Choose another pixel or cancel sampling.".into(),
                                    });
                                }
                            }
                            continue;
                        }
                        if let Some((world_pos, uv_pos)) = cursor.and_then(hit) {
                            let stroke_id = stroke_id_gen.next();
                            let space_id = canvas_plane.plane_id;
                            paint_mode.current_stroke = Some(StrokeState {
                                stroke_id,
                                space_id,
                                start_time: (current_time * 1000.) as u64,
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
                            began = true;
                            info!(
                                "Stroke started: id={}, pos={:?}, uv={:?}",
                                stroke_id, world_pos, uv_pos
                            );
                        }
                    }
                } else {
                    paint_mode.sample_press_owned = false;
                    if paint_mode.current_stroke.take().is_some() {
                        paint_events.write(PaintEvent::StrokeEnd);
                        info!("Stroke ended");
                    }
                }
            }
            WindowEvent::WindowFocused(event) if event.window == window_entity => {
                focus_lost = !event.focused;
                if focus_lost {
                    paint_mode.sample_color = false;
                    paint_mode.sample_press_owned = false;
                    if paint_mode.current_stroke.take().is_some() {
                        paint_events.write(PaintEvent::StrokeEnd);
                    }
                    cursor = None;
                    last_cursor.0 = None;
                }
            }
            WindowEvent::KeyboardInput(event)
                if event.window == window_entity
                    && event.key_code == KeyCode::Escape
                    && event.state.is_pressed()
                    && !focus_lost
                    && !input_blocks.blocks_keyboard() =>
            {
                paint_mode.sample_color = false;
                if paint_mode.current_stroke.take().is_some() {
                    paint_events.write(PaintEvent::StrokeCancel);
                    info!("Stroke cancelled");
                }
            }
            _ => {}
        }
    }
    if !mouse_button.pressed(MouseButton::Left) {
        paint_mode.sample_press_owned = false;
        // A host reset may release ownership without delivering a button event.
        if paint_mode.current_stroke.take().is_some() {
            paint_events.write(PaintEvent::StrokeEnd);
        }
    } else if !began && !moved {
        if let Some(state) = paint_mode.current_stroke.as_mut() {
            if let Some((world_pos, uv_pos)) = cursor.and_then(hit) {
                emit_move(state, world_pos, uv_pos, current_time, &mut paint_events);
            }
        }
    }
}

fn emit_move(
    state: &mut StrokeState,
    world_pos: Vec3,
    uv_pos: Vec2,
    current_time: f64,
    events: &mut MessageWriter<PaintEvent>,
) {
    let dt = (current_time - state.last_time) as f32;
    let speed = state
        .last_world_pos
        .filter(|_| dt > 0.)
        .map_or(0., |last| world_pos.distance(last) / dt);
    state.last_world_pos = Some(world_pos);
    state.last_time = current_time;
    events.write(PaintEvent::StrokeMove {
        world_pos,
        uv_pos,
        pressure: 1.,
        speed,
    });
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

#[cfg(test)]
mod input_routing_tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn entering_ui_ends_stroke_without_painting_across_the_panel() {
        let mut world = World::new();
        world.init_resource::<ButtonInput<MouseButton>>();
        world
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        world.init_resource::<Messages<WindowEvent>>();
        world.spawn((Window::default(), PrimaryWindow));
        world.init_resource::<Messages<PaintEvent>>();
        world.init_resource::<ActiveCanvasPlane>();
        world.init_resource::<StrokeIdGenerator>();
        world.init_resource::<Time>();
        world.insert_resource(FrontendInputBlockState {
            block_pointer: true,
            block_keyboard: false,
        });
        world.insert_resource(PaintMode {
            active: true,
            current_stroke: Some(StrokeState {
                stroke_id: 1,
                space_id: 1,
                start_time: 0,
                last_world_pos: None,
                last_time: 0.0,
            }),
            ..default()
        });
        world.init_resource::<crate::PaintingResource>();
        world.init_resource::<crate::OutboundUiMessages>();
        world.run_system_once(handle_paint_input).unwrap();
        assert!(world.resource::<PaintMode>().current_stroke.is_none());
        let events: Vec<_> = world
            .resource_mut::<Messages<PaintEvent>>()
            .drain()
            .collect();
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], PaintEvent::StrokeEnd));
    }
}
