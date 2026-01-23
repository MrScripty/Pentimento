//! Render pipeline extensions for UI compositing
//!
//! Supports multiple compositing modes:
//! - Capture: Offscreen webview with framebuffer capture (default)
//! - Overlay: Transparent child window composited by desktop compositor
//! - Cef: CEF (Chromium) offscreen rendering with framebuffer capture
//! - Tauri: Bevy WASM in Tauri webview (requires separate build)

use bevy::prelude::*;

use crate::config::{CompositeMode, PentimentoConfig};

mod ui_composite;
mod ui_overlay;
#[cfg(feature = "cef")]
mod ui_cef;

// Re-export types needed by the input module
pub use ui_composite::WebviewResource;
pub use ui_overlay::OverlayWebviewResource;
#[cfg(feature = "cef")]
pub use ui_cef::CefWebviewResource;

pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        let config = app.world().resource::<PentimentoConfig>();
        let mode = config.composite_mode;

        match mode {
            CompositeMode::Capture => {
                // Existing capture mode setup
                app.init_resource::<ui_composite::LastWindowSize>()
                    .init_resource::<ui_composite::WebviewStatus>();

                app.add_systems(Startup, ui_composite::setup_ui_composite)
                    .add_systems(Update, ui_composite::update_ui_texture)
                    .add_systems(Update, ui_composite::handle_window_resize);

                info!("Render plugin initialized with CAPTURE mode");
            }
            CompositeMode::Overlay => {
                // Overlay mode setup
                // Note: Resources are initialized by setup_ui_overlay, not here
                // because OverlayStatus has non-default initial values

                app.add_systems(Startup, ui_overlay::setup_ui_overlay)
                    // update_overlay_webview is an exclusive system (takes &mut World)
                    // because it handles deferred webview creation
                    .add_systems(Update, ui_overlay::update_overlay_webview)
                    .add_systems(Update, ui_overlay::handle_overlay_resize.after(ui_overlay::update_overlay_webview))
                    .add_systems(Update, ui_overlay::sync_overlay_position.after(ui_overlay::update_overlay_webview))
                    .add_systems(Update, ui_overlay::sync_overlay_visibility.after(ui_overlay::update_overlay_webview));

                info!("Render plugin initialized with OVERLAY mode");
            }
            #[cfg(feature = "cef")]
            CompositeMode::Cef => {
                // CEF mode setup
                app.init_resource::<ui_cef::CefLastWindowSize>()
                    .init_resource::<ui_cef::CefWebviewStatus>();

                app.add_systems(Startup, ui_cef::setup_ui_cef)
                    .add_systems(Update, ui_cef::update_cef_ui_texture)
                    .add_systems(Update, ui_cef::handle_cef_window_resize);

                info!("Render plugin initialized with CEF mode");
            }
            #[cfg(not(feature = "cef"))]
            CompositeMode::Cef => {
                error!("CEF mode requires the 'cef' feature. Build with: cargo build --features cef");
                panic!("CEF mode not available - rebuild with --features cef");
            }
            CompositeMode::Tauri => {
                // Tauri mode is handled differently - Bevy runs as WASM in Tauri's webview
                // In native builds, this mode is not applicable
                warn!("Tauri mode requires building for WASM and running inside Tauri");
                info!("Render plugin: Tauri mode - no native render setup needed");
            }
        }
    }
}
