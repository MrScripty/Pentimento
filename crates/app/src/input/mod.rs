//! Input handling - forwards Bevy input events to the frontend backend
//!
//! This module provides a unified input handling system that works with all
//! compositing modes (Capture, Overlay, CEF, Dioxus) through the `FrontendBackend`
//! system parameter.
//!
//! # Architecture
//!
//! The input system is organized into submodules:
//! - `backend`: Unified backend abstraction for sending events
//! - `mouse`: Mouse position tracking and event forwarding
//! - `keyboard`: Keyboard event forwarding and key conversion
//! - `hotkeys`: Global hotkey handling (DevTools, Undo, Add Menu)
//!
//! # Usage
//!
//! Systems use the `FrontendBackend` system parameter instead of matching on
//! composite mode and accessing individual backend resources:
//!
//! ```ignore
//! fn my_system(mut backend: FrontendBackend) {
//!     backend.send_mouse_event(MouseEvent::Move { x: 100.0, y: 200.0 });
//! }
//! ```

use bevy::input::mouse::MouseMotion;
use bevy::input::InputSystems;
use bevy::prelude::*;
use std::time::Instant;

mod backend;
mod hotkeys;
mod keyboard;
mod mouse;

// Re-export for use by other modules
pub use backend::FrontendBackend;
pub use keyboard::bevy_keycode_to_web_key;

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MouseState>()
            // Run in PreUpdate to get the freshest input state before other systems
            .add_systems(
                PreUpdate,
                (
                    clear_motion_events,
                    mouse::track_mouse_position,
                )
                    .chain()
                    .after(InputSystems),
            )
            .add_systems(
                PreUpdate,
                (
                    mouse::forward_mouse_buttons,
                    mouse::forward_mouse_scroll,
                    keyboard::forward_keyboard,
                )
                    .after(mouse::track_mouse_position),
            );

        // CEF DevTools hotkey (Ctrl+Shift+I)
        #[cfg(feature = "cef")]
        app.add_systems(PreUpdate, hotkeys::handle_devtools_hotkey.after(InputSystems));

        // Paint undo hotkey (Ctrl+Z)
        app.add_systems(PreUpdate, hotkeys::handle_paint_undo_hotkey.after(InputSystems));

        // Add object menu hotkey (Shift+A)
        app.add_systems(PreUpdate, hotkeys::handle_add_menu_hotkey.after(InputSystems));

        info!("Input plugin initialized");
    }
}

/// Track mouse position in both window and scaled webview coordinates
#[derive(Resource)]
pub struct MouseState {
    /// Position in window coordinates
    pub window_x: f32,
    pub window_y: f32,
    /// Position scaled to webview coordinates
    pub webview_x: f32,
    pub webview_y: f32,
    /// Last time we sent a mouse move event (for throttling)
    pub last_move_sent: Instant,
}

impl Default for MouseState {
    fn default() -> Self {
        Self {
            window_x: 0.0,
            window_y: 0.0,
            webview_x: 0.0,
            webview_y: 0.0,
            last_move_sent: Instant::now(),
        }
    }
}

/// Clear motion events - we use cursor position instead
fn clear_motion_events(mut motion_events: MessageReader<MouseMotion>) {
    motion_events.clear();
}
