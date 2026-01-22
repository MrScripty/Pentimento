//! Pentimento Bevy WASM Build
//!
//! This crate compiles Bevy to WebAssembly for running inside a Tauri webview.
//! The 3D scene renders to a canvas element while Svelte UI overlays it.

use bevy::input::mouse::{MouseButton, MouseMotion, MouseWheel};
use bevy::prelude::*;
use pentimento_ipc::{BevyToUi, CameraCommand, UiToBevy};
use pentimento_scene::ScenePlugin;
use wasm_bindgen::prelude::*;

mod bridge;

/// Main entry point for the WASM module
#[wasm_bindgen(start)]
pub fn main() {
    // Set up panic hook for better error messages in browser console
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();

    // Initialize the bridge for IPC with the Svelte UI
    bridge::init_bridge();

    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        canvas: Some("#bevy-canvas".to_string()),
                        fit_canvas_to_parent: true,
                        prevent_default_event_handling: false,
                        ..default()
                    }),
                    ..default()
                })
                .set(bevy::log::LogPlugin {
                    level: bevy::log::Level::INFO,
                    ..default()
                }),
        )
        .add_plugins(ScenePlugin)
        .add_plugins(TauriIpcPlugin)
        .run();
}

/// Plugin for handling IPC between Bevy WASM and the Svelte UI
pub struct TauriIpcPlugin;

impl Plugin for TauriIpcPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, handle_ui_messages);
        app.add_systems(Update, debug_mouse_input);
    }
}

/// Debug system to log mouse input
fn debug_mouse_input(
    mouse_button: Res<ButtonInput<MouseButton>>,
    mut motion_events: MessageReader<MouseMotion>,
    mut scroll_events: MessageReader<MouseWheel>,
) {
    // Log any button press
    if mouse_button.just_pressed(MouseButton::Middle) {
        info!("DEBUG: Middle mouse button PRESSED!");
    }
    if mouse_button.just_released(MouseButton::Middle) {
        info!("DEBUG: Middle mouse button RELEASED!");
    }
    if mouse_button.just_pressed(MouseButton::Left) {
        info!("DEBUG: Left mouse button PRESSED!");
    }
    if mouse_button.just_pressed(MouseButton::Right) {
        info!("DEBUG: Right mouse button PRESSED!");
    }

    // Log held state periodically (only when motion happens)
    let has_motion = !motion_events.is_empty();
    if has_motion && mouse_button.pressed(MouseButton::Middle) {
        info!("DEBUG: Middle mouse HELD while moving");
    }

    for event in motion_events.read() {
        if event.delta.length() > 5.0 {
            info!("DEBUG: Mouse motion: {:?}", event.delta);
        }
    }

    for event in scroll_events.read() {
        info!("DEBUG: Mouse scroll: {:?}", event);
    }
}

/// System that polls for messages from the Svelte UI
fn handle_ui_messages(mut camera_query: Query<&mut Transform, With<Camera3d>>) {
    // Check for pending messages from the UI
    while let Some(msg) = bridge::poll_ui_message() {
        match msg {
            UiToBevy::CameraCommand(cmd) => match cmd {
                CameraCommand::Reset => {
                    // Reset camera to default position
                    for mut transform in camera_query.iter_mut() {
                        *transform =
                            Transform::from_xyz(5.0, 5.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y);
                        info!("Camera reset to default position");
                    }
                }
                CameraCommand::SetPosition { position } => {
                    for mut transform in camera_query.iter_mut() {
                        transform.translation = Vec3::from_array(position);
                        info!("Camera position set to: {:?}", position);
                    }
                }
                _ => {
                    info!("Camera command: {:?}", cmd);
                }
            },
            UiToBevy::UiDirty => {
                // UI has changed, but in Tauri mode we don't need to capture
                // since the UI is rendered directly by the browser
            }
            _ => {
                info!("Received UI message: {:?}", msg);
            }
        }
    }
}

/// Send a message to the Svelte UI
#[allow(dead_code)]
pub fn send_to_ui(msg: BevyToUi) {
    bridge::send_to_ui(msg);
}
