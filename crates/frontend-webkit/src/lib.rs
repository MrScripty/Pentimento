//! WebKit/capture mode backend for Pentimento
//!
//! This crate provides a WebKitGTK-based backend for the Pentimento UI,
//! implementing the `CompositeBackend` trait for integration with Bevy rendering.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use gio::Cancellable;
use pentimento_frontend_core::{CaptureResult, CompositeBackend, FrontendError};
use pentimento_ipc::{BevyToUi, KeyboardEvent, MouseEvent, UiToBevy};
use tokio::sync::mpsc;
use webkit2gtk::{WebView as WebKitWebView, WebViewExt};

pub mod capture;
pub mod initialization;
pub mod input_keyboard;
pub mod input_mouse;
pub mod resize;
pub mod state;
pub mod utils;

use state::{WebviewState, READY_GTK_ITERATIONS, WARMUP_FRAMES, WARMUP_GTK_ITERATIONS};

/// WebKit backend for Pentimento capture mode
///
/// Uses an offscreen GTK window with WebKitGTK to render the UI,
/// capturing snapshots for compositing with Bevy.
pub struct WebKitBackend {
    webview: wry::WebView,
    webkit_webview: WebKitWebView,
    #[allow(dead_code)]
    container: gtk::Fixed,
    /// Offscreen window to host the container (needed for widget realization)
    #[allow(dead_code)]
    offscreen_window: gtk::OffscreenWindow,
    size: (u32, u32),
    dirty: Arc<AtomicBool>,
    /// Cached snapshot result from async capture
    snapshot_cache: Rc<RefCell<Option<image::RgbaImage>>>,
    /// Flag indicating snapshot is in progress
    snapshot_pending: Rc<RefCell<bool>>,
    /// Current lifecycle state
    state: WebviewState,
    /// Flag set when WebKit reports load finished
    load_finished: Rc<RefCell<bool>>,
    /// Frames to wait after mouse event before allowing capture
    /// Prevents capturing intermediate render state during RAF callback processing
    frames_until_capture_allowed: u32,
    /// Current device scale factor for HiDPI rendering
    scale_factor: f64,
    /// Channel for sending messages to the UI
    to_ui_tx: Option<mpsc::UnboundedSender<BevyToUi>>,
    /// Channel for receiving messages from the UI
    from_ui_rx: Option<mpsc::UnboundedReceiver<UiToBevy>>,
}

impl WebKitBackend {
    /// Set channels for IPC communication
    pub fn set_channels(
        &mut self,
        to_ui_tx: mpsc::UnboundedSender<BevyToUi>,
        from_ui_rx: mpsc::UnboundedReceiver<UiToBevy>,
    ) {
        self.to_ui_tx = Some(to_ui_tx);
        self.from_ui_rx = Some(from_ui_rx);
    }

    /// Poll the GTK event loop and update state
    fn poll_internal(&mut self) {
        // Determine how many GTK iterations based on current state
        let iterations = match self.state {
            WebviewState::Initializing | WebviewState::WarmingUp { .. } => WARMUP_GTK_ITERATIONS,
            WebviewState::Ready | WebviewState::Resizing { .. } => READY_GTK_ITERATIONS,
        };

        // Pump GTK events
        for _ in 0..iterations {
            if gtk::events_pending() {
                gtk::main_iteration_do(false);
            }
        }

        // Handle state transitions
        self.update_state();
    }

