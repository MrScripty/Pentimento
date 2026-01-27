//! Gizmo input handling: click and hotkey systems

use bevy::input::mouse::MouseButton;
use bevy::prelude::*;
use pentimento_ipc::{GizmoAxis, GizmoMode};

#[cfg(feature = "selection")]
use crate::gizmo_raycast::GizmoHandle;
#[cfg(feature = "selection")]
use crate::selection::{Selected, SelectionState};

use super::state::{handle_axis_key, GizmoState};

/// Handle mouse clicks on gizmo handles
#[cfg(feature = "selection")]
pub(crate) fn handle_gizmo_click(
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
pub(crate) fn handle_gizmo_hotkeys(
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
