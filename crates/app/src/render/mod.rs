//! Render pipeline extensions for UI compositing
//!
//! Provides a unified `FrontendResource` that abstracts over different UI rendering backends
//! via the `CompositeBackend` trait. The render system polls the backend and handles
//! framebuffer capture results polymorphically:
//!
//! - `CaptureResult::Rgba` - Upload RGBA texture (WebKit/Capture mode)
//! - `CaptureResult::Bgra` - Upload BGRA texture (CEF mode)
//! - `CaptureResult::CompositorManaged` - No texture update (Overlay/Dioxus modes)
//!
//! # Supported Modes
//!
//! - **Capture**: Offscreen WebKitGTK webview with framebuffer capture (default)
//! - **Overlay**: Transparent child window composited by desktop compositor
//! - **Cef**: CEF (Chromium) offscreen rendering with framebuffer capture
//! - **Dioxus**: Native Rust UI with Vello GPU renderer (zero-copy, uses separate plugin)
//! - **Tauri**: Bevy WASM in Tauri webview (requires separate build)

use std::sync::Arc;
use std::time::{Duration, Instant};

use bevy::asset::RenderAssetUsages;
use bevy::picking::prelude::Pickable;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::window::RawHandleWrapper;
use pentimento_frontend_core::{CaptureResult, CompositeBackend, FrontendError};
use pentimento_ipc::UiToBevy;
use pentimento_scene::{
    AddObjectEvent, CanvasPlaneEvent, DepthViewSettings, OutboundUiMessages, SceneAmbientOcclusion, SceneLighting,
};

use crate::config::{CompositeMode, PentimentoConfig};
use crate::embedded_ui::UiAssets;

// Keep submodules for mode-specific initialization helpers
mod ui_composite;
mod ui_overlay;
#[cfg(feature = "cef")]
mod ui_cef;
#[cfg(feature = "dioxus")]
mod ui_blend_material;
#[cfg(feature = "dioxus")]
mod ui_dioxus;

// Re-export types needed by the input module
pub use ui_composite::WebviewResource;
pub use ui_overlay::OverlayWebviewResource;
#[cfg(feature = "cef")]
pub use ui_cef::CefWebviewResource;
#[cfg(feature = "dioxus")]
pub use ui_dioxus::DioxusRendererResource;

// ============================================================================
// Unified Frontend Resource
// ============================================================================

/// Unified resource holding any UI backend that implements `CompositeBackend`.
///
/// This abstraction allows the render system to work polymorphically with all
/// capture-based backends (WebKit, CEF, Overlay). The system calls `poll()`,
/// checks `is_ready()`, and handles `capture_if_dirty()` results uniformly.
pub struct FrontendResource {
    /// The backend implementation (boxed trait object for dynamic dispatch)
    pub backend: Box<dyn CompositeBackend>,
    /// Texture format used by this backend (RGBA or BGRA)
    pub texture_format: TextureFormat,
}

/// Resource holding the UI texture handle for capture-based modes.
#[derive(Resource)]
pub struct UiTextureHandle {
    pub handle: Handle<Image>,
}

/// Marker component for the UI overlay node.
#[derive(Component)]
pub struct UiOverlay;

/// Track frontend initialization and capture state.
#[derive(Resource)]
pub struct FrontendStatus {
    pub initialized: bool,
    pub first_capture_done: bool,
    pub last_capture: Instant,
    /// Current composite mode
    pub mode: CompositeMode,
}

impl Default for FrontendStatus {
    fn default() -> Self {
        Self {
            initialized: false,
            first_capture_done: false,
            last_capture: Instant::now(),
            mode: CompositeMode::default(),
        }
    }
}

/// Track the last known window size for resize detection.
#[derive(Resource, Default)]
pub struct LastWindowSize {
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
}

/// Heartbeat interval for marking the UI dirty (forces periodic capture).
const CAPTURE_HEARTBEAT_INTERVAL: Duration = Duration::from_millis(16);

// ============================================================================
// Factory Function
// ============================================================================

