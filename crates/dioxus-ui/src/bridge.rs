//! IPC bridge between Dioxus UI and Bevy
//!
//! Uses Rust channels instead of console.log interception like CEF mode.

use pentimento_ipc::{
    BevyToUi, CameraCommand, DiffusionRequest, LightingSettings, MaterialCommand, ObjectCommand,
    UiToBevy,
};
use std::sync::mpsc;

/// Bridge for sending messages from Dioxus UI to Bevy
#[derive(Clone)]
pub struct DioxusBridge {
    to_bevy: mpsc::Sender<UiToBevy>,
}

// Manual PartialEq implementation for Dioxus Props compatibility.
// The bridge is always considered equal since it's a singleton channel.
impl PartialEq for DioxusBridge {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl DioxusBridge {
    /// Create a new bridge pair
    pub fn new() -> (Self, DioxusBridgeHandle) {
        let (to_bevy_tx, to_bevy_rx) = mpsc::channel();
        let (from_bevy_tx, from_bevy_rx) = mpsc::channel();

        let bridge = Self {
            to_bevy: to_bevy_tx,
        };

        let handle = DioxusBridgeHandle {
            to_ui: from_bevy_tx,
            from_ui: to_bevy_rx,
            to_ui_rx: from_bevy_rx,
        };

        (bridge, handle)
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
    /// Receiver for UI to poll messages from Bevy (used internally)
    to_ui_rx: mpsc::Receiver<BevyToUi>,
}

impl DioxusBridgeHandle {
    /// Try to receive a message from the UI (non-blocking)
    pub fn try_recv(&self) -> Option<UiToBevy> {
        self.from_ui.try_recv().ok()
    }

    /// Send a message to the UI
    pub fn send(&self, msg: BevyToUi) {
        let _ = self.to_ui.send(msg);
    }

    /// Try to receive a message intended for the UI (used by UI polling)
    pub fn try_recv_for_ui(&self) -> Option<BevyToUi> {
        self.to_ui_rx.try_recv().ok()
    }
}
