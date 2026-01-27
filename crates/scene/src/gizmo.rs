//! Transform gizmo system with Blender-style controls
//!
//! Hotkeys:
//! - G = Grab/Move
//! - S = Scale
//! - R = Rotate (press again to switch to Orbit, third press cancels)
//! - X/Y/Z = Axis constraint (press once for Global, twice for Local, thrice to clear)
//! - Shift+X/Y/Z = Plane constraint (exclude that axis, e.g. Shift+Z = XY plane)
//! - Esc = Cancel
//! - Enter/LMB = Confirm
//!
//! Axis constraint toggle (like Blender):
//! - First press (e.g., X): Constrain to Global X axis
//! - Second press (X again): Switch to Local X axis (object-relative)
//! - Third press (X again): Remove constraint
//!
//! Rotation mode toggle:
//! - First R: Rotate (single-axis rotation based on constraint)
//! - Second R: Trackball (free rotation - horizontal mouse = Y, vertical mouse = X)
//! - Third R: Cancel operation

use bevy::gizmos::gizmos::Gizmos;
use bevy::input::mouse::{MouseButton, MouseMotion};
use bevy::math::Isometry3d;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use pentimento_ipc::{CoordinateSpace, GizmoAxis, GizmoMode};

#[cfg(feature = "selection")]
use crate::gizmo_raycast::{raycast_gizmo, GizmoGeometry, GizmoHandle};
#[cfg(feature = "selection")]
use crate::selection::{Selected, SelectionState};
#[cfg(feature = "selection")]
use crate::MainCamera;

/// Resource tracking current gizmo state
#[derive(Resource)]
pub struct GizmoState {
    /// Current transform mode
    pub mode: GizmoMode,
    /// Axis constraint for the operation
    pub axis_constraint: GizmoAxis,
    /// Coordinate space (global vs local)
    pub coordinate_space: CoordinateSpace,
    /// Last single-axis pressed (for toggle detection: X→Local X→None)
    last_axis_pressed: Option<GizmoAxis>,
    /// Whether a transform operation is currently active
    pub is_active: bool,
    /// Original transforms before operation started (for cancel)
    original_transforms: Vec<(Entity, Transform)>,
    /// Accumulated mouse delta during operation
    accumulated_delta: Vec2,
    /// Currently hovered handle (for visual feedback)
    #[cfg(feature = "selection")]
    pub hovered_handle: GizmoHandle,
    /// Hit point of currently hovered handle (stored for click handling)
    #[cfg(feature = "selection")]
    hovered_hit_point: Option<Vec3>,
    /// Currently active (being dragged) handle
    #[cfg(feature = "selection")]
    pub active_handle: GizmoHandle,
    /// World-space point where user grabbed a rotation ring (for tangent calculation)
    #[cfg(feature = "selection")]
    pub rotation_grab_point: Option<Vec3>,
    /// Whether gizmo should always be visible when selection exists
    pub always_visible: bool,
}

impl Default for GizmoState {
    fn default() -> Self {
        Self {
            mode: GizmoMode::None,
            axis_constraint: GizmoAxis::None,
            coordinate_space: CoordinateSpace::Global,
            last_axis_pressed: None,
            is_active: false,
            original_transforms: Vec::new(),
            accumulated_delta: Vec2::ZERO,
            #[cfg(feature = "selection")]
            hovered_handle: GizmoHandle::None,
            #[cfg(feature = "selection")]
            hovered_hit_point: None,
            #[cfg(feature = "selection")]
            active_handle: GizmoHandle::None,
            #[cfg(feature = "selection")]
            rotation_grab_point: None,
            always_visible: true,
        }
    }
}

impl GizmoState {
    /// Start a new transform operation
    fn start_operation(&mut self, mode: GizmoMode) {
        self.mode = mode;
        self.axis_constraint = GizmoAxis::None;
        self.coordinate_space = CoordinateSpace::Global;
        self.last_axis_pressed = None;
        self.is_active = true;
        self.accumulated_delta = Vec2::ZERO;
    }

