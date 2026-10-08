use pentimento_frontend_core::cef_keyboard::{KeyEventKind, encode_keyboard};
use pentimento_ipc::{KeyboardEvent, Modifiers};

fn input(code: &str, key: &str, text: Option<&str>) -> KeyboardEvent {
    KeyboardEvent {
        code: code.into(),
        key: key.into(),
        text: text.map(str::to_owned),
        pressed: true,
        modifiers: Modifiers::default(),
    }
}
fn chars(event: &KeyboardEvent) -> Vec<u16> {
    encode_keyboard(event)
        .into_iter()
        .filter(|e| e.kind == KeyEventKind::Char)
        .map(|e| e.character)
        .collect()
}

#[test]
fn enter_commits_fields_with_return_and_never_types_an_e() {
    for code in ["Enter", "NumpadEnter"] {
        let e = input(code, "Enter", None);
        let encoded = encode_keyboard(&e);
        assert_eq!(encoded[0].windows_key_code, 13);
        assert_eq!(encoded[0].character, 13);
        assert_eq!(chars(&e), [13]);
    }
}
#[test]
fn named_keys_have_authoritative_virtual_keys_and_no_printable_characters() {
    for (code, key, vk) in [
        ("ControlLeft", "Control", 17),
        ("ShiftRight", "Shift", 16),
        ("AltLeft", "Alt", 18),
        ("SuperLeft", "Meta", 91),
        ("ArrowLeft", "ArrowLeft", 37),
        ("Delete", "Delete", 46),
        ("F1", "F1", 112),
    ] {
        let e = input(code, key, None);
        assert_eq!(encode_keyboard(&e)[0].windows_key_code, vk, "{code}");
        assert!(chars(&e).is_empty(), "{code}");
    }
    assert_eq!(chars(&input("Tab", "Tab", None)), [9]);
    assert_eq!(chars(&input("Backspace", "Backspace", None)), [8]);
    assert_eq!(
        encode_keyboard(&input("ArrowLeft", "ArrowLeft", None))[0].modifiers & (1 << 10),
        0
    );
}
#[test]
fn decimal_point_is_not_delete_and_radius_text_is_exact() {
    let e = input("Period", ".", Some("."));
    assert_eq!(encode_keyboard(&e)[0].windows_key_code, 190);
    let mut entered = Vec::new();
    for e in [
        input("Digit0", "0", Some("0")),
        e,
        input("Digit8", "8", Some("8")),
    ] {
        entered.extend(chars(&e));
    }
    assert_eq!(String::from_utf16(&entered).unwrap(), "0.8");
}
#[test]
fn shifted_native_text_supports_hex_fields_without_changing_physical_identity() {
    let mut hash = input("Digit3", "3", Some("#"));
    hash.modifiers.shift = true;
    assert_eq!(encode_keyboard(&hash)[0].windows_key_code, 51);
    assert_eq!(chars(&hash), [35]);
    let mut entered = chars(&hash);
    for (code, key) in [
        ("KeyF", "f"),
        ("KeyF", "f"),
        ("Digit0", "0"),
        ("Digit0", "0"),
        ("KeyF", "f"),
        ("KeyF", "f"),
    ] {
        entered.extend(chars(&input(code, key, Some(key))));
    }
    assert_eq!(String::from_utf16(&entered).unwrap(), "#ff00ff");
}
#[test]
fn shortcut_chords_and_releases_do_not_insert_text() {
    let mut e = input("KeyA", "a", Some("a"));
    e.modifiers.ctrl = true;
    let encoded = encode_keyboard(&e);
    assert_eq!(encoded.len(), 1);
    assert_eq!(encoded[0].windows_key_code, 65);
    assert_eq!(encoded[0].modifiers & 4, 4);
    for modifier in ["ctrl", "meta", "alt"] {
        let mut e = input("KeyA", "a", Some("a"));
        match modifier {
            "ctrl" => e.modifiers.ctrl = true,
            "meta" => e.modifiers.meta = true,
            _ => e.modifiers.alt = true,
        }
        assert!(chars(&e).is_empty());
    }
    e.modifiers.ctrl = false;
    e.pressed = false;
    assert_eq!(encode_keyboard(&e)[0].kind, KeyEventKind::KeyUp);
    assert!(chars(&e).is_empty());
}
#[test]
fn native_unicode_and_multi_character_text_preserve_utf16() {
    let text = "λ🙂¨a";
    assert_eq!(
        String::from_utf16(&chars(&input("KeyA", "a", Some(text)))).unwrap(),
        text
    );
    assert!(
        chars(&input("Quote", "'", None)).is_empty(),
        "dead keys without native text must not insert a guessed apostrophe"
    );
}
#[test]
fn legacy_payloads_remain_readable_and_named_keys_are_safe() {
    let e:KeyboardEvent=serde_json::from_str(r#"{"key":"Enter","pressed":true,"modifiers":{"shift":false,"ctrl":false,"alt":false,"meta":false}}"#).unwrap();
    assert!(e.code.is_empty());
    assert!(e.text.is_none());
    assert_eq!(encode_keyboard(&e)[0].windows_key_code, 13);
    assert_eq!(chars(&e), [13]);
    assert_eq!(
        encode_keyboard(&input("Unknown", "Unidentified", None))[0].windows_key_code,
        0
    );
    assert!(chars(&input("Unknown", "Unidentified", None)).is_empty());
}

#[test]
fn alt_graph_inserts_native_text_with_distinct_cef_flags() {
    let mut e = input("KeyQ", "q", Some("@"));
    e.modifiers.alt = true;
    e.modifiers.ctrl = true;
    e.modifiers.alt_graph = true;
    assert_eq!(chars(&e), [64]);
    let raw = encode_keyboard(&e)[0];
    assert_eq!(raw.modifiers & (4096 | 4 | 8), 4096);
    assert_eq!(raw.windows_key_code, 81);
    e.code = "AltRight".into();
    e.key = "Alt".into();
    e.text = None;
    assert_eq!(encode_keyboard(&e)[0].windows_key_code, 0xe1);
    assert!(chars(&e).is_empty());
}
