//! Main IPC message enums for communication between Bevy and UI.

use serde::{Deserialize, Serialize};

use crate::commands::{
    AddPaintCanvasRequest, CameraCommand, GizmoCommand, GizmoMode, MaterialCommand,
    MeshEditCommand, MeshEditTool, MeshSelectionMode, ObjectCommand, PaintCommand, EditMode,
};
use crate::types::{
    AddObjectRequest, AmbientOcclusionSettings, AppSettings, DiffusionRequest, LayoutInfo,
    LightingSettings, MaterialProperties, NodeGraphState, SceneInfo, SceneObject,
};

/// Messages from Bevy to the Svelte UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum BevyToUi {
    /// Initial state sync when UI loads
    Initialize {
        scene_info: SceneInfo,
        settings: AppSettings,
    },

    /// Scene state updates
    SceneUpdated(SceneInfo),

    /// Object selection changed
    SelectionChanged { selected_ids: Vec<String> },

    /// Material property update
    MaterialUpdated {
        material_id: String,
        properties: MaterialProperties,
    },

    /// Diffusion generation progress
    DiffusionProgress {
        task_id: String,
        progress: f32,
        preview_available: bool,
    },

    /// Diffusion generation complete
    DiffusionComplete { task_id: String, texture_id: String },

    /// Render statistics
    RenderStats {
        fps: f32,
        frame_time_ms: f32,
        draw_calls: u32,
        triangles: u32,
    },

    /// Mouse entered a UI region
    MouseEnter { region_id: String },

    /// Mouse left a UI region
    MouseLeave { region_id: String },

    /// Error notification
    Error { code: String, message: String },

    /// Show/hide add object menu (triggered by Shift+A)
    ShowAddObjectMenu {
        show: bool,
        /// Screen position for menu (if show is true)
        position: Option<[f32; 2]>,
    },

    /// Object was added to scene
    ObjectAdded { object: SceneObject },

    /// Gizmo mode changed (for UI sync)
    GizmoModeChanged { mode: GizmoMode },

    /// Ambient occlusion settings changed
    AmbientOcclusionChanged { settings: AmbientOcclusionSettings },

    /// Edit mode changed (paint mode, etc.)
    EditModeChanged { mode: EditMode },

    /// Projection mode changed
    ProjectionModeChanged { live_projection: bool },

    /// Mesh edit mode state changed
    MeshEditModeChanged {
        /// Whether mesh edit mode is active
        active: bool,
        /// Current selection mode (vertex/edge/face)
        selection_mode: MeshSelectionMode,
        /// Current active tool
        tool: MeshEditTool,
    },

    /// Sub-object selection changed in mesh edit mode
    MeshEditSelectionChanged {
        /// Number of selected vertices
        vertex_count: usize,
        /// Number of selected edges
        edge_count: usize,
        /// Number of selected faces
        face_count: usize,
    },

    /// Close all open menus (triggered when clicking outside UI)
    CloseMenus,
}

/// Messages from Svelte UI to Bevy.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum UiToBevy {
    /// UI has rendered and needs capture
    UiDirty,

    /// UI layout changed (for input routing)
    LayoutUpdate(LayoutInfo),

    /// Camera control commands
    CameraCommand(CameraCommand),

    /// Object manipulation
    ObjectCommand(ObjectCommand),

    /// Material editing
    MaterialCommand(MaterialCommand),

    /// Start diffusion generation
    StartDiffusion(DiffusionRequest),

    /// Cancel diffusion generation
    CancelDiffusion { task_id: String },

    /// Settings changed
    UpdateSettings(AppSettings),

    /// Lighting settings changed
    UpdateLighting(LightingSettings),

    /// Node graph connection changed
    NodeGraphUpdate(NodeGraphState),

    /// Add a new object to the scene
    AddObject(AddObjectRequest),

    /// Ambient occlusion settings changed
    UpdateAmbientOcclusion(AmbientOcclusionSettings),

    /// Gizmo command (from keyboard hotkeys)
    GizmoCommand(GizmoCommand),

    /// Add a paint canvas and enter paint mode
    AddPaintCanvas(AddPaintCanvasRequest),

    /// Paint-specific commands (brush settings, undo, etc.)
    PaintCommand(PaintCommand),

    /// Mesh edit mode commands
    MeshEditCommand(MeshEditCommand),

    /// Toggle depth view mode
    SetDepthView { enabled: bool },
}
