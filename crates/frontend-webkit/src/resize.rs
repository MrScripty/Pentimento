//! WebKit resize handling

use gio::Cancellable;
use gtk::prelude::*;
use webkit2gtk::WebViewExt;

use crate::state::{WebviewState, RESIZE_DEBOUNCE_FRAMES};
use crate::WebKitBackend;

impl WebKitBackend {
    /// Resize the webview to new dimensions
    pub fn resize_webview(&mut self, width: u32, height: u32) {
        tracing::info!(
            "Webview resize: {:?} -> ({}, {})",
            self.size,
            width,
            height
        );

        self.size = (width, height);
        let (logical_width, logical_height) = self.logical_size();
        let scale = if self.scale_factor > 0.0 {
            self.scale_factor
        } else {
            1.0
        };

        // Resize the offscreen window, container, and webkit webview
        self.offscreen_window
            .set_default_size(width as i32, height as i32);
        self.offscreen_window.resize(width as i32, height as i32);
        self.container.set_size_request(width as i32, height as i32);

        // Also set size on the webkit webview widget itself
        self.webkit_webview
            .set_size_request(width as i32, height as i32);

        // Update wry webview bounds (critical for Linux with gtk::Fixed)
        // Without this, the webview renders fuzzy. This matches overlay mode.
        self.webview
            .set_bounds(wry::Rect {
                position: wry::dpi::PhysicalPosition::new(0, 0).into(),
                size: wry::dpi::PhysicalSize::new(width, height).into(),
            })
            .ok();

        // Force WebKit to re-layout by triggering a resize event in JavaScript
        // This helps ensure the viewport updates to match the new size
        // IMPORTANT: Must update viewport meta tag, not just CSS dimensions!
        self.webkit_webview.run_javascript(
            &format!(
                r#"(function() {{
                    // Update viewport meta tag with exact dimensions
                    var meta = document.querySelector('meta[name="viewport"]');
                    if (meta) {{
                        meta.setAttribute('content', 'width={width}, height={height}, initial-scale={scale}, minimum-scale={scale}, maximum-scale={scale}, user-scalable=no');
                    }}
                    // Also set explicit CSS dimensions
                    document.body.style.width = '{width}px';
                    document.body.style.height = '{height}px';
                    document.documentElement.style.width = '{width}px';
                    document.documentElement.style.height = '{height}px';
                    // Dispatch resize event
                    window.dispatchEvent(new Event('resize'));
                    console.log('Viewport resized to', {width}, 'x', {height});
                }})()"#,
                width = logical_width,
                height = logical_height,
                scale = scale
            ),
            Cancellable::NONE,
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
}
