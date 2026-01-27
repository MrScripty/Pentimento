//! Input event types for mouse and keyboard.

use serde::{Deserialize, Serialize};

/// Mouse input events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MouseEvent {
    Move { x: f32, y: f32 },
    ButtonDown { button: MouseButton, x: f32, y: f32 },
    ButtonUp { button: MouseButton, x: f32, y: f32 },
    Scroll { delta_x: f32, delta_y: f32, x: f32, y: f32 },
}

/// Mouse button identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Keyboard input event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyboardEvent {
    pub key: String,
    pub pressed: bool,
    pub modifiers: Modifiers,
}

/// Keyboard modifier keys state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}
