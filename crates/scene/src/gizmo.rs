//! Transform gizmo system with Blender-style controls
//!
//! Hotkeys:
//! - G = Grab/Move
//! - S = Scale
//! - R = Rotate
//! - X/Y/Z = Axis constraint
//! - Shift+X/Y/Z = Exclude axis (constrain to other two)
//! - Esc = Cancel
//! - Enter/LMB = Confirm

use bevy::gizmos::gizmos::Gizmos;
use bevy::input::mouse::{MouseButton, MouseMotion};
use bevy::math::Isometry3d;
use bevy::prelude::*;
use pentimento_ipc::{GizmoAxis, GizmoMode};

#[cfg(feature = "selection")]
use crate::selection::{Selected, SelectionState};

/// Resource tracking current gizmo state
#[derive(Resource)]
pub struct GizmoState {
    /// Current transform mode
    pub mode: GizmoMode,
    /// Axis constraint for the operation
    pub axis_constraint: GizmoAxis,
    /// Whether a transform operation is currently active
    pub is_active: bool,
    /// Original transforms before operation started (for cancel)
    original_transforms: Vec<(Entity, Transform)>,
    /// Accumulated mouse delta during operation
    accumulated_delta: Vec2,
    /// Initial mouse position when operation started
    start_mouse_pos: Vec2,
}

impl Default for GizmoState {
    fn default() -> Self {
        Self {
            mode: GizmoMode::None,
            axis_constraint: GizmoAxis::None,
            is_active: false,
            original_transforms: Vec::new(),
            accumulated_delta: Vec2::ZERO,
            start_mouse_pos: Vec2::ZERO,
        }
    }
}

impl GizmoState {
    /// Start a new transform operation
    fn start_operation(&mut self, mode: GizmoMode) {
        self.mode = mode;
        self.axis_constraint = GizmoAxis::None;
        self.is_active = true;
        self.accumulated_delta = Vec2::ZERO;
    }

    /// Cancel the current operation and restore original transforms
    fn cancel(&mut self) {
        self.mode = GizmoMode::None;
        self.axis_constraint = GizmoAxis::None;
        self.is_active = false;
        // original_transforms will be used by the system to restore
    }

    /// Confirm the current operation
    fn confirm(&mut self) {
        self.mode = GizmoMode::None;
        self.axis_constraint = GizmoAxis::None;
        self.is_active = false;
        self.original_transforms.clear();
    }
}

/// Plugin for transform gizmos
pub struct GizmoPlugin;

impl Plugin for GizmoPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GizmoState>();

        // Only add gizmo systems if selection feature is enabled
        #[cfg(feature = "selection")]
        app.add_systems(
            Update,
            (
                handle_gizmo_hotkeys,
                handle_gizmo_mouse_input.after(handle_gizmo_hotkeys),
                apply_gizmo_transform.after(handle_gizmo_mouse_input),
                render_gizmo.after(apply_gizmo_transform),
            ),
        );
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

    // Handle axis constraints (X/Y/Z or Shift+X/Y/Z)
    if key_input.just_pressed(KeyCode::KeyX) {
        gizmo_state.axis_constraint = if shift_held {
            GizmoAxis::YZ // Exclude X = constrain to YZ plane
        } else {
            GizmoAxis::X
        };
        info!("Gizmo: Axis constraint {:?}", gizmo_state.axis_constraint);
    }
    if key_input.just_pressed(KeyCode::KeyY) {
        gizmo_state.axis_constraint = if shift_held {
            GizmoAxis::XZ // Exclude Y = constrain to XZ plane
        } else {
            GizmoAxis::Y
        };
        info!("Gizmo: Axis constraint {:?}", gizmo_state.axis_constraint);
    }
    if key_input.just_pressed(KeyCode::KeyZ) {
        gizmo_state.axis_constraint = if shift_held {
            GizmoAxis::XY // Exclude Z = constrain to XY plane
        } else {
            GizmoAxis::Z
        };
        info!("Gizmo: Axis constraint {:?}", gizmo_state.axis_constraint);
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

    // Confirm with Enter or left click
    if key_input.just_pressed(KeyCode::Enter) || mouse_button.just_pressed(MouseButton::Left) {
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

/// Apply gizmo transform to selected objects
/// Uses Blender-style behavior: transforms are calculated relative to original positions,
/// so the object position is always original_position + (total_mouse_delta * sensitivity).
/// This gives smooth, predictable movement without acceleration.
#[cfg(feature = "selection")]
fn apply_gizmo_transform(
    gizmo_state: Res<GizmoState>,
    mut selected_query: Query<(Entity, &mut Transform), With<Selected>>,
) {
    if !gizmo_state.is_active {
        return;
    }

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
                // Calculate offset from original position based on total mouse delta
                let movement = apply_axis_constraint(
                    Vec3::new(delta.x, -delta.y, 0.0) * sensitivity,
                    gizmo_state.axis_constraint,
                );
                // Set position = original + offset (not incremental!)
                transform.translation = original.translation + movement;
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
                // Set scale = original * multiplier (not incremental!)
                transform.scale = original.scale * scale_multiplier;
            }
            GizmoMode::Rotate => {
                // Rotation based on total mouse X movement
                let rotation_amount = delta.x * sensitivity;
                let rotation = match gizmo_state.axis_constraint {
                    GizmoAxis::None | GizmoAxis::Y | GizmoAxis::XZ => {
                        Quat::from_rotation_y(rotation_amount)
                    }
                    GizmoAxis::X | GizmoAxis::YZ => Quat::from_rotation_x(rotation_amount),
                    GizmoAxis::Z | GizmoAxis::XY => Quat::from_rotation_z(rotation_amount),
                };
                // Set rotation = delta_rotation * original (not incremental!)
                transform.rotation = rotation * original.rotation;
            }
            GizmoMode::None => {}
        }
    }
}

