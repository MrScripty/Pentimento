//! Gizmo transform application and projection math

use bevy::prelude::*;
use pentimento_ipc::{CoordinateSpace, GizmoAxis, GizmoMode};

#[cfg(feature = "selection")]
use crate::selection::Selected;
#[cfg(feature = "selection")]
use crate::MainCamera;

use super::state::GizmoState;

/// Calculate the gizmo center and orientation from selected objects
#[cfg(feature = "selection")]
pub(crate) fn get_gizmo_transform(
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
pub(crate) fn apply_gizmo_transform(
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
