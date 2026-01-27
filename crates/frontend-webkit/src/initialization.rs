//! WebKit backend initialization

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use gio::prelude::*;
use gtk::prelude::*;
use pentimento_frontend_core::FrontendError;
use pentimento_ipc::UiToBevy;
use tokio::sync::mpsc;
use webkit2gtk::{LoadEvent, WebView as WebKitWebView, WebViewExt};
use wry::WebViewBuilderExtUnix;

use crate::state::WebviewState;
use crate::WebKitBackend;

impl WebKitBackend {
    /// Create a new WebKit backend
    pub fn new(
        html_content: &str,
        size: (u32, u32),
        dirty: Arc<AtomicBool>,
        from_ui_tx: mpsc::UnboundedSender<UiToBevy>,
    ) -> Result<Self, FrontendError> {
        // Initialize GTK if not already done
        if !gtk::is_initialized() {
            gtk::init().map_err(|e| FrontendError::Backend(format!("Failed to initialize GTK: {}", e)))?;
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
        // CRITICAL: Set explicit bounds - without this, wry defaults to a small size
        // and the webview renders fuzzy. This matches overlay mode which works perfectly.
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
                    // Mark dirty when UI sends UiDirty message
                    if matches!(ui_msg, UiToBevy::UiDirty) {
                        dirty_clone.store(true, Ordering::SeqCst);
                    }
                    let _ = from_ui_tx.send(ui_msg);
                }
            })
            .build_gtk(&container)
            .map_err(|e| FrontendError::Backend(format!("Failed to create webview: {}", e)))?;

        // Extract the WebKitWebView from the container for snapshot capture
        // wry places the WebView as a child of the container
        let webkit_webview = Self::find_webkit_webview(&container)
            .ok_or_else(|| FrontendError::Backend("Failed to find WebKitWebView in container".into()))?;

        // CRITICAL: Set the webkit webview size to match the intended viewport
        // Without this, the viewport defaults to 200x200 and coordinate mapping breaks
        webkit_webview.set_size_request(size.0 as i32, size.1 as i32);

        // Also resize the offscreen window (set_default_size only affects initial size)
        offscreen_window.resize(size.0 as i32, size.1 as i32);

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
            frames_until_capture_allowed: 0,
            scale_factor: 1.0,
            to_ui_tx: None,
            from_ui_rx: None,
        })
    }

    /// Find the WebKitWebView widget within a GTK container
    pub(crate) fn find_webkit_webview(container: &gtk::Fixed) -> Option<WebKitWebView> {
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
}