    /// Start a transform operation from a handle click
    #[cfg(feature = "selection")]
    fn start_operation_from_handle(&mut self, handle: GizmoHandle) {
        let (mode, axis) = match handle {
            GizmoHandle::TranslateX => (GizmoMode::Translate, GizmoAxis::X),
            GizmoHandle::TranslateY => (GizmoMode::Translate, GizmoAxis::Y),
            GizmoHandle::TranslateZ => (GizmoMode::Translate, GizmoAxis::Z),
            GizmoHandle::RotateX => (GizmoMode::Rotate, GizmoAxis::X),
            GizmoHandle::RotateY => (GizmoMode::Rotate, GizmoAxis::Y),
            GizmoHandle::RotateZ => (GizmoMode::Rotate, GizmoAxis::Z),
            GizmoHandle::ScaleX => (GizmoMode::Scale, GizmoAxis::X),
            GizmoHandle::ScaleY => (GizmoMode::Scale, GizmoAxis::Y),
            GizmoHandle::ScaleZ => (GizmoMode::Scale, GizmoAxis::Z),
            GizmoHandle::ScaleUniform => (GizmoMode::Scale, GizmoAxis::None),
            GizmoHandle::None => return,
        };
        self.mode = mode;
        self.axis_constraint = axis;
        self.coordinate_space = CoordinateSpace::Global;
        self.last_axis_pressed = None;
        self.is_active = true;
        self.accumulated_delta = Vec2::ZERO;
        self.active_handle = handle;
    }

    /// Cancel the current operation and restore original transforms
    fn cancel(&mut self) {
        self.mode = GizmoMode::None;
        self.axis_constraint = GizmoAxis::None;
        self.coordinate_space = CoordinateSpace::Global;
        self.last_axis_pressed = None;
        self.is_active = false;
        #[cfg(feature = "selection")]
        {
            self.active_handle = GizmoHandle::None;
            self.rotation_grab_point = None;
        }
        // original_transforms will be used by the system to restore
    }

    /// Confirm the current operation
    fn confirm(&mut self) {
        self.mode = GizmoMode::None;
        self.axis_constraint = GizmoAxis::None;
        self.coordinate_space = CoordinateSpace::Global;
        self.last_axis_pressed = None;
        self.is_active = false;
        self.original_transforms.clear();
        #[cfg(feature = "selection")]
        {
            self.active_handle = GizmoHandle::None;
            self.rotation_grab_point = None;
        }
    }
}

/// Handle axis key press with Blender-style toggle behavior.
/// - First press: constrain to Global axis
/// - Second press (same axis): switch to Local axis
/// - Third press (same axis): remove constraint
/// - Shift+axis: constrain to plane (exclude that axis)
/// - Different axis: switch to new axis in Global space
fn handle_axis_key(gizmo_state: &mut GizmoState, axis: GizmoAxis, shift_held: bool) {
    if shift_held {
        // Shift+axis = plane constraint (exclude that axis)
        let plane_constraint = match axis {
            GizmoAxis::X => GizmoAxis::YZ,
            GizmoAxis::Y => GizmoAxis::XZ,
            GizmoAxis::Z => GizmoAxis::XY,
            other => other, // Shouldn't happen, but handle gracefully
        };
        gizmo_state.axis_constraint = plane_constraint;
        gizmo_state.coordinate_space = CoordinateSpace::Global;
        gizmo_state.last_axis_pressed = Some(plane_constraint);
        info!(
            "Gizmo: Plane constraint {:?} (Global)",
            gizmo_state.axis_constraint
        );
    } else if gizmo_state.last_axis_pressed == Some(axis) {
        // Same axis pressed again - toggle coordinate space or clear
        match gizmo_state.coordinate_space {
            CoordinateSpace::Global => {
                // Switch to Local
                gizmo_state.coordinate_space = CoordinateSpace::Local;
                info!(
                    "Gizmo: Axis constraint {:?} (Local)",
                    gizmo_state.axis_constraint
                );
            }
            CoordinateSpace::Local => {
                // Third press - remove constraint
                gizmo_state.axis_constraint = GizmoAxis::None;
                gizmo_state.coordinate_space = CoordinateSpace::Global;
                gizmo_state.last_axis_pressed = None;
                info!("Gizmo: Axis constraint removed");
            }
        }
    } else {
        // Different axis - set new constraint in Global space
        gizmo_state.axis_constraint = axis;
        gizmo_state.coordinate_space = CoordinateSpace::Global;
        gizmo_state.last_axis_pressed = Some(axis);
        info!(
            "Gizmo: Axis constraint {:?} (Global)",
            gizmo_state.axis_constraint
        );
    }
}

/// Plugin for transform gizmos
pub struct GizmoPlugin;

