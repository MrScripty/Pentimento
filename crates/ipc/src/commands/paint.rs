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
    /// Select a brush preset by ID
    SelectBrushPreset { preset_id: u32 },
    /// Undo last stroke
    Undo,
    /// Enable/disable live projection mode (paint-as-project)
    SetLiveProjection { enabled: bool },
    /// Project current canvas contents to all visible meshes (one-shot)
    ProjectToScene,
    /// Add a new layer (empty name for auto-generated)
    AddLayer { name: String },
    /// Remove a layer by ID
    RemoveLayer { layer_id: u32 },
    /// Set the active (painting target) layer
    SetActiveLayer { layer_id: u32 },
    /// Toggle layer visibility
    SetLayerVisibility { layer_id: u32, visible: bool },
    /// Set layer opacity (0.0-1.0)
    SetLayerOpacity { layer_id: u32, opacity: f32 },
    /// Reorder layer to a new index position
    ReorderLayer { layer_id: u32, new_index: u32 },
    /// Rename a layer
    RenameLayer { layer_id: u32, name: String },
}

/// Layer metadata for UI synchronization.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LayerInfo {
    /// Unique layer ID
    pub id: u32,
    /// Human-readable name
    pub name: String,
    /// Whether the layer is visible
    pub visible: bool,
    /// Layer opacity (0.0-1.0)
    pub opacity: f32,
    /// Whether this is the currently active (painting target) layer
    pub is_active: bool,
}

/// Request to add a paint canvas and enter paint mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddPaintCanvasRequest {
    /// Canvas width in pixels (defaults to 1024)
    pub width: Option<u32>,
    /// Canvas height in pixels (defaults to 1024)
    pub height: Option<u32>,
}
