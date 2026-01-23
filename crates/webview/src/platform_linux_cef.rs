//! CEF (Chromium Embedded Framework) offscreen webview implementation
//!
//! This module provides a webview implementation using CEF instead of WebKitGTK.
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
//! 3. Copy the BGRA pixel buffer to our RGBA framebuffer
//!
//! # References
//!
//! - CEF C API: https://bitbucket.org/chromiumembedded/cef/wiki/GeneralUsage
//! - Offscreen rendering: https://bitbucket.org/chromiumembedded/cef/wiki/GeneralUsage#markdown-header-off-screen-rendering

use crate::error::WebviewError;
use cef::args::Args;
use cef::rc::Rc as _;
use cef::{
    api_hash, sys, wrap_app, wrap_client, wrap_display_handler, wrap_render_handler, App, Browser,
    BrowserSettings, CefString, CefStringUtf16, Client, DisplayHandler, ImplApp, ImplBrowser,
    ImplBrowserHost, ImplClient, ImplDisplayHandler, ImplFrame, ImplRenderHandler, KeyEvent,
    KeyEventType, LogSeverity, MouseButtonType, PaintElementType, Rect, RenderHandler, Settings,
    WindowInfo, WrapApp, WrapClient, WrapDisplayHandler, WrapRenderHandler,
};
use pentimento_ipc::{KeyboardEvent, MouseButton, MouseEvent, UiToBevy};
use std::ffi::c_int;
use std::mem::size_of;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;
use tokio::sync::mpsc;

/// Global flag indicating whether CEF has been initialized
static CEF_INITIALIZED: OnceLock<bool> = OnceLock::new();

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

/// Shared framebuffer state between RenderHandler and LinuxCefWebview
struct SharedState {
    /// Raw BGRA pixel data from CEF (no conversion needed)
    framebuffer: Mutex<Option<Vec<u8>>>,
    /// Dimensions of the framebuffer
    framebuffer_size: Mutex<(u32, u32)>,
    dirty: Arc<AtomicBool>,
    size: Mutex<(u32, u32)>,
    /// Channel for sending UI messages to Bevy (for IPC via console messages)
    from_ui_tx: mpsc::UnboundedSender<UiToBevy>,
}

/// IPC message prefix used in console.log messages from JavaScript
const IPC_PREFIX: &str = "__PENTIMENTO_IPC__:";

/// CEF-based offscreen webview
///
/// This implementation uses Chromium for rendering instead of WebKitGTK.
/// The main advantages are:
/// - Better web standards compliance
/// - More consistent rendering across platforms
/// - GPU-accelerated compositing options
pub struct LinuxCefWebview {
    size: (u32, u32),
    state: CefState,
    shared: Arc<SharedState>,
    browser: Option<Browser>,
    #[allow(dead_code)]
    from_ui_tx: mpsc::UnboundedSender<UiToBevy>,
}

/// Custom render handler data
#[derive(Clone)]
pub(crate) struct OsrRenderHandler {
    shared: Arc<SharedState>,
}

impl OsrRenderHandler {
    fn new(shared: Arc<SharedState>) -> Self {
        Self { shared }
    }
}

// Macro generates RenderHandlerBuilder which wraps OsrRenderHandler
wrap_render_handler! {
    pub(crate) struct RenderHandlerBuilder {
        handler: OsrRenderHandler,
    }

    impl RenderHandler {
        fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
            if let Some(rect) = rect {
                let size = self.handler.shared.size.lock().unwrap();
                rect.x = 0;
                rect.y = 0;
                rect.width = size.0 as c_int;
                rect.height = size.1 as c_int;
            }
        }

        fn on_paint(
            &self,
            _browser: Option<&mut Browser>,
            type_: PaintElementType,
            _dirty_rects: Option<&[Rect]>,
            buffer: *const u8,
            width: c_int,
            height: c_int,
        ) {
            // Only handle VIEW (main view), not POPUP
            if type_ != PaintElementType::VIEW {
                return;
            }

            if buffer.is_null() || width <= 0 || height <= 0 {
                return;
            }

            let width = width as u32;
            let height = height as u32;
            let buffer_size = (width * height * 4) as usize;

            // Safety: CEF guarantees the buffer is valid for the duration of on_paint
            let bgra = unsafe { std::slice::from_raw_parts(buffer, buffer_size) };

            // PERFORMANCE INSTRUMENTATION: Time the buffer copy (no conversion needed!)
            let copy_start = Instant::now();

            // Just copy the BGRA buffer directly - GPU will interpret it as BGRA
            // This avoids the expensive per-pixel BGRA→RGBA conversion
            let buffer_copy = bgra.to_vec();

            let copy_elapsed = copy_start.elapsed();

            // Store the raw BGRA buffer
            let lock_start = Instant::now();
            *self.handler.shared.framebuffer.lock().unwrap() = Some(buffer_copy);
            *self.handler.shared.framebuffer_size.lock().unwrap() = (width, height);
            let lock_elapsed = lock_start.elapsed();

            self.handler.shared.dirty.store(true, Ordering::SeqCst);

            // Log performance metrics
            let total_elapsed = copy_start.elapsed();
            if total_elapsed.as_millis() > 2 {
                tracing::warn!(
                    "CEF on_paint PERF: {}x{} - copy: {:.2}ms, lock+store: {:.2}ms, total: {:.2}ms",
                    width, height,
                    copy_elapsed.as_secs_f64() * 1000.0,
                    lock_elapsed.as_secs_f64() * 1000.0,
                    total_elapsed.as_secs_f64() * 1000.0
                );
            }
        }
    }
}

