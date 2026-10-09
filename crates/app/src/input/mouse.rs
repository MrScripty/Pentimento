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
    generation: Option<u64>,
    suppressed_buttons: ButtonInput<bevy::input::mouse::MouseButton>,
    suppressed_touch: Option<u64>,
    pending_move: Option<Vec2>,
    buttons: ButtonInput<bevy::input::mouse::MouseButton>,
    ui_buttons: ButtonInput<bevy::input::mouse::MouseButton>,
    scene_buttons: ButtonInput<bevy::input::mouse::MouseButton>,
    alt_graph: ButtonInput<KeyCode>,
    layout_reported: bool,
    focus_suspended: bool,
    scene_cursor: Option<Vec2>,
    touch: Option<NativeTouch>,
}

struct NativeTouch {
    last: bevy::input::touch::TouchInput,
    scene: bool,
    ui: bool,
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
/// Only consecutive moves coalesce; every touch, button, wheel, key and focus event
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
    mut scene_input: ResMut<pentimento_scene::FrontendScenePointerInput>,
    mut paint_history: ResMut<super::hotkeys::EguiPaintHistory>,
    mut state: Local<NativeInputState>,
    paint_mode: Res<pentimento_scene::PaintMode>,
    owner: Option<Res<pentimento_scene::ProjectOwner>>,
) {
    paint_history.actions.clear();
    let Ok((window_id, window)) = windows.single() else {
        events.clear();
        scene_input.clear();
        return;
    };
    let generation = owner.as_ref().map_or(0, |o| o.generation());
    if state.generation.is_some_and(|old| old != generation) {
        let buttons = state.buttons.clone();
        let touch = state
            .touch
            .as_ref()
            .map(|t| t.last.id)
            .or(state.suppressed_touch);
        let suspended = state.focus_suspended;
        state.pending_move = None;
        release_on_focus_loss(&mut state, &mut mouse, &mut backend);
        *state = NativeInputState {
            generation: Some(generation),
            suppressed_buttons: buttons.clone(),
            buttons,
            suppressed_touch: touch,
            focus_suspended: suspended,
            ..default()
        };
        scene_input.clear();
        layout.pointer_captured = false;
    }
    state.generation = Some(generation);
    let batch: Vec<_> = events.read().collect();
    let svelte_browser = matches!(
        config.composite_mode,
        crate::config::CompositeMode::Capture
            | crate::config::CompositeMode::Overlay
            | crate::config::CompositeMode::Cef
    );
    let region_frontend =
        svelte_browser || config.composite_mode == crate::config::CompositeMode::Egui;
    let arbitrates = region_frontend
        || (config.composite_mode == crate::config::CompositeMode::Dioxus && layout.received);
    let scene_ready = !region_frontend || layout.received;
    let mut scene_events = Vec::new();
    let direct = paint_mode.active && paint_mode.target == pentimento_ipc::PaintTarget::DirectUv;
    if !direct {
        close_scene_touch(&mut state, true, &mut scene_events);
    }
    // A first/stationary button can arrive without CursorMoved. Only use the
    // window cursor when this batch has no later move that could change origin.
    if state.touch.is_none()
        && !batch
            .iter()
            .any(|e| matches!(e, WindowEvent::CursorMoved(_)))
    {
        if let Some(position) = window.cursor_position().filter(|p| p.is_finite()) {
            let (x, y) =
                backend.scale_coordinates(position.x, position.y, window.resolution.scale_factor());
            mouse.window_x = position.x;
            mouse.window_y = position.y;
            mouse.webview_x = x;
            mouse.webview_y = y;
        }
    }
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
    // egui focus is normally acknowledged in PostUpdate. A fresh UI press owns
    // subsequent keys in this native batch, while valid earlier keys keep order.
    let egui = config.composite_mode == crate::config::CompositeMode::Egui;
    let mut egui_keyboard_owned = input_blocks.blocks_keyboard()
        || ui_owned_in_frame
        || state.touch.as_ref().is_some_and(|touch| touch.ui);
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
                if state.touch.is_some() {
                    continue;
                }
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
                if scene_ready
                    && !state.focus_suspended
                    && !over_ui(&layout, Vec2::new(x, y))
                    && state.ui_buttons.get_pressed().next().is_none()
                {
                    state.scene_cursor = Some(event.position);
                    scene_events.push(WindowEvent::CursorMoved(event.clone()));
                } else {
                    close_scene_buttons(&mut state, window_id, &mut scene_events);
                }
            }
            WindowEvent::MouseButtonInput(event) if event.window == window_id => {
                if state.suppressed_buttons.pressed(event.button) {
                    if !event.state.is_pressed() {
                        state.suppressed_buttons.release(event.button);
                        state.buttons.release(event.button);
                    }
                    continue;
                }
                if state.touch.is_some() || state.suppressed_touch.is_some() {
                    continue;
                }
                flush_move(&mut state, &mut mouse, &mut backend);
                let position = Vec2::new(mouse.webview_x, mouse.webview_y);
                if event.state.is_pressed() {
                    if scene_ready
                        && !state.focus_suspended
                        && !state.buttons.pressed(event.button)
                        && !over_ui(&layout, position)
                        && state.ui_buttons.get_pressed().next().is_none()
                    {
                        let origin = Vec2::new(mouse.window_x, mouse.window_y);
                        if state.scene_cursor != Some(origin) {
                            scene_events.push(WindowEvent::CursorMoved(
                                bevy::window::CursorMoved {
                                    window: window_id,
                                    position: origin,
                                    delta: None,
                                },
                            ));
                            state.scene_cursor = Some(origin);
                        }
                        state.scene_buttons.press(event.button);
                        scene_events.push(WindowEvent::MouseButtonInput(*event));
                    }
                    if !state.buttons.pressed(event.button) && over_ui(&layout, position) {
                        state.ui_buttons.press(event.button);
                        egui_keyboard_owned |= egui;
                    }
                    state.buttons.press(event.button);
                    ui_owned_in_frame |= state.ui_buttons.get_pressed().next().is_some();
                } else {
                    if state.scene_buttons.pressed(event.button) {
                        scene_events.push(WindowEvent::MouseButtonInput(*event));
                        state.scene_buttons.release(event.button);
                    }
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
            WindowEvent::TouchInput(event) if event.window == window_id => {
                if let Some(id) = state.suppressed_touch {
                    if event.id == id
                        && matches!(
                            event.phase,
                            bevy::input::touch::TouchPhase::Ended
                                | bevy::input::touch::TouchPhase::Canceled
                        )
                    {
                        state.suppressed_touch = None;
                    }
                    continue;
                }
                // Preserve chronology before a touch takes exclusive ownership.
                flush_move(&mut state, &mut mouse, &mut backend);
                forward_touch(
                    *event,
                    direct && scene_ready,
                    &mut state,
                    &mut mouse,
                    &mut backend,
                    &layout,
                    window.resolution.scale_factor(),
                    &mut scene_events,
                );
                egui_keyboard_owned |= egui && state.touch.as_ref().is_some_and(|touch| touch.ui);
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
                let admitted = !input_blocks.blocks_keyboard()
                    && !(egui && egui_keyboard_owned)
                    && (!egui || scene_ready)
                    && !state.focus_suspended;
                if admitted {
                    scene_events.push(WindowEvent::KeyboardInput(event.clone()));
                }
                flush_move(&mut state, &mut mouse, &mut backend);
                if let Some(translated) = translated.next() {
                    if egui
                        && admitted
                        && event.key_code == KeyCode::KeyZ
                        && translated.pressed
                        && !event.repeat
                        && translated.modifiers.ctrl
                    {
                        paint_history.actions.push(if translated.modifiers.shift {
                            super::hotkeys::PaintHistoryAction::Redo
                        } else {
                            super::hotkeys::PaintHistoryAction::Undo
                        });
                    }
                    backend.send_keyboard_event(translated);
                }
            }
            WindowEvent::KeyboardFocusLost(_) => {
                close_scene_buttons(&mut state, window_id, &mut scene_events);
                close_scene_touch(&mut state, true, &mut scene_events);
                scene_events.push(WindowEvent::WindowFocused(bevy::window::WindowFocused {
                    window: window_id,
                    focused: false,
                }));
                release_on_focus_loss(&mut state, &mut mouse, &mut backend);
            }
            WindowEvent::WindowFocused(event) if event.window == window_id => {
                scene_events.push(WindowEvent::WindowFocused(event.clone()));
                if event.focused {
                    flush_move(&mut state, &mut mouse, &mut backend);
                    state.focus_suspended = false;
                } else {
                    close_scene_buttons(&mut state, window_id, &mut scene_events);
                    close_scene_touch(&mut state, true, &mut scene_events);
                    release_on_focus_loss(&mut state, &mut mouse, &mut backend);
                }
            }
            _ => {}
        }
    }
    layout.pointer_captured = state.ui_buttons.get_pressed().next().is_some()
        || state.touch.as_ref().is_some_and(|t| t.ui);
    // Region frontends block scene input until hit-testing has rectangles.
    // egui also publishes real panel/modal geometry, so the first press and a
    // stationary release cannot leak through a widget into a sculpt/paint stroke.
    if arbitrates {
        if !scene_ready
            || state.focus_suspended
            || layout.pointer_captured
            || over_ui(&layout, Vec2::new(mouse.webview_x, mouse.webview_y))
        {
            close_scene_buttons(&mut state, window_id, &mut scene_events);
            let suspended = state.focus_suspended;
            close_scene_touch(&mut state, suspended, &mut scene_events);
        }
        scene_input.publish(window_id, scene_events);
        input_blocks.block_pointer = (region_frontend && !layout.received)
            || focus_lost
            || state.focus_suspended
            || ui_owned_in_frame
            || state.touch.is_some()
            || state.suppressed_touch.is_some()
            || state.suppressed_buttons.get_pressed().next().is_some()
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
    } else {
        scene_input.clear();
    }
    // Idle hover is throttled; held drags must reach their latest position.
    if state.buttons.get_pressed().next().is_some()
        || mouse.last_move_sent.elapsed() >= MOUSE_MOVE_THROTTLE
    {
        flush_move(&mut state, &mut mouse, &mut backend);
    }
}

