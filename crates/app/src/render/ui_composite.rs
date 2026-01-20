//! UI Compositing - Renders the webview framebuffer over the 3D scene
//!
//! This module handles capturing the offscreen webview's framebuffer and
//! displaying it as a transparent overlay on top of the Bevy 3D scene.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use pentimento_webview::OffscreenWebview;

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
#[derive(Resource, Default)]
pub struct WebviewStatus {
    pub initialized: bool,
    pub first_capture_done: bool,
}

/// Initialize the webview and UI overlay
pub fn setup_ui_composite(world: &mut World) {
    let (width, height) = {
        let mut window_query = world.query::<&Window>();
        let window = window_query.iter(world).next().expect("No window found");
        // Use resolution (logical size) rather than physical size to avoid DPI scaling issues
        // The physical size may change as the window surface is configured
        let resolution = &window.resolution;
        (resolution.width() as u32, resolution.height() as u32)
    };

    info!("Setting up UI composite system ({}x{})", width, height);

    // Get HTML content for the webview
    let html = UiAssets::get_html();

    // Create the offscreen webview
    match OffscreenWebview::new(&html, (width, height)) {
        Ok(webview) => {
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
    world.insert_resource(LastWindowSize { width, height });

    // Create a full-screen UI node with the texture
    // We use a single ImageNode that fills the entire screen
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

    // Use resolution (logical size) to match the initial setup
    let width = window.resolution.width() as u32;
    let height = window.resolution.height() as u32;

    // Check if size changed
    if width == last_size.width && height == last_size.height {
        return;
    }

    if width == 0 || height == 0 {
        return;
    }

    info!("Window resized to {}x{}, updating webview", width, height);
    last_size.width = width;
    last_size.height = height;

    // Resize the webview
    webview_res.webview.resize(width, height);

    // Resize the texture
    if let Some(image) = images.get_mut(&ui_texture.handle) {
        image.resize(Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        });
    }
}
