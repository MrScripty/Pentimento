//! Frontend overlay backend for Pentimento
//!
//! This crate provides an overlay-based UI rendering backend using transparent GTK windows.
//! The desktop compositor handles the actual blending, avoiding the need for framebuffer capture.
//!
//! Input handling uses a selective passthrough approach:
//! - UI regions (toolbar, sidebar) receive native input for proper Svelte interaction
//! - The 3D viewport area is click-through, passing events to Bevy underneath

pub mod sync;
pub mod window;

use std::cell::RefCell;
use std::rc::Rc;

use gio::Cancellable;
use gtk::prelude::*;
use pentimento_frontend_core::{CaptureResult, CompositeBackend, FrontendError};
use pentimento_ipc::{BevyToUi, KeyboardEvent, MouseButton, MouseEvent, UiToBevy};
use raw_window_handle::RawWindowHandle;
use tokio::sync::mpsc;
use webkit2gtk::{LoadEvent, WebViewExt};
use wry::WebViewBuilderExtUnix;

/// Errors specific to overlay backend operations
#[derive(Debug, thiserror::Error)]
pub enum OverlayError {
    /// GTK initialization failed
    #[error("GTK initialization failed: {0}")]
    GtkInit(String),

    /// Window creation failed
    #[error("Failed to create window: {0}")]
    WindowCreate(String),

    /// WebView creation failed
    #[error("Failed to create webview: {0}")]
    WebviewCreate(String),

    /// JavaScript evaluation failed
    #[error("Failed to evaluate script: {0}")]
    EvalScript(String),
}

/// Overlay webview state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayState {
    /// Waiting for content to load
    Initializing,
    /// Content loaded, ready for use
    Ready,
}

/// Overlay backend using transparent GTK window
///
/// This backend creates a transparent window that overlays the Bevy window.
/// The compositor handles blending, so `capture_if_dirty()` returns `CompositorManaged`.
pub struct OverlayBackend {
    webview: wry::WebView,
    webkit_webview: webkit2gtk::WebView,
    window: gtk::Window,
    container: gtk::Fixed,
    size: (u32, u32),
    state: OverlayState,
    load_finished: Rc<RefCell<bool>>,
    /// Parent window XID for state tracking (X11 only)
    parent_xid: Option<u64>,
    /// Channel receiver for UI messages
    from_ui_rx: mpsc::UnboundedReceiver<UiToBevy>,
}

impl OverlayBackend {
    /// Create a new overlay backend
    pub fn new(
        parent_handle: RawWindowHandle,
        html_content: &str,
        size: (u32, u32),
    ) -> Result<Self, OverlayError> {
        // Initialize GTK if not already done
        if !gtk::is_initialized() {
            gtk::init().map_err(|e| OverlayError::GtkInit(e.to_string()))?;
        }

        // Create transparent window
        let window = window::create_transparent_window(size)?;

        // Create a Fixed container for the webview
        let container = gtk::Fixed::new();
        container.set_size_request(size.0 as i32, size.1 as i32);
        window.add(&container);

        // Set up load detection and IPC channel
        let load_finished = Rc::new(RefCell::new(false));
        let (from_ui_tx, from_ui_rx) = mpsc::unbounded_channel();

        // Create the webview with explicit bounds
        let load_finished_clone = load_finished.clone();
        let webview = wry::WebViewBuilder::new()
            .with_html(html_content)
            .with_transparent(true)
            .with_bounds(wry::Rect {
                position: wry::dpi::PhysicalPosition::new(0, 0).into(),
                size: wry::dpi::PhysicalSize::new(size.0, size.1).into(),
            })
            .with_ipc_handler(move |msg: wry::http::Request<String>| {
                let body = msg.body();
                if let Ok(ui_msg) = serde_json::from_str::<UiToBevy>(body) {
                    let _ = from_ui_tx.send(ui_msg);
                }
            })
            .build_gtk(&container)
            .map_err(|e| OverlayError::WebviewCreate(e.to_string()))?;

        // Find the WebKitWebView to set up load detection and sizing
        let webkit_webview = Self::find_webkit_webview(&container)
            .ok_or_else(|| OverlayError::WebviewCreate("Failed to find WebKitWebView".into()))?;

        // Set the webkit_webview size to match the container
        webkit_webview.set_size_request(size.0 as i32, size.1 as i32);

        // Connect load detection handler
        let load_finished_for_handler = load_finished_clone;
        webkit_webview.connect_load_changed(move |_webview, load_event| {
            if load_event == LoadEvent::Finished {
                *load_finished_for_handler.borrow_mut() = true;
                tracing::info!("Overlay WebView content loaded");
            }
        });

        // Position the overlay window and set up window grouping
        let parent_xid = sync::setup_window_relationship(&window, parent_handle);

        // Show the window
        window.show_all();

        // Set up selective input passthrough
        window::update_input_regions(&window, size.0, size.1);

        // Process GTK events to initialize
        for _ in 0..50 {
            if gtk::events_pending() {
                gtk::main_iteration_do(false);
            }
        }

        tracing::info!("Overlay backend created at size {:?}", size);

        Ok(Self {
            webview,
            webkit_webview,
            window,
            container,
            size,
            state: OverlayState::Initializing,
            load_finished,
            parent_xid,
            from_ui_rx,
        })
    }

