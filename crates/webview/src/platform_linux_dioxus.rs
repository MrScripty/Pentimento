//! Dioxus native rendering backend
//!
//! This module provides a native Rust UI implementation using Dioxus.
//! Unlike CEF or WebKitGTK, this renders directly using Rust without
//! a browser engine.
//!
//! # Architecture
//!
//! The Dioxus renderer:
//! 1. Runs the Dioxus VirtualDOM with Pentimento UI components
//! 2. Uses dioxus-desktop to render to a headless webview
//! 3. Captures the rendered output as RGBA pixels
//! 4. Provides the framebuffer to Bevy for texture upload
//!
//! # IPC
//!
//! Unlike CEF mode (which uses console.log interception), Dioxus mode
//! uses direct Rust channels for IPC between the UI and Bevy.

use crate::error::WebviewError;
use pentimento_dioxus_ui::{DioxusBridge, DioxusBridgeHandle};
use pentimento_ipc::{BevyToUi, KeyboardEvent, MouseEvent, UiToBevy};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{debug, info};

/// Dioxus renderer state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DioxusState {
    /// Ready for capture
    Ready,
}

/// Native Dioxus-based UI renderer
///
/// This implementation uses Dioxus with its native renderer instead of a browser.
/// The main advantages are:
/// - Fast startup (no browser engine initialization)
/// - Low memory usage
/// - Pure Rust implementation
/// - Direct IPC via Rust channels
pub struct LinuxDioxusRenderer {
    size: (u32, u32),
    state: DioxusState,
    dirty: Arc<AtomicBool>,
    /// Bridge handle for Bevy-side IPC
    bridge_handle: Option<DioxusBridgeHandle>,
    /// RGBA framebuffer for texture upload
    framebuffer: Option<Vec<u8>>,
}

impl LinuxDioxusRenderer {
    /// Create a new Dioxus renderer
    pub fn new(
        size: (u32, u32),
        dirty: Arc<AtomicBool>,
    ) -> Result<Self, WebviewError> {
        info!("Creating Dioxus renderer {}x{}", size.0, size.1);

        // Create the IPC bridge
        let (_bridge, bridge_handle) = DioxusBridge::new();

        // For now, we'll render a simple transparent overlay
        // The full Dioxus integration will require running the VirtualDOM
        // in a separate thread and using a custom renderer

        // Create initial transparent framebuffer
        let framebuffer_size = (size.0 * size.1 * 4) as usize;
        let mut framebuffer = vec![0u8; framebuffer_size];

        // Render a simple toolbar placeholder (semi-transparent dark bar at top)
        Self::render_placeholder_ui(&mut framebuffer, size.0, size.1);

        dirty.store(true, Ordering::SeqCst);

        Ok(Self {
            size,
            state: DioxusState::Ready,
            dirty,
            bridge_handle: Some(bridge_handle),
            framebuffer: Some(framebuffer),
        })
    }

    /// Render a placeholder UI (toolbar + side panel shapes)
    fn render_placeholder_ui(framebuffer: &mut [u8], width: u32, height: u32) {
        let width = width as usize;
        let height = height as usize;

        // Toolbar: dark semi-transparent bar at top (48px high)
        let toolbar_height = 48.min(height);
        for y in 0..toolbar_height {
            for x in 0..width {
                let idx = (y * width + x) * 4;
                if idx + 3 < framebuffer.len() {
                    framebuffer[idx] = 30;     // R
                    framebuffer[idx + 1] = 30; // G
                    framebuffer[idx + 2] = 30; // B
                    framebuffer[idx + 3] = 216; // A (85% opacity)
                }
            }
        }

        // Side panel: dark semi-transparent panel on right (300px wide)
        let panel_width = 300.min(width);
        let panel_top = 56.min(height);
        let panel_margin = 8;
        for y in panel_top..height.saturating_sub(panel_margin) {
            for x in (width.saturating_sub(panel_width + panel_margin))..width.saturating_sub(panel_margin) {
                let idx = (y * width + x) * 4;
                if idx + 3 < framebuffer.len() {
                    framebuffer[idx] = 30;     // R
                    framebuffer[idx + 1] = 30; // G
                    framebuffer[idx + 2] = 30; // B
                    framebuffer[idx + 3] = 216; // A (85% opacity)
                }
            }
        }

        // Draw "Pentimento" text placeholder in toolbar (white rectangle)
        let text_x = 16;
        let text_y = 14;
        let text_width = 80;
        let text_height = 20;
        for y in text_y..(text_y + text_height).min(toolbar_height) {
            for x in text_x..(text_x + text_width).min(width) {
                let idx = (y * width + x) * 4;
                if idx + 3 < framebuffer.len() {
                    framebuffer[idx] = 255;    // R
                    framebuffer[idx + 1] = 255; // G
                    framebuffer[idx + 2] = 255; // B
                    framebuffer[idx + 3] = 255; // A
                }
            }
        }
    }

