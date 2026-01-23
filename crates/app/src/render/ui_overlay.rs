//! UI Overlay Mode - Renders webview as transparent child window
//!
//! This module creates a transparent GTK window that overlays the Bevy window.
//! The desktop compositor handles blending, avoiding framebuffer capture overhead.

use bevy::prelude::*;
use bevy::window::{RawHandleWrapper, WindowMoved, WindowOccluded};
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
    /// Frames to wait before creating the overlay (allows Bevy window to stabilize)
    pub startup_delay_frames: u32,
    /// Whether the overlay has been created yet
    pub created: bool,
}

/// Track window size for resize handling
#[derive(Resource, Default)]
pub struct OverlayLastWindowSize {
    pub width: u32,
    pub height: u32,
}

/// Track overlay window position for sync with Bevy window
#[derive(Resource, Default)]
pub struct OverlayPosition {
    pub x: i32,
    pub y: i32,
}

/// Initialize overlay tracking resources (defers actual webview creation)
pub fn setup_ui_overlay(world: &mut World) {
    // Get window size for tracking
    let (width, height) = {
        let mut window_query = world.query::<&Window>();
        if let Some(window) = window_query.iter(world).next() {
            (
                window.resolution.physical_width(),
                window.resolution.physical_height(),
            )
        } else {
            (1920, 1080) // Default, will be updated
        }
    };

    info!("Setting up UI overlay system (deferred creation, {}x{})", width, height);

    // Initialize tracking resources - webview creation is deferred to avoid GLX conflicts
    // The overlay will be created after a few frames when the Bevy window is stable
    world.insert_resource(OverlayStatus {
        initialized: false,
        startup_delay_frames: 5, // Wait 5 frames for window to stabilize
        created: false,
    });
    world.insert_resource(OverlayLastWindowSize { width, height });
    world.insert_resource(OverlayPosition::default());

    info!("UI overlay setup complete (webview creation deferred)");
}

/// Actually create the overlay webview (called after startup delay)
fn create_overlay_webview(world: &mut World) -> bool {
    // Get window entity and properties
    let window_entity = world
        .query_filtered::<Entity, With<bevy::window::PrimaryWindow>>()
        .iter(world)
        .next();

    let Some(window_entity) = window_entity else {
        error!("No primary window found for overlay creation");
        return false;
    };

    // Get window size - use PHYSICAL size for overlay since GTK operates in physical pixels
    let (width, height) = {
        let window = world.get::<Window>(window_entity).unwrap();
        (
            window.resolution.physical_width(),
            window.resolution.physical_height(),
        )
    };

    info!("Creating overlay webview ({}x{})", width, height);

    // Get the raw window handle from Bevy
    let raw_handle = world.get::<RawHandleWrapper>(window_entity);
    let Some(raw_handle) = raw_handle else {
        error!("No RawHandleWrapper found on primary window");
        return false;
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

            // Update size tracking
            if let Some(mut last_size) = world.get_resource_mut::<OverlayLastWindowSize>() {
                last_size.width = width;
                last_size.height = height;
            }
            true
        }
        Err(e) => {
            error!("Failed to create overlay webview: {}", e);
            false
        }
    }
}

/// Poll the overlay webview and handle window tracking
pub fn update_overlay_webview(world: &mut World) {
    // Check if we need to create the webview (deferred creation)
    let should_create = {
        let mut status = world.resource_mut::<OverlayStatus>();
        if !status.created {
            if status.startup_delay_frames > 0 {
                status.startup_delay_frames -= 1;
                return;
            }
            true
        } else {
            false
        }
    };

    if should_create {
        let success = create_overlay_webview(world);
        let mut status = world.resource_mut::<OverlayStatus>();
        status.created = success;
        if !success {
            return;
        }
    }

    // Now poll the webview if it exists
    let overlay_res = world.get_non_send_resource_mut::<OverlayWebviewResource>();
    let Some(mut overlay) = overlay_res else {
        return;
    };

    // Poll GTK events
    overlay.webview.poll();

    // Check ready state
    if !overlay.webview.is_ready() {
        return;
    }

    drop(overlay);

    let mut status = world.resource_mut::<OverlayStatus>();
    if !status.initialized {
        info!("Overlay webview ready");
        status.initialized = true;
    }
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

    // Use PHYSICAL size for overlay since GTK operates in physical pixels
    let width = window.resolution.physical_width();
    let height = window.resolution.physical_height();

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

/// Sync overlay position when Bevy window moves
pub fn sync_overlay_position(
    mut moved_events: MessageReader<WindowMoved>,
    overlay_res: Option<NonSendMut<OverlayWebviewResource>>,
    mut position: ResMut<OverlayPosition>,
    status: Res<OverlayStatus>,
) {
    // Don't process moves until overlay is ready
    if !status.initialized {
        // Still consume events to avoid backlog
        moved_events.clear();
        return;
    }

    let Some(mut overlay) = overlay_res else {
        moved_events.clear();
        return;
    };

    for event in moved_events.read() {
        let new_x = event.position.x;
        let new_y = event.position.y;

        // Only update if position changed
        if new_x != position.x || new_y != position.y {
            position.x = new_x;
            position.y = new_y;

            overlay.webview.set_position(new_x, new_y);
            info!("Overlay synced to position ({}, {})", new_x, new_y);
        }
    }
}

/// Sync overlay visibility when Bevy window is minimized/restored (occluded/unoccluded)
pub fn sync_overlay_visibility(
    mut occluded_events: MessageReader<WindowOccluded>,
    overlay_res: Option<NonSendMut<OverlayWebviewResource>>,
    status: Res<OverlayStatus>,
) {
    // Don't process until overlay is ready
    if !status.initialized {
        occluded_events.clear();
        return;
    }

    let Some(mut overlay) = overlay_res else {
        occluded_events.clear();
        return;
    };

    for event in occluded_events.read() {
        // When window is occluded (minimized/hidden), hide overlay
        // When window is visible again, show overlay
        let parent_visible = !event.occluded;
        overlay.webview.sync_visibility(parent_visible);
        info!(
            "Overlay visibility synced: parent_visible={}",
            parent_visible
        );
    }
}
