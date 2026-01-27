//! CEF (Chromium Embedded Framework) offscreen webview implementation
//!
//! This crate provides a webview implementation using CEF for Pentimento.
//! CEF renders to an offscreen buffer that can be captured and uploaded as a texture.
//!
//! # Setup
//!
//! CEF binaries must be downloaded before use:
//! ```bash
//! ./scripts/setup-cef.sh
//! ```
//!
//! # Architecture
//!
//! CEF uses a multi-process architecture:
//! - Browser process (main app) hosts the webview
//! - Render process handles JavaScript execution
//! - GPU process handles accelerated rendering
//!
//! For offscreen rendering (OSR), we:
//! 1. Create a browser with windowless rendering enabled
//! 2. Implement a RenderHandler that receives paint callbacks
//! 3. Store the BGRA pixel buffer for zero-copy sharing via Arc
//!
//! # References
//!
//! - CEF C API: https://bitbucket.org/chromiumembedded/cef/wiki/GeneralUsage
//! - Offscreen rendering: https://bitbucket.org/chromiumembedded/cef/wiki/GeneralUsage#markdown-header-off-screen-rendering

pub mod browser;
pub mod capture;
pub mod devtools;

use browser::{SharedState, IPC_PREFIX};
use cef::{Browser, CefStringUtf16, ImplBrowser, ImplBrowserHost, ImplFrame, KeyEvent, KeyEventType, MouseButtonType};
use pentimento_frontend_core::{CaptureResult, CompositeBackend, FrontendError};
use pentimento_ipc::{BevyToUi, KeyboardEvent, MouseButton, MouseEvent, UiToBevy};
use std::ffi::c_int;
use std::mem::size_of;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// CEF webview state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CefState {
    /// CEF is initializing
    Initializing,
    /// Browser is loading content
    Loading,
    /// Ready for capture
    Ready,
    /// CEF encountered an error
    Error,
}

/// CEF-based offscreen webview backend
///
/// This implementation uses Chromium for rendering instead of WebKitGTK.
/// The main advantages are:
/// - Better web standards compliance
/// - More consistent rendering across platforms
/// - GPU-accelerated compositing options
pub struct CefBackend {
    size: (u32, u32),
    state: CefState,
    shared: Arc<SharedState>,
    browser: Option<Browser>,
    from_ui_tx: mpsc::UnboundedSender<UiToBevy>,
    from_ui_rx: mpsc::UnboundedReceiver<UiToBevy>,
    to_ui_messages: Vec<BevyToUi>,
}

impl CefBackend {
    /// Create a new CEF offscreen webview backend
    ///
    /// # Arguments
    /// * `html_content` - HTML to load (via data: URL)
    /// * `size` - Initial viewport size
    pub fn new(html_content: &str, size: (u32, u32)) -> Result<Self, FrontendError> {
        // Initialize CEF if not already done
        browser::ensure_cef_initialized()?;

        // Create channels for IPC
        let (from_ui_tx, from_ui_rx) = mpsc::unbounded_channel();

        // Create shared state for communication between handlers and this struct
        let shared = Arc::new(SharedState {
            framebuffer: Mutex::new(None),
            framebuffer_size: Mutex::new((0, 0)),
            dirty: Arc::new(AtomicBool::new(false)),
            size: Mutex::new(size),
            from_ui_tx: from_ui_tx.clone(),
        });

        // Create the browser
        let browser = browser::create_browser(html_content, size, &shared)?;

        Ok(Self {
            size,
            state: CefState::Loading,
            shared,
            browser: Some(browser),
            from_ui_tx,
            from_ui_rx,
            to_ui_messages: Vec::new(),
        })
    }

    /// Get the current webview state
    pub fn state(&self) -> CefState {
        self.state
    }

    /// Inject the JavaScript IPC bridge that mimics wry's window.ipc.postMessage()
    fn inject_ipc_bridge(&self) {
        let ipc_bridge_js = format!(
            r#"
            (function() {{
                if (window.ipc) return; // Already injected

                window.ipc = {{
                    postMessage: function(message) {{
                        // Send IPC messages via console.log with our special prefix
                        console.log('{}' + message);
                    }}
                }};

                // Also trigger initial UiDirty to signal that IPC is ready
                window.ipc.postMessage(JSON.stringify({{ type: 'UiDirty' }}));

                console.log('Pentimento IPC bridge initialized');
            }})();
            "#,
            IPC_PREFIX
        );

        if let Err(e) = self.eval(&ipc_bridge_js) {
            tracing::error!("Failed to inject IPC bridge: {}", e);
        } else {
            tracing::info!("CEF IPC bridge injected");
        }
    }

