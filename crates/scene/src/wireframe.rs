//! Wireframe overlay rendering
//!
//! Provides a faint wireframe overlay on all 3D objects.
//! This feature requires the `wireframe` feature flag and only works on native
//! builds (not WASM/WebGL2 due to GPU feature requirements).

use bevy::pbr::wireframe::{WireframeConfig, WireframePlugin};
use bevy::prelude::*;

/// Wireframe display settings
#[derive(Resource)]
pub struct WireframeSettings {
    /// Whether wireframe is enabled
    pub enabled: bool,
    /// Wireframe color
    pub color: Color,
}

impl Default for WireframeSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            // Faint white wireframe
            color: Color::srgba(0.8, 0.8, 0.8, 0.3),
        }
    }
}

/// Plugin for wireframe overlay rendering
pub struct WireframeOverlayPlugin;

impl Plugin for WireframeOverlayPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(WireframePlugin::default())
            .init_resource::<WireframeSettings>()
            .add_systems(Update, sync_wireframe_config);
    }
}

/// Sync WireframeConfig with WireframeSettings
fn sync_wireframe_config(
    settings: Res<WireframeSettings>,
    mut config: ResMut<WireframeConfig>,
) {
    if settings.is_changed() {
        config.global = settings.enabled;
        config.default_color = settings.color;

        if settings.enabled {
            info!("Wireframe overlay enabled");
        } else {
            info!("Wireframe overlay disabled");
        }
    }
}
