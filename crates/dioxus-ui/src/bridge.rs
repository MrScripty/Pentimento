//! IPC bridge between Dioxus UI and Bevy
//!
//! Uses Rust channels instead of console.log interception like CEF mode.

use pentimento_ipc::{
    AddObjectRequest, AddPaintCanvasRequest, AmbientOcclusionSettings, BevyToUi, BlendMode,
    CameraCommand, DiffusionRequest, EditMode, LightingSettings, MaterialCommand,
    MeshEditCommand, MeshEditTool, MeshSelectionMode, ObjectCommand, PaintCommand, PrimitiveType,
    UiToBevy,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc, Mutex,
};

/// Shared state that persists between renders.
/// Used for state that needs to be set from Bevy and read by the component.
#[derive(Clone, PartialEq)]
pub struct SharedUiState {
    /// Show add object menu at position
    pub show_add_menu: bool,
    pub add_menu_position: (f32, f32),
    /// Current edit mode (None/Paint/MeshEdit)
    pub edit_mode: EditMode,
    /// Mesh edit mode state (only valid when edit_mode == MeshEdit)
    pub mesh_edit_active: bool,
    pub mesh_selection_mode: MeshSelectionMode,
    pub mesh_edit_tool: MeshEditTool,
    /// Selection counts
    pub selected_vertex_count: usize,
    pub selected_edge_count: usize,
    pub selected_face_count: usize,
}

impl Default for SharedUiState {
    fn default() -> Self {
        Self {
            show_add_menu: false,
            add_menu_position: (0.0, 0.0),
            edit_mode: EditMode::None,
            mesh_edit_active: false,
            mesh_selection_mode: MeshSelectionMode::Vertex,
            mesh_edit_tool: MeshEditTool::Select,
            selected_vertex_count: 0,
            selected_edge_count: 0,
            selected_face_count: 0,
        }
    }
}

/// Bridge for sending messages from Dioxus UI to Bevy
#[derive(Clone)]
pub struct DioxusBridge {
    to_bevy: mpsc::Sender<UiToBevy>,
    from_bevy: Arc<Mutex<mpsc::Receiver<BevyToUi>>>,
    /// Flag indicating messages are pending from Bevy.
    /// Set by DioxusBridgeHandle when sending, cleared when messages are consumed.
    pending_messages: Arc<AtomicBool>,
    /// Shared UI state that can be updated from Bevy and read by the component.
    /// This ensures state persists and is available on any render.
    shared_state: Arc<Mutex<SharedUiState>>,
}

// Manual PartialEq implementation for Dioxus Props compatibility.
// Always returns false to opt out of Dioxus memoization, ensuring the component
// re-runs when force_render() is called to process external channel messages.
impl PartialEq for DioxusBridge {
    fn eq(&self, _other: &Self) -> bool {
        false
    }
}

impl DioxusBridge {
    /// Create a new bridge pair
    pub fn new() -> (Self, DioxusBridgeHandle) {
        let (to_bevy_tx, to_bevy_rx) = mpsc::channel();
        let (from_bevy_tx, from_bevy_rx) = mpsc::channel();
        let pending_messages = Arc::new(AtomicBool::new(false));
        let shared_state = Arc::new(Mutex::new(SharedUiState::default()));

        let bridge = Self {
            to_bevy: to_bevy_tx,
            from_bevy: Arc::new(Mutex::new(from_bevy_rx)),
            pending_messages: pending_messages.clone(),
            shared_state: shared_state.clone(),
        };

        let handle = DioxusBridgeHandle {
            to_ui: from_bevy_tx,
            from_ui: to_bevy_rx,
            pending_messages,
            shared_state,
        };

        (bridge, handle)
    }

    /// Check if there are pending messages from Bevy.
    /// This can be used to trigger a re-render when messages arrive.
    pub fn has_pending_messages(&self) -> bool {
        self.pending_messages.load(Ordering::Acquire)
    }

    /// Clear the pending messages flag.
    /// Call this after processing all messages.
    pub fn clear_pending(&self) {
        self.pending_messages.store(false, Ordering::Release);
    }

    /// Get the shared UI state.
    /// Returns a copy of the current state.
    pub fn get_shared_state(&self) -> SharedUiState {
        self.shared_state.lock().unwrap().clone()
    }

    /// Update the add menu visibility.
    /// This is called by the component to close the menu.
    pub fn set_add_menu_visible(&self, show: bool) {
        let mut state = self.shared_state.lock().unwrap();
        state.show_add_menu = show;
    }

