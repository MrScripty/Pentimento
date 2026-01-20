//! Linux-specific webview implementation using GTK and WebKitGTK

use crate::error::WebviewError;
use pentimento_ipc::{KeyboardEvent, MouseEvent, UiToBevy};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

use gtk::prelude::*;
use wry::WebViewBuilderExtUnix;

/// Linux webview implementation using GTK Fixed container
pub struct LinuxWebview {
    webview: wry::WebView,
    #[allow(dead_code)]
    container: gtk::Fixed,
    size: (u32, u32),
    dirty: Arc<AtomicBool>,
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

        // Create an offscreen GTK container
        let container = gtk::Fixed::new();
        container.set_size_request(size.0 as i32, size.1 as i32);

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

        Ok(Self {
            webview,
            container,
            size,
            dirty,
        })
    }

    pub fn poll(&mut self) {
        // Pump GTK events without blocking
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
    }

    pub fn capture(&self) -> Option<image::RgbaImage> {
        // TODO: Implement webkit_web_view_get_snapshot via webkit2gtk bindings
        // For now, return a placeholder
        //
        // The actual implementation would:
        // 1. Call webkit_web_view_get_snapshot() async
        // 2. Pump GTK events until complete
        // 3. Convert Cairo surface to RGBA

        tracing::warn!("Webview capture not yet implemented - returning placeholder");

        // Create a semi-transparent placeholder image
        let width = self.size.0;
        let height = self.size.1;
        let mut img = image::RgbaImage::new(width, height);

        // Fill with semi-transparent dark background
        for pixel in img.pixels_mut() {
            *pixel = image::Rgba([30, 30, 30, 200]);
        }

        Some(img)
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.size = (width, height);
        self.container.set_size_request(width as i32, height as i32);
        self.dirty.store(true, Ordering::SeqCst);
    }

    pub fn inject_mouse(&mut self, _event: MouseEvent) {
        // TODO: Synthesize GDK mouse events
        // This requires creating synthetic GdkEvent structures
        // and dispatching them to the webview widget
    }

    pub fn inject_keyboard(&mut self, _event: KeyboardEvent) {
        // TODO: Synthesize GDK keyboard events
    }

    pub fn eval(&self, js: &str) -> Result<(), WebviewError> {
        self.webview
            .evaluate_script(js)
            .map_err(|e| WebviewError::EvalScript(e.to_string()))
    }
}
