//! Input handling - forwards Bevy input events to the webview
//!
//! Supports capture, overlay, and CEF compositing modes.

use bevy::input::keyboard::KeyboardInput;
use bevy::input::mouse::{MouseButtonInput, MouseMotion, MouseWheel};
use bevy::input::InputSystems;
use bevy::prelude::*;
use bevy::window::CursorMoved;
use pentimento_config::DisplayConfig;
use pentimento_ipc::{KeyboardEvent, Modifiers, MouseButton as IpcMouseButton, MouseEvent};
use std::time::{Duration, Instant};

use crate::config::{CompositeMode, PentimentoConfig};
use crate::render::{OverlayWebviewResource, WebviewResource};
#[cfg(feature = "cef")]
use crate::render::CefWebviewResource;

/// Minimum interval between mouse move events sent to webview (throttling)
const MOUSE_MOVE_THROTTLE: Duration = Duration::from_millis(16); // ~60fps max

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MouseState>()
            // Run in PreUpdate to get the freshest input state before other systems
            .add_systems(PreUpdate, track_mouse_position.after(InputSystems))
            .add_systems(
                PreUpdate,
                (forward_mouse_buttons, forward_mouse_scroll, forward_keyboard)
                    .after(track_mouse_position),
            );

        // CEF DevTools hotkey (Ctrl+Shift+I)
        #[cfg(feature = "cef")]
        app.add_systems(PreUpdate, handle_devtools_hotkey.after(InputSystems));

        info!("Input plugin initialized");
    }
}