    /// Open the add menu at a specific position.
    /// This is called by the component when Shift+A is pressed.
    pub fn open_add_menu_at(&self, position: (f32, f32)) {
        let mut state = self.shared_state.lock().unwrap();
        state.show_add_menu = true;
        state.add_menu_position = position;
    }

    /// Try to receive a message from Bevy (non-blocking)
    pub fn try_recv_from_bevy(&self) -> Option<BevyToUi> {
        let lock_result = self.from_bevy.lock();
        if lock_result.is_err() {
            tracing::warn!("try_recv_from_bevy: mutex lock failed!");
            return None;
        }
        let guard = lock_result.unwrap();
        guard.try_recv().ok()
    }

    fn send(&self, msg: UiToBevy) {
        let _ = self.to_bevy.send(msg);
    }

    // ========================================================================
    // Camera commands
    // ========================================================================

    pub fn camera_reset(&self) {
        self.send(UiToBevy::CameraCommand(CameraCommand::Reset));
    }

    pub fn camera_orbit(&self, delta_x: f32, delta_y: f32) {
        self.send(UiToBevy::CameraCommand(CameraCommand::Orbit {
            delta_x,
            delta_y,
        }));
    }

    pub fn camera_pan(&self, delta_x: f32, delta_y: f32) {
        self.send(UiToBevy::CameraCommand(CameraCommand::Pan {
            delta_x,
            delta_y,
        }));
    }

    pub fn camera_zoom(&self, delta: f32) {
        self.send(UiToBevy::CameraCommand(CameraCommand::Zoom { delta }));
    }

    // ========================================================================
    // Object commands
    // ========================================================================

    pub fn select_objects(&self, ids: Vec<String>) {
        self.send(UiToBevy::ObjectCommand(ObjectCommand::Select { ids }));
    }

    pub fn delete_objects(&self, ids: Vec<String>) {
        self.send(UiToBevy::ObjectCommand(ObjectCommand::Delete { ids }));
    }

    pub fn duplicate_objects(&self, ids: Vec<String>) {
        self.send(UiToBevy::ObjectCommand(ObjectCommand::Duplicate { ids }));
    }

    // ========================================================================
    // Material commands
    // ========================================================================

    pub fn update_material_property(
        &self,
        material_id: String,
        property: String,
        value: serde_json::Value,
    ) {
        self.send(UiToBevy::MaterialCommand(MaterialCommand::UpdateProperty {
            material_id,
            property,
            value,
        }));
    }

    // ========================================================================
    // Diffusion commands
    // ========================================================================

    pub fn start_diffusion(&self, request: DiffusionRequest) {
        self.send(UiToBevy::StartDiffusion(request));
    }

    pub fn cancel_diffusion(&self, task_id: String) {
        self.send(UiToBevy::CancelDiffusion { task_id });
    }

    // ========================================================================
    // Settings commands
    // ========================================================================

    pub fn update_lighting(&self, settings: LightingSettings) {
        self.send(UiToBevy::UpdateLighting(settings));
    }

    pub fn update_ambient_occlusion(&self, settings: AmbientOcclusionSettings) {
        self.send(UiToBevy::UpdateAmbientOcclusion(settings));
    }

    // ========================================================================
    // Add object commands
    // ========================================================================

    pub fn add_object(
        &self,
        primitive_type: PrimitiveType,
        position: Option<[f32; 3]>,
        name: Option<String>,
    ) {
        self.send(UiToBevy::AddObject(AddObjectRequest {
            primitive_type,
            position,
            name,
        }));
    }

    // ========================================================================
    // Paint canvas commands
    // ========================================================================

    /// Add a paint canvas in front of the camera and enter paint mode
    pub fn add_paint_canvas(&self, width: Option<u32>, height: Option<u32>) {
        self.send(UiToBevy::AddPaintCanvas(AddPaintCanvasRequest { width, height }));
    }

    // ========================================================================
    // Paint commands
    // ========================================================================

    /// Set brush color (RGBA, 0.0-1.0)
    pub fn set_brush_color(&self, color: [f32; 4]) {
        self.send(UiToBevy::PaintCommand(PaintCommand::SetBrushColor { color }));
    }

    /// Set brush size in pixels
    pub fn set_brush_size(&self, size: f32) {
        self.send(UiToBevy::PaintCommand(PaintCommand::SetBrushSize { size }));
    }

