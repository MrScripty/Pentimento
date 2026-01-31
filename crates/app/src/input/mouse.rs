//! Mouse input handling - forwards Bevy mouse events to the frontend backend
//!
//! This module handles:
//! - Mouse position tracking (window and webview coordinates)
//! - Mouse button forwarding (click, press, release)
//! - Mouse scroll forwarding

use bevy::input::mouse::{MouseButtonInput, MouseWheel};
use bevy::prelude::*;
use bevy::window::CursorMoved;
use pentimento_ipc::{MouseButton as IpcMouseButton, MouseEvent};
use std::time::{Duration, Instant};

use super::backend::FrontendBackend;
use super::MouseState;

/// Minimum interval between mouse move events sent to webview (throttling)
pub const MOUSE_MOVE_THROTTLE: Duration = Duration::from_millis(16); // ~60fps max

/// Track mouse cursor position and forward mouse move events (throttled)
/// This system MUST run before forward_mouse_buttons and forward_mouse_scroll
pub fn track_mouse_position(
    mut mouse_state: ResMut<MouseState>,
    mut cursor_events: MessageReader<CursorMoved>,
    mut backend: FrontendBackend,
    windows: Query<&Window>,
) {
    let Ok(window) = windows.single() else {
        cursor_events.clear();
        return;
    };

    // Process CursorMoved events - these contain the actual cursor position
    // Use the LAST event position as that's the most recent
    let mut had_cursor_event = false;
    for event in cursor_events.read() {
        // Scale coordinates based on backend requirements
        let (webview_x, webview_y) = backend.scale_coordinates(
            event.position.x,
            event.position.y,
            window.resolution.scale_factor(),
        );

        mouse_state.window_x = event.position.x;
        mouse_state.window_y = event.position.y;
        mouse_state.webview_x = webview_x;
        mouse_state.webview_y = webview_y;
        had_cursor_event = true;
    }

    // Only send mouse move to webview if there was cursor movement AND throttle allows
    if !had_cursor_event {
        return;
    }

    let now = Instant::now();
    if now.duration_since(mouse_state.last_move_sent) < MOUSE_MOVE_THROTTLE {
        return;
    }

    let mouse_event = MouseEvent::Move {
        x: mouse_state.webview_x,
        y: mouse_state.webview_y,
    };

    if backend.send_mouse_event(mouse_event) {
        mouse_state.last_move_sent = now;
    }
}

/// Forward mouse button events to the webview
/// Runs after track_mouse_position so MouseState is up-to-date
pub fn forward_mouse_buttons(
    mut button_events: MessageReader<MouseButtonInput>,
    mouse_state: Res<MouseState>,
    mut backend: FrontendBackend,
) {
    // Use the tracked position (updated by track_mouse_position which runs first)
    let click_x = mouse_state.webview_x;
    let click_y = mouse_state.webview_y;

    // Collect events first to avoid borrow issues
    let events: Vec<_> = button_events.read().cloned().collect();
    if events.is_empty() {
        return;
    }

    for event in &events {
        let Some(button) = convert_mouse_button(event.button) else {
            continue;
        };

        if event.state.is_pressed() {
            info!("Click at webview ({:.1}, {:.1})", click_x, click_y);
        }

        let mouse_event = if event.state.is_pressed() {
            MouseEvent::ButtonDown {
                button,
                x: click_x,
                y: click_y,
            }
        } else {
            MouseEvent::ButtonUp {
                button,
                x: click_x,
                y: click_y,
            }
        };

        backend.send_mouse_event(mouse_event);
    }
}

/// Forward mouse scroll events to the webview
/// Runs after track_mouse_position so MouseState is up-to-date
pub fn forward_mouse_scroll(
    mut scroll_events: MessageReader<MouseWheel>,
    mouse_state: Res<MouseState>,
    mut backend: FrontendBackend,
) {
    let scroll_x = mouse_state.webview_x;
    let scroll_y = mouse_state.webview_y;

    let events: Vec<_> = scroll_events.read().cloned().collect();
    if events.is_empty() {
        return;
    }

    for event in &events {
        let (delta_x, delta_y) = convert_scroll_delta(event);
        backend.send_mouse_event(MouseEvent::Scroll {
            delta_x,
            delta_y: -delta_y, // Invert Y for web conventions
            x: scroll_x,
            y: scroll_y,
        });
    }
}

/// Convert Bevy mouse button to IPC mouse button
fn convert_mouse_button(button: bevy::input::mouse::MouseButton) -> Option<IpcMouseButton> {
    match button {
        bevy::input::mouse::MouseButton::Left => Some(IpcMouseButton::Left),
        bevy::input::mouse::MouseButton::Right => Some(IpcMouseButton::Right),
        bevy::input::mouse::MouseButton::Middle => Some(IpcMouseButton::Middle),
        _ => None,
    }
}

/// Convert scroll deltas based on unit type
fn convert_scroll_delta(event: &bevy::input::mouse::MouseWheel) -> (f32, f32) {
    match event.unit {
        bevy::input::mouse::MouseScrollUnit::Line => (event.x * 40.0, event.y * 40.0),
        bevy::input::mouse::MouseScrollUnit::Pixel => (event.x, event.y),
    }
}
