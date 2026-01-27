//! Dioxus frontend backend implementation for Pentimento
//!
//! Provides a thin wrapper implementing the `CompositeBackend` trait for Dioxus mode.
//! The actual rendering (Vello, BlitzDocument) happens in `crates/app/src/render/ui_dioxus.rs`
//! because it requires Bevy render world access.

mod bridge;

pub use bridge::{DioxusBridge, DioxusBridgeHandle, DioxusBridgeHandleExt};
use pentimento_frontend_core::{CaptureResult, CompositeBackend, FrontendError};
use pentimento_ipc::{BevyToUi, KeyboardEvent, MouseEvent, UiToBevy};

/// Backend implementation for Dioxus-based UI rendering.
///
/// This is a thin wrapper that provides the `CompositeBackend` interface
/// for the Dioxus UI system. The actual rendering is handled by Vello
/// directly to GPU textures, so this backend returns `CompositorManaged`
/// for framebuffer capture.
pub struct DioxusBackend {
    /// Handle for IPC communication with the Dioxus UI
    bridge_handle: DioxusBridgeHandle,
    /// Current surface width
    width: u32,
    /// Current surface height
    height: u32,
    /// Whether the backend is ready for rendering
    ready: bool,
    /// Queued mouse events to be processed
    mouse_events: Vec<MouseEvent>,
    /// Queued keyboard events to be processed
    keyboard_events: Vec<KeyboardEvent>,
}

impl DioxusBackend {
    /// Create a new Dioxus backend with the given bridge handle
    pub fn new(bridge_handle: DioxusBridgeHandle) -> Self {
        Self {
            bridge_handle,
            width: 800,
            height: 600,
            ready: false,
            mouse_events: Vec::new(),
            keyboard_events: Vec::new(),
        }
    }

    /// Create a new Dioxus backend with initial dimensions
    pub fn with_size(bridge_handle: DioxusBridgeHandle, width: u32, height: u32) -> Self {
        Self {
            bridge_handle,
            width,
            height,
            ready: false,
            mouse_events: Vec::new(),
            keyboard_events: Vec::new(),
        }
    }

    /// Mark the backend as ready
    pub fn set_ready(&mut self, ready: bool) {
        self.ready = ready;
    }

    /// Get a reference to the bridge handle
    pub fn bridge_handle(&self) -> &DioxusBridgeHandle {
        &self.bridge_handle
    }

    /// Get a mutable reference to the bridge handle
    pub fn bridge_handle_mut(&mut self) -> &mut DioxusBridgeHandle {
        &mut self.bridge_handle
    }

    /// Take queued mouse events (drains the queue)
    pub fn take_mouse_events(&mut self) -> Vec<MouseEvent> {
        std::mem::take(&mut self.mouse_events)
    }

    /// Take queued keyboard events (drains the queue)
    pub fn take_keyboard_events(&mut self) -> Vec<KeyboardEvent> {
        std::mem::take(&mut self.keyboard_events)
    }
}

impl CompositeBackend for DioxusBackend {
    fn poll(&mut self) {
        // Dioxus polling is handled externally by the Bevy systems
        // that manage the BlitzDocument and VirtualDom
    }

    fn is_ready(&self) -> bool {
        self.ready
    }

    fn capture_if_dirty(&mut self) -> Option<CaptureResult> {
        // Dioxus uses Vello which renders directly to GPU textures.
        // No CPU-side framebuffer capture is needed.
        Some(CaptureResult::CompositorManaged)
    }

    fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.width = width;
            self.height = height;
            tracing::debug!("DioxusBackend resized to {}x{}", width, height);
        }
    }

    fn send_mouse_event(&mut self, event: MouseEvent) {
        // Queue the event for processing by the Bevy systems
        self.mouse_events.push(event);
    }

    fn send_keyboard_event(&mut self, event: KeyboardEvent) {
        // Queue the event for processing by the Bevy systems
        self.keyboard_events.push(event);
    }

    fn send_to_ui(&mut self, msg: BevyToUi) -> Result<(), FrontendError> {
        self.bridge_handle.send(msg);
        Ok(())
    }

    fn try_recv_from_ui(&mut self) -> Option<UiToBevy> {
        self.bridge_handle.try_recv()
    }
}
