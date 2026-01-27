//! Transparent window creation and positioning for overlay mode
//!
//! Provides functionality for creating transparent GTK windows that can overlay
//! the Bevy rendering window. The compositor handles the actual blending.

use gdk::cairo;
use gdk::prelude::*;
use gtk::prelude::*;

use crate::OverlayError;

/// UI layout constants matching Svelte CSS
pub const TOOLBAR_HEIGHT: i32 = 72; // 48px + padding for dropdowns
pub const SIDEBAR_WIDTH: i32 = 316; // 300px + margins
pub const SIDEBAR_TOP: i32 = 56;
pub const SIDEBAR_MARGIN: i32 = 8;

/// Create a transparent GTK toplevel window configured for overlay use
pub fn create_transparent_window(size: (u32, u32)) -> Result<gtk::Window, OverlayError> {
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_decorated(false);
    window.set_app_paintable(true);
    window.set_default_size(size.0 as i32, size.1 as i32);

    // Prevent the overlay from stealing focus (avoids winit XIM unfocus errors)
    window.set_accept_focus(false);
    window.set_focus_on_map(false);
    window.set_skip_taskbar_hint(true);
    window.set_skip_pager_hint(true);

    // Enable RGBA visual for transparency
    if let Some(screen) = gtk::prelude::WidgetExt::screen(&window) {
        if let Some(visual) = screen.rgba_visual() {
            window.set_visual(Some(&visual));
            tracing::info!("RGBA visual enabled for transparent overlay");
        } else {
            tracing::warn!("No RGBA visual available - transparency may not work");
        }
    }

    // Set up transparent background via CSS
    let css_provider = gtk::CssProvider::new();
    css_provider
        .load_from_data(b"window { background-color: transparent; }")
        .map_err(|e| OverlayError::WindowCreate(format!("CSS load failed: {}", e)))?;

    if let Some(screen) = gtk::prelude::WidgetExt::screen(&window) {
        gtk::StyleContext::add_provider_for_screen(
            &screen,
            &css_provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    Ok(window)
}

/// Update input regions to allow selective passthrough
///
/// Creates an input shape that covers only UI elements (toolbar, sidebar),
/// making the 3D viewport area click-through to Bevy underneath.
pub fn update_input_regions(window: &gtk::Window, width: u32, height: u32) {
    let Some(gdk_window) = window.window() else {
        tracing::warn!("Could not get GDK window for input region setup");
        return;
    };

    let w = width as i32;
    let h = height as i32;

    // Create a region covering only the UI elements
    let region = cairo::Region::create();

    // Add toolbar rectangle (full width, at top)
    let toolbar_rect = cairo::RectangleInt::new(0, 0, w, TOOLBAR_HEIGHT);
    let _ = region.union_rectangle(&toolbar_rect);

    // Add sidebar rectangle (right side, below toolbar)
    let sidebar_rect = cairo::RectangleInt::new(
        w - SIDEBAR_WIDTH,
        SIDEBAR_TOP,
        SIDEBAR_WIDTH,
        h - SIDEBAR_TOP - SIDEBAR_MARGIN,
    );
    let _ = region.union_rectangle(&sidebar_rect);

    // Set the input shape - only these regions will receive input
    // Everything else (the 3D viewport) will be click-through
    gdk_window.input_shape_combine_region(&region, 0, 0);

    tracing::debug!(
        "Input regions set: toolbar (0,0,{},{}), sidebar ({},{},{},{})",
        w,
        TOOLBAR_HEIGHT,
        w - SIDEBAR_WIDTH,
        SIDEBAR_TOP,
        SIDEBAR_WIDTH,
        h - SIDEBAR_TOP - SIDEBAR_MARGIN
    );
}

/// Position the overlay window at the given coordinates
pub fn set_position(window: &gtk::Window, x: i32, y: i32) {
    window.move_(x, y);
}

/// Resize the overlay window
pub fn resize_window(window: &gtk::Window, width: u32, height: u32) {
    window.resize(width as i32, height as i32);
}
