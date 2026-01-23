//! UI Compositing - Renders the webview framebuffer over the 3D scene
//!
//! This module handles capturing the offscreen webview's framebuffer and
//! displaying it as a transparent overlay on top of the Bevy 3D scene.

use bevy::asset::RenderAssetUsages;
use bevy::picking::prelude::Pickable;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use pentimento_webview::OffscreenWebview;
use std::time::{Duration, Instant};

use crate::embedded_ui::UiAssets;

/// Non-Send resource holding the offscreen webview (GTK is single-threaded)
pub struct WebviewResource {
    pub webview: OffscreenWebview,
}

/// Resource holding the UI texture handle
#[derive(Resource)]
pub struct UiTextureHandle {
    pub handle: Handle<Image>,
}

/// Marker component for the UI overlay node
#[derive(Component)]
pub struct UiOverlay;

/// Track webview initialization state for Bevy systems
#[derive(Resource)]
pub struct WebviewStatus {
    pub initialized: bool,
    pub first_capture_done: bool,
    pub last_capture: Instant,
}

impl Default for WebviewStatus {
    fn default() -> Self {
        Self {
            initialized: false,
            first_capture_done: false,
            last_capture: Instant::now(),
        }
    }
}

const CAPTURE_HEARTBEAT_INTERVAL: Duration = Duration::from_millis(16);

/// Initialize the webview and UI overlay
pub fn setup_ui_composite(world: &mut World) {
    let (width, height, scale_factor) = {
        let mut window_query = world.query::<&Window>();
        let window = window_query.iter(world).next().expect("No window found");
        let resolution = &window.resolution;
        (
            resolution.physical_width(),
            resolution.physical_height(),
            f64::from(resolution.scale_factor()),
        )
    };

    info!(
        "Setting up UI composite system ({}x{} physical, scale {:.2})",
        width, height, scale_factor
    );

    // Get HTML content for the webview
    let html = UiAssets::get_html();

    // Create the offscreen webview
    match OffscreenWebview::new(&html, (width, height)) {
        Ok(mut webview) => {
            webview.set_scale_factor(scale_factor);
            world.insert_non_send_resource(WebviewResource { webview });
            info!("Offscreen webview created successfully");
        }
        Err(e) => {
            error!("Failed to create offscreen webview: {}", e);
            return;
        }
    }

    // Create an initial transparent texture for the UI overlay
    // Use Rgba8UnormSrgb for proper gamma-corrected display
    let mut image = Image::new_fill(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0], // Transparent
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );

    // Enable texture to be updated and sampled
    image.texture_descriptor.usage =
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;

    let texture_handle = world.resource_mut::<Assets<Image>>().add(image);

    world.insert_resource(UiTextureHandle {
        handle: texture_handle.clone(),
    });

    // Initialize the last window size to prevent immediate resize detection
    world.insert_resource(LastWindowSize {
        width,
        height,
        scale_factor,
    });

    // Create a full-screen UI node with the texture
    // We use a single ImageNode that fills the entire screen
    // Pickable::IGNORE allows raycasts to pass through to 3D meshes for selection
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
        // Make sure it's on top of everything
        ZIndex(i32::MAX),
        UiOverlay,
        Pickable::IGNORE,
    ));

    info!("UI overlay created");
}

/// Update the UI texture from the webview capture
pub fn update_ui_texture(
    webview_res: Option<NonSendMut<WebviewResource>>,
    ui_texture: Option<Res<UiTextureHandle>>,
    mut images: ResMut<Assets<Image>>,
    mut status: ResMut<WebviewStatus>,
) {
    let Some(mut webview_res) = webview_res else {
        return;
    };
    let Some(ui_texture) = ui_texture else {
        return;
    };

    // Poll the webview to process GTK events and advance state machine
    webview_res.webview.poll();

    // Check ready state
    if !webview_res.webview.is_ready() {
        // Still warming up - don't attempt capture
        return;
    }

    if !status.initialized {
        info!("Webview ready, enabling captures");
        status.initialized = true;
    }

    if status.last_capture.elapsed() >= CAPTURE_HEARTBEAT_INTERVAL {
        webview_res.webview.mark_dirty();
        status.last_capture = Instant::now();
    }

    // Try to capture if dirty
    if let Some(captured) = webview_res.webview.capture_if_dirty() {
        let cap_width = captured.width();
        let cap_height = captured.height();

        // Count non-transparent pixels for debugging
        let non_transparent = captured.pixels().filter(|p| p.0[3] > 0).count();

        if !status.first_capture_done {
            info!(
                "First successful capture! {}x{}, non-transparent pixels: {}",
                cap_width, cap_height, non_transparent
            );
            status.first_capture_done = true;
        }

        // Update the Bevy texture with the captured framebuffer
        if let Some(image) = images.get_mut(&ui_texture.handle) {
            // Resize if needed
            if image.width() != cap_width || image.height() != cap_height {
                info!(
                    "Resizing texture from {}x{} to {}x{}",
                    image.width(),
                    image.height(),
                    cap_width,
                    cap_height
                );
                image.resize(Extent3d {
                    width: cap_width,
                    height: cap_height,
                    depth_or_array_layers: 1,
                });
            }

            // Copy the RGBA data
            image.data = Some(captured.into_raw());
        }
    }
}

/// Resource to track the last known window size
#[derive(Resource, Default)]
pub struct LastWindowSize {
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
}

/// Handle window resize by polling current size
pub fn handle_window_resize(
    webview_res: Option<NonSendMut<WebviewResource>>,
    ui_texture: Option<Res<UiTextureHandle>>,
    mut images: ResMut<Assets<Image>>,
    mut last_size: ResMut<LastWindowSize>,
    status: Res<WebviewStatus>,
    windows: Query<&Window>,
) {
    // Don't process resize during warmup phase
    if !status.initialized {
        return;
    }

    let Some(mut webview_res) = webview_res else {
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
    let scale_changed = scale_factor != last_size.scale_factor;
    if !size_changed && !scale_changed {
        return;
    }

    if width == 0 || height == 0 {
        return;
    }

    info!(
        "Window resized to {}x{} physical (scale {:.2}), updating webview",
        width, height, scale_factor
    );
    last_size.width = width;
    last_size.height = height;
    last_size.scale_factor = scale_factor;

    if scale_changed {
        webview_res.webview.set_scale_factor(scale_factor);
    }
    webview_res.webview.resize(width, height);

    // Resize the texture
    if size_changed {
        if let Some(image) = images.get_mut(&ui_texture.handle) {
            image.resize(Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            });
        }
    }
}
