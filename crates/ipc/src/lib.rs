//! IPC message protocol for Pentimento
//!
//! Defines all message types exchanged between the Bevy backend and Svelte UI.

pub mod commands;
pub mod error;
pub mod input;
pub mod messages;
pub mod types;

// Re-export all public types at the crate root for API compatibility.
// This allows existing imports like `use pentimento_ipc::BevyToUi` to continue working.

// Main message enums
pub use messages::{BevyToUi, UiToBevy};

// Types
pub use types::{
    AddObjectRequest, AmbientOcclusionSettings, AppSettings, CameraInfo, DiffusionRequest,
    LayoutInfo, LayoutRegion, LightInfo, LightType, LightingSettings, MaterialProperties,
    NodeConnection, NodeGraphState, NodeInfo, PrimitiveType, SceneInfo, SceneObject, TextureSlot,
    Transform3D,
};

// Commands
pub use commands::{
    AddPaintCanvasRequest, BlendMode, CameraCommand, CoordinateSpace, EditMode, GizmoAxis,
    GizmoCommand, GizmoMode, MaterialCommand, MeshEditCommand, MeshEditTool, MeshSelectionMode,
    ObjectCommand, PaintCommand,
};

// Input types
pub use input::{KeyboardEvent, Modifiers, MouseButton, MouseEvent};

// Error types
pub use error::IpcError;
