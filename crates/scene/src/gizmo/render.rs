//! Gizmo rendering system

use bevy::gizmos::gizmos::Gizmos;
use bevy::math::Isometry3d;
use bevy::prelude::*;
use pentimento_ipc::{GizmoAxis, GizmoMode};

#[cfg(feature = "selection")]
use crate::gizmo_raycast::{GizmoGeometry, GizmoHandle};
#[cfg(feature = "selection")]
use crate::selection::{Selected, SelectionState};

use super::state::GizmoState;
use super::transform::get_gizmo_transform;

/// Render gizmo visualization using Bevy's gizmos API
#[cfg(feature = "selection")]
pub(crate) fn render_gizmo(
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
pub(crate) fn get_axis_colors(constraint: GizmoAxis) -> (Color, Color, Color) {
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
