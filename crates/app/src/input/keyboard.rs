//! Keyboard input handling - forwards Bevy keyboard events to the frontend backend
//!
//! This module handles:
//! - Keyboard event forwarding
//! - Modifier key tracking (shift, ctrl, alt, meta)
//! - Bevy KeyCode to web key string conversion

use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;
use pentimento_ipc::{KeyboardEvent, Modifiers};

use super::backend::FrontendBackend;

/// Forward keyboard events to the webview
pub fn forward_keyboard(
    mut key_events: MessageReader<KeyboardInput>,
    key_input: Res<ButtonInput<KeyCode>>,
    mut backend: FrontendBackend,
) {
    // Build current modifier state
    let modifiers = build_modifiers(&key_input);

    let events: Vec<_> = key_events.read().cloned().collect();
    if events.is_empty() {
        return;
    }

    for event in &events {
        let key = bevy_keycode_to_web_key(event.key_code);
        backend.send_keyboard_event(KeyboardEvent {
            key,
            pressed: event.state.is_pressed(),
            modifiers: modifiers.clone(),
        });
    }
}

/// Build the current modifier state from Bevy's ButtonInput
pub fn build_modifiers(key_input: &ButtonInput<KeyCode>) -> Modifiers {
    Modifiers {
        shift: key_input.pressed(KeyCode::ShiftLeft) || key_input.pressed(KeyCode::ShiftRight),
        ctrl: key_input.pressed(KeyCode::ControlLeft) || key_input.pressed(KeyCode::ControlRight),
        alt: key_input.pressed(KeyCode::AltLeft) || key_input.pressed(KeyCode::AltRight),
        meta: key_input.pressed(KeyCode::SuperLeft) || key_input.pressed(KeyCode::SuperRight),
    }
}

/// Convert Bevy KeyCode to web key string
/// See: https://developer.mozilla.org/en-US/docs/Web/API/KeyboardEvent/key/Key_Values
pub fn bevy_keycode_to_web_key(key_code: KeyCode) -> String {
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
