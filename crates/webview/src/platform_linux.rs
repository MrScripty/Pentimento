//! Linux-specific webview implementation using GTK and WebKitGTK

use crate::error::WebviewError;
use pentimento_ipc::{KeyboardEvent, MouseButton, MouseEvent, UiToBevy};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

use cairo::{Format, ImageSurface};
use gio::prelude::*;
use gtk::prelude::*;
use webkit2gtk::{LoadEvent, SnapshotOptions, SnapshotRegion, WebView as WebKitWebView, WebViewExt};
use wry::WebViewBuilderExtUnix;

/// Webview lifecycle states for managing capture timing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebviewState {
    /// Just created, waiting for content to load
    Initializing,
    /// Content loaded, warming up for first capture
    WarmingUp { frames_remaining: u32 },
    /// Ready for normal capture operations
    Ready,
    /// Resize in progress, waiting for stabilization
    Resizing { frames_remaining: u32 },
}

/// Number of frames to wait during warmup before first capture (~1 second at 60fps)
const WARMUP_FRAMES: u32 = 60;

/// Number of GTK iterations per poll during warmup/initialization
const WARMUP_GTK_ITERATIONS: u32 = 20;

/// Number of frames to wait after resize before capture (increased for WebKit to process)
const RESIZE_DEBOUNCE_FRAMES: u32 = 30;

/// Number of GTK iterations for async snapshot completion
const SNAPSHOT_GTK_ITERATIONS: u32 = 50;

/// Linux webview implementation using GTK Fixed container
pub struct LinuxWebview {
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
}

impl LinuxWebview {
    pub fn new(
        html_content: &str,
        size: (u32, u32),
        dirty: Arc<AtomicBool>,
        from_ui_tx: mpsc::UnboundedSender<UiToBevy>,
    ) -> Result<Self, WebviewError> {
        // Initialize GTK if not already done
        if !gtk::is_initialized() {
            gtk::init().map_err(|e| WebviewError::GtkInit(e.to_string()))?;
        }

        // Create an offscreen window to host the webview container
        // This is required for the widgets to be properly realized and have valid GL contexts
        let offscreen_window = gtk::OffscreenWindow::new();
        offscreen_window.set_default_size(size.0 as i32, size.1 as i32);

        // Create a Fixed container inside the offscreen window
        let container = gtk::Fixed::new();
        container.set_size_request(size.0 as i32, size.1 as i32);
        offscreen_window.add(&container);

        // Clone for IPC handler
        let dirty_clone = dirty.clone();

        // Create WebView using wry's GTK extension
        let webview = wry::WebViewBuilder::new()
            .with_html(html_content)
            .with_transparent(true)
            .with_ipc_handler(move |msg: wry::http::Request<String>| {
                let body = msg.body();
                if let Ok(ui_msg) = serde_json::from_str::<UiToBevy>(body) {
                    // Mark dirty when UI sends UiDirty message
                    if matches!(ui_msg, UiToBevy::UiDirty) {
                        dirty_clone.store(true, Ordering::SeqCst);
                    }
                    let _ = from_ui_tx.send(ui_msg);
                }
            })
            .build_gtk(&container)
            .map_err(|e| WebviewError::WebviewCreate(e.to_string()))?;

        // Extract the WebKitWebView from the container for snapshot capture
        // wry places the WebView as a child of the container
        let webkit_webview = Self::find_webkit_webview(&container)
            .ok_or_else(|| WebviewError::WebviewCreate("Failed to find WebKitWebView in container".into()))?;

        // Set up load detection - track when WebKit finishes loading content
        let load_finished = Rc::new(RefCell::new(false));
        let load_finished_clone = load_finished.clone();
        webkit_webview.connect_load_changed(move |_webview, load_event| {
            if load_event == LoadEvent::Finished {
                *load_finished_clone.borrow_mut() = true;
                tracing::info!("WebKitGTK content load finished");
            }
        });

        // Show the offscreen window to realize all widgets and set up GL contexts
        // This is needed for WebKit to properly render content
        offscreen_window.show_all();

        // Process GTK events to allow the window and widgets to fully initialize
        // This helps prevent GL context conflicts with Bevy
        for _ in 0..50 {
            if gtk::events_pending() {
                gtk::main_iteration_do(false);
            }
        }

        // Verify realization
        if !container.is_realized() {
            tracing::warn!("Container widget not realized - GL errors may occur");
        } else {
            tracing::debug!("Container widget realized successfully");
        }

        Ok(Self {
            webview,
            webkit_webview,
            container,
            offscreen_window,
            size,
            dirty,
            snapshot_cache: Rc::new(RefCell::new(None)),
            snapshot_pending: Rc::new(RefCell::new(false)),
            state: WebviewState::Initializing,
            load_finished,
        })
    }

