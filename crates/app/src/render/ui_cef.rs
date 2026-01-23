//! CEF UI Compositing - Renders CEF webview framebuffer over the 3D scene
//!
//! This module handles capturing the CEF offscreen webview's framebuffer and
//! displaying it as a transparent overlay on top of the Bevy 3D scene.
//!
//! Similar to ui_composite.rs but uses CEF (Chromium) instead of WebKitGTK.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use pentimento_webview::CefWebview;
use std::time::Instant;

use crate::embedded_ui::UiAssets;

/// Non-Send resource holding the CEF webview
pub struct CefWebviewResource {
    pub webview: CefWebview,
}

/// Resource holding the UI texture handle
#[derive(Resource)]
pub struct CefUiTextureHandle {
    pub handle: Handle<Image>,
}

/// Marker component for the UI overlay node
#[derive(Component)]
pub struct CefUiOverlay;

/// Track CEF webview initialization state
#[derive(Resource, Default)]
pub struct CefWebviewStatus {
    pub initialized: bool,
    pub first_capture_done: bool,
}

/// Resource to track the last known window size
#[derive(Resource, Default)]
pub struct CefLastWindowSize {
    pub width: u32,
    pub height: u32,
}

/// Initialize the CEF webview and UI overlay
pub fn setup_ui_cef(world: &mut World) {
    // Use PHYSICAL resolution for sharp rendering on HiDPI displays
    // Logical resolution would cause fuzzy/blurry UI when scaled up
    let (width, height) = {
        let mut window_query = world.query::<&Window>();
        let window = window_query.iter(world).next().expect("No window found");
        (
            window.resolution.physical_width(),
            window.resolution.physical_height(),
        )
    };

    info!("Setting up CEF UI composite system ({}x{} physical)", width, height);

    // Get HTML content for the webview
    let html = UiAssets::get_html();

    // Create the CEF offscreen webview
    match CefWebview::new(&html, (width, height)) {
        Ok(webview) => {
            world.insert_non_send_resource(CefWebviewResource { webview });
            info!("CEF webview created successfully");
        }
        Err(e) => {
            error!("Failed to create CEF webview: {}", e);
            return;
        }
    }

    // Create an initial transparent texture for the UI overlay
    // Use BGRA format to match CEF's native output - avoids expensive CPU conversion
    let mut image = Image::new_fill(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0], // Transparent (BGRA)
        TextureFormat::Bgra8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );

    image.texture_descriptor.usage =
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;

    let texture_handle = world.resource_mut::<Assets<Image>>().add(image);

    world.insert_resource(CefUiTextureHandle {
        handle: texture_handle.clone(),
    });

    world.insert_resource(CefLastWindowSize { width, height });

    // Create a full-screen UI node with the texture
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
        CefUiOverlay,
    ));

    info!("CEF UI overlay created");
}

/// Update the UI texture from the CEF webview capture
pub fn update_cef_ui_texture(
    webview_res: Option<NonSendMut<CefWebviewResource>>,
    ui_texture: Option<Res<CefUiTextureHandle>>,
    mut images: ResMut<Assets<Image>>,
    mut status: ResMut<CefWebviewStatus>,
) {
    let Some(mut webview_res) = webview_res else {
        return;
    };
    let Some(ui_texture) = ui_texture else {
        return;
    };

    // Poll CEF message loop
    webview_res.webview.poll();

    // Check ready state
    if !webview_res.webview.is_ready() {
        return;
    }

    if !status.initialized {
        info!("CEF webview ready, enabling captures");
        status.initialized = true;
    }

    // PERFORMANCE INSTRUMENTATION: Time the entire capture-to-texture pipeline
    let capture_start = Instant::now();

    // Try to capture if dirty - returns raw BGRA bytes with dimensions
    if let Some((bgra_data, cap_width, cap_height)) = webview_res.webview.capture_if_dirty() {
        let capture_elapsed = capture_start.elapsed();

        // Count non-transparent pixels (alpha is at offset 3 in BGRA)
        let non_transparent = bgra_data.chunks(4).filter(|p| p[3] > 0).count();

        if !status.first_capture_done {
            info!(
                "First CEF capture! {}x{}, non-transparent pixels: {}, format: BGRA (no conversion)",
                cap_width, cap_height, non_transparent
            );
            status.first_capture_done = true;
        }

        let texture_start = Instant::now();
        if let Some(image) = images.get_mut(&ui_texture.handle) {
            if image.width() != cap_width || image.height() != cap_height {
                info!(
                    "Resizing CEF texture from {}x{} to {}x{}",
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

            // Direct upload of BGRA data - no conversion needed!
            image.data = Some(bgra_data);
        }
        let texture_elapsed = texture_start.elapsed();

        let total_elapsed = capture_start.elapsed();
        if total_elapsed.as_millis() > 2 {
            warn!(
                "CEF texture update PERF: capture: {:.2}ms, texture_update: {:.2}ms, total: {:.2}ms",
                capture_elapsed.as_secs_f64() * 1000.0,
                texture_elapsed.as_secs_f64() * 1000.0,
                total_elapsed.as_secs_f64() * 1000.0
            );
        }
    }
}

/// Handle window resize for CEF mode
pub fn handle_cef_window_resize(
    webview_res: Option<NonSendMut<CefWebviewResource>>,
    ui_texture: Option<Res<CefUiTextureHandle>>,
    mut images: ResMut<Assets<Image>>,
    mut last_size: ResMut<CefLastWindowSize>,
    status: Res<CefWebviewStatus>,
    windows: Query<&Window>,
) {
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

    // Use PHYSICAL resolution for sharp rendering on HiDPI displays
    let width = window.resolution.physical_width();
    let height = window.resolution.physical_height();

    if width == last_size.width && height == last_size.height {
        return;
    }

    if width == 0 || height == 0 {
        return;
    }

    info!("Window resized to {}x{} physical, updating CEF webview", width, height);
    last_size.width = width;
    last_size.height = height;

    webview_res.webview.resize(width, height);

    if let Some(image) = images.get_mut(&ui_texture.handle) {
        image.resize(Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        });
    }
}
