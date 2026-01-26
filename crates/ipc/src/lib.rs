//! IPC message protocol for Pentimento
//!
//! Defines all message types exchanged between the Bevy backend and Svelte UI.

use serde::{Deserialize, Serialize};

/// Messages from Bevy to the Svelte UI
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

    /// Close all open menus (triggered when clicking outside UI)
    CloseMenus,
}

/// Messages from Svelte UI to Bevy
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
}

// ============================================================================
// Scene Types
// ============================================================================

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SceneInfo {
    pub objects: Vec<SceneObject>,
    pub cameras: Vec<CameraInfo>,
    pub lights: Vec<LightInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneObject {
    pub id: String,
    pub name: String,
    pub transform: Transform3D,
    pub material_id: Option<String>,
    pub visible: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Transform3D {
    pub position: [f32; 3],
    pub rotation: [f32; 4], // Quaternion (x, y, z, w)
    pub scale: [f32; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraInfo {
    pub id: String,
    pub name: String,
    pub transform: Transform3D,
    pub fov: f32,
    pub near: f32,
    pub far: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightInfo {
    pub id: String,
    pub name: String,
    pub light_type: LightType,
    pub color: [f32; 3],
    pub intensity: f32,
    pub transform: Transform3D,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LightType {
    Directional,
    Point { range: f32 },
    Spot { range: f32, inner_angle: f32, outer_angle: f32 },
}

// ============================================================================
// Material Types
// ============================================================================

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MaterialProperties {
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub emissive: [f32; 3],
    pub texture_slots: Vec<TextureSlot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextureSlot {
    pub slot_name: String,
    pub texture_id: Option<String>,
}

// ============================================================================
// Layout Types
// ============================================================================

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LayoutInfo {
    pub regions: Vec<LayoutRegion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutRegion {
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub z_index: i32,
    pub accepts_keyboard: bool,
}

// ============================================================================
// Command Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CameraCommand {
    Orbit { delta_x: f32, delta_y: f32 },
    Pan { delta_x: f32, delta_y: f32 },
    Zoom { delta: f32 },
    SetPosition { position: [f32; 3] },
    SetTarget { target: [f32; 3] },
    Reset,
}

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

// ============================================================================
// Diffusion Types
// ============================================================================

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

// ============================================================================
// Settings Types
// ============================================================================

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

// ============================================================================
// Lighting Types
// ============================================================================

/// Configurable lighting settings for the scene
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

// ============================================================================
// Gizmo Types
// ============================================================================

/// Transform gizmo operation mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum GizmoMode {
    #[default]
    None,
    Translate,
    Rotate,
    /// Trackball rotation (free rotation - press R twice to toggle from Rotate)
    Trackball,
    Scale,
}

/// Axis constraint for gizmo operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum GizmoAxis {
    #[default]
    None,
    X,
    Y,
    Z,
    /// Constrain to XY plane (exclude Z)
    XY,
    /// Constrain to XZ plane (exclude Y)
    XZ,
    /// Constrain to YZ plane (exclude X)
    YZ,
}

/// Coordinate space for gizmo operations (global vs local/object-relative)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CoordinateSpace {
    /// World/global coordinate space
    #[default]
    Global,
    /// Object-local coordinate space (axes rotate with object)
    Local,
}

/// Commands for controlling the transform gizmo
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GizmoCommand {
    /// Set the active gizmo mode (G/S/R keys)
    SetMode(GizmoMode),
    /// Constrain to specific axis (X/Y/Z keys)
    ConstrainAxis(GizmoAxis),
    /// Cancel current transform operation (Escape)
    Cancel,
    /// Confirm current transform operation (Enter/LMB)
    Confirm,
}

// ============================================================================
// Edit Mode Types
// ============================================================================

/// Edit mode types for specialized editing (paint, sculpt, etc.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum EditMode {
    /// Normal object/scene editing mode
    #[default]
    None,
    /// Paint mode - painting on a canvas plane
    Paint,
}

/// Request to add a paint canvas and enter paint mode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddPaintCanvasRequest {
    /// Canvas width in pixels (defaults to 1024)
    pub width: Option<u32>,
    /// Canvas height in pixels (defaults to 1024)
    pub height: Option<u32>,
}

// ============================================================================
// Paint Command Types
// ============================================================================

/// Blend mode for painting operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum BlendMode {
    #[default]
    Normal = 0,
    Erase = 1,
}

/// Commands for controlling the painting system
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

// ============================================================================
// Add Object Types
// ============================================================================

/// Primitive mesh types for object creation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrimitiveType {
    Cube,
    Sphere,
    Cylinder,
    Plane,
    Torus,
    Cone,
    Capsule,
}

/// Request to add a new object to the scene
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddObjectRequest {
    pub primitive_type: PrimitiveType,
    /// Optional world position (defaults to origin)
    pub position: Option<[f32; 3]>,
    /// Optional custom name
    pub name: Option<String>,
}

// ============================================================================
// Ambient Occlusion Types
// ============================================================================

/// Screen-space ambient occlusion settings
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

// ============================================================================
// Node Graph Types
// ============================================================================

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NodeGraphState {
    pub nodes: Vec<NodeInfo>,
    pub connections: Vec<NodeConnection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: String,
    pub node_type: String,
    pub position: [f32; 2],
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConnection {
    pub from_node: String,
    pub from_output: String,
    pub to_node: String,
    pub to_input: String,
}

// ============================================================================
// Input Event Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MouseEvent {
    Move { x: f32, y: f32 },
    ButtonDown { button: MouseButton, x: f32, y: f32 },
    ButtonUp { button: MouseButton, x: f32, y: f32 },
    Scroll { delta_x: f32, delta_y: f32, x: f32, y: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyboardEvent {
    pub key: String,
    pub pressed: bool,
    pub modifiers: Modifiers,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

// ============================================================================
// Error Types
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    #[error("Failed to serialize message: {0}")]
    Serialize(#[from] serde_json::Error),

    #[error("Invalid message format: {0}")]
    InvalidFormat(String),
}