    /// Poll for events (process IPC messages)
    pub fn poll(&mut self) {
        // Process any pending messages from the UI
        if let Some(ref handle) = self.bridge_handle {
            while let Some(_msg) = handle.try_recv() {
                // Handle UI -> Bevy messages
                debug!("Received message from Dioxus UI");
            }
        }
    }

    /// Check if the renderer is ready
    pub fn is_ready(&self) -> bool {
        self.state == DioxusState::Ready
    }

    /// Capture the framebuffer if dirty
    ///
    /// Returns RGBA pixel data with dimensions (data, width, height).
    pub fn capture(&mut self) -> Option<(Vec<u8>, u32, u32)> {
        if !self.is_ready() {
            return None;
        }

        self.framebuffer
            .take()
            .map(|fb| (fb, self.size.0, self.size.1))
    }

    /// Capture if dirty flag is set
    pub fn capture_if_dirty(&mut self) -> Option<(Vec<u8>, u32, u32)> {
        if !self.is_ready() {
            return None;
        }

        if self.dirty.swap(false, Ordering::SeqCst) {
            // Re-render the framebuffer
            let framebuffer_size = (self.size.0 * self.size.1 * 4) as usize;
            let mut framebuffer = vec![0u8; framebuffer_size];
            Self::render_placeholder_ui(&mut framebuffer, self.size.0, self.size.1);

            Some((framebuffer, self.size.0, self.size.1))
        } else {
            None
        }
    }

    /// Mark the UI as needing recapture
    pub fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::SeqCst);
    }

    /// Resize the renderer
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }

        info!("Dioxus renderer resize: {}x{}", width, height);
        self.size = (width, height);

        // Create new framebuffer
        let framebuffer_size = (width * height * 4) as usize;
        let mut framebuffer = vec![0u8; framebuffer_size];
        Self::render_placeholder_ui(&mut framebuffer, width, height);
        self.framebuffer = Some(framebuffer);

        self.mark_dirty();
    }

    /// Forward a mouse event to Dioxus
    pub fn inject_mouse(&mut self, _event: MouseEvent) {
        // TODO: Forward to Dioxus event system
        self.mark_dirty();
    }

    /// Forward a keyboard event to Dioxus
    pub fn inject_keyboard(&mut self, _event: KeyboardEvent) {
        // TODO: Forward to Dioxus event system
    }

    /// Send a message to the UI
    pub fn send_to_ui(&mut self, msg: BevyToUi) {
        if let Some(ref handle) = self.bridge_handle {
            handle.send(msg);
        }
    }

    /// Try to receive a message from the UI
    pub fn try_recv_from_ui(&mut self) -> Option<UiToBevy> {
        self.bridge_handle.as_ref()?.try_recv()
    }

    /// Get the bridge for direct access
    pub fn take_bridge_handle(&mut self) -> Option<DioxusBridgeHandle> {
        self.bridge_handle.take()
    }
}
