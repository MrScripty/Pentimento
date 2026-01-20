//! Pentimento - Bevy + Svelte Compositing Desktop Application

use bevy::prelude::*;
use bevy::window::WindowResolution;

mod config;
mod embedded_ui;
mod input;
mod render;
mod scene;

use config::{CompositeMode, PentimentoConfig};

fn main() {
    // Parse configuration from environment
    let config = PentimentoConfig::default();

    info!(
        "Starting Pentimento with {:?} compositing mode",
        config.composite_mode
    );

    // Initialize GTK for webview on Linux (needed for both modes)
    #[cfg(target_os = "linux")]
    {
        gtk::init().expect("Failed to initialize GTK");
    }

    // Configure window based on compositing mode
    let window_config = Window {
        title: "Pentimento".into(),
        resolution: WindowResolution::new(1920, 1080),
        present_mode: bevy::window::PresentMode::AutoVsync,
        // Transparent window helps with overlay mode blending
        transparent: config.composite_mode == CompositeMode::Overlay,
        ..default()
    };

    App::new()
        .insert_resource(config)
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(window_config),
                    ..default()
                })
                .set(bevy::log::LogPlugin {
                    level: bevy::log::Level::INFO,
                    ..default()
                }),
        )
        .add_plugins(scene::ScenePlugin)
        .add_plugins(render::RenderPlugin)
        .add_plugins(input::InputPlugin)
        .run();
}