    /// Find the WebKitWebView widget within a GTK container
    fn find_webkit_webview(container: &gtk::Fixed) -> Option<webkit2gtk::WebView> {
        for child in container.children() {
            if let Ok(wv) = child.clone().downcast::<webkit2gtk::WebView>() {
                return Some(wv);
            }
            if let Ok(bin) = child.downcast::<gtk::Bin>() {
                if let Some(inner) = bin.child() {
                    if let Ok(wv) = inner.downcast::<webkit2gtk::WebView>() {
                        return Some(wv);
                    }
                }
            }
        }
        None
    }

    /// Sync overlay visibility with the given parent window visibility state
    pub fn sync_visibility(&mut self, parent_visible: bool) {
        sync::sync_visibility(&self.window, parent_visible);
    }

    /// Set the overlay window position
    pub fn set_position(&mut self, x: i32, y: i32) {
        window::set_position(&self.window, x, y);
    }

    /// Set overlay visibility
    pub fn set_visible(&mut self, visible: bool) {
        if visible {
            self.window.show();
        } else {
            self.window.hide();
        }
    }

    /// Inject a mouse event into the webview via JavaScript
    pub fn inject_mouse(&mut self, event: MouseEvent) {
        let js = match event {
            MouseEvent::Move { x, y } => {
                format!(
                    r#"(function() {{
                        const target = document.elementFromPoint({x}, {y}) || document.body;
                        target.dispatchEvent(new MouseEvent('mousemove', {{
                            bubbles: true, cancelable: true,
                            clientX: {x}, clientY: {y}, view: window
                        }}));
                    }})()"#,
                    x = x,
                    y = y
                )
            }
            MouseEvent::ButtonDown { button, x, y } => {
                let button_num = match button {
                    MouseButton::Left => 0,
                    MouseButton::Middle => 1,
                    MouseButton::Right => 2,
                };
                format!(
                    r#"(function() {{
                        const target = document.elementFromPoint({x}, {y}) || document.body;
                        target.dispatchEvent(new MouseEvent('mousedown', {{
                            bubbles: true, cancelable: true,
                            clientX: {x}, clientY: {y}, button: {button}, view: window
                        }}));
                    }})()"#,
                    x = x,
                    y = y,
                    button = button_num
                )
            }
            MouseEvent::ButtonUp { button, x, y } => {
                let button_num = match button {
                    MouseButton::Left => 0,
                    MouseButton::Middle => 1,
                    MouseButton::Right => 2,
                };
                format!(
                    r#"(function() {{
                        const target = document.elementFromPoint({x}, {y}) || document.body;
                        target.dispatchEvent(new MouseEvent('mouseup', {{
                            bubbles: true, cancelable: true,
                            clientX: {x}, clientY: {y}, button: {button}, view: window
                        }}));
                        if ({button} === 0) {{
                            target.dispatchEvent(new MouseEvent('click', {{
                                bubbles: true, cancelable: true,
                                clientX: {x}, clientY: {y}, button: 0, view: window
                            }}));
                        }}
                    }})()"#,
                    x = x,
                    y = y,
                    button = button_num
                )
            }
            MouseEvent::Scroll {
                delta_x,
                delta_y,
                x,
                y,
            } => {
                format!(
                    r#"(function() {{
                        const target = document.elementFromPoint({x}, {y}) || document.body;
                        target.dispatchEvent(new WheelEvent('wheel', {{
                            bubbles: true, cancelable: true,
                            clientX: {x}, clientY: {y},
                            deltaX: {delta_x}, deltaY: {delta_y}, deltaMode: 0,
                            view: window
                        }}));
                    }})()"#,
                    x = x,
                    y = y,
                    delta_x = delta_x,
                    delta_y = delta_y
                )
            }
        };

        let _ = self.webview.evaluate_script(&js);
    }

    /// Inject a keyboard event into the webview via JavaScript
    pub fn inject_keyboard(&mut self, event: KeyboardEvent) {
        let event_type = if event.pressed { "keydown" } else { "keyup" };
        let key_escaped = event.key.replace('\\', "\\\\").replace('\'', "\\'");

        let js = format!(
            r#"(function() {{
                const target = document.activeElement || document.body;
                target.dispatchEvent(new KeyboardEvent('{event_type}', {{
                    bubbles: true, cancelable: true,
                    key: '{key}',
                    shiftKey: {shift}, ctrlKey: {ctrl},
                    altKey: {alt}, metaKey: {meta},
                    view: window
                }}));
            }})()"#,
            event_type = event_type,
            key = key_escaped,
            shift = event.modifiers.shift,
            ctrl = event.modifiers.ctrl,
            alt = event.modifiers.alt,
            meta = event.modifiers.meta
        );

        let _ = self.webview.evaluate_script(&js);
    }

    /// Evaluate JavaScript in the webview
    pub fn eval(&self, js: &str) -> Result<(), OverlayError> {
        self.webview
            .evaluate_script(js)
            .map_err(|e| OverlayError::EvalScript(e.to_string()))
    }
}