/// Configuration needed to create a frontend backend.
pub struct FrontendConfig {
    /// HTML content to load in the webview
    pub html: String,
    /// Initial viewport dimensions (width, height)
    pub size: (u32, u32),
    /// Scale factor for HiDPI displays
    pub scale_factor: f64,
    /// Raw window handle (needed for overlay mode)
    pub window_handle: Option<raw_window_handle::RawWindowHandle>,
}

/// Create the appropriate frontend backend based on the composite mode.
///
/// Returns a `FrontendResource` containing the backend and its texture format,
/// or an error if backend creation fails.
///
/// # Arguments
///
/// * `mode` - The composite mode to use
/// * `config` - Configuration for the frontend (HTML, size, window handle)
///
/// # Errors
///
/// Returns `FrontendError` if the backend fails to initialize.
pub fn create_frontend(
    mode: CompositeMode,
    config: FrontendConfig,
) -> Result<FrontendResource, FrontendError> {
    match mode {
        CompositeMode::Capture => {
            // WebKit capture mode - RGBA format
            let mut webview = pentimento_webview::OffscreenWebview::new(&config.html, config.size)
                .map_err(|e| FrontendError::Backend(e.to_string()))?;
            webview.set_scale_factor(config.scale_factor);

            Ok(FrontendResource {
                backend: Box::new(webview),
                texture_format: TextureFormat::Rgba8UnormSrgb,
            })
        }

        CompositeMode::Overlay => {
            // Overlay mode - compositor-managed (no texture capture needed)
            let window_handle = config
                .window_handle
                .ok_or_else(|| FrontendError::Backend("Overlay mode requires window handle".into()))?;

            let webview = pentimento_webview::OverlayWebview::new(window_handle, &config.html, config.size)
                .map_err(|e| FrontendError::Backend(e.to_string()))?;

            // Overlay uses RGBA format for the placeholder texture (not actually used for capture)
            Ok(FrontendResource {
                backend: Box::new(webview),
                texture_format: TextureFormat::Rgba8UnormSrgb,
            })
        }

        #[cfg(feature = "cef")]
        CompositeMode::Cef => {
            // CEF mode - BGRA format (native Chromium format)
            let webview = pentimento_webview::CefWebview::new(&config.html, config.size)
                .map_err(|e| FrontendError::Backend(e.to_string()))?;

            Ok(FrontendResource {
                backend: Box::new(webview),
                texture_format: TextureFormat::Bgra8UnormSrgb,
            })
        }

        #[cfg(not(feature = "cef"))]
        CompositeMode::Cef => {
            Err(FrontendError::Backend(
                "CEF mode requires the 'cef' feature. Build with: cargo build --features cef".into(),
            ))
        }

        CompositeMode::Dioxus => {
            // Dioxus mode uses a separate plugin with GPU-based rendering
            // Return an error here since Dioxus doesn't use the capture-based pipeline
            Err(FrontendError::Backend(
                "Dioxus mode uses DioxusRenderPlugin, not the capture pipeline".into(),
            ))
        }

        CompositeMode::Tauri => {
            // Tauri mode is handled differently - Bevy runs as WASM in Tauri's webview
            Err(FrontendError::Backend(
                "Tauri mode requires building for WASM and running inside Tauri".into(),
            ))
        }
    }
}

// ============================================================================
// Unified Systems
// ============================================================================