impl Plugin for GizmoPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GizmoState>();

        // Only add gizmo systems if selection feature is enabled
        #[cfg(feature = "selection")]
        {
            app.init_resource::<GizmoGeometry>();
            app.add_systems(
                Update,
                (
                    detect_gizmo_hover,
                    handle_gizmo_click.after(detect_gizmo_hover),
                    handle_gizmo_hotkeys.after(handle_gizmo_click),
                    handle_gizmo_mouse_input.after(handle_gizmo_hotkeys),
                    apply_gizmo_transform.after(handle_gizmo_mouse_input),
                    render_gizmo.after(apply_gizmo_transform),
                ),
            );
        }
    }
}

/// Calculate the gizmo center and orientation from selected objects
#[cfg(feature = "selection")]
fn get_gizmo_transform(
    selected_query: &Query<&Transform, With<Selected>>,
    coordinate_space: CoordinateSpace,
) -> Option<(Vec3, Quat)> {
    let mut center = Vec3::ZERO;
    let mut count = 0;
    let mut first_rotation = Quat::IDENTITY;

    for (i, transform) in selected_query.iter().enumerate() {
        center += transform.translation;
        count += 1;
        if i == 0 {
            first_rotation = transform.rotation;
        }
    }

    if count == 0 {
        return None;
    }

    center /= count as f32;
    let orientation = match coordinate_space {
        CoordinateSpace::Global => Quat::IDENTITY,
        CoordinateSpace::Local => first_rotation,
    };

    Some((center, orientation))
}

/// Detect which gizmo handle the cursor is hovering over
#[cfg(feature = "selection")]
fn detect_gizmo_hover(
    window_query: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    selected_query: Query<&Transform, With<Selected>>,
    selection: Res<SelectionState>,
    mut gizmo_state: ResMut<GizmoState>,
    geometry: Res<GizmoGeometry>,
) {
    // Don't update hover while dragging
    if gizmo_state.active_handle != GizmoHandle::None {
        return;
    }

    // Reset hover state
    gizmo_state.hovered_handle = GizmoHandle::None;
    gizmo_state.hovered_hit_point = None;

    // Only show gizmo if something is selected and always_visible is true
    if selection.selected_ids.is_empty() || !gizmo_state.always_visible {
        return;
    }

    // Get cursor position
    let Ok(window) = window_query.single() else {
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };

    // Get camera
    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };

    // Create ray from cursor
    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_pos) else {
        return;
    };

    // Get gizmo center and orientation
    let Some((gizmo_center, gizmo_orientation)) =
        get_gizmo_transform(&selected_query, gizmo_state.coordinate_space)
    else {
        return;
    };

    // Raycast against gizmo handles
    if let Some(hit) = raycast_gizmo(
        ray.origin,
        ray.direction.into(),
        gizmo_center,
        gizmo_orientation,
        &geometry,
    ) {
        gizmo_state.hovered_handle = hit.handle;
        gizmo_state.hovered_hit_point = Some(hit.hit_point);
    }
}

/// Handle mouse clicks on gizmo handles
#[cfg(feature = "selection")]
fn handle_gizmo_click(
    mouse_button: Res<ButtonInput<MouseButton>>,
    mut gizmo_state: ResMut<GizmoState>,
    selected_query: Query<(Entity, &Transform), With<Selected>>,
) {
    // Start drag on mouse down when hovering a handle
    if mouse_button.just_pressed(MouseButton::Left) {
        if gizmo_state.hovered_handle != GizmoHandle::None && !gizmo_state.is_active {
            // Store original transforms for cancel
            gizmo_state.original_transforms = selected_query
                .iter()
                .map(|(e, t)| (e, *t))
                .collect();

            let handle = gizmo_state.hovered_handle;

            // Store grab point for rotation handles (for tangent-based rotation)
            if handle.is_rotate() {
                gizmo_state.rotation_grab_point = gizmo_state.hovered_hit_point;
            }

            gizmo_state.start_operation_from_handle(handle);
            info!("Gizmo: Started {:?} from handle click", handle);
        }
    }

    // End drag on mouse up (only for handle-initiated drags)
    if mouse_button.just_released(MouseButton::Left) {
        if gizmo_state.active_handle != GizmoHandle::None {
            gizmo_state.confirm();
            info!("Gizmo: Handle drag confirmed");
        }
    }
}

