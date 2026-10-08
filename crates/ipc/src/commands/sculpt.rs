//! Supported sculpt controls, independent of the optional sculpting engine dependency.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SculptTool {
    Push,
    Pull,
    Grab,
    Smooth,
    Flatten,
    Inflate,
    Pinch,
    Crease,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SculptFalloff {
    Linear,
    Smooth,
    Sharp,
    Constant,
    Sphere,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SculptCommand {
    Undo,
    Redo,
    SetTool { tool: SculptTool },
    SetRadius { radius: f32 },
    SetStrength { strength: f32 },
    SetHardness { hardness: f32 },
    SetFalloff { falloff: SculptFalloff },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SculptBrushSettings {
    pub tool: SculptTool,
    /// World-space radius, not screen pixels.
    pub radius: f32,
    pub strength: f32,
    pub hardness: f32,
    pub falloff: SculptFalloff,
}