/// Display handler for intercepting console messages (used for IPC)
#[derive(Clone)]
pub(crate) struct OsrDisplayHandler {
    shared: Arc<SharedState>,
}

impl OsrDisplayHandler {
    fn new(shared: Arc<SharedState>) -> Self {
        Self { shared }
    }
}

// Macro generates DisplayHandlerBuilder which wraps OsrDisplayHandler
wrap_display_handler! {
    pub(crate) struct DisplayHandlerBuilder {
        handler: OsrDisplayHandler,
    }

    impl DisplayHandler {
        fn on_console_message(
            &self,
            _browser: Option<&mut Browser>,
            _level: LogSeverity,
            message: Option<&CefString>,
            _source: Option<&CefString>,
            _line: c_int,
        ) -> c_int {
            // Check if this is an IPC message
            if let Some(msg) = message {
                let msg_str = msg.to_string();
                if let Some(json_str) = msg_str.strip_prefix(IPC_PREFIX) {
                    // Parse the JSON message and send to Bevy
                    match serde_json::from_str::<UiToBevy>(json_str) {
                        Ok(ui_msg) => {
                            // Mark dirty when UI sends UiDirty message
                            if matches!(ui_msg, UiToBevy::UiDirty) {
                                self.handler.shared.dirty.store(true, Ordering::SeqCst);
                            }
                            let _ = self.handler.shared.from_ui_tx.send(ui_msg);
                            tracing::trace!("CEF IPC received: {}", json_str);
                        }
                        Err(e) => {
                            tracing::warn!("Failed to parse CEF IPC message: {} - {}", json_str, e);
                        }
                    }
                    // Return 1 to suppress the console message (we handled it)
                    return 1;
                }
            }
            // Return 0 to allow normal console message handling
            0
        }
    }
}

impl DisplayHandlerBuilder {
    pub fn build(handler: OsrDisplayHandler) -> DisplayHandler {
        Self::new(handler)
    }
}

// Macro generates ClientBuilder which wraps RenderHandler and DisplayHandler
wrap_client! {
    pub(crate) struct ClientBuilder {
        render_handler: RenderHandler,
        display_handler: DisplayHandler,
    }

    impl Client {
        fn render_handler(&self) -> Option<cef::RenderHandler> {
            Some(self.render_handler.clone())
        }

        fn display_handler(&self) -> Option<cef::DisplayHandler> {
            Some(self.display_handler.clone())
        }
    }
}

// Implement builder methods to match the expected pattern
impl RenderHandlerBuilder {
    pub fn build(handler: OsrRenderHandler) -> RenderHandler {
        Self::new(handler)
    }
}

impl ClientBuilder {
    pub fn build(shared: Arc<SharedState>) -> Client {
        let render_handler = RenderHandlerBuilder::build(OsrRenderHandler::new(Arc::clone(&shared)));
        let display_handler = DisplayHandlerBuilder::build(OsrDisplayHandler::new(shared));
        Self::new(render_handler, display_handler)
    }
}

/// Minimal App for CEF initialization
/// CEF requires an App to be passed to initialize() for proper setup
#[derive(Clone)]
pub struct OsrApp;

wrap_app! {
    pub(crate) struct AppBuilder {
        app: OsrApp,
    }

    impl App {
        // We don't need any custom app behavior for OSR
    }
}

impl AppBuilder {
    pub fn build(app: OsrApp) -> App {
        Self::new(app)
    }
}