/// Handle Blender-style hotkeys for gizmo control
#[cfg(feature = "selection")]
fn handle_gizmo_hotkeys(
    key_input: Res<ButtonInput<KeyCode>>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    mut gizmo_state: ResMut<GizmoState>,
    selection: Res<SelectionState>,
    // Use ParamSet to avoid Query conflict - both queries access Transform
    mut queries: ParamSet<(
        Query<(Entity, &Transform), With<Selected>>,
        Query<&mut Transform>,
    )>,
) {
    // Only process hotkeys if something is selected
    if selection.selected_ids.is_empty() {
        if gizmo_state.is_active {
            gizmo_state.cancel();
        }
        return;
    }

    let shift_held =
        key_input.pressed(KeyCode::ShiftLeft) || key_input.pressed(KeyCode::ShiftRight);

    // If not in active operation, check for mode initiation keys
    if !gizmo_state.is_active {
        if key_input.just_pressed(KeyCode::KeyG) {
            // Store original transforms for potential cancel
            gizmo_state.original_transforms = queries
                .p0()
                .iter()
                .map(|(e, t)| (e, *t))
                .collect();
            gizmo_state.start_operation(GizmoMode::Translate);
            info!("Gizmo: Translate mode activated");
        } else if key_input.just_pressed(KeyCode::KeyS) && !shift_held {
            // S without shift = Scale (Shift+S might be used for other things)
            gizmo_state.original_transforms = queries
                .p0()
                .iter()
                .map(|(e, t)| (e, *t))
                .collect();
            gizmo_state.start_operation(GizmoMode::Scale);
            info!("Gizmo: Scale mode activated");
        } else if key_input.just_pressed(KeyCode::KeyR) {
            gizmo_state.original_transforms = queries
                .p0()
                .iter()
                .map(|(e, t)| (e, *t))
                .collect();
            gizmo_state.start_operation(GizmoMode::Rotate);
            info!("Gizmo: Rotate mode activated");
        }
        return;
    }

    // Handle R toggle: Rotate → Orbit → cancel
    if key_input.just_pressed(KeyCode::KeyR) {
        match gizmo_state.mode {
            GizmoMode::Rotate => {
                gizmo_state.mode = GizmoMode::Trackball;
                gizmo_state.accumulated_delta = Vec2::ZERO; // Reset delta for clean switch
                info!("Gizmo: Trackball mode activated");
            }
            GizmoMode::Trackball => {
                // Third R press - cancel operation
                let originals: Vec<_> = gizmo_state.original_transforms.clone();
                let mut transform_query = queries.p1();
                for (entity, original) in &originals {
                    if let Ok(mut transform) = transform_query.get_mut(*entity) {
                        *transform = *original;
                    }
                }
                gizmo_state.cancel();
                info!("Gizmo: Operation cancelled via R toggle");
                return;
            }
            _ => {}
        }
    }

    // Handle axis constraints (X/Y/Z or Shift+X/Y/Z)
    // Blender-style toggle: X → Global X → Local X → None
    if key_input.just_pressed(KeyCode::KeyX) {
        handle_axis_key(&mut gizmo_state, GizmoAxis::X, shift_held);
    }
    if key_input.just_pressed(KeyCode::KeyY) {
        handle_axis_key(&mut gizmo_state, GizmoAxis::Y, shift_held);
    }
    if key_input.just_pressed(KeyCode::KeyZ) {
        handle_axis_key(&mut gizmo_state, GizmoAxis::Z, shift_held);
    }

    // Cancel with Escape - need to restore original transforms
    if key_input.just_pressed(KeyCode::Escape) {
        // Clone the original transforms so we can iterate over them
        let originals: Vec<_> = gizmo_state.original_transforms.clone();
        // Now we can safely access p1() for mutable transform access
        let mut transform_query = queries.p1();
        for (entity, original) in &originals {
            if let Ok(mut transform) = transform_query.get_mut(*entity) {
                *transform = *original;
            }
        }
        gizmo_state.cancel();
        info!("Gizmo: Operation cancelled");
    }

    // Confirm with Enter or left click (but not for handle-initiated drags - those use mouse release)
    let lmb_confirm = mouse_button.just_pressed(MouseButton::Left)
        && gizmo_state.active_handle == GizmoHandle::None;
    if key_input.just_pressed(KeyCode::Enter) || lmb_confirm {
        gizmo_state.confirm();
        info!("Gizmo: Operation confirmed");
    }
}

/// Handle mouse input during active gizmo operation
#[cfg(feature = "selection")]
fn handle_gizmo_mouse_input(
    mut motion_events: MessageReader<MouseMotion>,
    mut gizmo_state: ResMut<GizmoState>,
) {
    if !gizmo_state.is_active {
        motion_events.clear();
        return;
    }

    let mut delta = Vec2::ZERO;
    for event in motion_events.read() {
        delta += event.delta;
    }

    gizmo_state.accumulated_delta += delta;
}

