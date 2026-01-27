//! Gizmo hover detection and mouse input handling

use bevy::input::mouse::MouseMotion;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

#[cfg(feature = "selection")]
use crate::gizmo_raycast::{raycast_gizmo, GizmoGeometry, GizmoHandle};
#[cfg(feature = "selection")]
use crate::selection::{Selected, SelectionState};
#[cfg(feature = "selection")]
use crate::MainCamera;

use super::state::GizmoState;
use super::transform::get_gizmo_transform;

/// Detect which gizmo handle the cursor is hovering over
#[cfg(feature = "selection")]
pub(crate) fn detect_gizmo_hover(
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

/// Handle mouse input during active gizmo operation
#[cfg(feature = "selection")]
pub(crate) fn handle_gizmo_mouse_input(
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
