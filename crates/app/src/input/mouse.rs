//! Mouse input handling - forwards Bevy mouse events to the frontend backend
//!
//! This module handles:
//! - Mouse position tracking (window and webview coordinates)
//! - Mouse button forwarding (click, press, release)
//! - Mouse scroll forwarding

use bevy::prelude::*;
use bevy::window::WindowEvent;
use pentimento_ipc::{MouseButton as IpcMouseButton, MouseEvent};
use std::time::{Duration, Instant};

use super::MouseState;
use super::backend::FrontendBackend;

#[cfg(test)]
#[path = "mouse_tests.rs"]
mod tests;

/// Minimum interval between mouse move events sent to webview (throttling)
pub const MOUSE_MOVE_THROTTLE: Duration = Duration::from_millis(16); // ~60fps max

/// Observe native chronology without changing forwarding or capture. Qualification
/// enables this module's debug logs to compare origin with the old final-coordinate path.
pub fn trace_pointer_origin(
    mut events: MessageReader<WindowEvent>,
    mut cursor: Local<Vec2>,
    windows: Query<Entity, With<Window>>,
    layout: Res<pentimento_scene::FrontendUiLayout>,
) {
    let Ok(window) = windows.single() else {
        events.clear();
        return;
    };
    for event in events.read() {
        match event {
            WindowEvent::CursorMoved(event) if event.window == window => {
                *cursor = event.position;
                debug!(
                    "Native pointer order: move ({:.1}, {:.1})",
                    cursor.x, cursor.y
                );
            }
            WindowEvent::MouseButtonInput(event) if event.window == window => {
                let over_ui = layout.regions.iter().any(|r| {
                    cursor.x >= r.x
                        && cursor.y >= r.y
                        && cursor.x < r.x + r.width
                        && cursor.y < r.y + r.height
                });
                debug!(
                    "Native pointer order: {:?} {:?} origin ({:.1}, {:.1}) layout_received={} over_ui={}",
                    event.button, event.state, cursor.x, cursor.y, layout.received, over_ui
                );
            }
            _ => {}
        }
    }
}

/// State owned by native forwarding, independent of Bevy's final frame state.
#[derive(Default)]
pub struct NativeInputState {
    pending_move: Option<Vec2>,
    buttons: ButtonInput<bevy::input::mouse::MouseButton>,
    ui_buttons: ButtonInput<bevy::input::mouse::MouseButton>,
    alt_graph: ButtonInput<KeyCode>,
    layout_reported: bool,
    focus_suspended: bool,
}

fn over_ui(layout: &pentimento_scene::FrontendUiLayout, position: Vec2) -> bool {
    layout.regions.iter().any(|r| {
        position.x >= r.x
            && position.y >= r.y
            && position.x < r.x + r.width
            && position.y < r.y + r.height
    })
}

fn flush_move(state: &mut NativeInputState, mouse: &mut MouseState, backend: &mut FrontendBackend) {
    if let Some(position) = state.pending_move.take()
        && backend.send_mouse_event(MouseEvent::Move {
            x: position.x,
            y: position.y,
        })
    {
        mouse.last_move_sent = Instant::now();
    }
}

