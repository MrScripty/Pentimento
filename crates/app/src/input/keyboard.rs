//! Keyboard input handling - forwards Bevy keyboard events to the frontend backend
//!
//! This module handles:
//! - Keyboard event forwarding
//! - Modifier key tracking (shift, ctrl, alt, meta)
//! - Bevy KeyCode to web key string conversion

use bevy::input::keyboard::{Key, KeyboardFocusLost, KeyboardInput};
use bevy::prelude::*;
use pentimento_ipc::{KeyboardEvent, Modifiers};

use super::backend::FrontendBackend;

/// Forward keyboard events to the webview
pub fn forward_keyboard(
    mut key_events: MessageReader<KeyboardInput>,
    mut focus_lost: MessageReader<KeyboardFocusLost>,
    key_input: Res<ButtonInput<KeyCode>>,
    mut alt_graph_keys: Local<ButtonInput<KeyCode>>,
    mut backend: FrontendBackend,
) {
    let events: Vec<_> = key_events.read().cloned().collect();
    for event in translate_keyboard_events(
        &events,
        &key_input,
        &mut alt_graph_keys,
        focus_lost.read().count() != 0,
    ) {
        backend.send_keyboard_event(event);
    }
}

/// Reconstruct the state before this batch from Bevy's final pressed state,
/// then advance it for each event. A complete Ctrl+A chord can fit in one frame.
fn translate_keyboard_events(
    events: &[KeyboardInput],
    final_pressed: &ButtonInput<KeyCode>,
    alt_graph_keys: &mut ButtonInput<KeyCode>,
    focus_lost: bool,
) -> Vec<KeyboardEvent> {
    // Focus loss clears Bevy physical state without corresponding release events.
    // Its batch cannot be reconstructed safely; discard it rather than turn a
    // shortcut into text. Later batches use fresh authoritative modifier flags.
    if focus_lost {
        alt_graph_keys.reset_all();
        return Vec::new();
    }
    let mut pressed = final_pressed.clone();
    for event in events.iter().rev() {
        if event.state.is_pressed() {
            if !event.repeat {
                pressed.release(event.key_code);
            }
        } else {
            pressed.press(event.key_code);
        }
    }
    events
        .iter()
        .map(|event| {
            if event.state.is_pressed() {
                pressed.press(event.key_code);
                if event.logical_key == Key::AltGraph {
                    alt_graph_keys.press(event.key_code);
                }
            } else {
                pressed.release(event.key_code);
                alt_graph_keys.release(event.key_code);
            }
            let mut modifiers = build_modifiers(&pressed);
            modifiers.alt_graph = alt_graph_keys
                .get_pressed()
                .any(|code| pressed.pressed(*code));
            KeyboardEvent {
                key: bevy_keycode_to_web_key(event.key_code),
                code: format!("{:?}", event.key_code),
                text: event
                    .state
                    .is_pressed()
                    .then(|| event.text.as_ref().map(ToString::to_string))
                    .flatten(),
                pressed: event.state.is_pressed(),
                modifiers,
            }
        })
        .collect()
}

