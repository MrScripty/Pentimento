//! UI Overlay Mode - Renders webview as transparent child window
//!
//! This module creates a transparent GTK window that overlays the Bevy window.
//! The desktop compositor handles blending, avoiding framebuffer capture overhead.

use bevy::prelude::*;
use bevy::window::RawHandleWrapper;
use pentimento_webview::OverlayWebview;

use crate::embedded_ui::UiAssets;

/// Non-Send resource holding the overlay webview (GTK is single-threaded)
pub struct OverlayWebviewResource {
    pub webview: OverlayWebview,
}

/// Track overlay webview initialization state
#[derive(Resource, Default)]
pub struct OverlayStatus {
    pub initialized: bool,
}

/// Track window size for resize handling
#[derive(Resource, Default)]
pub struct OverlayLastWindowSize {
    pub width: u32,
    pub height: u32,
}

/// Initialize the overlay webview
pub fn setup_ui_overlay(world: &mut World) {
    // Get window entity and properties
    let window_entity = world
        .query_filtered::<Entity, With<bevy::window::PrimaryWindow>>()
        .iter(world)
        .next();

    let Some(window_entity) = window_entity else {
        error!("No primary window found for overlay setup");
        return;
    };

    // Get window size
    let (width, height) = {
        let window = world.get::<Window>(window_entity).unwrap();
        (window.resolution.width() as u32, window.resolution.height() as u32)
    };

    info!("Setting up UI overlay system ({}x{})", width, height);

    // Get the raw window handle from Bevy
    let raw_handle = world.get::<RawHandleWrapper>(window_entity);
    let Some(raw_handle) = raw_handle else {
        error!("No RawHandleWrapper found on primary window");
        return;
    };

    // Extract the raw window handle using the public getter method
    let window_handle = raw_handle.get_window_handle();

    // Get HTML content for the webview
    let html = UiAssets::get_html();

    // Create the overlay webview
    match OverlayWebview::new(window_handle, &html, (width, height)) {
        Ok(webview) => {
            world.insert_non_send_resource(OverlayWebviewResource { webview });
            info!("Overlay webview created successfully");
        }
        Err(e) => {
            error!("Failed to create overlay webview: {}", e);
            return;
        }
    }

    // Initialize tracking resources
    world.insert_resource(OverlayStatus { initialized: false });
    world.insert_resource(OverlayLastWindowSize { width, height });

    info!("UI overlay setup complete");
}

/// Poll the overlay webview and handle window tracking
pub fn update_overlay_webview(
    overlay_res: Option<NonSendMut<OverlayWebviewResource>>,
    mut status: ResMut<OverlayStatus>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
) {
    let Some(mut overlay) = overlay_res else {
        return;
    };

    // Poll GTK events
    overlay.webview.poll();

    // Check ready state
    if !overlay.webview.is_ready() {
        return;
    }

    if !status.initialized {
        info!("Overlay webview ready");
        status.initialized = true;
    }

    // Get current window position for overlay tracking
    // Note: Bevy doesn't provide window position directly,
    // so the overlay may need additional platform-specific code
    // to track window moves on X11/Wayland
    let _window = windows.single();
}

/// Handle window resize for overlay mode
pub fn handle_overlay_resize(
    overlay_res: Option<NonSendMut<OverlayWebviewResource>>,
    mut last_size: ResMut<OverlayLastWindowSize>,
    status: Res<OverlayStatus>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
) {
    if !status.initialized {
        return;
    }

    let Some(mut overlay) = overlay_res else {
        return;
    };

    let Ok(window) = windows.single() else {
        return;
    };

    let width = window.resolution.width() as u32;
    let height = window.resolution.height() as u32;

    if width == last_size.width && height == last_size.height {
        return;
    }

    if width == 0 || height == 0 {
        return;
    }

    info!("Window resized to {}x{}, updating overlay", width, height);
    last_size.width = width;
    last_size.height = height;

    overlay.webview.resize(width, height);
}
