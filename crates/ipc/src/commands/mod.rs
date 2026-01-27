//! Command types for IPC messages.

mod gizmo;
mod mesh_edit;
mod paint;

pub use gizmo::*;
pub use mesh_edit::*;
pub use paint::*;

use crate::types::Transform3D;
use serde::{Deserialize, Serialize};

/// Camera control commands.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CameraCommand {
    Orbit { delta_x: f32, delta_y: f32 },
    Pan { delta_x: f32, delta_y: f32 },
    Zoom { delta: f32 },
    SetPosition { position: [f32; 3] },
    SetTarget { target: [f32; 3] },
    Reset,
}

/// Object manipulation commands.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ObjectCommand {
    Select { ids: Vec<String> },
    Deselect { ids: Vec<String> },
    Delete { ids: Vec<String> },
    Duplicate { ids: Vec<String> },
    Transform { id: String, transform: Transform3D },
    SetVisibility { id: String, visible: bool },
    Rename { id: String, name: String },
}

/// Material editing commands.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MaterialCommand {
    UpdateProperty {
        material_id: String,
        property: String,
        value: serde_json::Value,
    },
    AssignTexture {
        material_id: String,
        slot: String,
        texture_id: String,
    },
    Create { name: String },
    Delete { material_id: String },
}