/// Project a world-space direction to a screen-space direction.
///
/// This is used for tangent-based rotation: we calculate the tangent at the grab point
/// in world space, then project it to screen space to determine how mouse movement
/// maps to rotation.
#[cfg(feature = "selection")]
fn project_to_screen_direction(world_dir: Vec3, camera_transform: &Transform) -> Vec2 {
    // Get camera's view matrix (world-to-camera transform)
    let view = camera_transform.compute_affine().inverse();

    // Transform the direction to view space
    // We only care about direction, so transform as a vector (not a point)
    let view_dir = view.transform_vector3(world_dir);

    // Take the XY components as the screen direction
    // Y is negated because screen Y increases downward
    Vec2::new(view_dir.x, -view_dir.y).normalize_or_zero()
}

/// Apply gizmo transform to selected objects
/// Uses Blender-style behavior: transforms are calculated relative to original positions,
/// so the object position is always original_position + (total_mouse_delta * sensitivity).
/// This gives smooth, predictable movement without acceleration.
/// Movement is camera-relative so objects follow the cursor regardless of view angle.
#[cfg(feature = "selection")]
fn apply_gizmo_transform(
    gizmo_state: Res<GizmoState>,
    mut selected_query: Query<(Entity, &mut Transform), With<Selected>>,
    camera_query: Query<&Transform, (With<MainCamera>, Without<Selected>)>,
) {
    if !gizmo_state.is_active {
        return;
    }

    // Get camera orientation for view-relative movement
    let Ok(camera_transform) = camera_query.single() else {
        return;
    };

    // Camera's view-space axes in world coordinates
    let camera_right = camera_transform.rotation * Vec3::X;
    let camera_up = camera_transform.rotation * Vec3::Y;

    let delta = gizmo_state.accumulated_delta;
    let sensitivity = 0.01;

    // Apply transforms relative to original positions (stored when operation started)
    for (entity, mut transform) in selected_query.iter_mut() {
        // Find the original transform for this entity
        let Some((_e, original)) = gizmo_state
            .original_transforms
            .iter()
            .find(|(e, _)| *e == entity)
        else {
            continue;
        };

        match gizmo_state.mode {
            GizmoMode::Translate => {
                // Camera-relative movement: project mouse input through camera orientation
                // For Local mode, use the object's rotated axis directions
                let (axis_x, axis_y, axis_z) = if gizmo_state.coordinate_space == CoordinateSpace::Local {
                    (
                        original.rotation * Vec3::X,
                        original.rotation * Vec3::Y,
                        original.rotation * Vec3::Z,
                    )
                } else {
                    (Vec3::X, Vec3::Y, Vec3::Z)
                };

                let base_movement = match gizmo_state.axis_constraint {
                    GizmoAxis::None => {
                        // Unconstrained: mouse movement in camera's view plane
                        (camera_right * delta.x + camera_up * -delta.y) * sensitivity
                    }
                    GizmoAxis::X => {
                        // Project mouse movement onto X axis based on camera view
                        let x_from_mouse_x = camera_right.dot(axis_x);
                        let x_from_mouse_y = camera_up.dot(axis_x);
                        axis_x * (delta.x * x_from_mouse_x - delta.y * x_from_mouse_y) * sensitivity
                    }
                    GizmoAxis::Y => {
                        let y_from_mouse_x = camera_right.dot(axis_y);
                        let y_from_mouse_y = camera_up.dot(axis_y);
                        axis_y * (delta.x * y_from_mouse_x - delta.y * y_from_mouse_y) * sensitivity
                    }
                    GizmoAxis::Z => {
                        let z_from_mouse_x = camera_right.dot(axis_z);
                        let z_from_mouse_y = camera_up.dot(axis_z);
                        axis_z * (delta.x * z_from_mouse_x - delta.y * z_from_mouse_y) * sensitivity
                    }
                    GizmoAxis::XY => {
                        // Project onto XY plane using camera orientation
                        let move_x = camera_right.dot(axis_x) * delta.x - camera_up.dot(axis_x) * delta.y;
                        let move_y = camera_right.dot(axis_y) * delta.x - camera_up.dot(axis_y) * delta.y;
                        (axis_x * move_x + axis_y * move_y) * sensitivity
                    }
                    GizmoAxis::XZ => {
                        let move_x = camera_right.dot(axis_x) * delta.x - camera_up.dot(axis_x) * delta.y;
                        let move_z = camera_right.dot(axis_z) * delta.x - camera_up.dot(axis_z) * delta.y;
                        (axis_x * move_x + axis_z * move_z) * sensitivity
                    }
                    GizmoAxis::YZ => {
                        let move_y = camera_right.dot(axis_y) * delta.x - camera_up.dot(axis_y) * delta.y;
                        let move_z = camera_right.dot(axis_z) * delta.x - camera_up.dot(axis_z) * delta.y;
                        (axis_y * move_y + axis_z * move_z) * sensitivity
                    }
                };

                // Set position = original + offset (not incremental!)
                transform.translation = original.translation + base_movement;
            }
            GizmoMode::Scale => {
                // Scale factor based on total mouse X movement
                let scale_factor = 1.0 + delta.x * sensitivity * 0.1;
                let scale_multiplier = match gizmo_state.axis_constraint {
                    GizmoAxis::None => Vec3::splat(scale_factor),
                    GizmoAxis::X => Vec3::new(scale_factor, 1.0, 1.0),
                    GizmoAxis::Y => Vec3::new(1.0, scale_factor, 1.0),
                    GizmoAxis::Z => Vec3::new(1.0, 1.0, scale_factor),
                    GizmoAxis::XY => Vec3::new(scale_factor, scale_factor, 1.0),
                    GizmoAxis::XZ => Vec3::new(scale_factor, 1.0, scale_factor),
                    GizmoAxis::YZ => Vec3::new(1.0, scale_factor, scale_factor),
                };
                // Scale is always in local space (it affects object's own axes)
                // Set scale = original * multiplier (not incremental!)
                transform.scale = original.scale * scale_multiplier;
            }
            GizmoMode::Rotate => {
                // Determine rotation axis based on constraint
                let base_axis = match gizmo_state.axis_constraint {
                    GizmoAxis::None | GizmoAxis::Y | GizmoAxis::XZ => Vec3::Y,
                    GizmoAxis::X | GizmoAxis::YZ => Vec3::X,
                    GizmoAxis::Z | GizmoAxis::XY => Vec3::Z,
                };

                // Get the actual rotation axis (local or global)
                let axis = if gizmo_state.coordinate_space == CoordinateSpace::Local {
                    original.rotation * base_axis
                } else {
                    base_axis
                };

                // Calculate rotation amount based on grab point tangent (for handle drags)
                // or simple horizontal mouse movement (for hotkey activation)
                let rotation_amount = if let Some(grab_point) = gizmo_state.rotation_grab_point {
                    // Tangent-based rotation: direction depends on WHERE on the ring you grabbed
                    // This makes grabbing front and dragging right rotate one way,
                    // while grabbing back and dragging right rotates the opposite way

                    // Calculate tangent at grab point
                    let gizmo_center = original.translation;
                    let radial = (grab_point - gizmo_center).normalize();
                    let tangent = axis.cross(radial).normalize();

                    // Project tangent to screen space
                    let screen_tangent = project_to_screen_direction(tangent, camera_transform);

                    // Mouse movement along tangent direction = rotation
                    // Note: delta.y is negated because screen Y increases downward
                    // The overall result is negated because the cross product (axis × radial)
                    // gives tangent in the opposite direction of positive rotation
                    let mouse_delta = Vec2::new(delta.x, -delta.y);
                    -mouse_delta.dot(screen_tangent) * sensitivity
                } else {
                    // Fallback for hotkey-initiated rotation (no grab point)
                    delta.x * sensitivity
                };

                // Create rotation quaternion
                let rotation = Quat::from_axis_angle(axis, rotation_amount);

                // Set rotation = delta_rotation * original (not incremental!)
                transform.rotation = rotation * original.rotation;
            }
            GizmoMode::Trackball => {
                // Trackball rotation: free rotation based on mouse movement
                // Horizontal mouse → rotate around Y axis
                // Vertical mouse → rotate around X axis
                // Combined gives intuitive "grab and spin" behavior

                // Get axes (local or global)
                let (axis_x, axis_y, axis_z) = if gizmo_state.coordinate_space == CoordinateSpace::Local {
                    (
                        original.rotation * Vec3::X,
                        original.rotation * Vec3::Y,
                        original.rotation * Vec3::Z,
                    )
                } else {
                    (Vec3::X, Vec3::Y, Vec3::Z)
                };

                let rotation_x = -delta.y * sensitivity; // Vertical mouse rotates around X
                let rotation_y = delta.x * sensitivity;  // Horizontal mouse rotates around Y

                // Apply axis constraints if set
                let (rot_x, rot_y) = match gizmo_state.axis_constraint {
                    GizmoAxis::None => (rotation_x, rotation_y),
                    GizmoAxis::X | GizmoAxis::YZ => (rotation_x, 0.0), // Only X rotation
                    GizmoAxis::Y | GizmoAxis::XZ => (0.0, rotation_y), // Only Y rotation
                    GizmoAxis::Z | GizmoAxis::XY => {
                        // Z rotation uses combined mouse movement (like rolling)
                        let rotation_z = (delta.x + delta.y) * sensitivity * 0.5;
                        // Handle Z specially below
                        (rotation_z, 0.0)
                    }
                };

                // Build rotation based on constraint
                let trackball_rotation = match gizmo_state.axis_constraint {
                    GizmoAxis::Z | GizmoAxis::XY => {
                        // Z-axis rotation
                        Quat::from_axis_angle(axis_z, rot_x)
                    }
                    _ => {
                        // Standard trackball: combine X and Y rotations
                        let rot_around_x = Quat::from_axis_angle(axis_x, rot_x);
                        let rot_around_y = Quat::from_axis_angle(axis_y, rot_y);
                        rot_around_y * rot_around_x
                    }
                };

                // Apply trackball rotation to object
                transform.rotation = trackball_rotation * original.rotation;
            }
            GizmoMode::None => {}
        }
    }
}