    /// Evaluate JavaScript in the webview
    pub fn eval(&self, js: &str) -> Result<(), FrontendError> {
        let Some(browser) = &self.browser else {
            return Err(FrontendError::NotReady);
        };

        if let Some(frame) = browser.main_frame() {
            let js_string: CefStringUtf16 = js.into();
            let empty_url: CefStringUtf16 = "".into();
            frame.execute_java_script(Some(&js_string), Some(&empty_url), 0);
            Ok(())
        } else {
            Err(FrontendError::NotReady)
        }
    }

    /// Open Chrome DevTools for debugging
    pub fn show_dev_tools(&self) {
        if let Some(browser) = &self.browser {
            devtools::show_dev_tools(browser);
        } else {
            tracing::warn!("Cannot show DevTools: browser not initialized");
        }
    }

    /// Close DevTools if open
    pub fn close_dev_tools(&self) {
        if let Some(browser) = &self.browser {
            devtools::close_dev_tools(browser);
        }
    }

    /// Toggle DevTools (Ctrl+Shift+I behavior)
    pub fn toggle_dev_tools(&self) {
        if let Some(browser) = &self.browser {
            devtools::toggle_dev_tools(browser);
        }
    }

    /// Check if DevTools is currently open
    pub fn has_dev_tools(&self) -> bool {
        self.browser
            .as_ref()
            .map(|b| devtools::has_dev_tools(b))
            .unwrap_or(false)
    }

    /// Flush pending messages to the UI by evaluating JavaScript
    fn flush_to_ui_messages(&mut self) {
        if self.to_ui_messages.is_empty() || self.state != CefState::Ready {
            return;
        }

        let messages = std::mem::take(&mut self.to_ui_messages);
        for msg in messages {
            match serde_json::to_string(&msg) {
                Ok(json) => {
                    // Call the global message handler if it exists
                    let js = format!(
                        r#"if (window.__PENTIMENTO_RECV__) {{ window.__PENTIMENTO_RECV__('{}'); }}"#,
                        json.replace('\\', "\\\\").replace('\'', "\\'")
                    );
                    if let Err(e) = self.eval(&js) {
                        tracing::warn!("Failed to send message to UI: {}", e);
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to serialize message for UI: {}", e);
                }
            }
        }
    }
}

impl CompositeBackend for CefBackend {
    fn poll(&mut self) {
        // Process CEF message loop work
        cef::do_message_loop_work();

        // Check if we've received our first paint (framebuffer has data)
        if self.state == CefState::Loading {
            if capture::has_framebuffer(&self.shared) {
                self.state = CefState::Ready;
                tracing::info!("CEF webview ready");

                // Inject the IPC bridge into the page
                self.inject_ipc_bridge();
            }
        }

        // Flush any pending messages to the UI
        self.flush_to_ui_messages();
    }

    fn is_ready(&self) -> bool {
        self.state == CefState::Ready
    }

    fn capture_if_dirty(&mut self) -> Option<CaptureResult> {
        capture::capture_if_dirty(&self.shared)
    }

    fn size(&self) -> (u32, u32) {
        self.size
    }

    fn resize(&mut self, width: u32, height: u32) {
        if self.size == (width, height) {
            return;
        }

        self.size = (width, height);
        *self.shared.size.lock().unwrap() = (width, height);
        tracing::info!("CEF webview resized to {}x{}", width, height);

        // Notify CEF of resize
        if let Some(browser) = &self.browser {
            if let Some(host) = browser.host() {
                host.was_resized();
            }
        }
    }