/// Find the CEF helper binary path
///
/// The helper binary should be in the same directory as the main executable,
/// or specified via CEF_HELPER_PATH environment variable.
fn find_helper_binary() -> Option<String> {
    // First check environment variable
    if let Ok(path) = std::env::var("CEF_HELPER_PATH") {
        if std::path::Path::new(&path).exists() {
            return Some(path);
        }
        tracing::warn!("CEF_HELPER_PATH set but file not found: {}", path);
    }

    // Get the directory of the current executable
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let helper_path = exe_dir.join("pentimento-cef-helper");
            if helper_path.exists() {
                return helper_path.to_str().map(|s| s.to_string());
            }
            tracing::debug!("Helper not found at: {:?}", helper_path);
        }
    }

    None
}

/// Initialize CEF (once per process)
fn ensure_cef_initialized() -> Result<(), WebviewError> {
    CEF_INITIALIZED.get_or_init(|| {
        tracing::info!("Initializing CEF...");

        // Find the helper binary - this is critical for avoiding subprocess issues
        let helper_path = match find_helper_binary() {
            Some(path) => {
                tracing::info!("Using CEF helper binary: {}", path);
                path
            }
            None => {
                tracing::error!(
                    "CEF helper binary not found. Ensure pentimento-cef-helper is in the same directory as the main executable, or set CEF_HELPER_PATH environment variable."
                );
                return false;
            }
        };

        // Set up CEF settings for offscreen rendering
        let mut settings = Settings::default();
        settings.windowless_rendering_enabled = 1;
        settings.no_sandbox = 1; // Disable sandbox for simpler setup
        settings.external_message_pump = 1; // We'll pump messages ourselves
        settings.multi_threaded_message_loop = 0;

        // CRITICAL: Set the subprocess helper path to prevent runaway process spawning
        // Without this, CEF uses the main executable for subprocesses, which causes
        // GTK/Bevy initialization conflicts
        settings.browser_subprocess_path = helper_path.as_str().into();

        // Get CEF path from environment (for debugging)
        if let Ok(cef_path) = std::env::var("CEF_PATH") {
            tracing::info!("CEF_PATH: {}", cef_path);
        }

        // Validate CEF API version (like OSR example does)
        let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);

        // Create Args from command line (important for proper CEF setup)
        let args = Args::new();

        // Create the CEF App - required for proper initialization
        let mut app = AppBuilder::build(OsrApp);

        // Call execute_process first (like OSR example does)
        // For the browser process, this returns -1
        // For subprocesses, it handles them and returns >= 0
        let exec_result =
            cef::execute_process(Some(args.as_main_args()), Some(&mut app), std::ptr::null_mut());
        tracing::info!("execute_process returned: {}", exec_result);

        // Should return -1 for browser process
        if exec_result >= 0 {
            // This shouldn't happen if we're using browser_subprocess_path
            tracing::warn!(
                "execute_process returned {} - this should be a subprocess",
                exec_result
            );
        }

        // Initialize CEF with the App
        let result = cef::initialize(
            Some(args.as_main_args()),
            Some(&settings),
            Some(&mut app),
            std::ptr::null_mut(),
        );

        if result == 0 {
            tracing::error!("Failed to initialize CEF");
            return false;
        }

        tracing::info!("CEF initialized successfully");
        true
    });

    if *CEF_INITIALIZED.get().unwrap_or(&false) {
        Ok(())
    } else {
        Err(WebviewError::InitializationFailed(
            "CEF initialization failed".into(),
        ))
    }
}

impl LinuxCefWebview {
    /// Create a new CEF offscreen webview
    ///
    /// # Arguments
    /// * `html_content` - HTML to load (via data: URL)
    /// * `size` - Initial viewport size
    /// * `dirty` - Shared flag for dirty tracking
    /// * `from_ui_tx` - Channel for UI -> Bevy messages
    pub fn new(
        html_content: &str,
        size: (u32, u32),
        dirty: Arc<AtomicBool>,
        from_ui_tx: mpsc::UnboundedSender<UiToBevy>,
    ) -> Result<Self, WebviewError> {
        // Initialize CEF if not already done
        ensure_cef_initialized()?;

        // Create shared state for communication between handlers and this struct
        let shared = Arc::new(SharedState {
            framebuffer: Mutex::new(None),
            framebuffer_size: Mutex::new((0, 0)),
            dirty,
            size: Mutex::new(size),
            from_ui_tx: from_ui_tx.clone(),
        });

        // Create the client with render handler and display handler (for IPC)
        let mut client = ClientBuilder::build(Arc::clone(&shared));

        // Configure window info for offscreen rendering
        let mut window_info = WindowInfo::default();
        window_info.windowless_rendering_enabled = 1;
        window_info.bounds.width = size.0 as c_int;
        window_info.bounds.height = size.1 as c_int;

        // Browser settings
        let mut browser_settings = BrowserSettings::default();

        // Encode HTML as data URL
        let encoded_html = urlencoding::encode(html_content);
        let data_url = format!("data:text/html,{}", encoded_html);
        let mut url: CefStringUtf16 = data_url.as_str().into();

        tracing::info!("Creating CEF browser with size {}x{}", size.0, size.1);

        // Create the browser
        let browser = cef::browser_host_create_browser_sync(
            Some(&mut window_info),
            Some(&mut client),
            Some(&mut url),
            Some(&mut browser_settings),
            None, // extra_info
            None, // request_context
        );

        if browser.is_none() {
            tracing::error!("Failed to create CEF browser");
            return Err(WebviewError::InitializationFailed(
                "Failed to create CEF browser".into(),
            ));
        }

        tracing::info!("CEF browser created successfully");

        Ok(Self {
            size,
            state: CefState::Loading,
            shared,
            browser,
            from_ui_tx,
        })
    }

