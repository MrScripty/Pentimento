//! Wireframe overlay rendering
//!
//! Provides a faint wireframe overlay on all 3D objects.
//! This feature requires the `wireframe` feature flag and only works on native
//! builds (not WASM/WebGL2 due to GPU feature requirements).

use bevy::pbr::wireframe::{WireframeConfig, WireframePlugin};
use bevy::prelude::*;
use bevy::render::camera::{DirtySpecializations, DirtyWireframeSpecializations};
use bevy::render::{Render, RenderApp, RenderSystems};

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
            enabled: true, // Enabled by default for debugging sculpt topology
            // Faint white wireframe
            color: Color::srgba(0.8, 0.8, 0.8, 0.5),
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

        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.add_systems(
                Render,
                invalidate_changed_wireframe_views
                    .in_set(RenderSystems::Specialize)
                    .after(bevy::pbr::check_views_need_specialization)
                    .before(bevy::pbr::wireframe::specialize_wireframes),
            );
        }
    }
}

// Bevy 0.20 marks changed view layouts for mesh specialization only. Wireframe
// pipelines share those layouts, so depth-prepass and environment-map changes
// must also invalidate their cached pipelines before specialization and queueing.
fn invalidate_changed_wireframe_views(
    mesh: Res<DirtySpecializations>,
    mut wireframe: ResMut<DirtyWireframeSpecializations>,
) {
    wireframe.views.extend(mesh.views.iter().copied());
}

/// Sync WireframeConfig with WireframeSettings
fn sync_wireframe_config(settings: Res<WireframeSettings>, mut config: ResMut<WireframeConfig>) {
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