/// Render gizmo visualization using Bevy's gizmos API
#[cfg(feature = "selection")]
fn render_gizmo(
    gizmo_state: Res<GizmoState>,
    selection: Res<SelectionState>,
    geometry: Res<GizmoGeometry>,
    mut gizmos: Gizmos,
    selected_query: Query<&Transform, With<Selected>>,
) {
    // Determine if we should render the gizmo
    let should_render = gizmo_state.mode != GizmoMode::None
        || (gizmo_state.always_visible && !selection.selected_ids.is_empty());

    if !should_render {
        return;
    }

    // Get gizmo center and orientation
    let Some((center, orientation)) =
        get_gizmo_transform(&selected_query, gizmo_state.coordinate_space)
    else {
        return;
    };

    // Calculate axis directions (rotated for local space)
    let x_axis = orientation * Vec3::X;
    let y_axis = orientation * Vec3::Y;
    let z_axis = orientation * Vec3::Z;

    // Determine what to render based on mode
    let render_all = gizmo_state.mode == GizmoMode::None && gizmo_state.always_visible;
    let render_translate = render_all || gizmo_state.mode == GizmoMode::Translate;
    let render_rotate = render_all
        || gizmo_state.mode == GizmoMode::Rotate
        || gizmo_state.mode == GizmoMode::Trackball;
    let render_scale = render_all || gizmo_state.mode == GizmoMode::Scale;

    // Get colors with hover/active highlighting
    let get_handle_color = |handle: GizmoHandle, base_color: Color| -> Color {
        if gizmo_state.active_handle == handle {
            Color::srgb(1.0, 1.0, 0.2) // Yellow for active
        } else if gizmo_state.hovered_handle == handle {
            // Brighten on hover
            match base_color {
                Color::Srgba(c) => Color::srgb(
                    (c.red * 1.5).min(1.0),
                    (c.green * 1.5).min(1.0),
                    (c.blue * 1.5).min(1.0),
                ),
                _ => base_color,
            }
        } else {
            base_color
        }
    };

    // Base axis colors
    let base_x = Color::srgb(0.9, 0.2, 0.2);
    let base_y = Color::srgb(0.2, 0.9, 0.2);
    let base_z = Color::srgb(0.2, 0.2, 0.9);

    // Apply axis constraint highlighting when in a specific mode
    let (x_base, y_base, z_base) = if gizmo_state.is_active {
        get_axis_colors(gizmo_state.axis_constraint)
    } else {
        (base_x, base_y, base_z)
    };

    // Render translation arrows
    if render_translate {
        let arrow_length = geometry.arrow_length;

        let x_color = get_handle_color(GizmoHandle::TranslateX, x_base);
        let y_color = get_handle_color(GizmoHandle::TranslateY, y_base);
        let z_color = get_handle_color(GizmoHandle::TranslateZ, z_base);

        gizmos.arrow(center, center + x_axis * arrow_length, x_color);
        gizmos.arrow(center, center + y_axis * arrow_length, y_color);
        gizmos.arrow(center, center + z_axis * arrow_length, z_color);
    }

    // Render rotation rings
    if render_rotate {
        let radius = geometry.ring_radius;
        let trackball_scale = if gizmo_state.mode == GizmoMode::Trackball {
            1.2
        } else {
            1.0
        };

        let x_color = get_handle_color(GizmoHandle::RotateX, x_base);
        let y_color = get_handle_color(GizmoHandle::RotateY, y_base);
        let z_color = get_handle_color(GizmoHandle::RotateZ, z_base);

        // X rotation circle: perpendicular to X axis
        gizmos.circle(
            Isometry3d::new(
                center,
                orientation * Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
            ),
            radius * trackball_scale,
            x_color,
        );
        // Y rotation circle: perpendicular to Y axis
        gizmos.circle(
            Isometry3d::new(
                center,
                orientation * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
            ),
            radius * trackball_scale,
            y_color,
        );
        // Z rotation circle: perpendicular to Z axis
        gizmos.circle(
            Isometry3d::new(center, orientation),
            radius * trackball_scale,
            z_color,
        );

        // Draw trackball indicator sphere
        if gizmo_state.mode == GizmoMode::Trackball {
            gizmos.sphere(
                Isometry3d::from_translation(center),
                0.15,
                Color::srgba(1.0, 1.0, 1.0, 0.3),
            );
        }
    }

    // Render scale handles
    if render_scale {
        let handle_dist = geometry.arrow_length;
        let box_size = geometry.scale_cube_size;

        let x_color = get_handle_color(GizmoHandle::ScaleX, x_base);
        let y_color = get_handle_color(GizmoHandle::ScaleY, y_base);
        let z_color = get_handle_color(GizmoHandle::ScaleZ, z_base);
        let center_color = get_handle_color(GizmoHandle::ScaleUniform, Color::srgb(0.9, 0.9, 0.9));

        // Lines from center to scale handles
        gizmos.line(center, center + x_axis * handle_dist, x_color);
        gizmos.line(center, center + y_axis * handle_dist, y_color);
        gizmos.line(center, center + z_axis * handle_dist, z_color);

        // Scale cubes at axis ends
        gizmos.sphere(
            Isometry3d::from_translation(center + x_axis * handle_dist),
            box_size,
            x_color,
        );
        gizmos.sphere(
            Isometry3d::from_translation(center + y_axis * handle_dist),
            box_size,
            y_color,
        );
        gizmos.sphere(
            Isometry3d::from_translation(center + z_axis * handle_dist),
            box_size,
            z_color,
        );

        // Center sphere for uniform scale
        gizmos.sphere(
            Isometry3d::from_translation(center),
            geometry.center_sphere_radius,
            center_color,
        );
    }
}