    /// Poll CEF message loop
    ///
    /// Must be called each frame to process CEF events
    pub fn poll(&mut self) {
        // Process CEF message loop work
        cef::do_message_loop_work();

        // Check if we've received our first paint (framebuffer has data)
        if self.state == CefState::Loading {
            let has_framebuffer = self.shared.framebuffer.lock().unwrap().is_some();
            if has_framebuffer {
                self.state = CefState::Ready;
                tracing::info!("CEF webview ready");

                // Inject the IPC bridge into the page
                self.inject_ipc_bridge();
            }
        }
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

    /// Check if the webview is ready for capture
    pub fn is_ready(&self) -> bool {
        self.state == CefState::Ready
    }

    /// Capture the current framebuffer as raw BGRA bytes
    ///
    /// Returns the raw BGRA pixel data along with dimensions (width, height).
    /// The data is in BGRA format as provided by CEF - no conversion is performed.
    pub fn capture(&mut self) -> Option<(Vec<u8>, u32, u32)> {
        // PERFORMANCE INSTRUMENTATION: Time the framebuffer clone
        let start = Instant::now();
        let buffer = self.shared.framebuffer.lock().unwrap().clone()?;
        let (width, height) = *self.shared.framebuffer_size.lock().unwrap();
        let elapsed = start.elapsed();
        if elapsed.as_millis() > 1 {
            tracing::warn!(
                "CEF capture PERF: clone {}x{} ({:.2}MB) took {:.2}ms",
                width, height,
                buffer.len() as f64 / 1_000_000.0,
                elapsed.as_secs_f64() * 1000.0
            );
        }
        Some((buffer, width, height))
    }

    /// Resize the webview
    pub fn resize(&mut self, width: u32, height: u32) {
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

    /// Inject a mouse event into the webview
    pub fn inject_mouse(&mut self, event: MouseEvent) {
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

    /// Inject a keyboard event into the webview
    pub fn inject_keyboard(&mut self, event: KeyboardEvent) {
        let Some(browser) = &self.browser else { return };
        let Some(host) = browser.host() else { return };

        // Get the character from the key string
        let char_code = event.key.chars().next().unwrap_or('\0');

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
            windows_key_code: char_code as c_int,
            native_key_code: 0, // Platform-specific, not needed for basic input
            is_system_key: 0,
            character: char_code as u16,
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
                windows_key_code: char_code as c_int,
                native_key_code: 0,
                is_system_key: 0,
                character: char_code as u16,
                unmodified_character: char_code as u16,
                focus_on_editable_field: 0,
            };
            host.send_key_event(Some(&char_event));
        }
    }

    /// Evaluate JavaScript in the webview
    pub fn eval(&self, js: &str) -> Result<(), WebviewError> {
        let Some(browser) = &self.browser else {
            return Err(WebviewError::NotReady);
        };

        if let Some(frame) = browser.main_frame() {
            let js_string: CefStringUtf16 = js.into();
            let empty_url: CefStringUtf16 = "".into();
            frame.execute_java_script(Some(&js_string), Some(&empty_url), 0);
            Ok(())
        } else {
            Err(WebviewError::NotReady)
        }
    }

    /// Open Chrome DevTools for debugging the webview
    pub fn show_dev_tools(&self) {
        let Some(browser) = &self.browser else {
            tracing::warn!("Cannot show DevTools: browser not initialized");
            return;
        };

        let Some(host) = browser.host() else {
            tracing::warn!("Cannot show DevTools: no browser host");
            return;
        };

        // Create window info for the DevTools window (non-offscreen, regular window)
        let window_info = WindowInfo::default();

        // Use default browser settings for DevTools
        let settings = BrowserSettings::default();

        tracing::info!("Opening CEF DevTools window");
        host.show_dev_tools(Some(&window_info), None, Some(&settings), None);
    }
}

impl Drop for LinuxCefWebview {
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
