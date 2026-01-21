//! JavaScript bridge for Tauri IPC
//!
//! This module provides communication between Bevy WASM and the Svelte UI.
//! Messages are passed via CustomEvents on the window object.

use pentimento_ipc::{BevyToUi, UiToBevy};
use std::cell::RefCell;
use std::collections::VecDeque;
use wasm_bindgen::prelude::*;

thread_local! {
    /// Queue of messages received from the UI
    static MESSAGE_QUEUE: RefCell<VecDeque<UiToBevy>> = RefCell::new(VecDeque::new());
}

/// Initialize the JavaScript event listeners
pub fn init_bridge() {
    // Set up event listener for UI -> Bevy messages
    let window = web_sys::window().expect("no global window");

    let closure = Closure::wrap(Box::new(move |event: web_sys::CustomEvent| {
        if let Some(detail) = event.detail().as_string() {
            match serde_json::from_str::<UiToBevy>(&detail) {
                Ok(msg) => {
                    MESSAGE_QUEUE.with(|queue| {
                        queue.borrow_mut().push_back(msg);
                    });
                }
                Err(e) => {
                    web_sys::console::error_1(&format!("Failed to parse UI message: {}", e).into());
                }
            }
        }
    }) as Box<dyn FnMut(_)>);

    window
        .add_event_listener_with_callback("pentimento:ui-to-bevy", closure.as_ref().unchecked_ref())
        .expect("failed to add event listener");

    // Keep the closure alive
    closure.forget();

    web_sys::console::log_1(&"Pentimento WASM bridge initialized".into());
}

/// Poll for the next message from the UI (non-blocking)
pub fn poll_ui_message() -> Option<UiToBevy> {
    MESSAGE_QUEUE.with(|queue| queue.borrow_mut().pop_front())
}

/// Send a message to the Svelte UI
pub fn send_to_ui(msg: BevyToUi) {
    let window = web_sys::window().expect("no global window");

    match serde_json::to_string(&msg) {
        Ok(json) => {
            let init = web_sys::CustomEventInit::new();
            init.set_detail(&JsValue::from_str(&json));

            let event = web_sys::CustomEvent::new_with_event_init_dict("pentimento:bevy-to-ui", &init)
                .expect("failed to create event");

            window
                .dispatch_event(&event)
                .expect("failed to dispatch event");
        }
        Err(e) => {
            web_sys::console::error_1(&format!("Failed to serialize Bevy message: {}", e).into());
        }
    }
}