/// Get axis colors based on current constraint (highlighted axis is brighter)
fn get_axis_colors(constraint: GizmoAxis) -> (Color, Color, Color) {
    let dim = 0.4;
    let bright = 1.0;

    match constraint {
        GizmoAxis::None => (
            Color::srgb(bright, dim, dim),  // X = red
            Color::srgb(dim, bright, dim),  // Y = green
            Color::srgb(dim, dim, bright),  // Z = blue
        ),
        GizmoAxis::X => (
            Color::srgb(bright, bright, dim), // X highlighted
            Color::srgb(dim, dim, dim),
            Color::srgb(dim, dim, dim),
        ),
        GizmoAxis::Y => (
            Color::srgb(dim, dim, dim),
            Color::srgb(bright, bright, dim), // Y highlighted
            Color::srgb(dim, dim, dim),
        ),
        GizmoAxis::Z => (
            Color::srgb(dim, dim, dim),
            Color::srgb(dim, dim, dim),
            Color::srgb(bright, bright, dim), // Z highlighted
        ),
        GizmoAxis::XY => (
            Color::srgb(bright, dim, dim),
            Color::srgb(dim, bright, dim),
            Color::srgb(dim, dim, dim), // Z dimmed
        ),
        GizmoAxis::XZ => (
            Color::srgb(bright, dim, dim),
            Color::srgb(dim, dim, dim), // Y dimmed
            Color::srgb(dim, dim, bright),
        ),
        GizmoAxis::YZ => (
            Color::srgb(dim, dim, dim), // X dimmed
            Color::srgb(dim, bright, dim),
            Color::srgb(dim, dim, bright),
        ),
    }
}