    /// Find the WebKitWebView widget within a GTK container
    fn find_webkit_webview(container: &gtk::Fixed) -> Option<WebKitWebView> {
        // Iterate through children to find the WebKitWebView
        for child in container.children() {
            // Try to downcast to WebKitWebView directly
            if let Ok(wv) = child.clone().downcast::<WebKitWebView>() {
                return Some(wv);
            }
            // Also check if it's wrapped in another container
            if let Ok(bin) = child.downcast::<gtk::Bin>() {
                if let Some(inner) = bin.child() {
                    if let Ok(wv) = inner.downcast::<WebKitWebView>() {
                        return Some(wv);
                    }
                }
            }
        }
        None
    }

    pub fn poll(&mut self) {
        // Determine how many GTK iterations based on current state
        let iterations = match self.state {
            WebviewState::Initializing | WebviewState::WarmingUp { .. } => {
                WARMUP_GTK_ITERATIONS
            }
            WebviewState::Ready | WebviewState::Resizing { .. } => 10,
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
                    let (width, height) = self.size;
                    self.webkit_webview.run_javascript(
                        &format!(
                            "document.body.style.width = '{}px'; \
                             document.body.style.height = '{}px'; \
                             document.documentElement.style.width = '{}px'; \
                             document.documentElement.style.height = '{}px';",
                            width, height, width, height
                        ),
                        gio::Cancellable::NONE,
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

    /// Check if the webview is ready to accept capture requests
    pub fn is_ready(&self) -> bool {
        self.state == WebviewState::Ready
    }

    /// Get the current state (for debugging)
    pub fn state(&self) -> WebviewState {
        self.state
    }

    pub fn capture(&self) -> Option<image::RgbaImage> {
        // Check if we have a cached snapshot ready
        if let Some(img) = self.snapshot_cache.borrow_mut().take() {
            return Some(img);
        }

        // Only start new captures in Ready state
        if self.state != WebviewState::Ready {
            tracing::trace!("Capture skipped: webview in {:?} state", self.state);
            return None;
        }

        // If a snapshot is already pending, don't start another one
        if *self.snapshot_pending.borrow() {
            return None;
        }

        // Start async snapshot capture
        *self.snapshot_pending.borrow_mut() = true;

        let cache = self.snapshot_cache.clone();
        let pending = self.snapshot_pending.clone();

        // Use webkit_web_view_snapshot with async callback
        // Use FullDocument to get the complete render regardless of viewport
        self.webkit_webview.snapshot(
            SnapshotRegion::FullDocument,
            SnapshotOptions::TRANSPARENT_BACKGROUND,
            gio::Cancellable::NONE,
            move |result| {
                *pending.borrow_mut() = false;

                match result {
                    Ok(surface) => {
                        match Self::cairo_surface_to_rgba(&surface) {
                            Ok(img) => {
                                *cache.borrow_mut() = Some(img);
                                tracing::debug!("Snapshot captured successfully");
                            }
                            Err(e) => {
                                tracing::error!("Failed to convert Cairo surface: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("WebKitGTK snapshot failed: {}", e);
                    }
                }
            },
        );

        // Pump GTK events to allow the async operation to progress
        // Use more iterations than before to give the async snapshot time to complete
        for i in 0..SNAPSHOT_GTK_ITERATIONS {
            if gtk::events_pending() {
                gtk::main_iteration_do(false);
            }

            // Check if completed early
            if !*self.snapshot_pending.borrow() {
                tracing::trace!("Snapshot completed after {} iterations", i + 1);
                break;
            }
        }

        // Check if the snapshot completed during our event pumping
        self.snapshot_cache.borrow_mut().take()
    }

    /// Convert a Cairo surface to an RGBA image
    /// Uses a large fixed size to capture the full WebKit render
    fn cairo_surface_to_rgba(surface: &cairo::Surface) -> Result<image::RgbaImage, String> {
        // Use a large surface to ensure we capture everything WebKit renders
        // The actual content will be smaller, and Bevy will scale as needed
        let width = 1920;
        let height = 1080;

        let mut img_surface = ImageSurface::create(Format::ARgb32, width, height)
            .map_err(|e| format!("Failed to create image surface: {}", e))?;

        let ctx = cairo::Context::new(&img_surface)
            .map_err(|e| format!("Failed to create context: {}", e))?;

        ctx.set_source_surface(surface, 0.0, 0.0)
            .map_err(|e| format!("Failed to set source surface: {}", e))?;

        ctx.paint()
            .map_err(|e| format!("Failed to paint: {}", e))?;

        drop(ctx);
        img_surface.flush();

        // Get the raw pixel data
        let data = img_surface
            .data()
            .map_err(|e| format!("Failed to get surface data: {}", e))?;

        // Convert from Cairo's BGRA/ARGB format to RGBA
        // Cairo uses pre-multiplied alpha in native byte order
        let mut rgba_data = Vec::with_capacity((width * height * 4) as usize);

        for chunk in data.chunks_exact(4) {
            // Cairo on Linux (little-endian) stores as BGRA
            let b = chunk[0];
            let g = chunk[1];
            let r = chunk[2];
            let a = chunk[3];

            // Un-premultiply alpha if needed
            let (r, g, b) = if a > 0 && a < 255 {
                let alpha = a as f32 / 255.0;
                (
                    (r as f32 / alpha).min(255.0) as u8,
                    (g as f32 / alpha).min(255.0) as u8,
                    (b as f32 / alpha).min(255.0) as u8,
                )
            } else {
                (r, g, b)
            };

            rgba_data.push(r);
            rgba_data.push(g);
            rgba_data.push(b);
            rgba_data.push(a);
        }

        image::RgbaImage::from_raw(width as u32, height as u32, rgba_data)
            .ok_or_else(|| "Failed to create image from raw data".to_string())
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        // Ignore resize if dimensions unchanged
        if self.size == (width, height) {
            return;
        }

        tracing::info!(
            "Webview resize: {:?} -> ({}, {})",
            self.size,
            width,
            height
        );

        self.size = (width, height);

        // Resize the offscreen window, container, and webkit webview
        self.offscreen_window
            .set_default_size(width as i32, height as i32);
        self.offscreen_window.resize(width as i32, height as i32);
        self.container.set_size_request(width as i32, height as i32);

        // Also set size on the webkit webview widget itself
        self.webkit_webview
            .set_size_request(width as i32, height as i32);

        // Force WebKit to re-layout by triggering a resize event in JavaScript
        // This helps ensure the viewport updates to match the new size
        self.webkit_webview.run_javascript(
            &format!(
                "window.dispatchEvent(new Event('resize')); \
                 document.body.style.width = '{}px'; \
                 document.body.style.height = '{}px';",
                width, height
            ),
            gio::Cancellable::NONE,
            |_| {},
        );

        // Pump more GTK events to help the resize propagate
        // WebKit needs more iterations to fully process the resize
        for _ in 0..30 {
            if gtk::events_pending() {
                gtk::main_iteration_do(false);
            }
        }

        // Clear any cached snapshot since it's now the wrong size
        *self.snapshot_cache.borrow_mut() = None;
        *self.snapshot_pending.borrow_mut() = false;

        // Only transition to Resizing state if we're already Ready
        // Don't interrupt Initializing or WarmingUp states
        if self.state == WebviewState::Ready {
            self.state = WebviewState::Resizing {
                frames_remaining: RESIZE_DEBOUNCE_FRAMES,
            };
            // Don't mark dirty immediately - wait for resize to stabilize
            // The dirty flag will be set when state transitions back to Ready
        }
    }

    pub fn inject_mouse(&mut self, event: MouseEvent) {
        // Use JavaScript to dispatch DOM events
        // This is more reliable than synthesizing GDK events
        //
        // For click events, we use a two-phase approach:
        // 1. Dispatch the DOM event
        // 2. Use requestAnimationFrame to wait for Svelte to re-render
        // 3. Send IPC message to mark dirty AFTER the DOM has updated
        let (js, needs_raf_dirty) = match event {
            MouseEvent::Move { x, y } => {
                // Mouse move doesn't need dirty update
                (format!(
                    r#"(function() {{
                        const target = document.elementFromPoint({x}, {y}) || document.body;
                        target.dispatchEvent(new MouseEvent('mousemove', {{
                            bubbles: true,
                            cancelable: true,
                            clientX: {x},
                            clientY: {y},
                            view: window
                        }}));
                    }})()"#,
                    x = x,
                    y = y
                ), false)
            }
            MouseEvent::ButtonDown { button, x, y } => {
                let button_num = match button {
                    MouseButton::Left => 0,
                    MouseButton::Middle => 1,
                    MouseButton::Right => 2,
                };
                // mousedown alone typically doesn't change visible UI state much
                (format!(
                    r#"(function() {{
                        const target = document.elementFromPoint({x}, {y}) || document.body;
                        target.dispatchEvent(new MouseEvent('mousedown', {{
                            bubbles: true,
                            cancelable: true,
                            clientX: {x},
                            clientY: {y},
                            button: {button},
                            view: window
                        }}));
                    }})()"#,
                    x = x,
                    y = y,
                    button = button_num
                ), false) // Don't mark dirty yet - wait for click
            }
            MouseEvent::ButtonUp { button, x, y } => {
                let button_num = match button {
                    MouseButton::Left => 0,
                    MouseButton::Middle => 1,
                    MouseButton::Right => 2,
                };
                // Click is where state changes happen - use RAF to wait for DOM update
                (format!(
                    r#"(function() {{
                        const target = document.elementFromPoint({x}, {y}) || document.body;
                        target.dispatchEvent(new MouseEvent('mouseup', {{
                            bubbles: true,
                            cancelable: true,
                            clientX: {x},
                            clientY: {y},
                            button: {button},
                            view: window
                        }}));
                        // Also dispatch click for left button
                        if ({button} === 0) {{
                            target.dispatchEvent(new MouseEvent('click', {{
                                bubbles: true,
                                cancelable: true,
                                clientX: {x},
                                clientY: {y},
                                button: 0,
                                view: window
                            }}));
                            // Wait for DOM to update after click, then notify
                            requestAnimationFrame(() => {{
                                requestAnimationFrame(() => {{
                                    if (window.ipc) {{
                                        window.ipc.postMessage(JSON.stringify({{ type: 'UiDirty' }}));
                                    }}
                                }});
                            }});
                        }}
                    }})()"#,
                    x = x,
                    y = y,
                    button = button_num
                ), true)
            }
            MouseEvent::Scroll { delta_x, delta_y, x, y } => {
                (format!(
                    r#"(function() {{
                        const target = document.elementFromPoint({x}, {y}) || document.body;
                        target.dispatchEvent(new WheelEvent('wheel', {{
                            bubbles: true,
                            cancelable: true,
                            clientX: {x},
                            clientY: {y},
                            deltaX: {delta_x},
                            deltaY: {delta_y},
                            deltaMode: 0,
                            view: window
                        }}));
                        // Scroll might change visible content
                        requestAnimationFrame(() => {{
                            if (window.ipc) {{
                                window.ipc.postMessage(JSON.stringify({{ type: 'UiDirty' }}));
                            }}
                        }});
                    }})()"#,
                    x = x,
                    y = y,
                    delta_x = delta_x,
                    delta_y = delta_y
                ), true)
            }
        };

        // Execute the JavaScript to dispatch the event
        self.webkit_webview.run_javascript(&js, gio::Cancellable::NONE, |_| {});

        // For events that use RAF to mark dirty via IPC, we need to pump GTK events
        // to let the JavaScript and RAF callbacks execute
        if needs_raf_dirty {
            // Pump enough iterations for JS execution + 2 RAF cycles
            for _ in 0..50 {
                gtk::main_iteration_do(false);
            }
        }
    }

    pub fn inject_keyboard(&mut self, event: KeyboardEvent) {
        // Use JavaScript to dispatch DOM keyboard events
        let event_type = if event.pressed { "keydown" } else { "keyup" };

        // Escape the key for JavaScript string
        let key_escaped = event.key.replace('\\', "\\\\").replace('\'', "\\'");

        let js = format!(
            r#"(function() {{
                const target = document.activeElement || document.body;
                target.dispatchEvent(new KeyboardEvent('{event_type}', {{
                    bubbles: true,
                    cancelable: true,
                    key: '{key}',
                    shiftKey: {shift},
                    ctrlKey: {ctrl},
                    altKey: {alt},
                    metaKey: {meta},
                    view: window
                }}));
                // For text input, also dispatch input event for printable keys
                if ('{event_type}' === 'keydown' && '{key}'.length === 1 && !{ctrl} && !{alt} && !{meta}) {{
                    if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable) {{
                        // Let the browser handle text input naturally
                    }}
                }}
            }})()"#,
            event_type = event_type,
            key = key_escaped,
            shift = event.modifiers.shift,
            ctrl = event.modifiers.ctrl,
            alt = event.modifiers.alt,
            meta = event.modifiers.meta
        );

        self.webkit_webview.run_javascript(&js, gio::Cancellable::NONE, |_| {});

        // Only mark dirty for key presses (not releases) that might change UI
        // Modifier keys alone don't typically change visible UI
        let is_modifier = matches!(
            event.key.as_str(),
            "Shift" | "Control" | "Alt" | "Meta"
        );
        if event.pressed && !is_modifier {
            self.dirty.store(true, Ordering::SeqCst);
        }
    }

    pub fn eval(&self, js: &str) -> Result<(), WebviewError> {
        self.webview
            .evaluate_script(js)
            .map_err(|e| WebviewError::EvalScript(e.to_string()))
    }
}