/// Winit publishes an ordered WindowEvent stream as well as typed messages.
/// Consume the ordered stream without taking typed input away from Bevy/egui.
/// Only consecutive moves coalesce; every button, wheel, key and focus event
/// is a barrier, so clicks retain their origin and focus changes precede text.
pub fn forward_native_input(
    mut events: MessageReader<WindowEvent>,
    mut mouse: ResMut<MouseState>,
    mut backend: FrontendBackend,
    windows: Query<(Entity, &Window)>,
    key_input: Res<ButtonInput<KeyCode>>,
    config: Res<crate::config::PentimentoConfig>,
    mut layout: ResMut<pentimento_scene::FrontendUiLayout>,
    mut input_blocks: ResMut<pentimento_scene::FrontendInputBlockState>,
    mut state: Local<NativeInputState>,
) {
    let Ok((window_id, window)) = windows.single() else {
        events.clear();
        return;
    };
    let batch: Vec<_> = events.read().collect();
    let focus_lost = batch.iter().any(|event| match event {
        WindowEvent::KeyboardFocusLost(_) => true,
        WindowEvent::WindowFocused(event) => event.window == window_id && !event.focused,
        _ => false,
    });
    let keys: Vec<_> = batch
        .iter()
        .filter_map(|event| match event {
            WindowEvent::KeyboardInput(event) if event.window == window_id => Some(event.clone()),
            _ => None,
        })
        .collect();
    let translated = super::keyboard::translate_keyboard_events(
        &keys,
        &key_input,
        &mut state.alt_graph,
        focus_lost,
    );
    let mut translated = translated.into_iter();
    let mut ui_owned_in_frame = state.ui_buttons.get_pressed().next().is_some();
    if layout.received && !state.layout_reported {
        debug!(
            "Native input layout received: {} regions",
            layout.regions.len()
        );
        state.layout_reported = true;
    }
    for event in batch {
        match event {
            WindowEvent::CursorMoved(event) if event.window == window_id => {
                let (x, y) = backend.scale_coordinates(
                    event.position.x,
                    event.position.y,
                    window.resolution.scale_factor(),
                );
                mouse.window_x = event.position.x;
                mouse.window_y = event.position.y;
                mouse.webview_x = x;
                mouse.webview_y = y;
                state.pending_move = Some(Vec2::new(x, y));
            }
            WindowEvent::MouseButtonInput(event) if event.window == window_id => {
                flush_move(&mut state, &mut mouse, &mut backend);
                let position = Vec2::new(mouse.webview_x, mouse.webview_y);
                if event.state.is_pressed() {
                    if !state.buttons.pressed(event.button) && over_ui(&layout, position) {
                        state.ui_buttons.press(event.button);
                    }
                    state.buttons.press(event.button);
                    ui_owned_in_frame |= state.ui_buttons.get_pressed().next().is_some();
                } else {
                    ui_owned_in_frame |= state.ui_buttons.pressed(event.button);
                    state.ui_buttons.release(event.button);
                    state.buttons.release(event.button);
                }
                if let Some(button) = convert_mouse_button(event.button) {
                    let packet = if event.state.is_pressed() {
                        info!("Click at webview ({:.1}, {:.1})", position.x, position.y);
                        MouseEvent::ButtonDown {
                            button,
                            x: position.x,
                            y: position.y,
                        }
                    } else {
                        MouseEvent::ButtonUp {
                            button,
                            x: position.x,
                            y: position.y,
                        }
                    };
                    backend.send_mouse_event(packet);
                }
            }
            WindowEvent::MouseWheel(event) if event.window == window_id => {
                flush_move(&mut state, &mut mouse, &mut backend);
                let (delta_x, delta_y) = convert_scroll_delta(event);
                backend.send_mouse_event(MouseEvent::Scroll {
                    delta_x,
                    delta_y: -delta_y,
                    x: mouse.webview_x,
                    y: mouse.webview_y,
                });
            }
            WindowEvent::KeyboardInput(event) if event.window == window_id => {
                flush_move(&mut state, &mut mouse, &mut backend);
                if let Some(event) = translated.next() {
                    backend.send_keyboard_event(event);
                }
            }
            WindowEvent::KeyboardFocusLost(_) => {
                release_on_focus_loss(&mut state, &mut mouse, &mut backend);
            }
            WindowEvent::WindowFocused(event) if event.window == window_id => {
                if event.focused {
                    flush_move(&mut state, &mut mouse, &mut backend);
                    state.focus_suspended = false;
                } else {
                    release_on_focus_loss(&mut state, &mut mouse, &mut backend);
                }
            }
            _ => {}
        }
    }
    layout.pointer_captured = state.ui_buttons.get_pressed().next().is_some();
    // egui owns its own flags. Browser startup blocks scene input until native
    // hit-testing has rectangles; Dioxus/Tauri do not use the Svelte reporter.
    let svelte_browser = matches!(
        config.composite_mode,
        crate::config::CompositeMode::Capture
            | crate::config::CompositeMode::Overlay
            | crate::config::CompositeMode::Cef
    );
    if svelte_browser
        || (config.composite_mode == crate::config::CompositeMode::Dioxus && layout.received)
    {
        input_blocks.block_pointer = (svelte_browser && !layout.received)
            || focus_lost
            || state.focus_suspended
            || ui_owned_in_frame
            || layout.pointer_captured
            || over_ui(&layout, Vec2::new(mouse.webview_x, mouse.webview_y));
        debug!(
            "Native capture after batch: last ({:.1}, {:.1}) blocked={} latched={} layout_received={}",
            mouse.webview_x,
            mouse.webview_y,
            input_blocks.block_pointer,
            layout.pointer_captured,
            layout.received
        );
    }
    // Idle hover is throttled; held drags must reach their latest position.
    if state.buttons.get_pressed().next().is_some()
        || mouse.last_move_sent.elapsed() >= MOUSE_MOVE_THROTTLE
    {
        flush_move(&mut state, &mut mouse, &mut backend);
    }
}

fn release_on_focus_loss(
    state: &mut NativeInputState,
    mouse: &mut MouseState,
    backend: &mut FrontendBackend,
) {
    flush_move(state, mouse, backend);
    // Native focus loss may omit physical releases. Do not leave widgets held.
    for button in state.buttons.get_pressed().copied() {
        if let Some(button) = convert_mouse_button(button) {
            backend.send_mouse_event(MouseEvent::ButtonUp {
                button,
                x: mouse.webview_x,
                y: mouse.webview_y,
            });
        }
    }
    state.focus_suspended = true;
    state.buttons.reset_all();
    state.ui_buttons.reset_all();
    state.alt_graph.reset_all();
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
