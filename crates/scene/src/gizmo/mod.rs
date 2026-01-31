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

mod hover;
mod input;
mod render;
mod state;
mod transform;

use bevy::prelude::*;
use pentimento_ipc::EditMode;

use crate::edit_mode::EditModeState;
#[cfg(feature = "selection")]
use crate::gizmo_raycast::GizmoGeometry;

// Re-export main types
pub use state::GizmoState;

/// Plugin for transform gizmos
pub struct GizmoPlugin;

impl Plugin for GizmoPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GizmoState>();

        // Only add gizmo systems if selection feature is enabled
        #[cfg(feature = "selection")]
        {
            use hover::{detect_gizmo_hover, handle_gizmo_mouse_input};
            use input::{handle_gizmo_click, handle_gizmo_hotkeys};
            use render::render_gizmo;
            use transform::apply_gizmo_transform;

            app.init_resource::<GizmoGeometry>();
            app.add_systems(
                Update,
                (
                    sync_gizmo_visibility_with_edit_mode,
                    detect_gizmo_hover.after(sync_gizmo_visibility_with_edit_mode),
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

/// Disable gizmo visibility when in edit modes (Paint, Sculpt, MeshEdit).
///
/// The gizmo interferes with brush strokes and vertex selection in these modes,
/// so we hide it until the user exits back to normal object mode.
fn sync_gizmo_visibility_with_edit_mode(
    edit_mode: Res<EditModeState>,
    mut gizmo_state: ResMut<GizmoState>,
) {
    // Gizmo should only be visible in None mode (normal object manipulation)
    let should_show_gizmo = edit_mode.mode == EditMode::None;

    if gizmo_state.always_visible != should_show_gizmo {
        gizmo_state.always_visible = should_show_gizmo;
    }
}
