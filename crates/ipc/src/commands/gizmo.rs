//! Gizmo command types for transform operations.

use serde::{Deserialize, Serialize};

/// Transform gizmo operation mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum GizmoMode {
    #[default]
    None,
    Translate,
    Rotate,
    /// Trackball rotation (free rotation - press R twice to toggle from Rotate)
    Trackball,
    Scale,
}

/// Axis constraint for gizmo operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum GizmoAxis {
    #[default]
    None,
    X,
    Y,
    Z,
    /// Constrain to XY plane (exclude Z)
    XY,
    /// Constrain to XZ plane (exclude Y)
    XZ,
    /// Constrain to YZ plane (exclude X)
    YZ,
}

/// Coordinate space for gizmo operations (global vs local/object-relative).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CoordinateSpace {
    /// World/global coordinate space
    #[default]
    Global,
    /// Object-local coordinate space (axes rotate with object)
    Local,
}

/// Commands for controlling the transform gizmo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GizmoCommand {
    /// Set the active gizmo mode (G/S/R keys)
    SetMode(GizmoMode),
    /// Constrain to specific axis (X/Y/Z keys)
    ConstrainAxis(GizmoAxis),
    /// Cancel current transform operation (Escape)
    Cancel,
    /// Confirm current transform operation (Enter/LMB)
    Confirm,
}
