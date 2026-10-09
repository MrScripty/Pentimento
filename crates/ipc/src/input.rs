//! Input event types for mouse and keyboard.

use serde::{Deserialize, Serialize};

/// Mouse input events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MouseEvent {
    Move {
        x: f32,
        y: f32,
    },
    ButtonDown {
        button: MouseButton,
        x: f32,
        y: f32,
    },
    ButtonUp {
        button: MouseButton,
        x: f32,
        y: f32,
    },
    Scroll {
        delta_x: f32,
        delta_y: f32,
        x: f32,
        y: f32,
    },
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
    /// Existing key name used by frontend adapters.
    pub key: String,
    /// Physical native key (Bevy/DOM code name), independent of typed text.
    #[serde(default)]
    pub code: String,
    /// Layout-correct text produced by the native keypress, when available.
    #[serde(default)]
    pub text: Option<String>,
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
    /// Native AltGraph differs from an ordinary Alt/Ctrl shortcut.
    #[serde(default)]
    pub alt_graph: bool,
}
