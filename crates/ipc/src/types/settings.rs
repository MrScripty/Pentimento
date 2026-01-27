//! Application and lighting settings types.

use serde::{Deserialize, Serialize};

/// Application-wide settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub render_scale: f32,
    pub vsync: bool,
    pub msaa_samples: u32,
    pub show_wireframe: bool,
    pub show_grid: bool,
    pub diffusion_server_url: Option<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            render_scale: 1.0,
            vsync: true,
            msaa_samples: 4,
            show_wireframe: false,
            show_grid: true,
            diffusion_server_url: None,
        }
    }
}

/// Configurable lighting settings for the scene.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightingSettings {
    /// Sun direction as normalized vector (pointing toward light source)
    pub sun_direction: [f32; 3],
    /// Sun color as RGB (0.0-1.0)
    pub sun_color: [f32; 3],
    /// Sun intensity in lux (typical outdoor: 10000-100000)
    pub sun_intensity: f32,
    /// Ambient light color as RGB (0.0-1.0)
    pub ambient_color: [f32; 3],
    /// Ambient light intensity (0.0-1.0 typical range)
    pub ambient_intensity: f32,
    /// Time of day in hours (0.0 - 24.0) for sun position calculation
    pub time_of_day: f32,
    /// Cloudiness factor (0.0 = clear, 1.0 = fully overcast)
    pub cloudiness: f32,
    /// Whether to auto-calculate sun direction from time_of_day
    pub use_time_of_day: bool,
}

impl Default for LightingSettings {
    fn default() -> Self {
        Self {
            // Default sun direction: from upper-left-front (normalized)
            sun_direction: [-0.5, -0.7, -0.5],
            // Warm white sun color
            sun_color: [1.0, 0.98, 0.95],
            // Bright outdoor illuminance
            sun_intensity: 10000.0,
            // Cool sky-blue ambient
            ambient_color: [0.6, 0.7, 1.0],
            // Moderate ambient fill
            ambient_intensity: 500.0,
            // Default to noon
            time_of_day: 12.0,
            // Clear sky
            cloudiness: 0.0,
            // Use time-based sun positioning
            use_time_of_day: true,
        }
    }
}

/// Screen-space ambient occlusion settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmbientOcclusionSettings {
    /// Enable/disable SSAO
    pub enabled: bool,
    /// Quality level: 0=Low, 1=Medium, 2=High, 3=Ultra
    pub quality_level: u8,
    /// Constant object thickness for ray marching (0.0625 - 4.0)
    pub constant_object_thickness: f32,
}

impl Default for AmbientOcclusionSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            quality_level: 2, // High
            constant_object_thickness: 0.25,
        }
    }
}

/// Diffusion generation request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffusionRequest {
    pub task_id: String,
    pub prompt: String,
    pub negative_prompt: Option<String>,
    pub width: u32,
    pub height: u32,
    pub steps: u32,
    pub guidance_scale: f32,
    pub seed: Option<u64>,
    /// Target material slot: (material_id, slot_name)
    pub target_material_slot: Option<(String, String)>,
}

/// Node graph state for material editing.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NodeGraphState {
    pub nodes: Vec<NodeInfo>,
    pub connections: Vec<NodeConnection>,
}

/// Node information in a node graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: String,
    pub node_type: String,
    pub position: [f32; 2],
    pub data: serde_json::Value,
}

/// Connection between nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConnection {
    pub from_node: String,
    pub from_output: String,
    pub to_node: String,
    pub to_input: String,
}
