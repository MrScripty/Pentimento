//! WebKit capture functionality using Cairo snapshots

use std::sync::atomic::Ordering;

use cairo::{Format, ImageSurface};
use gio::Cancellable;
use webkit2gtk::{SnapshotOptions, SnapshotRegion, WebViewExt};

use crate::state::WebviewState;
use crate::WebKitBackend;

impl WebKitBackend {
    /// Capture the current webview content as an RGBA image
    pub fn capture(&mut self) -> Option<image::RgbaImage> {
        // Check if we have a cached snapshot ready
        if let Some(img) = self.snapshot_cache.borrow_mut().take() {
            return Some(img);
        }

        // Only start new captures in Ready state
        if self.state != WebviewState::Ready {
            tracing::trace!("Capture skipped: webview in {:?} state", self.state);
            return None;
        }

        // Wait for settling after mouse events to avoid capturing intermediate render state
        if self.frames_until_capture_allowed > 0 {
            self.frames_until_capture_allowed -= 1;
            tracing::trace!(
                "Capture delayed: {} frames remaining for mouse event settling",
                self.frames_until_capture_allowed
            );
            // Keep dirty true so we retry once settling completes.
            self.dirty.store(true, Ordering::SeqCst);
            return None;
        }

        // If a snapshot is already pending, don't start another one
        if *self.snapshot_pending.borrow() {
            // Preserve dirty so we pull the pending snapshot on a later frame.
            self.dirty.store(true, Ordering::SeqCst);
            return None;
        }

        // Start async snapshot capture
        *self.snapshot_pending.borrow_mut() = true;

        let cache = self.snapshot_cache.clone();
        let pending = self.snapshot_pending.clone();
        // Capture the known viewport size for the async closure
        let (width, height) = self.size;

        // Use webkit_web_view_snapshot with async callback
        // Use Visible region to capture exactly the viewport (not the full document)
        // This ensures we get the correct resolution matching self.size
        self.webkit_webview.snapshot(
            SnapshotRegion::Visible,
            SnapshotOptions::TRANSPARENT_BACKGROUND,
            Cancellable::NONE,
            move |result| {
                *pending.borrow_mut() = false;

                match result {
                    Ok(surface) => {
                        // Pass explicit dimensions to ensure correct output size
                        match cairo_surface_to_rgba(&surface, width as i32, height as i32) {
                            Ok(img) => {
                                *cache.borrow_mut() = Some(img);
                                tracing::debug!(
                                    "Snapshot captured successfully at {}x{}",
                                    width,
                                    height
                                );
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

        // Check if the snapshot completed immediately.
        let captured = self.snapshot_cache.borrow_mut().take();
        if captured.is_none() {
            // Keep dirty so we keep polling until the async snapshot finishes.
            self.dirty.store(true, Ordering::SeqCst);
        }
        captured
    }
}

/// Convert a Cairo surface to an RGBA image at the specified dimensions
/// Uses explicit width/height instead of extracting from surface for reliable sizing
pub fn cairo_surface_to_rgba(
    surface: &cairo::Surface,
    width: i32,
    height: i32,
) -> Result<image::RgbaImage, String> {
    if width <= 0 || height <= 0 {
        return Err(format!("Invalid dimensions: {}x{}", width, height));
    }

    tracing::trace!("Converting Cairo surface to RGBA at {}x{}", width, height);

    let mut img_surface = ImageSurface::create(Format::ARgb32, width, height)
        .map_err(|e| format!("Failed to create image surface: {}", e))?;

    let ctx =
        cairo::Context::new(&img_surface).map_err(|e| format!("Failed to create context: {}", e))?;

    ctx.set_source_surface(surface, 0.0, 0.0)
        .map_err(|e| format!("Failed to set source surface: {}", e))?;

    ctx.paint().map_err(|e| format!("Failed to paint: {}", e))?;

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
