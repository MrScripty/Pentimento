//! Event bridging between Bevy input and Dioxus UI.
//!
//! This module handles conversion of Bevy input events to Blitz UI events
//! and manages the IPC bridge between Bevy and Dioxus.

use bevy::prelude::*;
use pentimento_dioxus_ui::{
    BlitzDocument, BlitzKey, BlitzKeyCode, BlitzKeyEvent, BlitzKeyLocation, BlitzModifiers,
    BlitzPointerId, BlitzPointerEvent, BlitzWheelDelta, BlitzWheelEvent, DioxusBridgeHandle,
    KeyState, MouseEventButton, MouseEventButtons, PointerCoords, PointerDetails, UiEvent,
};
use pentimento_ipc::MouseEvent;

// ============================================================================
// Bridge Resources (NonSend)
// ============================================================================

/// Bridge handle for IPC (main world, non-send due to mpsc::Receiver).
pub struct DioxusBridgeResource {
    pub bridge_handle: DioxusBridgeHandle,
}

/// The BlitzDocument that manages the Dioxus VirtualDom and Blitz DOM/layout.
/// This is a NonSend resource because VirtualDom is !Send.
pub struct BlitzDocumentResource {
    pub document: BlitzDocument,
}

// ============================================================================
// Event Channel
// ============================================================================

/// Event sender for channel-based Dioxus UI event handling.
#[derive(Clone)]
pub struct DioxusEventSender(pub std::sync::mpsc::Sender<UiEvent>);

/// Channel receiver for UI events, processed in build_ui_scene.
pub struct DioxusEventReceiver(pub std::sync::mpsc::Receiver<UiEvent>);

/// Create a new event channel pair for UI input events.
pub fn create_event_channel() -> (DioxusEventSender, DioxusEventReceiver) {
    let (tx, rx) = std::sync::mpsc::channel();
    (DioxusEventSender(tx), DioxusEventReceiver(rx))
}

// ============================================================================
// Dioxus Renderer Resource
// ============================================================================

/// Click tolerance in logical pixels. Movement within this distance from mousedown
/// won't trigger drag mode, making clicks more reliable on sensitive input devices.
const CLICK_TOLERANCE: f32 = 8.0;

/// Resource for sending events to the Dioxus UI thread via channel.
/// Uses click tolerance to prevent small mouse movement from triggering drag detection.
#[derive(Resource)]
pub struct DioxusRendererResource {
    sender: DioxusEventSender,
    mouse_x: f32,
    mouse_y: f32,
    buttons_pressed: MouseEventButtons,
    /// Position where the mouse button was pressed (for click vs drag detection)
    mousedown_x: f32,
    mousedown_y: f32,
}

impl DioxusRendererResource {
    pub fn new(sender: DioxusEventSender) -> Self {
        Self {
            sender,
            mouse_x: 0.0,
            mouse_y: 0.0,
            buttons_pressed: MouseEventButtons::empty(),
            mousedown_x: 0.0,
            mousedown_y: 0.0,
        }
    }

    pub fn send_mouse_event(&mut self, event: MouseEvent) {
        let ui_event = match event {
            MouseEvent::Move { x, y } => {
                self.mouse_x = x;
                self.mouse_y = y;
                // Only report buttons as pressed if we've moved beyond click tolerance.
                // This prevents small mouse jitter from triggering Blitz's drag detection
                // (which would prevent the click from being synthesized).
                let buttons = if self.buttons_pressed.is_empty() {
                    MouseEventButtons::empty()
                } else {
                    let dx = (x - self.mousedown_x).abs();
                    let dy = (y - self.mousedown_y).abs();
                    if dx > CLICK_TOLERANCE || dy > CLICK_TOLERANCE {
                        self.buttons_pressed
                    } else {
                        MouseEventButtons::empty()
                    }
                };
                UiEvent::PointerMove(self.create_pointer_event_with_buttons(
                    x,
                    y,
                    MouseEventButton::Main,
                    buttons,
                ))
            }
            MouseEvent::ButtonDown { x, y, button } => {
                self.mouse_x = x;
                self.mouse_y = y;
                self.mousedown_x = x;
                self.mousedown_y = y;
                let btn = self.convert_button(button);
                self.buttons_pressed.insert(MouseEventButtons::from(btn));
                UiEvent::PointerDown(self.create_pointer_event(x, y, btn))
            }
            MouseEvent::ButtonUp { x, y, button } => {
                self.mouse_x = x;
                self.mouse_y = y;
                let btn = self.convert_button(button);
                self.buttons_pressed.remove(MouseEventButtons::from(btn));
                UiEvent::PointerUp(self.create_pointer_event(x, y, btn))
            }
            MouseEvent::Scroll { delta_x, delta_y, x, y } => {
                self.mouse_x = x;
                self.mouse_y = y;
                UiEvent::Wheel(BlitzWheelEvent {
                    delta: BlitzWheelDelta::Pixels(delta_x as f64, delta_y as f64),
                    coords: PointerCoords {
                        page_x: x,
                        page_y: y,
                        screen_x: x,
                        screen_y: y,
                        client_x: x,
                        client_y: y,
                    },
                    buttons: self.buttons_pressed,
                    mods: BlitzModifiers::empty(),
                })
            }
        };

        // Send through channel (ignore errors if receiver is dropped)
        if let Err(e) = self.sender.0.send(ui_event) {
            error!("Failed to send UI event through channel: {}", e);
        }
    }