/// Track mouse position in both window and scaled webview coordinates
#[derive(Resource)]
pub struct MouseState {
    /// Position in window coordinates
    pub window_x: f32,
    pub window_y: f32,
    /// Position scaled to webview coordinates (1920x1080)
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

/// Scale window coordinates to webview coordinates
fn scale_to_webview(
    window_x: f32,
    window_y: f32,
    window_width: f32,
    window_height: f32,
    display_config: &DisplayConfig,
) -> (f32, f32) {
    let scale_x = display_config.width_f32() / window_width;
    let scale_y = display_config.height_f32() / window_height;
    (window_x * scale_x, window_y * scale_y)
}

/// Track mouse cursor position and forward mouse move events (throttled)
/// This system MUST run before forward_mouse_buttons and forward_mouse_scroll
fn track_mouse_position(
    mut mouse_state: ResMut<MouseState>,
    mut motion_events: MessageReader<MouseMotion>,
    mut cursor_events: MessageReader<CursorMoved>,
    config: Res<PentimentoConfig>,
    display_config: Res<DisplayConfig>,
    capture_webview: Option<NonSendMut<WebviewResource>>,
    overlay_webview: Option<NonSendMut<OverlayWebviewResource>>,
    #[cfg(feature = "cef")] cef_webview: Option<NonSendMut<CefWebviewResource>>,
    windows: Query<&Window>,
) {
    // Always clear motion events - we use cursor position instead
    motion_events.clear();

    let Ok(window) = windows.single() else {
        cursor_events.clear();
        return;
    };

    // Process CursorMoved events - these contain the actual cursor position
    // Use the LAST event position as that's the most recent
    let mut had_cursor_event = false;
    for event in cursor_events.read() {
        // Bevy's CursorMoved events are in LOGICAL coordinates.
        // CEF, Overlay, and Capture render at PHYSICAL resolution, so scale by DPI factor.
        let (webview_x, webview_y) = match config.composite_mode {
            #[cfg(feature = "cef")]
            CompositeMode::Cef => {
                // CEF renders at physical resolution
                let scale_factor = window.resolution.scale_factor();
                (event.position.x * scale_factor, event.position.y * scale_factor)
            }
            CompositeMode::Overlay => {
                // Overlay uses physical resolution like CEF
                let scale_factor = window.resolution.scale_factor();
                (event.position.x * scale_factor, event.position.y * scale_factor)
            }
            CompositeMode::Capture => {
                let scale_factor = window.resolution.scale_factor();
                (event.position.x * scale_factor, event.position.y * scale_factor)
            }
            _ => {
                // Fallback for other modes (Tauri handles its own input)
                (event.position.x, event.position.y)
            }
        };

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

    // Forward to appropriate webview based on mode
    match config.composite_mode {
        CompositeMode::Capture => {
            if let Some(mut webview) = capture_webview {
                webview.webview.send_mouse_event(mouse_event);
                mouse_state.last_move_sent = now;
            }
        }
        CompositeMode::Overlay => {
            if let Some(mut webview) = overlay_webview {
                webview.webview.send_mouse_event(mouse_event);
                mouse_state.last_move_sent = now;
            }
        }
        #[cfg(feature = "cef")]
        CompositeMode::Cef => {
            if let Some(mut webview) = cef_webview {
                webview.webview.send_mouse_event(mouse_event);
                mouse_state.last_move_sent = now;
            }
        }
        #[cfg(not(feature = "cef"))]
        CompositeMode::Cef => {}
        CompositeMode::Tauri => {
            // Tauri mode handles input in the browser
        }
    }
}

/// Forward mouse button events to the webview
/// Runs after track_mouse_position so MouseState is up-to-date
fn forward_mouse_buttons(
    mut button_events: MessageReader<MouseButtonInput>,
    mouse_state: Res<MouseState>,
    config: Res<PentimentoConfig>,
    capture_webview: Option<NonSendMut<WebviewResource>>,
    overlay_webview: Option<NonSendMut<OverlayWebviewResource>>,
    #[cfg(feature = "cef")] cef_webview: Option<NonSendMut<CefWebviewResource>>,
) {
    // Use the tracked position (updated by track_mouse_position which runs first)
    let click_x = mouse_state.webview_x;
    let click_y = mouse_state.webview_y;

    // Collect events first to avoid borrow issues
    let events: Vec<_> = button_events.read().cloned().collect();
    if events.is_empty() {
        return;
    }

    // Helper to convert button
    let convert_button = |button: bevy::input::mouse::MouseButton| -> Option<IpcMouseButton> {
        match button {
            bevy::input::mouse::MouseButton::Left => Some(IpcMouseButton::Left),
            bevy::input::mouse::MouseButton::Right => Some(IpcMouseButton::Right),
            bevy::input::mouse::MouseButton::Middle => Some(IpcMouseButton::Middle),
            _ => None,
        }
    };

    // Process events based on mode
    match config.composite_mode {
        CompositeMode::Capture => {
            let Some(mut webview) = capture_webview else { return };
            for event in &events {
                let Some(button) = convert_button(event.button) else { continue };
                if event.state.is_pressed() {
                    info!("Click at webview ({:.1}, {:.1})", click_x, click_y);
                }
                let mouse_event = if event.state.is_pressed() {
                    MouseEvent::ButtonDown { button, x: click_x, y: click_y }
                } else {
                    MouseEvent::ButtonUp { button, x: click_x, y: click_y }
                };
                webview.webview.send_mouse_event(mouse_event);
            }
        }
        CompositeMode::Overlay => {
            let Some(mut webview) = overlay_webview else { return };
            for event in &events {
                let Some(button) = convert_button(event.button) else { continue };
                if event.state.is_pressed() {
                    info!("Click at webview ({:.1}, {:.1})", click_x, click_y);
                }
                let mouse_event = if event.state.is_pressed() {
                    MouseEvent::ButtonDown { button, x: click_x, y: click_y }
                } else {
                    MouseEvent::ButtonUp { button, x: click_x, y: click_y }
                };
                webview.webview.send_mouse_event(mouse_event);
            }
        }
        #[cfg(feature = "cef")]
        CompositeMode::Cef => {
            let Some(mut webview) = cef_webview else { return };
            for event in &events {
                let Some(button) = convert_button(event.button) else { continue };
                if event.state.is_pressed() {
                    info!("CEF Click at webview ({:.1}, {:.1})", click_x, click_y);
                }
                let mouse_event = if event.state.is_pressed() {
                    MouseEvent::ButtonDown { button, x: click_x, y: click_y }
                } else {
                    MouseEvent::ButtonUp { button, x: click_x, y: click_y }
                };
                webview.webview.send_mouse_event(mouse_event);
            }
        }
        #[cfg(not(feature = "cef"))]
        CompositeMode::Cef => {}
        CompositeMode::Tauri => {}
    }
}

/// Forward mouse scroll events to the webview
/// Runs after track_mouse_position so MouseState is up-to-date
fn forward_mouse_scroll(
    mut scroll_events: MessageReader<MouseWheel>,
    mouse_state: Res<MouseState>,
    config: Res<PentimentoConfig>,
    capture_webview: Option<NonSendMut<WebviewResource>>,
    overlay_webview: Option<NonSendMut<OverlayWebviewResource>>,
    #[cfg(feature = "cef")] cef_webview: Option<NonSendMut<CefWebviewResource>>,
) {
    let scroll_x = mouse_state.webview_x;
    let scroll_y = mouse_state.webview_y;

    let events: Vec<_> = scroll_events.read().cloned().collect();
    if events.is_empty() {
        return;
    }

    // Helper to convert scroll deltas
    let convert_delta = |event: &bevy::input::mouse::MouseWheel| -> (f32, f32) {
        match event.unit {
            bevy::input::mouse::MouseScrollUnit::Line => (event.x * 40.0, event.y * 40.0),
            bevy::input::mouse::MouseScrollUnit::Pixel => (event.x, event.y),
        }
    };

    match config.composite_mode {
        CompositeMode::Capture => {
            let Some(mut webview) = capture_webview else { return };
            for event in &events {
                let (delta_x, delta_y) = convert_delta(event);
                webview.webview.send_mouse_event(MouseEvent::Scroll {
                    delta_x,
                    delta_y: -delta_y,
                    x: scroll_x,
                    y: scroll_y,
                });
            }
        }
        CompositeMode::Overlay => {
            let Some(mut webview) = overlay_webview else { return };
            for event in &events {
                let (delta_x, delta_y) = convert_delta(event);
                webview.webview.send_mouse_event(MouseEvent::Scroll {
                    delta_x,
                    delta_y: -delta_y,
                    x: scroll_x,
                    y: scroll_y,
                });
            }
        }
        #[cfg(feature = "cef")]
        CompositeMode::Cef => {
            let Some(mut webview) = cef_webview else { return };
            for event in &events {
                let (delta_x, delta_y) = convert_delta(event);
                webview.webview.send_mouse_event(MouseEvent::Scroll {
                    delta_x,
                    delta_y: -delta_y,
                    x: scroll_x,
                    y: scroll_y,
                });
            }
        }
        #[cfg(not(feature = "cef"))]
        CompositeMode::Cef => {}
        CompositeMode::Tauri => {}
    }
}

/// Forward keyboard events to the webview
fn forward_keyboard(
    mut key_events: MessageReader<KeyboardInput>,
    key_input: Res<ButtonInput<KeyCode>>,
    config: Res<PentimentoConfig>,
    capture_webview: Option<NonSendMut<WebviewResource>>,
    overlay_webview: Option<NonSendMut<OverlayWebviewResource>>,
    #[cfg(feature = "cef")] cef_webview: Option<NonSendMut<CefWebviewResource>>,
) {
    // Build current modifier state
    let modifiers = Modifiers {
        shift: key_input.pressed(KeyCode::ShiftLeft) || key_input.pressed(KeyCode::ShiftRight),
        ctrl: key_input.pressed(KeyCode::ControlLeft) || key_input.pressed(KeyCode::ControlRight),
        alt: key_input.pressed(KeyCode::AltLeft) || key_input.pressed(KeyCode::AltRight),
        meta: key_input.pressed(KeyCode::SuperLeft) || key_input.pressed(KeyCode::SuperRight),
    };

    let events: Vec<_> = key_events.read().cloned().collect();
    if events.is_empty() {
        return;
    }

    match config.composite_mode {
        CompositeMode::Capture => {
            let Some(mut webview) = capture_webview else { return };
            for event in &events {
                let key = bevy_keycode_to_web_key(event.key_code);
                webview.webview.send_keyboard_event(KeyboardEvent {
                    key,
                    pressed: event.state.is_pressed(),
                    modifiers: modifiers.clone(),
                });
            }
        }
        CompositeMode::Overlay => {
            let Some(mut webview) = overlay_webview else { return };
            for event in &events {
                let key = bevy_keycode_to_web_key(event.key_code);
                webview.webview.send_keyboard_event(KeyboardEvent {
                    key,
                    pressed: event.state.is_pressed(),
                    modifiers: modifiers.clone(),
                });
            }
        }
        #[cfg(feature = "cef")]
        CompositeMode::Cef => {
            let Some(mut webview) = cef_webview else { return };
            for event in &events {
                let key = bevy_keycode_to_web_key(event.key_code);
                webview.webview.send_keyboard_event(KeyboardEvent {
                    key,
                    pressed: event.state.is_pressed(),
                    modifiers: modifiers.clone(),
                });
            }
        }
        #[cfg(not(feature = "cef"))]
        CompositeMode::Cef => {}
        CompositeMode::Tauri => {}
    }
}

/// Convert Bevy KeyCode to web key string
/// See: https://developer.mozilla.org/en-US/docs/Web/API/KeyboardEvent/key/Key_Values
fn bevy_keycode_to_web_key(key_code: KeyCode) -> String {
    match key_code {
        // Alphabet
        KeyCode::KeyA => "a".to_string(),
        KeyCode::KeyB => "b".to_string(),
        KeyCode::KeyC => "c".to_string(),
        KeyCode::KeyD => "d".to_string(),
        KeyCode::KeyE => "e".to_string(),
        KeyCode::KeyF => "f".to_string(),
        KeyCode::KeyG => "g".to_string(),
        KeyCode::KeyH => "h".to_string(),
        KeyCode::KeyI => "i".to_string(),
        KeyCode::KeyJ => "j".to_string(),
        KeyCode::KeyK => "k".to_string(),
        KeyCode::KeyL => "l".to_string(),
        KeyCode::KeyM => "m".to_string(),
        KeyCode::KeyN => "n".to_string(),
        KeyCode::KeyO => "o".to_string(),
        KeyCode::KeyP => "p".to_string(),
        KeyCode::KeyQ => "q".to_string(),
        KeyCode::KeyR => "r".to_string(),
        KeyCode::KeyS => "s".to_string(),
        KeyCode::KeyT => "t".to_string(),
        KeyCode::KeyU => "u".to_string(),
        KeyCode::KeyV => "v".to_string(),
        KeyCode::KeyW => "w".to_string(),
        KeyCode::KeyX => "x".to_string(),
        KeyCode::KeyY => "y".to_string(),
        KeyCode::KeyZ => "z".to_string(),

        // Numbers
        KeyCode::Digit0 => "0".to_string(),
        KeyCode::Digit1 => "1".to_string(),
        KeyCode::Digit2 => "2".to_string(),
        KeyCode::Digit3 => "3".to_string(),
        KeyCode::Digit4 => "4".to_string(),
        KeyCode::Digit5 => "5".to_string(),
        KeyCode::Digit6 => "6".to_string(),
        KeyCode::Digit7 => "7".to_string(),
        KeyCode::Digit8 => "8".to_string(),
        KeyCode::Digit9 => "9".to_string(),

        // Function keys
        KeyCode::F1 => "F1".to_string(),
        KeyCode::F2 => "F2".to_string(),
        KeyCode::F3 => "F3".to_string(),
        KeyCode::F4 => "F4".to_string(),
        KeyCode::F5 => "F5".to_string(),
        KeyCode::F6 => "F6".to_string(),
        KeyCode::F7 => "F7".to_string(),
        KeyCode::F8 => "F8".to_string(),
        KeyCode::F9 => "F9".to_string(),
        KeyCode::F10 => "F10".to_string(),
        KeyCode::F11 => "F11".to_string(),
        KeyCode::F12 => "F12".to_string(),

        // Special keys
        KeyCode::Space => " ".to_string(),
        KeyCode::Enter => "Enter".to_string(),
        KeyCode::Escape => "Escape".to_string(),
        KeyCode::Backspace => "Backspace".to_string(),
        KeyCode::Tab => "Tab".to_string(),
        KeyCode::Delete => "Delete".to_string(),
        KeyCode::Insert => "Insert".to_string(),
        KeyCode::Home => "Home".to_string(),
        KeyCode::End => "End".to_string(),
        KeyCode::PageUp => "PageUp".to_string(),
        KeyCode::PageDown => "PageDown".to_string(),

        // Arrow keys
        KeyCode::ArrowUp => "ArrowUp".to_string(),
        KeyCode::ArrowDown => "ArrowDown".to_string(),
        KeyCode::ArrowLeft => "ArrowLeft".to_string(),
        KeyCode::ArrowRight => "ArrowRight".to_string(),

        // Modifier keys
        KeyCode::ShiftLeft | KeyCode::ShiftRight => "Shift".to_string(),
        KeyCode::ControlLeft | KeyCode::ControlRight => "Control".to_string(),
        KeyCode::AltLeft | KeyCode::AltRight => "Alt".to_string(),
        KeyCode::SuperLeft | KeyCode::SuperRight => "Meta".to_string(),

        // Punctuation and symbols
        KeyCode::Comma => ",".to_string(),
        KeyCode::Period => ".".to_string(),
        KeyCode::Slash => "/".to_string(),
        KeyCode::Backslash => "\\".to_string(),
        KeyCode::Semicolon => ";".to_string(),
        KeyCode::Quote => "'".to_string(),
        KeyCode::BracketLeft => "[".to_string(),
        KeyCode::BracketRight => "]".to_string(),
        KeyCode::Minus => "-".to_string(),
        KeyCode::Equal => "=".to_string(),
        KeyCode::Backquote => "`".to_string(),

        // Numpad
        KeyCode::Numpad0 => "0".to_string(),
        KeyCode::Numpad1 => "1".to_string(),
        KeyCode::Numpad2 => "2".to_string(),
        KeyCode::Numpad3 => "3".to_string(),
        KeyCode::Numpad4 => "4".to_string(),
        KeyCode::Numpad5 => "5".to_string(),
        KeyCode::Numpad6 => "6".to_string(),
        KeyCode::Numpad7 => "7".to_string(),
        KeyCode::Numpad8 => "8".to_string(),
        KeyCode::Numpad9 => "9".to_string(),
        KeyCode::NumpadAdd => "+".to_string(),
        KeyCode::NumpadSubtract => "-".to_string(),
        KeyCode::NumpadMultiply => "*".to_string(),
        KeyCode::NumpadDivide => "/".to_string(),
        KeyCode::NumpadDecimal => ".".to_string(),
        KeyCode::NumpadEnter => "Enter".to_string(),

        // Default for unmapped keys
        _ => format!("{:?}", key_code),
    }
}

/// Handle Ctrl+Shift+I to open DevTools (CEF mode only)
#[cfg(feature = "cef")]
fn handle_devtools_hotkey(
    key_input: Res<ButtonInput<KeyCode>>,
    config: Res<PentimentoConfig>,
    cef_webview: Option<NonSend<CefWebviewResource>>,
) {
    // Only handle in CEF mode
    if config.composite_mode != CompositeMode::Cef {
        return;
    }

    // Check for Ctrl+Shift+I
    let ctrl = key_input.pressed(KeyCode::ControlLeft) || key_input.pressed(KeyCode::ControlRight);
    let shift = key_input.pressed(KeyCode::ShiftLeft) || key_input.pressed(KeyCode::ShiftRight);
    let i_pressed = key_input.just_pressed(KeyCode::KeyI);

    if ctrl && shift && i_pressed {
        if let Some(webview) = cef_webview {
            info!("Opening CEF DevTools (Ctrl+Shift+I)");
            webview.webview.show_dev_tools();
        }
    }
}