    /// Set brush opacity (0.0-1.0)
    pub fn set_brush_opacity(&self, opacity: f32) {
        self.send(UiToBevy::PaintCommand(PaintCommand::SetBrushOpacity { opacity }));
    }

    /// Set brush hardness (0.0-1.0)
    pub fn set_brush_hardness(&self, hardness: f32) {
        self.send(UiToBevy::PaintCommand(PaintCommand::SetBrushHardness { hardness }));
    }

    /// Set blend mode (Normal or Erase)
    pub fn set_blend_mode(&self, mode: BlendMode) {
        self.send(UiToBevy::PaintCommand(PaintCommand::SetBlendMode { mode }));
    }

    /// Undo last paint stroke
    pub fn paint_undo(&self) {
        self.send(UiToBevy::PaintCommand(PaintCommand::Undo));
    }

    /// Enable/disable live projection mode (paint projects to meshes in real-time)
    pub fn set_live_projection(&self, enabled: bool) {
        self.send(UiToBevy::PaintCommand(PaintCommand::SetLiveProjection { enabled }));
    }

    /// Project current canvas contents to all visible meshes (one-shot)
    pub fn project_to_scene(&self) {
        self.send(UiToBevy::PaintCommand(PaintCommand::ProjectToScene));
    }

    // ========================================================================
    // Mesh edit commands
    // ========================================================================

    /// Set mesh edit selection mode (Vertex/Edge/Face)
    pub fn set_mesh_selection_mode(&self, mode: MeshSelectionMode) {
        self.send(UiToBevy::MeshEditCommand(MeshEditCommand::SetSelectionMode(
            mode,
        )));
    }

    /// Set mesh edit tool
    pub fn set_mesh_edit_tool(&self, tool: MeshEditTool) {
        self.send(UiToBevy::MeshEditCommand(MeshEditCommand::SetTool(tool)));
    }

    /// Select all mesh elements
    pub fn mesh_select_all(&self) {
        self.send(UiToBevy::MeshEditCommand(MeshEditCommand::SelectAll));
    }

    /// Deselect all mesh elements
    pub fn mesh_deselect_all(&self) {
        self.send(UiToBevy::MeshEditCommand(MeshEditCommand::DeselectAll));
    }

    // ========================================================================
    // UI dirty notification
    // ========================================================================

    pub fn mark_dirty(&self) {
        self.send(UiToBevy::UiDirty);
    }
}

/// Handle given to Bevy side for IPC
pub struct DioxusBridgeHandle {
    /// Send messages to the UI
    pub to_ui: mpsc::Sender<BevyToUi>,
    /// Receive messages from the UI
    pub from_ui: mpsc::Receiver<UiToBevy>,
    /// Shared flag to notify UI that messages are pending
    pending_messages: Arc<AtomicBool>,
    /// Shared UI state for immediate updates (bypasses channel for critical state)
    shared_state: Arc<Mutex<SharedUiState>>,
}

impl DioxusBridgeHandle {
    /// Try to receive a message from the UI (non-blocking)
    pub fn try_recv(&self) -> Option<UiToBevy> {
        self.from_ui.try_recv().ok()
    }

    /// Send a message to the UI and set the pending flag.
    /// For certain messages, also updates shared state directly.
    pub fn send(&self, msg: BevyToUi) {
        // Update shared state directly for certain messages
        {
            let mut state = self.shared_state.lock().unwrap();
            match &msg {
                BevyToUi::ShowAddObjectMenu { show, position } => {
                    state.show_add_menu = *show;
                    if let Some([x, y]) = position {
                        state.add_menu_position = (*x, *y);
                    }
                }
                BevyToUi::EditModeChanged { mode } => {
                    state.edit_mode = *mode;
                }
                BevyToUi::MeshEditModeChanged {
                    active,
                    selection_mode,
                    tool,
                } => {
                    state.mesh_edit_active = *active;
                    state.mesh_selection_mode = *selection_mode;
                    state.mesh_edit_tool = *tool;
                }
                BevyToUi::MeshEditSelectionChanged {
                    vertex_count,
                    edge_count,
                    face_count,
                } => {
                    state.selected_vertex_count = *vertex_count;
                    state.selected_edge_count = *edge_count;
                    state.selected_face_count = *face_count;
                }
                _ => {}
            }
        }

        match self.to_ui.send(msg) {
            Ok(()) => {
                // Set pending flag so component knows to check for messages
                self.pending_messages.store(true, Ordering::Release);
            }
            Err(e) => tracing::error!("DioxusBridgeHandle::send() failed: {:?}", e),
        }
    }
}