    fn send_mouse_event(&mut self, event: MouseEvent) {
        let Some(browser) = &self.browser else { return };
        let Some(host) = browser.host() else { return };

        match event {
            MouseEvent::Move { x, y } => {
                let mouse_event = cef::MouseEvent {
                    x: x as c_int,
                    y: y as c_int,
                    modifiers: 0,
                };
                host.send_mouse_move_event(Some(&mouse_event), 0); // mouse_leave = false
            }
            MouseEvent::ButtonDown { button, x, y } => {
                let mouse_event = cef::MouseEvent {
                    x: x as c_int,
                    y: y as c_int,
                    modifiers: 0,
                };
                let cef_button = match button {
                    MouseButton::Left => MouseButtonType::LEFT,
                    MouseButton::Middle => MouseButtonType::MIDDLE,
                    MouseButton::Right => MouseButtonType::RIGHT,
                };
                host.send_mouse_click_event(Some(&mouse_event), cef_button, 0, 1); // mouse_up = false
            }
            MouseEvent::ButtonUp { button, x, y } => {
                let mouse_event = cef::MouseEvent {
                    x: x as c_int,
                    y: y as c_int,
                    modifiers: 0,
                };
                let cef_button = match button {
                    MouseButton::Left => MouseButtonType::LEFT,
                    MouseButton::Middle => MouseButtonType::MIDDLE,
                    MouseButton::Right => MouseButtonType::RIGHT,
                };
                host.send_mouse_click_event(Some(&mouse_event), cef_button, 1, 1); // mouse_up = true
            }
            MouseEvent::Scroll {
                x,
                y,
                delta_x,
                delta_y,
            } => {
                let mouse_event = cef::MouseEvent {
                    x: x as c_int,
                    y: y as c_int,
                    modifiers: 0,
                };
                host.send_mouse_wheel_event(Some(&mouse_event), delta_x as c_int, delta_y as c_int);
            }
        }
    }

    fn send_keyboard_event(&mut self, event: KeyboardEvent) {
        let Some(browser) = &self.browser else { return };
        let Some(host) = browser.host() else { return };

        // Get the character from the key string
        let char_code = event.key.chars().next().unwrap_or('\0');

        // For Windows virtual key codes, letter keys must use uppercase (VK_KEY_A = 65, not 97)
        // This is how Windows virtual key codes work - they're always uppercase for letters
        let vk_code = if char_code.is_ascii_lowercase() {
            char_code.to_ascii_uppercase() as c_int
        } else {
            char_code as c_int
        };

        // The actual character that would be typed (depends on shift state)
        let typed_char = if event.modifiers.shift && char_code.is_ascii_lowercase() {
            char_code.to_ascii_uppercase()
        } else {
            char_code
        };

        // Build modifiers from the event
        let mut modifiers: u32 = 0;
        if event.modifiers.shift {
            modifiers |= 1 << 1; // EVENTFLAG_SHIFT_DOWN
        }
        if event.modifiers.ctrl {
            modifiers |= 1 << 2; // EVENTFLAG_CONTROL_DOWN
        }
        if event.modifiers.alt {
            modifiers |= 1 << 3; // EVENTFLAG_ALT_DOWN
        }

        let key_event = KeyEvent {
            size: size_of::<KeyEvent>(),
            type_: if event.pressed {
                KeyEventType::RAWKEYDOWN
            } else {
                KeyEventType::KEYUP
            },
            modifiers,
            windows_key_code: vk_code,
            native_key_code: 0, // Platform-specific, not needed for basic input
            is_system_key: 0,
            character: typed_char as u16,
            unmodified_character: char_code as u16,
            focus_on_editable_field: 0,
        };
        host.send_key_event(Some(&key_event));

        // Also send char event for key presses (for text input)
        if event.pressed && !event.key.is_empty() && char_code != '\0' {
            let char_event = KeyEvent {
                size: size_of::<KeyEvent>(),
                type_: KeyEventType::CHAR,
                modifiers,
                windows_key_code: vk_code,
                native_key_code: 0,
                is_system_key: 0,
                character: typed_char as u16,
                unmodified_character: char_code as u16,
                focus_on_editable_field: 0,
            };
            host.send_key_event(Some(&char_event));
        }
    }

    fn send_to_ui(&mut self, msg: BevyToUi) -> Result<(), FrontendError> {
        // Queue the message for sending during poll()
        self.to_ui_messages.push(msg);
        Ok(())
    }

    fn try_recv_from_ui(&mut self) -> Option<UiToBevy> {
        self.from_ui_rx.try_recv().ok()
    }
}

impl Drop for CefBackend {
    fn drop(&mut self) {
        tracing::info!("Dropping CEF webview");

        // Close the browser
        if let Some(browser) = self.browser.take() {
            if let Some(host) = browser.host() {
                host.close_browser(1); // force_close = true (as c_int)
            }
        }

        // Note: CefShutdown() should only be called when the app exits,
        // not when individual webviews are dropped
    }
}
