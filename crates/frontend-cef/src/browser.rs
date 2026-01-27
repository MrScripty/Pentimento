//! CEF browser creation and subprocess handling
//!
//! This module handles:
//! - CEF initialization (once per process)
//! - Helper binary discovery for subprocess architecture
//! - Browser instance creation with offscreen rendering

use cef::args::Args;
use cef::rc::Rc as _;
use cef::{
    api_hash, sys, wrap_app, wrap_client, wrap_display_handler, wrap_render_handler, App, Browser,
    BrowserSettings, CefString, CefStringUtf16, Client, DisplayHandler, ImplApp, ImplClient,
    ImplDisplayHandler, ImplRenderHandler, LogSeverity, PaintElementType, Rect, RenderHandler,
    Settings, WindowInfo, WrapApp, WrapClient, WrapDisplayHandler, WrapRenderHandler,
};
use pentimento_frontend_core::FrontendError;
use pentimento_ipc::UiToBevy;
use std::ffi::c_int;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::mpsc;

/// Global flag indicating whether CEF has been initialized
static CEF_INITIALIZED: OnceLock<bool> = OnceLock::new();

/// IPC message prefix used in console.log messages from JavaScript
pub(crate) const IPC_PREFIX: &str = "__PENTIMENTO_IPC__:";

/// Shared state between RenderHandler, DisplayHandler, and CefBackend
pub(crate) struct SharedState {
    /// Raw BGRA pixel data wrapped in Arc for zero-copy sharing between threads.
    /// The Arc allows capture() to clone just the pointer (~20ns) instead of
    /// copying the entire 18MB buffer (~6-12ms at HiDPI).
    pub framebuffer: Mutex<Option<Arc<Vec<u8>>>>,
    /// Dimensions of the framebuffer
    pub framebuffer_size: Mutex<(u32, u32)>,
    /// Flag indicating the framebuffer has been updated
    pub dirty: Arc<AtomicBool>,
    /// Current viewport size
    pub size: Mutex<(u32, u32)>,
    /// Channel for sending UI messages to Bevy (for IPC via console messages)
    pub from_ui_tx: mpsc::UnboundedSender<UiToBevy>,
}

/// Custom render handler for offscreen rendering
#[derive(Clone)]
pub(crate) struct OsrRenderHandler {
    pub shared: Arc<SharedState>,
}

impl OsrRenderHandler {
    pub fn new(shared: Arc<SharedState>) -> Self {
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

            // Copy the BGRA buffer and wrap in Arc for zero-copy sharing.
            // The to_vec() copy is unavoidable (CEF owns the buffer), but wrapping
            // in Arc allows capture() to share without another copy.
            let buffer_copy = Arc::new(bgra.to_vec());

            // Store the Arc-wrapped buffer
            *self.handler.shared.framebuffer.lock().unwrap() = Some(buffer_copy);
            *self.handler.shared.framebuffer_size.lock().unwrap() = (width, height);
            self.handler.shared.dirty.store(true, Ordering::SeqCst);
        }
    }
}

impl RenderHandlerBuilder {
    pub fn build(handler: OsrRenderHandler) -> RenderHandler {
        Self::new(handler)
    }
}

/// Display handler for intercepting console messages (used for IPC)
#[derive(Clone)]
pub(crate) struct OsrDisplayHandler {
    pub shared: Arc<SharedState>,
}

impl OsrDisplayHandler {
    pub fn new(shared: Arc<SharedState>) -> Self {
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
pub fn find_helper_binary() -> Option<String> {
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
pub fn ensure_cef_initialized() -> Result<(), FrontendError> {
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
        Err(FrontendError::Backend("CEF initialization failed".into()))
    }
}

/// Create a new CEF browser with offscreen rendering
pub fn create_browser(
    html_content: &str,
    size: (u32, u32),
    shared: &Arc<SharedState>,
) -> Result<Browser, FrontendError> {
    // Create the client with render handler and display handler (for IPC)
    let mut client = ClientBuilder::build(Arc::clone(shared));

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

    match browser {
        Some(browser) => {
            tracing::info!("CEF browser created successfully");
            Ok(browser)
        }
        None => {
            tracing::error!("Failed to create CEF browser");
            Err(FrontendError::Backend("Failed to create CEF browser".into()))
        }
    }
}
