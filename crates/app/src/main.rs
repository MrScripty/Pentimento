//! Pentimento - Bevy + Svelte Compositing Desktop Application

use bevy::prelude::*;
use bevy::window::WindowResolution;
use pentimento_config::{DisplayConfig, DEFAULT_HEIGHT, DEFAULT_WIDTH};

#[cfg(feature = "wireframe")]
use bevy::render::{
    render_resource::WgpuFeatures,
    settings::{RenderCreation, WgpuSettings},
    RenderPlugin as BevyRenderPlugin,
};

mod config;
mod embedded_ui;
mod input;
mod render;

use config::{CompositeMode, PentimentoConfig};
use pentimento_scene::ScenePlugin;

fn main() {
    // Parse configuration from environment
    let config = PentimentoConfig::default();

    info!(
        "Starting Pentimento with {:?} compositing mode",
        config.composite_mode
    );

    // Initialize GTK for webview on Linux (needed for both modes)
    // Note: For CEF mode, GTK is not strictly required, but we initialize it
    // anyway for compatibility with non-CEF code paths
    #[cfg(target_os = "linux")]
    {
        gtk::init().expect("Failed to initialize GTK");
    }

    // Display configuration - single source of truth for window size
    let display_config = DisplayConfig::default();

    // Configure window based on compositing mode
    let window_config = Window {
        title: "Pentimento".into(),
        resolution: WindowResolution::new(DEFAULT_WIDTH, DEFAULT_HEIGHT),
        present_mode: bevy::window::PresentMode::AutoVsync,
        // Transparent window helps with overlay mode blending
        transparent: config.composite_mode == CompositeMode::Overlay,
        ..default()
    };

    let mut app = App::new();

    app.insert_resource(config)
        .insert_resource(display_config);

    // Configure plugins with optional wireframe support
    #[cfg(feature = "wireframe")]
    {
        app.add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(window_config),
                    ..default()
                })
                .set(bevy::log::LogPlugin {
                    level: bevy::log::Level::INFO,
                    ..default()
                })
                .set(BevyRenderPlugin {
                    render_creation: RenderCreation::Automatic(WgpuSettings {
                        features: WgpuFeatures::POLYGON_MODE_LINE,
                        ..default()
                    }),
                    ..default()
                }),
        );
    }

    #[cfg(not(feature = "wireframe"))]
    {
        app.add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(window_config),
                    ..default()
                })
                .set(bevy::log::LogPlugin {
                    level: bevy::log::Level::INFO,
                    ..default()
                }),
        );
    }

    app.add_plugins(ScenePlugin)
        .add_plugins(render::RenderPlugin)
        .add_plugins(input::InputPlugin)
        .run();
}