    pub fn send_keyboard_event(&mut self, event: pentimento_ipc::KeyboardEvent) {
        // Convert IPC modifiers to Blitz modifiers
        let mut mods = BlitzModifiers::empty();
        if event.modifiers.shift {
            mods.insert(BlitzModifiers::SHIFT);
        }
        if event.modifiers.ctrl {
            mods.insert(BlitzModifiers::CONTROL);
        }
        if event.modifiers.alt {
            mods.insert(BlitzModifiers::ALT);
        }
        if event.modifiers.meta {
            mods.insert(BlitzModifiers::META);
        }

        // Convert key string to Blitz Key type
        let key = self.convert_key(&event.key);
        let code = self.convert_code(&event.key);

        let key_event = BlitzKeyEvent {
            key,
            code,
            modifiers: mods,
            location: BlitzKeyLocation::Standard,
            is_auto_repeating: false,
            is_composing: false,
            state: if event.pressed { KeyState::Pressed } else { KeyState::Released },
            text: if event.pressed && event.key.len() == 1 {
                Some(event.key.clone().into())
            } else {
                None
            },
        };

        let ui_event = if event.pressed {
            UiEvent::KeyDown(key_event)
        } else {
            UiEvent::KeyUp(key_event)
        };

        // Send through channel (ignore errors if receiver is dropped)
        if let Err(e) = self.sender.0.send(ui_event) {
            error!("Failed to send keyboard event through channel: {}", e);
        }
    }

    fn convert_key(&self, key_str: &str) -> BlitzKey {
        match key_str {
            "Enter" => BlitzKey::Enter,
            "Escape" => BlitzKey::Escape,
            "Backspace" => BlitzKey::Backspace,
            "Tab" => BlitzKey::Tab,
            "Delete" => BlitzKey::Delete,
            "ArrowUp" => BlitzKey::ArrowUp,
            "ArrowDown" => BlitzKey::ArrowDown,
            "ArrowLeft" => BlitzKey::ArrowLeft,
            "ArrowRight" => BlitzKey::ArrowRight,
            "Home" => BlitzKey::Home,
            "End" => BlitzKey::End,
            "PageUp" => BlitzKey::PageUp,
            "PageDown" => BlitzKey::PageDown,
            "Shift" | "Control" | "Alt" | "Meta" => BlitzKey::Unidentified,
            k => BlitzKey::Character(k.into()),
        }
    }

    fn convert_code(&self, key_str: &str) -> BlitzKeyCode {
        match key_str.to_lowercase().as_str() {
            "a" => BlitzKeyCode::KeyA,
            "b" => BlitzKeyCode::KeyB,
            "c" => BlitzKeyCode::KeyC,
            "d" => BlitzKeyCode::KeyD,
            "e" => BlitzKeyCode::KeyE,
            "f" => BlitzKeyCode::KeyF,
            "g" => BlitzKeyCode::KeyG,
            "h" => BlitzKeyCode::KeyH,
            "i" => BlitzKeyCode::KeyI,
            "j" => BlitzKeyCode::KeyJ,
            "k" => BlitzKeyCode::KeyK,
            "l" => BlitzKeyCode::KeyL,
            "m" => BlitzKeyCode::KeyM,
            "n" => BlitzKeyCode::KeyN,
            "o" => BlitzKeyCode::KeyO,
            "p" => BlitzKeyCode::KeyP,
            "q" => BlitzKeyCode::KeyQ,
            "r" => BlitzKeyCode::KeyR,
            "s" => BlitzKeyCode::KeyS,
            "t" => BlitzKeyCode::KeyT,
            "u" => BlitzKeyCode::KeyU,
            "v" => BlitzKeyCode::KeyV,
            "w" => BlitzKeyCode::KeyW,
            "x" => BlitzKeyCode::KeyX,
            "y" => BlitzKeyCode::KeyY,
            "z" => BlitzKeyCode::KeyZ,
            "0" => BlitzKeyCode::Digit0,
            "1" => BlitzKeyCode::Digit1,
            "2" => BlitzKeyCode::Digit2,
            "3" => BlitzKeyCode::Digit3,
            "4" => BlitzKeyCode::Digit4,
            "5" => BlitzKeyCode::Digit5,
            "6" => BlitzKeyCode::Digit6,
            "7" => BlitzKeyCode::Digit7,
            "8" => BlitzKeyCode::Digit8,
            "9" => BlitzKeyCode::Digit9,
            " " => BlitzKeyCode::Space,
            "enter" => BlitzKeyCode::Enter,
            "escape" => BlitzKeyCode::Escape,
            "backspace" => BlitzKeyCode::Backspace,
            "tab" => BlitzKeyCode::Tab,
            _ => BlitzKeyCode::Unidentified,
        }
    }

    fn convert_button(&self, button: pentimento_ipc::MouseButton) -> MouseEventButton {
        match button {
            pentimento_ipc::MouseButton::Left => MouseEventButton::Main,
            pentimento_ipc::MouseButton::Right => MouseEventButton::Secondary,
            pentimento_ipc::MouseButton::Middle => MouseEventButton::Auxiliary,
        }
    }

    fn create_pointer_event(&self, x: f32, y: f32, button: MouseEventButton) -> BlitzPointerEvent {
        self.create_pointer_event_with_buttons(x, y, button, self.buttons_pressed)
    }

    fn create_pointer_event_with_buttons(
        &self,
        x: f32,
        y: f32,
        button: MouseEventButton,
        buttons: MouseEventButtons,
    ) -> BlitzPointerEvent {
        BlitzPointerEvent {
            id: BlitzPointerId::Mouse,
            is_primary: true,
            coords: PointerCoords {
                page_x: x,
                page_y: y,
                screen_x: x,
                screen_y: y,
                client_x: x,
                client_y: y,
            },
            button,
            buttons,
            mods: BlitzModifiers::empty(),
            details: PointerDetails::default(),
        }
    }
}