/// Initialize the frontend backend and UI overlay (startup system).
pub fn setup_frontend(world: &mut World) {
    let config = world.resource::<PentimentoConfig>();
    let mode = config.composite_mode;

    // Dioxus and Tauri modes use separate plugins
    if matches!(mode, CompositeMode::Dioxus | CompositeMode::Tauri) {
        return;
    }

    // Get window properties
    let (width, height, scale_factor, window_handle) = {
        let mut window_query = world.query::<(Entity, &Window)>();
        let Some((window_entity, window)) = window_query.iter(world).next() else {
            error!("No window found for frontend setup");
            return;
        };

        let resolution = &window.resolution;
        let size = (resolution.physical_width(), resolution.physical_height());
        let scale = f64::from(resolution.scale_factor());

        // Get raw window handle for overlay mode
        let handle = if mode == CompositeMode::Overlay {
            world
                .get::<RawHandleWrapper>(window_entity)
                .map(|wrapper| wrapper.get_window_handle())
        } else {
            None
        };

        (size.0, size.1, scale, handle)
    };

    info!(
        "Setting up frontend ({:?} mode, {}x{} physical, scale {:.2})",
        mode, width, height, scale_factor
    );

    // Get HTML content
    let html = UiAssets::get_html();

    // Create the frontend backend
    let frontend_config = FrontendConfig {
        html,
        size: (width, height),
        scale_factor,
        window_handle,
    };

    let frontend = match create_frontend(mode, frontend_config) {
        Ok(f) => f,
        Err(e) => {
            error!("Failed to create frontend: {}", e);
            return;
        }
    };

    let texture_format = frontend.texture_format;

    // Insert the frontend resource (NonSend because GTK is single-threaded)
    world.insert_non_send_resource(frontend);

    // Create the UI texture
    let mut image = Image::new_fill(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0], // Transparent
        texture_format,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );

    image.texture_descriptor.usage =
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;

    let texture_handle = world.resource_mut::<Assets<Image>>().add(image);

    world.insert_resource(UiTextureHandle {
        handle: texture_handle.clone(),
    });

    world.insert_resource(FrontendStatus {
        mode,
        ..Default::default()
    });

    world.insert_resource(LastWindowSize {
        width,
        height,
        scale_factor,
    });

    // Create full-screen UI overlay node
    world.spawn((
        ImageNode {
            image: texture_handle,
            ..default()
        },
        Node {
            width: Val::Vw(100.0),
            height: Val::Vh(100.0),
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            ..default()
        },
        ZIndex(i32::MAX),
        UiOverlay,
        Pickable::IGNORE,
    ));

    info!("Frontend initialized ({:?} mode)", mode);
}

/// Update the UI texture from the frontend capture (runs every frame).
///
/// This system:
/// 1. Polls the backend to process events and advance state
/// 2. Checks if the backend is ready
/// 3. Captures the framebuffer if dirty and uploads to the GPU texture
///
/// Handles all capture result types polymorphically:
/// - `Rgba`: Upload RGBA data directly
/// - `Bgra`: Upload BGRA data (Arc-wrapped for zero-copy when possible)
/// - `CompositorManaged`: No texture update needed (compositor handles blending)
pub fn update_ui_texture(
    frontend_res: Option<NonSendMut<FrontendResource>>,
    ui_texture: Option<Res<UiTextureHandle>>,
    mut images: ResMut<Assets<Image>>,
    mut status: ResMut<FrontendStatus>,
) {
    let Some(mut frontend) = frontend_res else {
        return;
    };
    let Some(ui_texture) = ui_texture else {
        return;
    };

    // Poll the backend to process events and advance state machine
    frontend.backend.poll();

    // Check if backend is ready
    if !frontend.backend.is_ready() {
        return;
    }

    if !status.initialized {
        info!("Frontend ready ({:?} mode), enabling captures", status.mode);
        status.initialized = true;
    }

    // Periodic heartbeat to force capture (ensures UI updates are visible)
    if status.last_capture.elapsed() >= CAPTURE_HEARTBEAT_INTERVAL {
        // For backends that support marking dirty externally, we'd call it here
        // Most backends handle this internally via their dirty flags
        status.last_capture = Instant::now();
    }

    // Capture and upload texture if dirty
    if let Some(capture_result) = frontend.backend.capture_if_dirty() {
        match capture_result {
            CaptureResult::Rgba(data, cap_width, cap_height) => {
                // RGBA format (WebKit/Capture mode)
                upload_texture_data(&mut images, &ui_texture.handle, data, cap_width, cap_height, &mut status);
            }

            CaptureResult::Bgra(arc_data, cap_width, cap_height) => {
                // BGRA format (CEF mode) - unwrap Arc to get owned Vec
                let bgra_data = Arc::try_unwrap(arc_data).unwrap_or_else(|arc| (*arc).clone());
                upload_texture_data(&mut images, &ui_texture.handle, bgra_data, cap_width, cap_height, &mut status);
            }

            CaptureResult::CompositorManaged => {
                // Compositor handles blending (Overlay mode)
                // No texture upload needed
            }
        }
    }
}