/// Apply axis constraint to a vector
fn apply_axis_constraint(v: Vec3, constraint: GizmoAxis) -> Vec3 {
    match constraint {
        GizmoAxis::None => v,
        GizmoAxis::X => Vec3::new(v.x, 0.0, 0.0),
        GizmoAxis::Y => Vec3::new(0.0, v.y, 0.0),
        GizmoAxis::Z => Vec3::new(0.0, 0.0, v.z),
        GizmoAxis::XY => Vec3::new(v.x, v.y, 0.0),
        GizmoAxis::XZ => Vec3::new(v.x, 0.0, v.z),
        GizmoAxis::YZ => Vec3::new(0.0, v.y, v.z),
    }
}

/// Render gizmo visualization using Bevy's gizmos API
#[cfg(feature = "selection")]
fn render_gizmo(
    gizmo_state: Res<GizmoState>,
    mut gizmos: Gizmos,
    selected_query: Query<&Transform, With<Selected>>,
) {
    if gizmo_state.mode == GizmoMode::None {
        return;
    }

    // Calculate center of selection
    let mut center = Vec3::ZERO;
    let mut count = 0;
    for transform in selected_query.iter() {
        center += transform.translation;
        count += 1;
    }
    if count == 0 {
        return;
    }
    center /= count as f32;

    // Get colors based on axis constraint
    let (x_color, y_color, z_color) = get_axis_colors(gizmo_state.axis_constraint);

    match gizmo_state.mode {
        GizmoMode::Translate => {
            // Draw translation arrows
            let arrow_length = 1.5;
            gizmos.arrow(center, center + Vec3::X * arrow_length, x_color);
            gizmos.arrow(center, center + Vec3::Y * arrow_length, y_color);
            gizmos.arrow(center, center + Vec3::Z * arrow_length, z_color);
        }
        GizmoMode::Rotate => {
            // Draw rotation circles
            let radius = 1.0;
            gizmos.circle(
                Isometry3d::new(center, Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)),
                radius,
                x_color,
            );
            gizmos.circle(
                Isometry3d::new(center, Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                radius,
                y_color,
            );
            gizmos.circle(Isometry3d::new(center, Quat::IDENTITY), radius, z_color);
        }
        GizmoMode::Scale => {
            // Draw scale handles (lines with boxes at end)
            let handle_dist = 1.0;
            let box_size = 0.1;

            gizmos.line(center, center + Vec3::X * handle_dist, x_color);
            gizmos.line(center, center + Vec3::Y * handle_dist, y_color);
            gizmos.line(center, center + Vec3::Z * handle_dist, z_color);

            // Draw small cubes at the ends
            gizmos.sphere(
                Isometry3d::from_translation(center + Vec3::X * handle_dist),
                box_size,
                x_color,
            );
            gizmos.sphere(
                Isometry3d::from_translation(center + Vec3::Y * handle_dist),
                box_size,
                y_color,
            );
            gizmos.sphere(
                Isometry3d::from_translation(center + Vec3::Z * handle_dist),
                box_size,
                z_color,
            );
        }
        GizmoMode::None => {}
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
