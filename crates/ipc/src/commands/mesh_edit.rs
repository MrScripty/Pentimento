//! Mesh editing command types.

use serde::{Deserialize, Serialize};

/// Edit mode types for specialized editing (paint, sculpt, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum EditMode {
    /// Normal object/scene editing mode
    #[default]
    None,
    /// Paint mode - painting on a canvas plane
    Paint,
    /// Mesh edit mode - editing vertices, edges, and faces
    MeshEdit,
}

/// Sub-object selection mode for mesh editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MeshSelectionMode {
    /// Select individual vertices
    #[default]
    Vertex,
    /// Select edges
    Edge,
    /// Select faces
    Face,
}

/// Active tool in mesh edit mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MeshEditTool {
    /// Selection tool (default)
    #[default]
    Select,
    /// Extrude geometry
    Extrude,
    /// Loop cut
    LoopCut,
    /// Knife tool
    Knife,
    /// Merge vertices
    Merge,
    /// Inset faces
    Inset,
}

/// Commands for controlling mesh edit mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MeshEditCommand {
    /// Set the selection mode (vertex/edge/face)
    SetSelectionMode(MeshSelectionMode),
    /// Set the active tool
    SetTool(MeshEditTool),
    /// Select all elements
    SelectAll,
    /// Deselect all elements
    DeselectAll,
    /// Invert selection
    InvertSelection,
}