/// Build the current modifier state from Bevy's ButtonInput
pub fn build_modifiers(key_input: &ButtonInput<KeyCode>) -> Modifiers {
    Modifiers {
        shift: key_input.pressed(KeyCode::ShiftLeft) || key_input.pressed(KeyCode::ShiftRight),
        ctrl: key_input.pressed(KeyCode::ControlLeft) || key_input.pressed(KeyCode::ControlRight),
        alt: key_input.pressed(KeyCode::AltLeft) || key_input.pressed(KeyCode::AltRight),
        meta: key_input.pressed(KeyCode::SuperLeft) || key_input.pressed(KeyCode::SuperRight),
        alt_graph: false,
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

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::{ButtonState, InputPlugin};

    fn key(code: KeyCode, logical: Key, pressed: bool, text: Option<&str>) -> KeyboardInput {
        KeyboardInput {
            key_code: code,
            logical_key: logical,
            state: if pressed {
                ButtonState::Pressed
            } else {
                ButtonState::Released
            },
            text: text.map(Into::into),
            repeat: false,
            window: Entity::PLACEHOLDER,
        }
    }
    fn translated(events: &[KeyboardInput]) -> Vec<KeyboardEvent> {
        let mut app = App::new();
        app.add_plugins(InputPlugin);
        for event in events {
            app.world_mut().write_message(event.clone());
        }
        app.update();
        translate_keyboard_events(
            events,
            app.world().resource::<ButtonInput<KeyCode>>(),
            &mut ButtonInput::default(),
            false,
        )
    }
    #[test]
    fn alt_graph_text_and_physical_identity_survive_batched_release() {
        let out = translated(&[
            key(KeyCode::AltRight, Key::AltGraph, true, None),
            key(KeyCode::KeyQ, Key::Character("@".into()), true, Some("@")),
            key(KeyCode::KeyQ, Key::Character("@".into()), false, None),
            key(KeyCode::AltRight, Key::AltGraph, false, None),
        ]);
        assert!(out[1].modifiers.alt_graph);
        assert_eq!(out[1].text.as_deref(), Some("@"));
        assert_eq!(out[1].code, "KeyQ");
        assert!(!out[3].modifiers.alt_graph);
        let mut stale = ButtonInput::default();
        stale.press(KeyCode::AltRight);
        let later = translate_keyboard_events(
            &[key(
                KeyCode::KeyA,
                Key::Character("a".into()),
                true,
                Some("a"),
            )],
            &ButtonInput::default(),
            &mut stale,
            false,
        );
        assert!(!later[0].modifiers.alt_graph);
    }
    #[test]
    fn focus_loss_discards_ambiguous_shortcut_batch_with_real_bevy_input() {
        let mut app = App::new();
        app.add_plugins(InputPlugin);
        app.world_mut()
            .write_message(key(KeyCode::ControlLeft, Key::Control, true, None));
        app.update();
        assert!(
            app.world()
                .resource::<ButtonInput<KeyCode>>()
                .pressed(KeyCode::ControlLeft)
        );
        let events = [key(
            KeyCode::KeyA,
            Key::Character("a".into()),
            true,
            Some("a"),
        )];
        app.world_mut().write_message(events[0].clone());
        app.world_mut().write_message(KeyboardFocusLost);
        app.update();
        assert!(
            !app.world()
                .resource::<ButtonInput<KeyCode>>()
                .pressed(KeyCode::ControlLeft)
        );
        assert!(
            translate_keyboard_events(
                &events,
                app.world().resource::<ButtonInput<KeyCode>>(),
                &mut ButtonInput::default(),
                true
            )
            .is_empty()
        );
    }
    #[test]
    fn focus_loss_clears_alt_graph_before_a_later_ordinary_alt_shortcut() {
        let mut app = App::new();
        app.add_plugins(InputPlugin);
        let mut alt_graph = ButtonInput::default();
        let held = [key(KeyCode::AltRight, Key::AltGraph, true, None)];
        app.world_mut().write_message(held[0].clone());
        app.update();
        assert!(
            translate_keyboard_events(
                &held,
                app.world().resource::<ButtonInput<KeyCode>>(),
                &mut alt_graph,
                false
            )[0]
            .modifiers
            .alt_graph
        );
        app.world_mut().write_message(KeyboardFocusLost);
        app.update();
        assert!(
            translate_keyboard_events(
                &[],
                app.world().resource::<ButtonInput<KeyCode>>(),
                &mut alt_graph,
                true
            )
            .is_empty()
        );
        let later = [
            key(KeyCode::AltLeft, Key::Alt, true, None),
            key(KeyCode::KeyF, Key::Character("f".into()), true, Some("f")),
        ];
        for event in &later {
            app.world_mut().write_message(event.clone());
        }
        app.update();
        let out = translate_keyboard_events(
            &later,
            app.world().resource::<ButtonInput<KeyCode>>(),
            &mut alt_graph,
            false,
        );
        assert!(
            out.iter()
                .all(|e| e.modifiers.alt && !e.modifiers.alt_graph)
        );
    }
    #[test]
    fn batched_control_a_preserves_shortcut_modifiers_in_event_order() {
        let events = [
            key(KeyCode::ControlLeft, Key::Control, true, None),
            key(KeyCode::KeyA, Key::Character("a".into()), true, Some("a")),
            key(KeyCode::KeyA, Key::Character("a".into()), false, None),
            key(KeyCode::ControlLeft, Key::Control, false, None),
        ];
        let out = translated(&events);
        assert_eq!(
            out.iter().map(|e| e.modifiers.ctrl).collect::<Vec<_>>(),
            [true, true, true, false]
        );
        assert_eq!(out[1].code, "KeyA");
    }
    #[test]
    fn shifted_text_is_preserved_even_when_shift_is_released_in_same_frame() {
        let events = [
            key(KeyCode::ShiftLeft, Key::Shift, true, None),
            key(KeyCode::Digit3, Key::Character("#".into()), true, Some("#")),
            key(KeyCode::Digit3, Key::Character("#".into()), false, None),
            key(KeyCode::ShiftLeft, Key::Shift, false, None),
        ];
        let out = translated(&events);
        assert_eq!(out[1].text.as_deref(), Some("#"));
        assert_eq!(out[1].code, "Digit3");
        assert!(out[1].modifiers.shift);
        assert!(out[2].text.is_none());
        assert!(!out[3].modifiers.shift);
    }
    #[test]
    fn prior_held_modifiers_and_two_control_keys_survive_partial_release() {
        let mut final_state = ButtonInput::default();
        final_state.press(KeyCode::ControlRight);
        let events = [
            key(KeyCode::ControlLeft, Key::Control, false, None),
            key(KeyCode::KeyA, Key::Character("a".into()), true, Some("a")),
        ];
        let out =
            translate_keyboard_events(&events, &final_state, &mut ButtonInput::default(), false);
        assert!(out.iter().all(|e| e.modifiers.ctrl));
    }
    #[test]
    fn repeats_preserve_held_modifier_state() {
        let mut final_state = ButtonInput::default();
        final_state.press(KeyCode::ShiftLeft);
        let mut event = key(KeyCode::ShiftLeft, Key::Shift, true, None);
        event.repeat = true;
        assert!(
            translate_keyboard_events(&[event], &final_state, &mut ButtonInput::default(), false)
                [0]
            .modifiers
            .shift
        );
    }
    #[test]
    fn focus_loss_final_state_does_not_leak_a_modifier_into_later_text() {
        let out = translate_keyboard_events(
            &[key(
                KeyCode::KeyA,
                Key::Character("a".into()),
                true,
                Some("a"),
            )],
            &ButtonInput::default(),
            &mut ButtonInput::default(),
            false,
        );
        assert!(!out[0].modifiers.ctrl);
        assert!(!out[0].modifiers.shift);
    }
}