/// Upload captured data to the Bevy texture.
fn upload_texture_data(
    images: &mut Assets<Image>,
    handle: &Handle<Image>,
    data: Vec<u8>,
    width: u32,
    height: u32,
    status: &mut FrontendStatus,
) {
    if !status.first_capture_done {
        let non_transparent = data.chunks(4).filter(|p| p.len() == 4 && p[3] > 0).count();
        info!(
            "First capture ({:?} mode): {}x{}, non-transparent pixels: {}",
            status.mode, width, height, non_transparent
        );
        status.first_capture_done = true;
    }

    if let Some(image) = images.get_mut(handle) {
        // Resize texture if dimensions changed
        if image.width() != width || image.height() != height {
            info!(
                "Resizing texture from {}x{} to {}x{}",
                image.width(),
                image.height(),
                width,
                height
            );
            image.resize(Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            });
        }

        // Copy pixel data
        image.data = Some(data);
    }
}

/// Handle window resize for the frontend.
pub fn handle_frontend_resize(
    frontend_res: Option<NonSendMut<FrontendResource>>,
    ui_texture: Option<Res<UiTextureHandle>>,
    mut images: ResMut<Assets<Image>>,
    mut last_size: ResMut<LastWindowSize>,
    status: Res<FrontendStatus>,
    windows: Query<&Window>,
) {
    if !status.initialized {
        return;
    }

    let Some(mut frontend) = frontend_res else {
        return;
    };
    let Some(ui_texture) = ui_texture else {
        return;
    };
    let Ok(window) = windows.single() else {
        return;
    };

    let width = window.resolution.physical_width();
    let height = window.resolution.physical_height();
    let scale_factor = f64::from(window.resolution.scale_factor());

    // Check if size or scale changed
    let size_changed = width != last_size.width || height != last_size.height;
    let scale_changed = (scale_factor - last_size.scale_factor).abs() > f64::EPSILON;

    if !size_changed && !scale_changed {
        return;
    }

    if width == 0 || height == 0 {
        return;
    }

    info!(
        "Window resized to {}x{} physical (scale {:.2}), updating frontend",
        width, height, scale_factor
    );
    last_size.width = width;
    last_size.height = height;
    last_size.scale_factor = scale_factor;

    // Resize the backend
    frontend.backend.resize(width, height);

    // Resize the texture (only if using capture-based mode)
    if !matches!(status.mode, CompositeMode::Overlay) {
        if let Some(image) = images.get_mut(&ui_texture.handle) {
            image.resize(Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            });
        }
    }
}

// ============================================================================
// IPC Message Handling
// ============================================================================

