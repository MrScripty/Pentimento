//! CEF's Linux key-event contract without a CEF runtime dependency.
//! Virtual keys describe the key; UTF-16 character fields carry native text.
//! See CEF's browser_window_osr_gtk.cc and cef_key_event_t documentation.

use pentimento_ipc::KeyboardEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEventKind {
    RawKeyDown,
    KeyUp,
    Char,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CefKeyboardEvent {
    pub kind: KeyEventKind,
    pub windows_key_code: i32,
    pub modifiers: u32,
    pub character: u16,
    pub unmodified_character: u16,
}

/// Encode native text as UTF-16 CHAR events after the physical key event.
/// Key releases, modifiers, navigation and shortcuts never insert printable text.
pub fn encode_keyboard(event: &KeyboardEvent) -> Vec<CefKeyboardEvent> {
    let vk =
        if event.modifiers.alt_graph && matches!(event.code.as_str(), "AltLeft" | "AltRight") {
            Some(0xe1)
        } else {
            virtual_key(&event.code)
        }
        .or_else(|| virtual_key(&event.key))
        .unwrap_or(0);
    let m = &event.modifiers;
    let modifiers = (u32::from(m.shift) << 1)
        | (u32::from(m.ctrl && !m.alt_graph) << 2)
        | (u32::from(m.alt && !m.alt_graph) << 3)
        | (u32::from(m.meta) << 7)
        | (u32::from(m.alt_graph) << 12)
        | (u32::from(event.code.starts_with("Numpad")) << 9)
        | (u32::from(matches!(
            event.code.as_str(),
            "ShiftLeft" | "ControlLeft" | "AltLeft" | "SuperLeft" | "MetaLeft"
        )) << 10)
        | (u32::from(matches!(
            event.code.as_str(),
            "ShiftRight" | "ControlRight" | "AltRight" | "SuperRight" | "MetaRight"
        )) << 11);
    let control_text = match vk {
        0x0d => Some("\r"),
        0x09 => Some("\t"),
        0x08 => Some("\u{8}"),
        _ => None,
    };
    let legacy_text = if event.code.is_empty() && event.key.chars().count() == 1 {
        Some(if m.shift {
            event.key.to_uppercase()
        } else {
            event.key.clone()
        })
    } else {
        None
    };
    let text = control_text
        .or(event.text.as_deref())
        .or(legacy_text.as_deref());
    let unmodified = text.and_then(|s| s.encode_utf16().next()).unwrap_or(0);
    // Ctrl/Meta/Alt shortcuts are handled by RAWKEYDOWN, not text insertion.
    let units: Vec<_> = if event.pressed && !m.meta && (m.alt_graph || (!m.ctrl && !m.alt)) {
        text.unwrap_or("")
            .chars()
            .filter(|c| !c.is_control() || matches!(c, '\r' | '\t' | '\u{8}'))
            .flat_map(|c| {
                let mut units = [0; 2];
                c.encode_utf16(&mut units).to_vec()
            })
            .collect()
    } else {
        Vec::new()
    };
    let raw = CefKeyboardEvent {
        kind: if event.pressed {
            KeyEventKind::RawKeyDown
        } else {
            KeyEventKind::KeyUp
        },
        windows_key_code: vk,
        modifiers,
        character: units.first().copied().unwrap_or(0),
        unmodified_character: unmodified,
    };
    let mut result = vec![raw];
    result.extend(units.into_iter().map(|character| CefKeyboardEvent {
        kind: KeyEventKind::Char,
        character,
        unmodified_character: character,
        ..raw
    }));
    result
}

fn virtual_key(code: &str) -> Option<i32> {
    let named = match code {
        "Backspace" => 0x08,
        "Tab" => 0x09,
        "Enter" | "NumpadEnter" => 0x0d,
        "Shift" | "ShiftLeft" | "ShiftRight" => 0x10,
        "Control" | "ControlLeft" | "ControlRight" => 0x11,
        "Alt" | "AltLeft" | "AltRight" => 0x12,
        "Pause" => 0x13,
        "CapsLock" => 0x14,
        "Escape" => 0x1b,
        "Space" | " " => 0x20,
        "PageUp" => 0x21,
        "PageDown" => 0x22,
        "End" => 0x23,
        "Home" => 0x24,
        "ArrowLeft" => 0x25,
        "ArrowUp" => 0x26,
        "ArrowRight" => 0x27,
        "ArrowDown" => 0x28,
        "PrintScreen" => 0x2c,
        "Insert" => 0x2d,
        "Delete" => 0x2e,
        "Meta" | "SuperLeft" | "MetaLeft" => 0x5b,
        "SuperRight" | "MetaRight" => 0x5c,
        "ContextMenu" => 0x5d,
        "NumpadMultiply" => 0x6a,
        "NumpadAdd" => 0x6b,
        "NumpadSubtract" => 0x6d,
        "NumpadDecimal" => 0x6e,
        "NumpadDivide" => 0x6f,
        "NumLock" => 0x90,
        "ScrollLock" => 0x91,
        "Semicolon" | ";" => 0xba,
        "Equal" | "=" => 0xbb,
        "Comma" | "," => 0xbc,
        "Minus" | "-" => 0xbd,
        "Period" | "." => 0xbe,
        "Slash" | "/" => 0xbf,
        "Backquote" | "`" => 0xc0,
        "BracketLeft" | "[" => 0xdb,
        "Backslash" | "\\" => 0xdc,
        "BracketRight" | "]" => 0xdd,
        "Quote" | "'" => 0xde,
        _ => {
            if let Some(letter) = code.strip_prefix("Key").filter(|s| s.len() == 1) {
                return letter.as_bytes()[0]
                    .is_ascii_uppercase()
                    .then_some(i32::from(letter.as_bytes()[0]));
            }
            if let Some(digit) = code.strip_prefix("Digit").filter(|s| s.len() == 1) {
                return digit.as_bytes()[0]
                    .is_ascii_digit()
                    .then_some(i32::from(digit.as_bytes()[0]));
            }
            if let Some(digit) = code.strip_prefix("Numpad").filter(|s| s.len() == 1) {
                return digit.as_bytes()[0]
                    .is_ascii_digit()
                    .then_some(0x60 + i32::from(digit.as_bytes()[0] - b'0'));
            }
            if let Some(number) = code
                .strip_prefix('F')
                .and_then(|s| s.parse::<i32>().ok())
                .filter(|n| (1..=24).contains(n))
            {
                return Some(0x70 + number - 1);
            }
            if code.len() == 1 {
                let c = code.as_bytes()[0];
                if c.is_ascii_alphanumeric() {
                    return Some(i32::from(c.to_ascii_uppercase()));
                }
            }
            return None;
        }
    };
    Some(named)
}
