//! Webview management for Pentimento
//!
//! Provides platform-specific implementations for webview compositing:
//! - Capture mode: Renders webview offscreen and captures framebuffer for texture upload
//! - Overlay mode: Creates transparent child window that composites via desktop compositor
//! - CEF mode: Uses Chromium Embedded Framework for offscreen rendering (requires `cef` feature)
//! - Dioxus mode: Native Rust UI with Dioxus (requires `dioxus` feature)

mod error;

#[cfg(target_os = "linux")]
mod platform_linux;
#[cfg(target_os = "linux")]
mod platform_linux_overlay;
#[cfg(all(target_os = "linux", feature = "cef"))]
mod platform_linux_cef;
#[cfg(all(target_os = "linux", feature = "dioxus"))]
mod platform_linux_dioxus;
#[cfg(target_os = "windows")]
mod platform_windows;

pub use error::WebviewError;

#[cfg(target_os = "linux")]
pub use platform_linux_overlay::LinuxOverlayWebview;
#[cfg(all(target_os = "linux", feature = "cef"))]
pub use platform_linux_cef::LinuxCefWebview;
#[cfg(all(target_os = "linux", feature = "dioxus"))]
pub use platform_linux_dioxus::LinuxDioxusRenderer;

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
        // Start NOT dirty - wait for warmup to complete before first capture
        let dirty = Arc::new(AtomicBool::new(false));
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
    /// Returns None if the UI hasn't changed or if the webview isn't ready.
    pub fn capture_if_dirty(&mut self) -> Option<image::RgbaImage> {
        // Only attempt capture if webview is ready
        if !self.is_ready() {
            return None;
        }

        if self.dirty.swap(false, Ordering::SeqCst) {
            self.inner.capture()
        } else {
            None
        }
    }

    /// Check if the webview is ready for capture operations
    pub fn is_ready(&self) -> bool {
        #[cfg(target_os = "linux")]
        return self.inner.is_ready();

        #[cfg(target_os = "windows")]
        return true; // Windows implementation TBD
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

    /// Update the scale factor for HiDPI rendering
    pub fn set_scale_factor(&mut self, scale_factor: f64) {
        self.inner.set_scale_factor(scale_factor);
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

/// Overlay webview that composites via transparent child window
/// This mode uses the desktop compositor for blending, avoiding framebuffer capture
pub struct OverlayWebview {
    #[cfg(target_os = "linux")]
    inner: platform_linux_overlay::LinuxOverlayWebview,

    size: (u32, u32),

    // IPC channels
    to_ui_tx: mpsc::UnboundedSender<BevyToUi>,
    from_ui_rx: mpsc::UnboundedReceiver<UiToBevy>,
}

impl OverlayWebview {
    /// Create a new overlay webview as a child of the given window
    ///
    /// # Arguments
    /// * `parent_window` - Raw window handle from Bevy's primary window
    /// * `html_content` - HTML content to load
    /// * `size` - Initial size (width, height)
    #[cfg(target_os = "linux")]
    pub fn new(
        parent_window: raw_window_handle::RawWindowHandle,
        html_content: &str,
        size: (u32, u32),
    ) -> Result<Self, WebviewError> {
        let (to_ui_tx, _to_ui_rx) = mpsc::unbounded_channel();
        let (from_ui_tx, from_ui_rx) = mpsc::unbounded_channel();

        let inner = platform_linux_overlay::LinuxOverlayWebview::new(
            parent_window,
            html_content,
            size,
            from_ui_tx,
        )?;

        Ok(Self {
            inner,
            size,
            to_ui_tx,
            from_ui_rx,
        })
    }

    /// Poll for events. Call this each frame.
    pub fn poll(&mut self) {
        self.inner.poll();
    }

    /// Check if the webview is ready
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }

    /// Get the current size of the webview
    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// Resize the webview
    pub fn resize(&mut self, width: u32, height: u32) {
        self.size = (width, height);
        self.inner.resize(width, height);
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

    /// Set the position of the overlay window (for tracking parent window moves)
    pub fn set_position(&mut self, x: i32, y: i32) {
        self.inner.set_position(x, y);
    }

    /// Show or hide the overlay window
    pub fn set_visible(&mut self, visible: bool) {
        self.inner.set_visible(visible);
    }

    /// Sync overlay visibility with parent window state
    /// Call this when the parent Bevy window is minimized/restored
    pub fn sync_visibility(&mut self, parent_visible: bool) {
        self.inner.sync_visibility(parent_visible);
    }
}

/// CEF-based offscreen webview that can be captured as a texture
///
/// Uses Chromium Embedded Framework instead of WebKitGTK.
/// Requires the `cef` feature and CEF binaries to be installed.
#[cfg(feature = "cef")]
pub struct CefWebview {
    #[cfg(target_os = "linux")]
    inner: platform_linux_cef::LinuxCefWebview,

    dirty: Arc<AtomicBool>,
    size: (u32, u32),

    // IPC channels
    to_ui_tx: mpsc::UnboundedSender<BevyToUi>,
    to_ui_rx: mpsc::UnboundedReceiver<BevyToUi>,
    from_ui_rx: mpsc::UnboundedReceiver<UiToBevy>,
}

#[cfg(feature = "cef")]
impl CefWebview {
    /// Create a new CEF offscreen webview with the given HTML content
    pub fn new(html_content: &str, size: (u32, u32)) -> Result<Self, WebviewError> {
        let dirty = Arc::new(AtomicBool::new(false));
        let (to_ui_tx, to_ui_rx) = mpsc::unbounded_channel();
        let (from_ui_tx, from_ui_rx) = mpsc::unbounded_channel();

        #[cfg(target_os = "linux")]
        let inner = platform_linux_cef::LinuxCefWebview::new(
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
            to_ui_rx,
            from_ui_rx,
        })
    }

    /// Poll for events. Call this each frame from Bevy's main loop.
    /// Also injects any pending messages into JavaScript.
    pub fn poll(&mut self) {
        self.inner.poll();

        // Inject any pending messages into JavaScript via the bridge's receiver function
        while let Ok(msg) = self.to_ui_rx.try_recv() {
            if let Ok(json) = serde_json::to_string(&msg) {
                // Call the bridge's receive function (set up by Svelte in native mode)
                let js = format!(
                    r#"if (window.__PENTIMENTO_RECEIVE__) {{ window.__PENTIMENTO_RECEIVE__('{}'); }}"#,
                    json.replace('\\', "\\\\").replace('\'', "\\'")
                );
                let _ = self.inner.eval(&js);
            }
        }
    }

    /// Capture the framebuffer if the UI has changed since last capture.
    ///
    /// Returns Arc-wrapped BGRA pixel data with dimensions (data, width, height).
    /// The Arc allows zero-copy sharing - callers can clone cheaply or unwrap for owned data.
    /// Use with `TextureFormat::Bgra8UnormSrgb` for zero-conversion texture upload.
    pub fn capture_if_dirty(&mut self) -> Option<(Arc<Vec<u8>>, u32, u32)> {
        if !self.is_ready() {
            return None;
        }

        if self.dirty.swap(false, Ordering::SeqCst) {
            self.inner.capture()
        } else {
            None
        }
    }

    /// Check if the webview is ready for capture operations
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }

    /// Force a capture regardless of dirty state
    ///
    /// Returns Arc-wrapped BGRA pixel data with dimensions (data, width, height).
    pub fn capture(&mut self) -> Option<(Arc<Vec<u8>>, u32, u32)> {
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

    /// Open Chrome DevTools for debugging the webview (Ctrl+Shift+I)
    pub fn show_dev_tools(&self) {
        self.inner.show_dev_tools();
    }
}

/// Dioxus-based native UI renderer
///
/// Uses Dioxus with native Rust rendering instead of a browser engine.
/// Requires the `dioxus` feature.
#[cfg(feature = "dioxus")]
pub struct DioxusWebview {
    #[cfg(target_os = "linux")]
    inner: platform_linux_dioxus::LinuxDioxusRenderer,

    dirty: Arc<AtomicBool>,
    size: (u32, u32),
}

#[cfg(feature = "dioxus")]
impl DioxusWebview {
    /// Create a new Dioxus-based UI renderer
    pub fn new(size: (u32, u32)) -> Result<Self, WebviewError> {
        let dirty = Arc::new(AtomicBool::new(false));

        #[cfg(target_os = "linux")]
        let inner = platform_linux_dioxus::LinuxDioxusRenderer::new(size, dirty.clone())?;

        Ok(Self {
            inner,
            dirty,
            size,
        })
    }

    /// Poll for events. Call this each frame from Bevy's main loop.
    pub fn poll(&mut self) {
        self.inner.poll();
    }

    /// Capture the framebuffer if the UI has changed since last capture.
    ///
    /// Returns RGBA pixel data with dimensions (data, width, height).
    pub fn capture_if_dirty(&mut self) -> Option<(Vec<u8>, u32, u32)> {
        if !self.is_ready() {
            return None;
        }

        self.inner.capture_if_dirty()
    }

    /// Check if the renderer is ready for capture operations
    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }

    /// Force a capture regardless of dirty state
    pub fn capture(&mut self) -> Option<(Vec<u8>, u32, u32)> {
        self.dirty.store(false, Ordering::SeqCst);
        self.inner.capture()
    }

    /// Mark the UI as dirty, triggering a capture on next poll
    pub fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::SeqCst);
    }

    /// Get the current size of the renderer
    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// Resize the renderer
    pub fn resize(&mut self, width: u32, height: u32) {
        self.size = (width, height);
        self.inner.resize(width, height);
        self.mark_dirty();
    }

    /// Forward a mouse event to Dioxus
    pub fn send_mouse_event(&mut self, event: MouseEvent) {
        self.inner.inject_mouse(event);
    }

    /// Forward a keyboard event to Dioxus
    pub fn send_keyboard_event(&mut self, event: KeyboardEvent) {
        self.inner.inject_keyboard(event);
    }

    /// Send a message to the Dioxus UI
    pub fn send_to_ui(&mut self, msg: BevyToUi) {
        self.inner.send_to_ui(msg);
    }

    /// Try to receive a message from the Dioxus UI (non-blocking)
    pub fn try_recv_from_ui(&mut self) -> Option<UiToBevy> {
        self.inner.try_recv_from_ui()
    }
}
