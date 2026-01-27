//! Material-related types for IPC messages.

use serde::{Deserialize, Serialize};

/// Material properties for PBR rendering.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MaterialProperties {
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub emissive: [f32; 3],
    pub texture_slots: Vec<TextureSlot>,
}

/// A texture slot in a material.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextureSlot {
    pub slot_name: String,
    pub texture_id: Option<String>,
}
