//! Parent window synchronization for overlay mode
//!
//! Handles the relationship between the overlay window and its parent (Bevy) window,
//! including transient-for hints and visibility synchronization.

use gdk::prelude::*;
use gdkx11::{X11Display, X11Window};
use gtk::prelude::*;
use raw_window_handle::RawWindowHandle;

/// Set up the window relationship with the parent (transient, grouping) and position
/// Returns the parent window XID for state tracking (X11 only)
pub fn setup_window_relationship(
    window: &gtk::Window,
    parent_handle: RawWindowHandle,
) -> Option<u64> {
    let mut parent_xid: Option<u64> = None;

    match parent_handle {
        RawWindowHandle::Xlib(handle) => {
            let xid = handle.window as u64;
            tracing::info!("X11 parent window (Xlib): {:?}", xid);
            parent_xid = Some(xid);
            window.move_(0, 0);
            set_transient_for_x11(window, xid);
        }
        RawWindowHandle::Xcb(handle) => {
            let xid = handle.window.get() as u64;
            tracing::info!("X11 parent window (XCB): {:?}", xid);
            parent_xid = Some(xid);
            window.move_(0, 0);
            set_transient_for_x11(window, xid);
        }
        RawWindowHandle::Wayland(_handle) => {
            // Wayland positioning is more complex due to security model
            // The compositor controls window positioning
            // Transient relationships work differently on Wayland
            tracing::info!("Wayland parent window - positioning controlled by compositor");
        }
        _ => {
            tracing::warn!("Unknown window handle type for parent positioning");
        }
    }

    parent_xid
}

/// Set the overlay window as transient for the parent X11 window
/// This makes the overlay minimize/restore with the parent window
fn set_transient_for_x11(window: &gtk::Window, parent_xid: u64) {
    // We need to realize the window first to get its GDK window
    window.realize();

    let Some(gdk_window) = window.window() else {
        tracing::warn!("Could not get GDK window for transient setup");
        return;
    };

    // Get the display and try to cast it to X11Display
    let display = gdk_window.display();
    let Some(x11_display) = display.downcast_ref::<X11Display>() else {
        tracing::warn!("Not running on X11 display");
        return;
    };

    // Get the parent window as an X11Window and set transient relationship
    let parent_x11_window = X11Window::foreign_new_for_display(x11_display, parent_xid);
    let parent_gdk: gdk::Window = parent_x11_window.upcast();

    // Set the transient-for relationship - this tells the WM to minimize together
    gdk_window.set_transient_for(&parent_gdk);

    // Also set type hint to indicate this is a utility window
    gdk_window.set_type_hint(gdk::WindowTypeHint::Utility);

    tracing::info!("Overlay set as transient for parent window {}", parent_xid);
}

/// Check parent window visibility via X11 and sync overlay visibility
/// Returns true if the overlay visibility was changed
pub fn check_parent_visibility(
    window: &gtk::Window,
    parent_xid: Option<u64>,
) -> bool {
    let Some(parent_xid) = parent_xid else {
        return false; // No parent tracking on non-X11
    };

    let Some(gdk_window) = window.window() else {
        return false;
    };

    // Get the display and try to cast it to X11Display
    let display = gdk_window.display();
    let Some(x11_display) = display.downcast_ref::<X11Display>() else {
        return false; // Not running on X11
    };

    // Get the parent window as an X11Window using foreign_new_for_display
    let parent_x11_window = X11Window::foreign_new_for_display(x11_display, parent_xid);

    // Cast to gdk::Window to access state() method
    let parent_gdk: gdk::Window = parent_x11_window.upcast();

    // Check if parent is iconified (minimized) or withdrawn
    let parent_state = parent_gdk.state();
    let is_iconified = parent_state.contains(gdk::WindowState::ICONIFIED);
    let is_withdrawn = parent_state.contains(gdk::WindowState::WITHDRAWN);
    let parent_hidden = is_iconified || is_withdrawn;

    // Sync our visibility to match
    let currently_visible = window.is_visible();
    if parent_hidden && currently_visible {
        window.hide();
        tracing::debug!("Overlay hidden (parent iconified/withdrawn)");
        return true;
    } else if !parent_hidden && !currently_visible {
        window.show();
        tracing::debug!("Overlay shown (parent visible)");
        return true;
    }

    false
}

/// Sync overlay visibility with the given parent window visibility state
/// Called externally when the parent window state changes
pub fn sync_visibility(window: &gtk::Window, parent_visible: bool) {
    let currently_visible = window.is_visible();
    if !parent_visible && currently_visible {
        window.hide();
        tracing::debug!("Overlay hidden (parent minimized)");
    } else if parent_visible && !currently_visible {
        window.show();
        tracing::debug!("Overlay shown (parent restored)");
    }
}
