//! Linux overlay webview implementation using transparent GTK window
//!
//! This implementation creates a transparent GTK window that overlays the Bevy window.
//! The desktop compositor handles the actual blending, avoiding the need for framebuffer capture.
//!
//! Input handling uses a selective passthrough approach:
//! - UI regions (toolbar, sidebar) receive native input for proper Svelte interaction
//! - The 3D viewport area is click-through, passing events to Bevy underneath
//! This allows both the UI and 3D scene to receive input appropriately.

use crate::error::WebviewError;
use pentimento_ipc::{KeyboardEvent, MouseButton, MouseEvent, UiToBevy};
use raw_window_handle::RawWindowHandle;
use std::cell::RefCell;
use std::rc::Rc;
use tokio::sync::mpsc;

use gdk::prelude::*;
use gdk::cairo;
use gdkx11::{X11Display, X11Window};
use gio::Cancellable;
use gtk::prelude::*;
use webkit2gtk::{LoadEvent, WebViewExt};
use wry::WebViewBuilderExtUnix;


/// Overlay webview state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayState {
    /// Waiting for content to load
    Initializing,
    /// Content loaded, ready for use
    Ready,
}

/// Linux overlay webview using transparent GTK window
pub struct LinuxOverlayWebview {
    webview: wry::WebView,
    webkit_webview: webkit2gtk::WebView,
    window: gtk::Window,
    container: gtk::Fixed,
    size: (u32, u32),
    state: OverlayState,
    load_finished: Rc<RefCell<bool>>,
    /// Parent window XID for state tracking (X11 only)
    parent_xid: Option<u64>,
}