    /// Update the webview state machine
    fn update_state(&mut self) {
        match self.state {
            WebviewState::Initializing => {
                if *self.load_finished.borrow() {
                    tracing::info!("WebView load complete, transitioning to WarmingUp state");
                    // Set viewport dimensions via JavaScript to ensure WebKit knows the size
                    // IMPORTANT: Must update viewport meta tag, not just CSS dimensions!
                    // CSS dimensions only affect layout, not the coordinate space for elementFromPoint()
                    let (width, height) = self.logical_size();
                    let scale = if self.scale_factor > 0.0 {
                        self.scale_factor
                    } else {
                        1.0
                    };
                    self.webkit_webview.run_javascript(
                        &format!(
                            r#"(function() {{
                                // Update viewport meta tag with exact dimensions
                                var meta = document.querySelector('meta[name="viewport"]');
                                if (meta) {{
                                    meta.setAttribute('content', 'width={width}, height={height}, initial-scale={scale}, minimum-scale={scale}, maximum-scale={scale}, user-scalable=no');
                                }} else {{
                                    meta = document.createElement('meta');
                                    meta.name = 'viewport';
                                    meta.content = 'width={width}, height={height}, initial-scale={scale}, minimum-scale={scale}, maximum-scale={scale}, user-scalable=no';
                                    document.head.appendChild(meta);
                                }}
                                // Also set explicit CSS dimensions
                                document.body.style.width = '{width}px';
                                document.body.style.height = '{height}px';
                                document.documentElement.style.width = '{width}px';
                                document.documentElement.style.height = '{height}px';
                                document.body.style.overflow = 'hidden';
                                document.documentElement.style.overflow = 'hidden';
                                console.log('Viewport set to', {width}, 'x', {height});
                            }})()"#,
                            width = width,
                            height = height,
                            scale = scale
                        ),
                        Cancellable::NONE,
                        |_| {},
                    );
                    self.state = WebviewState::WarmingUp {
                        frames_remaining: WARMUP_FRAMES,
                    };
                }
            }
            WebviewState::WarmingUp { frames_remaining } => {
                if frames_remaining == 0 {
                    tracing::info!("Warmup complete, transitioning to Ready state");
                    // Query viewport dimensions to verify coordinate space
                    self.webkit_webview.run_javascript(
                        r#"JSON.stringify({
                            innerWidth: window.innerWidth,
                            innerHeight: window.innerHeight,
                            docWidth: document.documentElement.clientWidth,
                            docHeight: document.documentElement.clientHeight
                        })"#,
                        Cancellable::NONE,
                        move |result| {
                            match result {
                                Ok(js_value) => {
                                    if let Some(value) = js_value.js_value() {
                                        let json_str = value.to_string();
                                        tracing::info!("WebKit viewport dimensions: {}", json_str);
                                    }
                                }
                                Err(e) => tracing::warn!("Failed to query viewport: {}", e),
                            }
                        },
                    );
                    self.state = WebviewState::Ready;
                    // Now safe to mark dirty for first capture
                    self.dirty.store(true, Ordering::SeqCst);
                } else {
                    self.state = WebviewState::WarmingUp {
                        frames_remaining: frames_remaining - 1,
                    };
                }
            }
            WebviewState::Resizing { frames_remaining } => {
                if frames_remaining == 0 {
                    tracing::info!("Resize stabilized, returning to Ready state");
                    // Query viewport dimensions after resize to verify coordinate space
                    self.webkit_webview.run_javascript(
                        r#"JSON.stringify({
                            innerWidth: window.innerWidth,
                            innerHeight: window.innerHeight,
                            docWidth: document.documentElement.clientWidth,
                            docHeight: document.documentElement.clientHeight
                        })"#,
                        Cancellable::NONE,
                        move |result| {
                            match result {
                                Ok(js_value) => {
                                    if let Some(value) = js_value.js_value() {
                                        let json_str = value.to_string();
                                        tracing::info!("WebKit viewport after resize: {}", json_str);
                                    }
                                }
                                Err(e) => tracing::warn!("Failed to query viewport: {}", e),
                            }
                        },
                    );
                    self.state = WebviewState::Ready;
                    // Safe to capture after resize
                    self.dirty.store(true, Ordering::SeqCst);
                } else {
                    self.state = WebviewState::Resizing {
                        frames_remaining: frames_remaining - 1,
                    };
                }
            }
            WebviewState::Ready => {
                // Normal operation, no transition needed
            }
        }
    }
}

impl CompositeBackend for WebKitBackend {
    fn poll(&mut self) {
        self.poll_internal();
    }

    fn is_ready(&self) -> bool {
        self.state == WebviewState::Ready
    }

    fn capture_if_dirty(&mut self) -> Option<CaptureResult> {
        if !self.dirty.swap(false, Ordering::SeqCst) {
            return None;
        }

        self.capture().map(|img| {
            let (width, height) = (img.width(), img.height());
            CaptureResult::Rgba(img.into_raw(), width, height)
        })
    }

    fn size(&self) -> (u32, u32) {
        self.size
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.resize_webview(width, height);
    }

    fn send_mouse_event(&mut self, event: MouseEvent) {
        self.inject_mouse(event);
    }

    fn send_keyboard_event(&mut self, event: KeyboardEvent) {
        self.inject_keyboard(event);
    }

    fn send_to_ui(&mut self, msg: BevyToUi) -> Result<(), FrontendError> {
        if let Some(tx) = &self.to_ui_tx {
            tx.send(msg)
                .map_err(|e| FrontendError::SendFailed(e.to_string()))
        } else {
            // Fall back to JavaScript evaluation for messages without channel
            let js = format!(
                "window.postMessage({}, '*');",
                serde_json::to_string(&msg).unwrap_or_default()
            );
            self.eval(&js)
        }
    }

    fn try_recv_from_ui(&mut self) -> Option<UiToBevy> {
        if let Some(rx) = &mut self.from_ui_rx {
            rx.try_recv().ok()
        } else {
            None
        }
    }
}
