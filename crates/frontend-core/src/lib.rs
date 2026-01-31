//! Frontend core abstractions for Pentimento
//!
//! Defines the `CompositeBackend` trait that abstracts over different UI rendering backends.

use std::sync::Arc;

use pentimento_ipc::{BevyToUi, KeyboardEvent, MouseEvent, UiToBevy};

/// Result of capturing the UI framebuffer
#[derive(Debug, Clone)]
pub enum CaptureResult {
    /// RGBA pixel data with dimensions
    Rgba(Vec<u8>, u32, u32),
    /// BGRA pixel data (shared) with dimensions
    Bgra(Arc<Vec<u8>>, u32, u32),
    /// Compositor-managed rendering (no capture needed)
    CompositorManaged,
}

/// Lifecycle state of a backend
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendLifecycle {
    /// Backend is initializing
    Initializing,
    /// Backend is loading (e.g., web page loading)
    Loading {
        /// Estimated frames remaining, if known
        frames_remaining: Option<u32>,
    },
    /// Backend is ready for normal operation
    Ready,
    /// Backend is resizing
    Resizing {
        /// Frames remaining until resize completes
        frames_remaining: u32,
    },
    /// Backend encountered an error
    Error,
}

/// Errors that can occur in frontend operations
#[derive(Debug, thiserror::Error)]
pub enum FrontendError {
    /// Failed to send message to UI
    #[error("Failed to send message to UI: {0}")]
    SendFailed(String),

    /// Failed to receive message from UI
    #[error("Failed to receive message from UI: {0}")]
    ReceiveFailed(String),

    /// Backend is not ready
    #[error("Backend is not ready")]
    NotReady,

    /// Capture failed
    #[error("Capture failed: {0}")]
    CaptureFailed(String),

    /// Invalid dimensions
    #[error("Invalid dimensions: {width}x{height}")]
    InvalidDimensions { width: u32, height: u32 },

    /// Backend-specific error
    #[error("Backend error: {0}")]
    Backend(String),
}

/// Trait for UI rendering backends that can be composited into the scene
pub trait CompositeBackend {
    /// Poll the backend for events and updates
    fn poll(&mut self);

    /// Check if the backend is ready for rendering
    fn is_ready(&self) -> bool;

    /// Capture the current framebuffer if it has changed
    ///
    /// Returns `Some(CaptureResult)` if the framebuffer has been updated,
    /// or `None` if the content hasn't changed since the last capture.
    fn capture_if_dirty(&mut self) -> Option<CaptureResult>;

    /// Get the current size of the backend surface
    fn size(&self) -> (u32, u32);

    /// Resize the backend surface
    fn resize(&mut self, width: u32, height: u32);

    /// Send a mouse event to the backend
    fn send_mouse_event(&mut self, event: MouseEvent);

    /// Send a keyboard event to the backend
    fn send_keyboard_event(&mut self, event: KeyboardEvent);

    /// Send a message to the UI
    fn send_to_ui(&mut self, msg: BevyToUi) -> Result<(), FrontendError>;

    /// Try to receive a message from the UI (non-blocking)
    fn try_recv_from_ui(&mut self) -> Option<UiToBevy>;

    /// Open developer tools for debugging (CEF only)
    ///
    /// Default implementation does nothing. Override in backends that support DevTools.
    fn show_dev_tools(&self) {
        // Default: no-op for backends that don't support DevTools
    }
}