/// Process IPC messages from the frontend (Capture, Overlay, and CEF modes).
///
/// This system forwards outbound messages (Bevy→UI) and processes inbound
/// messages (UI→Bevy) using the unified `FrontendResource` / `CompositeBackend`
/// trait, so it works identically for all capture-based backends.
fn handle_frontend_ipc_messages(world: &mut World) {
    // Send outbound messages to the UI first
    let outbound_msgs = {
        let Some(mut outbound) = world.get_resource_mut::<OutboundUiMessages>() else {
            return;
        };
        outbound.drain()
    };

    if !outbound_msgs.is_empty() {
        if let Some(mut frontend) = world.get_non_send_resource_mut::<FrontendResource>() {
            for msg in outbound_msgs {
                if let Err(e) = frontend.backend.send_to_ui(msg) {
                    warn!("Failed to send message to UI: {:?}", e);
                }
            }
        }
    }

    // Collect inbound messages (avoid borrow conflicts)
    let messages: Vec<UiToBevy> = {
        let Some(mut frontend) = world.get_non_send_resource_mut::<FrontendResource>() else {
            return;
        };
        let mut msgs = Vec::new();
        while let Some(msg) = frontend.backend.try_recv_from_ui() {
            msgs.push(msg);
        }
        msgs
    };

    // Process messages
    for msg in messages {
        match msg {
            UiToBevy::AddObject(request) => {
                if let Some(mut events) =
                    world.get_resource_mut::<bevy::ecs::message::Messages<AddObjectEvent>>()
                {
                    events.write(AddObjectEvent(request));
                    info!("Dispatched AddObjectEvent from UI");
                }
            }
            UiToBevy::AddPaintCanvas(request) => {
                if let Some(mut events) =
                    world.get_resource_mut::<bevy::ecs::message::Messages<CanvasPlaneEvent>>()
                {
                    events.write(CanvasPlaneEvent::CreateInFrontOfCamera {
                        width: request.width.unwrap_or(1024),
                        height: request.height.unwrap_or(1024),
                    });
                    info!("Dispatched CanvasPlaneEvent::CreateInFrontOfCamera from UI");
                }
            }
            UiToBevy::UiDirty => {
                // Already handled by dirty flag in webview
            }
            UiToBevy::UpdateLighting(settings) => {
                if let Some(mut lighting) = world.get_resource_mut::<SceneLighting>() {
                    lighting.settings = settings;
                    info!("Updated lighting settings from UI");
                }
            }
            UiToBevy::UpdateAmbientOcclusion(settings) => {
                if let Some(mut ao_resource) = world.get_resource_mut::<SceneAmbientOcclusion>() {
                    ao_resource.update(settings);
                    info!("Updated ambient occlusion settings from UI");
                }
            }
            UiToBevy::SetDepthView { enabled } => {
                if let Some(mut settings) = world.get_resource_mut::<DepthViewSettings>() {
                    settings.enabled = enabled;
                    info!("Depth view mode: {}", if enabled { "enabled" } else { "disabled" });
                }
            }
            _ => {
                debug!("Unhandled frontend IPC message: {:?}", msg);
            }
        }
    }
}

// ============================================================================
// Plugin
// ============================================================================

pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        let config = app.world().resource::<PentimentoConfig>();
        let mode = config.composite_mode;

        match mode {
            CompositeMode::Capture | CompositeMode::Overlay => {
                // Unified capture-based pipeline
                app.init_resource::<FrontendStatus>()
                    .init_resource::<LastWindowSize>()
                    .add_systems(Startup, setup_frontend)
                    .add_systems(Update, update_ui_texture)
                    .add_systems(Update, handle_frontend_resize)
                    .add_systems(Update, handle_frontend_ipc_messages);

                info!("Render plugin initialized with {:?} mode (unified pipeline)", mode);
            }

            #[cfg(feature = "cef")]
            CompositeMode::Cef => {
                // CEF also uses the unified pipeline
                app.init_resource::<FrontendStatus>()
                    .init_resource::<LastWindowSize>()
                    .add_systems(Startup, setup_frontend)
                    .add_systems(Update, update_ui_texture)
                    .add_systems(Update, handle_frontend_resize)
                    .add_systems(Update, handle_frontend_ipc_messages);

                info!("Render plugin initialized with CEF mode (unified pipeline)");
            }

            #[cfg(not(feature = "cef"))]
            CompositeMode::Cef => {
                error!("CEF mode requires the 'cef' feature. Build with: cargo build --features cef");
                panic!("CEF mode not available - rebuild with --features cef");
            }

            #[cfg(feature = "dioxus")]
            CompositeMode::Dioxus => {
                // Dioxus uses its own specialized plugin with GPU-based rendering
                app.add_plugins(ui_dioxus::DioxusRenderPlugin);

                info!("Render plugin initialized with DIOXUS mode (Vello zero-copy GPU renderer)");
            }

            #[cfg(not(feature = "dioxus"))]
            CompositeMode::Dioxus => {
                error!("Dioxus mode requires the 'dioxus' feature. Build with: cargo build --features dioxus");
                panic!("Dioxus mode not available - rebuild with --features dioxus");
            }

            CompositeMode::Tauri => {
                // Tauri mode - Bevy runs as WASM in Tauri's webview
                warn!("Tauri mode requires building for WASM and running inside Tauri");
                info!("Render plugin: Tauri mode - no native render setup needed");
            }
        }
    }
}
