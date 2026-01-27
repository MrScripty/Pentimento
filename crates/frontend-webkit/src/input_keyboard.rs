//! Keyboard input injection for WebKit backend

use std::sync::atomic::Ordering;

use gio::Cancellable;
use pentimento_ipc::KeyboardEvent;
use webkit2gtk::WebViewExt;

use crate::WebKitBackend;

impl WebKitBackend {
    /// Inject a keyboard event into the webview
    pub fn inject_keyboard(&mut self, event: KeyboardEvent) {
        // Use JavaScript to dispatch DOM keyboard events
        let event_type = if event.pressed { "keydown" } else { "keyup" };

        // Escape the key for JavaScript string
        let key_escaped = event.key.replace('\\', "\\\\").replace('\'', "\\'");

        let js = format!(
            r#"(function() {{
                const target = document.activeElement || document.body;
                target.dispatchEvent(new KeyboardEvent('{event_type}', {{
                    bubbles: true,
                    cancelable: true,
                    key: '{key}',
                    shiftKey: {shift},
                    ctrlKey: {ctrl},
                    altKey: {alt},
                    metaKey: {meta},
                    view: window
                }}));
                // For text input, also dispatch input event for printable keys
                if ('{event_type}' === 'keydown' && '{key}'.length === 1 && !{ctrl} && !{alt} && !{meta}) {{
                    if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable) {{
                        // Let the browser handle text input naturally
                    }}
                }}
            }})()"#,
            event_type = event_type,
            key = key_escaped,
            shift = event.modifiers.shift,
            ctrl = event.modifiers.ctrl,
            alt = event.modifiers.alt,
            meta = event.modifiers.meta
        );

        self.webkit_webview
            .run_javascript(&js, Cancellable::NONE, |_| {});

        // Only mark dirty for key presses (not releases) that might change UI
        // Modifier keys alone don't typically change visible UI
        let is_modifier = matches!(
            event.key.as_str(),
            "Shift" | "Control" | "Alt" | "Meta"
        );
        if event.pressed && !is_modifier {
            self.dirty.store(true, Ordering::SeqCst);
        }
    }
}
