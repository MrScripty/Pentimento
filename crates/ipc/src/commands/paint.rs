//! Paint command types for the painting system.

use serde::{Deserialize, Serialize};

/// Blend mode for painting operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum BlendMode {
    #[default]
    Normal = 0,
    Erase = 1,
}

/// Commands for controlling the painting system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PaintCommand {
    /// Set brush color (RGBA, 0.0-1.0)
    SetBrushColor { color: [f32; 4] },
    /// Set brush size in pixels
    SetBrushSize { size: f32 },
    /// Set brush opacity (0.0-1.0)
    SetBrushOpacity { opacity: f32 },
    /// Set brush hardness (0.0-1.0)
    SetBrushHardness { hardness: f32 },
    /// Set blend mode (Normal or Erase)
    SetBlendMode { mode: BlendMode },
    /// Undo last stroke
    Undo,
    /// Enable/disable live projection mode (paint-as-project)
    SetLiveProjection { enabled: bool },
    /// Project current canvas contents to all visible meshes (one-shot)
    ProjectToScene,
}

/// Request to add a paint canvas and enter paint mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddPaintCanvasRequest {
    /// Canvas width in pixels (defaults to 1024)
    pub width: Option<u32>,
    /// Canvas height in pixels (defaults to 1024)
    pub height: Option<u32>,
}
