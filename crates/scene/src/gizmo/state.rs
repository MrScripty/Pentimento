//! GizmoState resource and state machine methods

use bevy::prelude::*;
use pentimento_ipc::{CoordinateSpace, GizmoAxis, GizmoMode};

#[cfg(feature = "selection")]
use crate::gizmo_raycast::GizmoHandle;

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
    pub(crate) last_axis_pressed: Option<GizmoAxis>,
    /// Whether a transform operation is currently active
    pub is_active: bool,
    /// Original transforms before operation started (for cancel)
    pub(crate) original_transforms: Vec<(Entity, Transform)>,
    /// Accumulated mouse delta during operation
    pub(crate) accumulated_delta: Vec2,
    /// Currently hovered handle (for visual feedback)
    #[cfg(feature = "selection")]
    pub hovered_handle: GizmoHandle,
    /// Hit point of currently hovered handle (stored for click handling)
    #[cfg(feature = "selection")]
    pub(crate) hovered_hit_point: Option<Vec3>,
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
    pub(crate) fn start_operation(&mut self, mode: GizmoMode) {
        self.mode = mode;
        self.axis_constraint = GizmoAxis::None;
        self.coordinate_space = CoordinateSpace::Global;
        self.last_axis_pressed = None;
        self.is_active = true;
        self.accumulated_delta = Vec2::ZERO;
    }

    /// Start a transform operation from a handle click
    #[cfg(feature = "selection")]
    pub(crate) fn start_operation_from_handle(&mut self, handle: GizmoHandle) {
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
    pub(crate) fn cancel(&mut self) {
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
    pub(crate) fn confirm(&mut self) {
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
pub(crate) fn handle_axis_key(gizmo_state: &mut GizmoState, axis: GizmoAxis, shift_held: bool) {
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
