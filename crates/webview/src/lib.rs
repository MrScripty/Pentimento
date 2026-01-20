//! Offscreen webview management for Pentimento
//!
//! Provides platform-specific implementations for rendering a webview
//! offscreen and capturing its framebuffer for compositing in Bevy.

mod error;

#[cfg(target_os = "linux")]
mod platform_linux;
#[cfg(target_os = "windows")]
mod platform_windows;

pub use error::WebviewError;

use pentimento_ipc::{BevyToUi, KeyboardEvent, MouseEvent, UiToBevy};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Offscreen webview that can be captured as a texture
pub struct OffscreenWebview {
    #[cfg(target_os = "linux")]
    inner: platform_linux::LinuxWebview,
    #[cfg(target_os = "windows")]
    inner: platform_windows::WindowsWebview,

    dirty: Arc<AtomicBool>,
    size: (u32, u32),

    // IPC channels
    to_ui_tx: mpsc::UnboundedSender<BevyToUi>,
    from_ui_rx: mpsc::UnboundedReceiver<UiToBevy>,
}

impl OffscreenWebview {
    /// Create a new offscreen webview with the given HTML content
    pub fn new(html_content: &str, size: (u32, u32)) -> Result<Self, WebviewError> {
        let dirty = Arc::new(AtomicBool::new(true)); // Start dirty to trigger initial capture
        let (to_ui_tx, _to_ui_rx) = mpsc::unbounded_channel();
        let (from_ui_tx, from_ui_rx) = mpsc::unbounded_channel();

        #[cfg(target_os = "linux")]
        let inner = platform_linux::LinuxWebview::new(
            html_content,
            size,
            dirty.clone(),
            from_ui_tx,
        )?;

        #[cfg(target_os = "windows")]
        let inner = platform_windows::WindowsWebview::new(
            html_content,
            size,
            dirty.clone(),
            from_ui_tx,
        )?;

        Ok(Self {
            inner,
            dirty,
            size,
            to_ui_tx,
            from_ui_rx,
        })
    }

    /// Poll for events. Call this each frame from Bevy's main loop.
    pub fn poll(&mut self) {
        self.inner.poll();
    }

    /// Capture the framebuffer if the UI has changed since last capture.
    /// Returns None if the UI hasn't changed.
    pub fn capture_if_dirty(&mut self) -> Option<image::RgbaImage> {
        if self.dirty.swap(false, Ordering::SeqCst) {
            self.inner.capture()
        } else {
            None
        }
    }

    /// Force a capture regardless of dirty state
    pub fn capture(&mut self) -> Option<image::RgbaImage> {
        self.dirty.store(false, Ordering::SeqCst);
        self.inner.capture()
    }

    /// Mark the UI as dirty, triggering a capture on next poll
    pub fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::SeqCst);
    }

    /// Get the current size of the webview
    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// Resize the webview
    pub fn resize(&mut self, width: u32, height: u32) {
        self.size = (width, height);
        self.inner.resize(width, height);
        self.mark_dirty();
    }

    /// Forward a mouse event to the webview
    pub fn send_mouse_event(&mut self, event: MouseEvent) {
        self.inner.inject_mouse(event);
    }

    /// Forward a keyboard event to the webview
    pub fn send_keyboard_event(&mut self, event: KeyboardEvent) {
        self.inner.inject_keyboard(event);
    }

    /// Send a message to the Svelte UI
    pub fn send_to_ui(&self, msg: BevyToUi) -> Result<(), WebviewError> {
        self.to_ui_tx
            .send(msg)
            .map_err(|_| WebviewError::ChannelClosed)
    }

    /// Try to receive a message from the Svelte UI (non-blocking)
    pub fn try_recv_from_ui(&mut self) -> Option<UiToBevy> {
        self.from_ui_rx.try_recv().ok()
    }

    /// Evaluate JavaScript in the webview
    pub fn eval(&self, js: &str) -> Result<(), WebviewError> {
        self.inner.eval(js)
    }
}
