//! Linux-specific webview implementation using GTK and WebKitGTK

use crate::error::WebviewError;
use pentimento_ipc::{KeyboardEvent, MouseEvent, UiToBevy};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

use cairo::{Format, ImageSurface};
use gio::prelude::*;
use gtk::prelude::*;
use webkit2gtk::{SnapshotOptions, SnapshotRegion, WebView as WebKitWebView, WebViewExt};
use wry::WebViewBuilderExtUnix;

/// Linux webview implementation using GTK Fixed container
pub struct LinuxWebview {
    webview: wry::WebView,
    webkit_webview: WebKitWebView,
    #[allow(dead_code)]
    container: gtk::Fixed,
    size: (u32, u32),
    dirty: Arc<AtomicBool>,
    /// Cached snapshot result from async capture
    snapshot_cache: Rc<RefCell<Option<image::RgbaImage>>>,
    /// Flag indicating snapshot is in progress
    snapshot_pending: Rc<RefCell<bool>>,
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

        // Extract the WebKitWebView from the container for snapshot capture
        // wry places the WebView as a child of the container
        let webkit_webview = Self::find_webkit_webview(&container)
            .ok_or_else(|| WebviewError::WebviewCreate("Failed to find WebKitWebView in container".into()))?;

        Ok(Self {
            webview,
            webkit_webview,
            container,
            size,
            dirty,
            snapshot_cache: Rc::new(RefCell::new(None)),
            snapshot_pending: Rc::new(RefCell::new(false)),
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
        // Pump GTK events without blocking
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
    }

    pub fn capture(&self) -> Option<image::RgbaImage> {
        // Check if we have a cached snapshot ready
        if let Some(img) = self.snapshot_cache.borrow_mut().take() {
            return Some(img);
        }

        // If a snapshot is already pending, don't start another one
        if *self.snapshot_pending.borrow() {
            return None;
        }

        // Start async snapshot capture
        *self.snapshot_pending.borrow_mut() = true;

        let cache = self.snapshot_cache.clone();
        let pending = self.snapshot_pending.clone();
        let size = self.size;

        // Use webkit_web_view_snapshot with async callback
        self.webkit_webview.snapshot(
            SnapshotRegion::FullDocument,
            SnapshotOptions::TRANSPARENT_BACKGROUND,
            gio::Cancellable::NONE,
            move |result| {
                *pending.borrow_mut() = false;

                match result {
                    Ok(surface) => {
                        match Self::cairo_surface_to_rgba(&surface, size) {
                            Ok(img) => {
                                *cache.borrow_mut() = Some(img);
                            }
                            Err(e) => {
                                tracing::error!("Failed to convert Cairo surface: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("WebKitGTK snapshot failed: {}", e);
                    }
                }
            },
        );

        // Pump GTK events to allow the async operation to progress
        // We do a limited number of iterations to avoid blocking indefinitely
        for _ in 0..10 {
            if gtk::events_pending() {
                gtk::main_iteration_do(false);
            } else {
                break;
            }
        }

        // Check if the snapshot completed during our event pumping
        self.snapshot_cache.borrow_mut().take()
    }

    /// Convert a Cairo ImageSurface to an RGBA image
    fn cairo_surface_to_rgba(
        surface: &cairo::Surface,
        target_size: (u32, u32),
    ) -> Result<image::RgbaImage, String> {
        // Create a new ImageSurface with ARGB32 format to render the snapshot
        let width = target_size.0 as i32;
        let height = target_size.1 as i32;

        let mut img_surface = ImageSurface::create(Format::ARgb32, width, height)
            .map_err(|e| format!("Failed to create image surface: {}", e))?;

        // Create a context and draw the source surface
        let ctx = cairo::Context::new(&img_surface)
            .map_err(|e| format!("Failed to create context: {}", e))?;

        // The webkit snapshot typically returns the surface at the correct size,
        // so we just paint it directly. If scaling is needed, we can compute it
        // from the extents.
        ctx.set_source_surface(surface, 0.0, 0.0)
            .map_err(|e| format!("Failed to set source surface: {}", e))?;

        ctx.paint()
            .map_err(|e| format!("Failed to paint: {}", e))?;

        // Ensure all drawing is done
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