impl LinuxOverlayWebview {
    pub fn new(
        parent_handle: RawWindowHandle,
        html_content: &str,
        size: (u32, u32),
        from_ui_tx: mpsc::UnboundedSender<UiToBevy>,
    ) -> Result<Self, WebviewError> {
        // Initialize GTK if not already done
        if !gtk::is_initialized() {
            gtk::init().map_err(|e| WebviewError::GtkInit(e.to_string()))?;
        }

        // Create a transparent toplevel window (not popup, so transient relationships work)
        // We use Toplevel instead of Popup because Popup windows don't properly
        // respect WM_TRANSIENT_FOR relationships for minimize/restore behavior
        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        window.set_decorated(false);
        window.set_app_paintable(true);
        window.set_default_size(size.0 as i32, size.1 as i32);
        // Prevent the overlay from stealing focus (avoids winit XIM unfocus errors).
        window.set_accept_focus(false);
        window.set_focus_on_map(false);
        window.set_skip_taskbar_hint(true);
        window.set_skip_pager_hint(true);

        // Enable RGBA visual for transparency
        if let Some(screen) = gtk::prelude::WidgetExt::screen(&window) {
            if let Some(visual) = screen.rgba_visual() {
                window.set_visual(Some(&visual));
                tracing::info!("RGBA visual enabled for transparent overlay");
            } else {
                tracing::warn!("No RGBA visual available - transparency may not work");
            }
        }

        // Set up transparent background via CSS
        let css_provider = gtk::CssProvider::new();
        css_provider
            .load_from_data(b"window { background-color: transparent; }")
            .map_err(|e| WebviewError::WindowCreate(format!("CSS load failed: {}", e)))?;

        if let Some(screen) = gtk::prelude::WidgetExt::screen(&window) {
            gtk::StyleContext::add_provider_for_screen(
                &screen,
                &css_provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }

        // Create a Fixed container for the webview
        let container = gtk::Fixed::new();
        container.set_size_request(size.0 as i32, size.1 as i32);
        window.add(&container);

        // Set up load detection
        let load_finished = Rc::new(RefCell::new(false));

        // Create the webview with explicit bounds
        // On Linux with gtk::Fixed, we must set bounds or the webview defaults to 200x200
        // Use PhysicalSize since we receive physical pixels from Bevy
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
            .map_err(|e| WebviewError::WebviewCreate(e.to_string()))?;

        // Find the WebKitWebView to set up load detection and sizing
        let webkit_webview = Self::find_webkit_webview(&container)
            .ok_or_else(|| WebviewError::WebviewCreate("Failed to find WebKitWebView in container".into()))?;

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

        // Position the overlay window based on parent handle and set up window grouping
        let parent_xid = Self::setup_window_relationship(&window, parent_handle, size);

        // Show the window
        window.show_all();

        // Set up selective input passthrough: UI regions receive input, viewport is click-through
        Self::update_input_regions(&window, size.0, size.1);

        // Process GTK events to initialize
        for _ in 0..50 {
            if gtk::events_pending() {
                gtk::main_iteration_do(false);
            }
        }

        tracing::info!("Linux overlay webview created at size {:?}", size);

        Ok(Self {
            webview,
            webkit_webview,
            window,
            container,
            size,
            state: OverlayState::Initializing,
            load_finished,
            parent_xid,
        })
    }

    /// Set up the window relationship with the parent (transient, grouping) and position
    /// Returns the parent window XID for state tracking (X11 only)
    fn setup_window_relationship(
        window: &gtk::Window,
        parent_handle: RawWindowHandle,
        _size: (u32, u32),
    ) -> Option<u64> {
        let mut parent_xid: Option<u64> = None;

        // Try to get parent window position based on handle type
        match parent_handle {
            RawWindowHandle::Xlib(handle) => {
                let xid = handle.window as u64;
                tracing::info!("X11 parent window (Xlib): {:?}", xid);
                parent_xid = Some(xid);
                window.move_(0, 0);

                // Set transient hint via GDK after window is realized
                Self::set_transient_for_x11(window, xid);
            }
            RawWindowHandle::Xcb(handle) => {
                let xid = handle.window.get() as u64;
                tracing::info!("X11 parent window (XCB): {:?}", xid);
                parent_xid = Some(xid);
                window.move_(0, 0);

                // Set transient hint via GDK after window is realized
                Self::set_transient_for_x11(window, xid);
            }
            RawWindowHandle::Wayland(_handle) => {
                // Wayland positioning is more complex due to security model
                // The compositor controls window positioning
                // Transient relationships work differently on Wayland
                tracing::info!("Wayland parent window - positioning controlled by compositor");
            }
            _ => {
                tracing::warn!("Unknown window handle type for parent positioning");
            }
        }

        parent_xid
    }

    /// Set the overlay window as transient for the parent X11 window
    /// This makes the overlay minimize/restore with the parent window
    fn set_transient_for_x11(window: &gtk::Window, parent_xid: u64) {
        // We need to realize the window first to get its GDK window
        window.realize();

        let Some(gdk_window) = window.window() else {
            tracing::warn!("Could not get GDK window for transient setup");
            return;
        };

        // Get the display and try to cast it to X11Display
        let display = gdk_window.display();
        let Some(x11_display) = display.downcast_ref::<X11Display>() else {
            tracing::warn!("Not running on X11 display");
            return;
        };

        // Get the parent window as an X11Window and set transient relationship
        let parent_x11_window = X11Window::foreign_new_for_display(x11_display, parent_xid);
        let parent_gdk: gdk::Window = parent_x11_window.upcast();

        // Set the transient-for relationship - this tells the WM to minimize together
        gdk_window.set_transient_for(&parent_gdk);

        // Also set type hint to indicate this is a utility window
        gdk_window.set_type_hint(gdk::WindowTypeHint::Utility);

        tracing::info!("Overlay set as transient for parent window {}", parent_xid);
    }

    /// Update input regions to allow selective passthrough
    ///
    /// Creates an input shape that covers only UI elements (toolbar, sidebar),
    /// making the 3D viewport area click-through to Bevy underneath.
    fn update_input_regions(window: &gtk::Window, width: u32, height: u32) {
        let Some(gdk_window) = window.window() else {
            tracing::warn!("Could not get GDK window for input region setup");
            return;
        };

        // UI layout constants (matching Svelte CSS)
        // Toolbar: top 0, height 48px, full width
        // Sidebar: top 56px, right 8px, width 300px, bottom 8px
        let toolbar_height = 72; // 48px + some padding for dropdowns
        let sidebar_width = 316; // 300px + 8px margin + 8px padding
        let sidebar_top = 56;
        let sidebar_margin = 8;

        let w = width as i32;
        let h = height as i32;

        // Create a region covering only the UI elements
        let region = cairo::Region::create();

        // Add toolbar rectangle (full width, at top)
        let toolbar_rect = cairo::RectangleInt::new(0, 0, w, toolbar_height);
        region.union_rectangle(&toolbar_rect);

        // Add sidebar rectangle (right side, below toolbar)
        let sidebar_rect = cairo::RectangleInt::new(
            w - sidebar_width,
            sidebar_top,
            sidebar_width,
            h - sidebar_top - sidebar_margin,
        );
        region.union_rectangle(&sidebar_rect);

        // Set the input shape - only these regions will receive input
        // Everything else (the 3D viewport) will be click-through
        gdk_window.input_shape_combine_region(&region, 0, 0);

        tracing::debug!(
            "Input regions set: toolbar (0,0,{},{}), sidebar ({},{},{},{})",
            w, toolbar_height,
            w - sidebar_width, sidebar_top, sidebar_width, h - sidebar_top - sidebar_margin
        );
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

    pub fn poll(&mut self) {
        // Pump GTK events
        for _ in 0..10 {
            if gtk::events_pending() {
                gtk::main_iteration_do(false);
            }
        }

        // Sync visibility with parent window state (check every poll)
        self.check_parent_visibility();

        // Update state
        if self.state == OverlayState::Initializing && *self.load_finished.borrow() {
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
                Cancellable::NONE,
                |_| {},
            );

            self.state = OverlayState::Ready;
            tracing::info!("Overlay webview ready");
        }
    }

    /// Check parent window visibility via X11 and sync overlay visibility
    fn check_parent_visibility(&mut self) {
        let Some(parent_xid) = self.parent_xid else {
            return; // No parent tracking on non-X11
        };

        let Some(gdk_window) = self.window.window() else {
            return;
        };

        // Get the display and try to cast it to X11Display
        let display = gdk_window.display();
        let Some(x11_display) = display.downcast_ref::<X11Display>() else {
            return; // Not running on X11
        };

        // Get the parent window as an X11Window using foreign_new_for_display
        let parent_x11_window = X11Window::foreign_new_for_display(x11_display, parent_xid);

        // Cast to gdk::Window to access state() method
        let parent_gdk: gdk::Window = parent_x11_window.upcast();

        // Check if parent is iconified (minimized) or withdrawn
        let parent_state = parent_gdk.state();
        let is_iconified = parent_state.contains(gdk::WindowState::ICONIFIED);
        let is_withdrawn = parent_state.contains(gdk::WindowState::WITHDRAWN);
        let parent_hidden = is_iconified || is_withdrawn;

        // Sync our visibility to match
        let currently_visible = self.window.is_visible();
        if parent_hidden && currently_visible {
            self.window.hide();
            tracing::debug!("Overlay hidden (parent iconified/withdrawn)");
        } else if !parent_hidden && !currently_visible {
            self.window.show();
            tracing::debug!("Overlay shown (parent visible)");
        }
    }

    /// Sync overlay visibility with the given parent window visibility state
    /// Called externally when the parent window state changes
    pub fn sync_visibility(&mut self, parent_visible: bool) {
        let currently_visible = self.window.is_visible();
        if !parent_visible && currently_visible {
            self.window.hide();
            tracing::debug!("Overlay hidden (parent minimized)");
        } else if parent_visible && !currently_visible {
            self.window.show();
            tracing::debug!("Overlay shown (parent restored)");
        }
    }

    pub fn is_ready(&self) -> bool {
        self.state == OverlayState::Ready
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if self.size == (width, height) {
            return;
        }

        self.size = (width, height);

        // Resize all components: window, container, webkit_webview, and wry webview bounds
        self.window.resize(width as i32, height as i32);
        self.container.set_size_request(width as i32, height as i32);
        self.webkit_webview.set_size_request(width as i32, height as i32);

        // Update wry webview bounds (critical for Linux with gtk::Fixed)
        // Use PhysicalSize since we receive physical pixels from Bevy
        self.webview.set_bounds(wry::Rect {
            position: wry::dpi::PhysicalPosition::new(0, 0).into(),
            size: wry::dpi::PhysicalSize::new(width, height).into(),
        }).ok();

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
        Self::update_input_regions(&self.window, width, height);

        // Pump GTK events to help the resize propagate
        for _ in 0..30 {
            if gtk::events_pending() {
                gtk::main_iteration_do(false);
            }
        }

        tracing::info!("Overlay webview resized to ({}, {})", width, height);
    }

    pub fn set_position(&mut self, x: i32, y: i32) {
        self.window.move_(x, y);
    }

    pub fn set_visible(&mut self, visible: bool) {
        if visible {
            self.window.show();
        } else {
            self.window.hide();
        }
    }

    pub fn inject_mouse(&mut self, event: MouseEvent) {
        // For overlay mode with click-through, we inject synthetic events via JavaScript.
        // Note: Synthetic events may not trigger all browser behaviors (e.g., :active styles).
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
                    x = x, y = y
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
                    x = x, y = y, button = button_num
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
                    x = x, y = y, button = button_num
                )
            }
            MouseEvent::Scroll { delta_x, delta_y, x, y } => {
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
                    x = x, y = y, delta_x = delta_x, delta_y = delta_y
                )
            }
        };

        let _ = self.webview.evaluate_script(&js);
    }

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

    pub fn eval(&self, js: &str) -> Result<(), WebviewError> {
        self.webview
            .evaluate_script(js)
            .map_err(|e| WebviewError::EvalScript(e.to_string()))
    }
}
