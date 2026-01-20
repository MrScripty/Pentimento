//! Render pipeline extensions for UI compositing
//!
//! Supports two compositing modes:
//! - Capture: Offscreen webview with framebuffer capture (default)
//! - Overlay: Transparent child window composited by desktop compositor

use bevy::prelude::*;

use crate::config::{CompositeMode, PentimentoConfig};

mod ui_composite;
mod ui_overlay;

// Re-export types needed by the input module
pub use ui_composite::WebviewResource;
pub use ui_overlay::OverlayWebviewResource;

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
                // New overlay mode setup
                app.init_resource::<ui_overlay::OverlayStatus>()
                    .init_resource::<ui_overlay::OverlayLastWindowSize>()
                    .init_resource::<ui_overlay::OverlayPosition>();

                app.add_systems(Startup, ui_overlay::setup_ui_overlay)
                    .add_systems(Update, ui_overlay::update_overlay_webview)
                    .add_systems(Update, ui_overlay::handle_overlay_resize)
                    .add_systems(Update, ui_overlay::sync_overlay_position);

                info!("Render plugin initialized with OVERLAY mode");
            }
        }
    }
}
