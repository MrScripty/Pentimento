//! Frontend backend abstraction for input handling
//!
//! This module provides a unified interface for sending input events to different
//! frontend backends (Capture, Overlay, CEF, Dioxus) without requiring match statements
//! in every input system.
//!
//! # Architecture
//!
//! The `FrontendBackend` system parameter uses the unified `FrontendResource` which
//! wraps a `Box<dyn CompositeBackend>`. This provides a single interface for:
//! - Mouse events (move, click, scroll)
//! - Keyboard events
//! - Coordinate scaling (logical vs physical pixels)
//!
//! The Dioxus renderer is kept separate as it uses a different render pipeline.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use pentimento_config::DisplayConfig;
use pentimento_ipc::{KeyboardEvent, MouseEvent};

use crate::config::{CompositeMode, PentimentoConfig};
use crate::render::FrontendResource;
#[cfg(feature = "dioxus")]
use crate::render::DioxusRendererResource;

/// Unified system parameter for accessing the frontend backend.
///
/// This abstracts over the different webview/renderer resources, providing
/// a single interface for input handling systems. Uses `FrontendResource`
/// for Capture, Overlay, and CEF modes (all implement `CompositeBackend`).
#[derive(SystemParam)]
pub struct FrontendBackend<'w, 's> {
    config: Res<'w, PentimentoConfig>,
    #[allow(dead_code)]
    display_config: Res<'w, DisplayConfig>,
    /// Unified frontend resource for Capture, Overlay, and CEF modes
    frontend: Option<NonSendMut<'w, FrontendResource>>,
    /// Dioxus renderer (uses separate render pipeline)
    /// NOTE: Must be NonSendMut because DioxusRendererResource is inserted as NonSend
    #[cfg(feature = "dioxus")]
    dioxus_renderer: Option<NonSendMut<'w, DioxusRendererResource>>,
    #[doc(hidden)]
    _marker: std::marker::PhantomData<&'s ()>,
}

impl<'w, 's> FrontendBackend<'w, 's> {
    /// Get the current composite mode
    pub fn mode(&self) -> CompositeMode {
        self.config.composite_mode
    }

    /// Check if the backend is available and ready to receive events
    pub fn is_available(&self) -> bool {
        match self.config.composite_mode {
            CompositeMode::Capture | CompositeMode::Overlay => self.frontend.is_some(),
            #[cfg(feature = "cef")]
            CompositeMode::Cef => self.frontend.is_some(),
            #[cfg(not(feature = "cef"))]
            CompositeMode::Cef => false,
            #[cfg(feature = "dioxus")]
            CompositeMode::Dioxus => self.dioxus_renderer.is_some(),
            #[cfg(not(feature = "dioxus"))]
            CompositeMode::Dioxus => false,
            CompositeMode::Tauri => false, // Tauri handles its own input
        }
    }

    /// Scale coordinates from logical (Bevy) to backend-specific coordinates.
    ///
    /// Different backends have different coordinate expectations:
    /// - CEF: Uses logical/CSS coordinates (no scaling needed)
    /// - Dioxus: Uses logical/CSS coordinates (no scaling needed)
    /// - Capture/Overlay: Uses physical pixel coordinates (scaled by DPI)
    pub fn scale_coordinates(&self, x: f32, y: f32, scale_factor: f32) -> (f32, f32) {
        match self.config.composite_mode {
            #[cfg(feature = "cef")]
            CompositeMode::Cef => {
                // CEF expects logical/CSS coordinates for mouse events
                (x, y)
            }
            #[cfg(feature = "dioxus")]
            CompositeMode::Dioxus => {
                // Blitz expects logical (CSS) pixels, not physical pixels
                (x, y)
            }
            CompositeMode::Capture | CompositeMode::Overlay => {
                // WebKit-based backends use physical resolution
                (x * scale_factor, y * scale_factor)
            }
            _ => {
                // Fallback for other modes
                (x, y)
            }
        }
    }

    /// Send a mouse event to the backend.
    ///
    /// Returns true if the event was sent successfully, false if no backend is available.
    pub fn send_mouse_event(&mut self, event: MouseEvent) -> bool {
        match self.config.composite_mode {
            CompositeMode::Capture | CompositeMode::Overlay => {
                if let Some(ref mut frontend) = self.frontend {
                    frontend.backend.send_mouse_event(event);
                    return true;
                }
            }
            #[cfg(feature = "cef")]
            CompositeMode::Cef => {
                if let Some(ref mut frontend) = self.frontend {
                    frontend.backend.send_mouse_event(event);
                    return true;
                }
            }
            #[cfg(not(feature = "cef"))]
            CompositeMode::Cef => {}
            #[cfg(feature = "dioxus")]
            CompositeMode::Dioxus => {
                if let Some(ref mut renderer) = self.dioxus_renderer {
                    renderer.send_mouse_event(event);
                    return true;
                }
            }
            #[cfg(not(feature = "dioxus"))]
            CompositeMode::Dioxus => {}
            CompositeMode::Tauri => {
                // Tauri mode handles input in the browser
            }
        }
        false
    }

    /// Send a keyboard event to the backend.
    ///
    /// Returns true if the event was sent successfully, false if no backend is available.
    pub fn send_keyboard_event(&mut self, event: KeyboardEvent) -> bool {
        match self.config.composite_mode {
            CompositeMode::Capture | CompositeMode::Overlay => {
                if let Some(ref mut frontend) = self.frontend {
                    frontend.backend.send_keyboard_event(event);
                    return true;
                }
            }
            #[cfg(feature = "cef")]
            CompositeMode::Cef => {
                if let Some(ref mut frontend) = self.frontend {
                    frontend.backend.send_keyboard_event(event);
                    return true;
                }
            }
            #[cfg(not(feature = "cef"))]
            CompositeMode::Cef => {}
            #[cfg(feature = "dioxus")]
            CompositeMode::Dioxus => {
                if let Some(ref mut renderer) = self.dioxus_renderer {
                    renderer.send_keyboard_event(event);
                    return true;
                }
            }
            #[cfg(not(feature = "dioxus"))]
            CompositeMode::Dioxus => {}
            CompositeMode::Tauri => {
                // Tauri mode handles input in the browser
            }
        }
        false
    }
}

/// Marker trait for backends that support DevTools (currently only CEF)
#[cfg(feature = "cef")]
pub trait DevToolsCapable {
    fn show_dev_tools(&self);
}