/// One native contact owns either the scene or UI until its End/Cancel. A second
/// contact or companion mouse event cannot promote, steal, or close that owner.
fn forward_touch(
    event: bevy::input::touch::TouchInput,
    direct_ready: bool,
    state: &mut NativeInputState,
    mouse: &mut MouseState,
    backend: &mut FrontendBackend,
    layout: &pentimento_scene::FrontendUiLayout,
    scale: f32,
    scene: &mut Vec<WindowEvent>,
) {
    use bevy::input::touch::TouchPhase;
    if event.phase == TouchPhase::Started {
        if state.touch.is_some() || state.buttons.get_pressed().next().is_some() {
            return;
        }
        let position = Vec2::new(event.position.x, event.position.y);
        let (x, y) = backend.scale_coordinates(position.x, position.y, scale);
        let valid = position.is_finite() && x.is_finite() && y.is_finite();
        let ui = valid && !state.focus_suspended && over_ui(layout, Vec2::new(x, y));
        let owns_scene = valid
            && direct_ready
            && !ui
            && !state.focus_suspended
            && state.ui_buttons.get_pressed().next().is_none()
            && pentimento_scene::touch_pressure(event.force).is_some();
        state.touch = Some(NativeTouch {
            last: event,
            scene: owns_scene,
            ui,
        });
        if valid {
            mouse.window_x = position.x;
            mouse.window_y = position.y;
            mouse.webview_x = x;
            mouse.webview_y = y;
            if ui {
                backend.send_mouse_event(MouseEvent::Move { x, y });
                backend.send_mouse_event(MouseEvent::ButtonDown {
                    button: IpcMouseButton::Left,
                    x,
                    y,
                });
            }
        }
        if owns_scene {
            scene.push(WindowEvent::TouchInput(event));
        }
        return;
    }
    if !state.touch.as_ref().is_some_and(|t| t.last.id == event.id) {
        return;
    }
    let (x, y) = backend.scale_coordinates(event.position.x, event.position.y, scale);
    let valid = event.position.is_finite() && x.is_finite() && y.is_finite();
    if !valid || event.phase == TouchPhase::Canceled {
        close_scene_touch(state, true, scene);
    } else if state.touch.as_ref().unwrap().scene {
        if pentimento_scene::touch_pressure(event.force).is_none() {
            close_scene_touch(state, true, scene);
        } else if !direct_ready || over_ui(layout, Vec2::new(x, y)) {
            close_scene_touch(state, false, scene);
        } else {
            scene.push(WindowEvent::TouchInput(event));
            state.touch.as_mut().unwrap().last = event;
        }
    }
    if valid {
        mouse.window_x = event.position.x;
        mouse.window_y = event.position.y;
        mouse.webview_x = x;
        mouse.webview_y = y;
        if state.touch.as_ref().unwrap().ui {
            backend.send_mouse_event(MouseEvent::Move { x, y });
        }
    }
    if matches!(event.phase, TouchPhase::Ended | TouchPhase::Canceled) {
        if state.touch.as_ref().unwrap().ui {
            backend.send_mouse_event(MouseEvent::ButtonUp {
                button: IpcMouseButton::Left,
                x: mouse.webview_x,
                y: mouse.webview_y,
            });
        }
        state.touch = None;
    }
}
fn close_scene_touch(state: &mut NativeInputState, cancel: bool, events: &mut Vec<WindowEvent>) {
    if let Some(t) = state.touch.as_mut() {
        if t.scene {
            let mut end = t.last;
            end.phase = if cancel {
                bevy::input::touch::TouchPhase::Canceled
            } else {
                bevy::input::touch::TouchPhase::Ended
            };
            events.push(WindowEvent::TouchInput(end));
            t.scene = false;
        }
    }
}

fn close_scene_buttons(
    state: &mut NativeInputState,
    window: Entity,
    events: &mut Vec<WindowEvent>,
) {
    for button in state.scene_buttons.get_pressed().copied() {
        events.push(WindowEvent::MouseButtonInput(
            bevy::input::mouse::MouseButtonInput {
                window,
                button,
                state: bevy::input::ButtonState::Released,
            },
        ));
    }
    state.scene_buttons.reset_all();
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
    if state.touch.as_ref().is_some_and(|t| t.ui) {
        backend.send_mouse_event(MouseEvent::ButtonUp {
            button: IpcMouseButton::Left,
            x: mouse.webview_x,
            y: mouse.webview_y,
        });
    }
    state.touch = None;
    state.scene_cursor = None;
    state.focus_suspended = true;
    state.buttons.reset_all();
    state.ui_buttons.reset_all();
    state.alt_graph.reset_all();
    state.suppressed_buttons.reset_all();
    state.suppressed_touch = None;
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