impl CompositeBackend for OverlayBackend {
    fn poll(&mut self) {
        // Pump GTK events
        for _ in 0..10 {
            if gtk::events_pending() {
                gtk::main_iteration_do(false);
            }
        }

        // Sync visibility with parent window state
        sync::check_parent_visibility(&self.window, self.parent_xid);

        // Update state
        if self.state == OverlayState::Initializing && *self.load_finished.borrow() {
            // Set viewport dimensions via JavaScript
            let (width, height) = self.size;
            self.webkit_webview.run_javascript(
                &format!(
                    "document.body.style.width = '{}px'; \
                     document.body.style.height = '{}px'; \
                     document.documentElement.style.width = '{}px'; \
                     document.documentElement.style.height = '{}px';",
                    width, height, width, height
                ),
                Cancellable::NONE,
                |_| {},
            );

            self.state = OverlayState::Ready;
            tracing::info!("Overlay backend ready");
        }
    }

    fn is_ready(&self) -> bool {
        self.state == OverlayState::Ready
    }

    fn capture_if_dirty(&mut self) -> Option<CaptureResult> {
        // Overlay mode uses compositor-managed rendering
        // No framebuffer capture needed - compositor handles blending
        Some(CaptureResult::CompositorManaged)
    }

    fn size(&self) -> (u32, u32) {
        self.size
    }

    fn resize(&mut self, width: u32, height: u32) {
        if self.size == (width, height) {
            return;
        }

        self.size = (width, height);

        // Resize all components
        window::resize_window(&self.window, width, height);
        self.container.set_size_request(width as i32, height as i32);
        self.webkit_webview
            .set_size_request(width as i32, height as i32);

        // Update wry webview bounds
        self.webview
            .set_bounds(wry::Rect {
                position: wry::dpi::PhysicalPosition::new(0, 0).into(),
                size: wry::dpi::PhysicalSize::new(width, height).into(),
            })
            .ok();

        // Force WebKit to re-layout via JavaScript
        self.webkit_webview.run_javascript(
            &format!(
                "window.dispatchEvent(new Event('resize')); \
                 document.body.style.width = '{}px'; \
                 document.body.style.height = '{}px'; \
                 document.documentElement.style.width = '{}px'; \
                 document.documentElement.style.height = '{}px';",
                width, height, width, height
            ),
            Cancellable::NONE,
            |_| {},
        );

        // Update input regions for the new size
        window::update_input_regions(&self.window, width, height);

        // Pump GTK events to help the resize propagate
        for _ in 0..30 {
            if gtk::events_pending() {
                gtk::main_iteration_do(false);
            }
        }

        tracing::info!("Overlay backend resized to ({}, {})", width, height);
    }

    fn send_mouse_event(&mut self, event: MouseEvent) {
        self.inject_mouse(event);
    }

    fn send_keyboard_event(&mut self, event: KeyboardEvent) {
        self.inject_keyboard(event);
    }

    fn send_to_ui(&mut self, msg: BevyToUi) -> Result<(), FrontendError> {
        let json = serde_json::to_string(&msg)
            .map_err(|e| FrontendError::SendFailed(e.to_string()))?;

        let js = format!("window.dispatchEvent(new CustomEvent('bevy-message', {{ detail: {} }}));", json);
        self.webview
            .evaluate_script(&js)
            .map_err(|e| FrontendError::SendFailed(e.to_string()))
    }

    fn try_recv_from_ui(&mut self) -> Option<UiToBevy> {
        self.from_ui_rx.try_recv().ok()
    }
}
