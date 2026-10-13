//! Paint command types for the painting system.

use serde::{Deserialize, Serialize};

/// Canvas pixel source for the one-click color sampler.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColorSampleSource {
    #[default]
    VisibleLayers,
    ActiveLayer,
}

/// Blend mode for painting operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum BlendMode {
    #[default]
    Normal = 0,
    Erase = 1,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaintTarget {
    #[default]
    Canvas,
    DirectUv,
}

/// Authoritative target/transaction status for the existing paint controls.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PaintTargetState {
    pub mode: PaintTarget,
    pub direct_available: bool,
    pub target_name: Option<String>,
    pub active: bool,
    pub notice: Option<String>,
    pub retained_bytes: usize,
    pub pending_bytes: usize,
    pub limit_bytes: usize,
    pub evicted_strokes: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uv_layers: Option<UvLayerState>,
}

/// Commands for controlling the painting system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PaintCommand {
    UvLayers {
        command: UvLayerCommand,
    },
    SetTarget {
        target: PaintTarget,
    },
    CancelStroke,
    /// Discard a staged UV projection preview; source Canvas edits remain.
    CancelUvProjection,
    SetColorSampling {
        enabled: bool,
    },
    SetColorSampleSource {
        source: ColorSampleSource,
    },
    /// Set brush color (RGBA, 0.0-1.0)
    SetBrushColor {
        color: [f32; 4],
    },
    /// Set brush size in pixels
    SetBrushSize {
        size: f32,
    },
    /// Set brush opacity (0.0-1.0)
    SetBrushOpacity {
        opacity: f32,
    },
    /// Set brush hardness (0.0-1.0)
    SetBrushHardness {
        hardness: f32,
    },
    /// Set round-tip dab spacing as a fraction of diameter (0.01-1.0).
    SetBrushSpacing {
        spacing: f32,
    },
    /// Set blend mode (Normal or Erase)
    SetBlendMode {
        mode: BlendMode,
    },
    /// Select a brush preset by ID
    SelectBrushPreset {
        preset_id: u32,
    },
    /// Save the current paint brush, color and tool locally; same name replaces it.
    SaveBrushPreset {
        name: String,
    },
    /// Restore one backend-owned saved paint brush.
    SelectSavedBrushPreset {
        preset_id: u32,
    },
    /// Undo last stroke
    Undo,
    /// Restore the last undone canvas stroke.
    Redo,
    /// Show/hide the source canvas while retaining its projection and editing state.
    SetSourceVisible {
        visible: bool,
    },
    /// Enable/disable live projection mode (paint-as-project)
    SetLiveProjection {
        enabled: bool,
    },
    /// Project current canvas contents to all visible meshes (one-shot)
    ProjectToScene,
    /// Add a new layer (empty name for auto-generated)
    AddLayer {
        name: String,
    },
    /// Remove a layer by ID
    RemoveLayer {
        layer_id: u32,
    },
    /// Set the active (painting target) layer
    SetActiveLayer {
        layer_id: u32,
    },
    /// Toggle layer visibility
    SetLayerVisibility {
        layer_id: u32,
        visible: bool,
    },
    /// Set layer opacity (0.0-1.0)
    SetLayerOpacity {
        layer_id: u32,
        opacity: f32,
    },
    /// Reorder layer to a new index position
    ReorderLayer {
        layer_id: u32,
        new_index: u32,
    },
    /// Rename a layer
    RenameLayer {
        layer_id: u32,
        name: String,
    },
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

/// Backend-owned brush settings. Size is the full-pressure diameter in canvas pixels.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PaintBrushSettings {
    pub preset_id: u32,
    pub customized: bool,
    pub color: [f32; 4],
    pub size: f32,
    pub opacity: f32,
    pub hardness: f32,
    pub spacing: f32,
    pub blend_mode: BlendMode,
}

/// Supported preset metadata; frontends must not invent unsupported tip types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PaintBrushPresetInfo {
    pub id: u32,
    pub name: String,
}

/// UV target commands are separate from source Canvas layer commands.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum UvLayerBlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum UvLayerPaintTarget {
    #[default]
    Color,
    Mask,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UvLayerCommand {
    SelectReceiver {
        mesh_id: u32,
    },
    Enable,
    Create {
        name: String,
    },
    Duplicate {
        layer_id: u32,
    },
    Delete {
        layer_id: u32,
    },
    Select {
        layer_id: u32,
    },
    Rename {
        layer_id: u32,
        name: String,
    },
    Reorder {
        layer_id: u32,
        new_index: usize,
    },
    Visible {
        layer_id: u32,
        visible: bool,
    },
    Opacity {
        layer_id: u32,
        opacity: f32,
    },
    Lock {
        layer_id: u32,
        locked: bool,
    },
    BlendMode {
        layer_id: u32,
        mode: UvLayerBlendMode,
    },
    AddMask {
        layer_id: u32,
    },
    RemoveMask {
        layer_id: u32,
    },
    MaskEnabled {
        layer_id: u32,
        enabled: bool,
    },
    PaintTarget {
        layer_id: u32,
        target: UvLayerPaintTarget,
    },
    Undo,
    Redo,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UvLayerInfo {
    pub id: u32,
    pub name: String,
    pub visible: bool,
    pub opacity: f32,
    pub locked: bool,
    pub is_active: bool,
    #[serde(default)]
    pub blend_mode: UvLayerBlendMode,
    #[serde(default)]
    pub has_mask: bool,
    #[serde(default)]
    pub mask_enabled: bool,
    #[serde(default)]
    pub paint_target: UvLayerPaintTarget,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UvReceiverInfo {
    pub mesh_id: u32,
    pub name: String,
    pub layered: bool,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct UvLayerState {
    pub receivers: Vec<UvReceiverInfo>,
    pub receiver: Option<u32>,
    pub enabled: bool,
    pub layers: Vec<UvLayerInfo>,
    pub can_undo: bool,
    pub can_redo: bool,
    pub active: bool,
    #[serde(default)]
    pub projection_preview: bool,
    pub conflicted: bool,
    pub notice: Option<String>,
}
