//! Pentimento - Bevy + Svelte Compositing Desktop Application

use bevy::prelude::*;
use bevy::window::WindowResolution;

mod embedded_ui;
mod render;
mod scene;

fn main() {
    // Initialize GTK for the offscreen webview on Linux
    #[cfg(target_os = "linux")]
    {
        gtk::init().expect("Failed to initialize GTK");
    }

    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Pentimento".into(),
                        resolution: WindowResolution::new(1920, 1080),
                        present_mode: bevy::window::PresentMode::AutoVsync,
                        ..default()
                    }),
                    ..default()
                })
                .set(bevy::log::LogPlugin {
                    level: bevy::log::Level::INFO,
                    ..default()
                }),
        )
        .add_plugins(scene::ScenePlugin)
        .add_plugins(render::RenderPlugin)
        .run();
}
