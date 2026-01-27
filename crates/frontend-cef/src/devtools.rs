//! DevTools support (Ctrl+Shift+I)
//!
//! This module provides Chrome DevTools integration for debugging the CEF webview.

use cef::{Browser, BrowserSettings, ImplBrowser, ImplBrowserHost, WindowInfo};

/// Open Chrome DevTools for debugging the webview
///
/// Creates a new window containing the Chrome DevTools inspector
/// for debugging the webview's HTML, CSS, JavaScript, and network activity.
pub fn show_dev_tools(browser: &Browser) {
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

/// Close the DevTools window if it's open
pub fn close_dev_tools(browser: &Browser) {
    let Some(host) = browser.host() else {
        tracing::warn!("Cannot close DevTools: no browser host");
        return;
    };

    tracing::info!("Closing CEF DevTools window");
    host.close_dev_tools();
}

/// Check if the DevTools window is currently open
pub fn has_dev_tools(browser: &Browser) -> bool {
    browser
        .host()
        .map(|host| host.has_dev_tools() != 0)
        .unwrap_or(false)
}

/// Toggle DevTools visibility (Ctrl+Shift+I behavior)
///
/// If DevTools is open, closes it. Otherwise, opens it.
pub fn toggle_dev_tools(browser: &Browser) {
    if has_dev_tools(browser) {
        close_dev_tools(browser);
    } else {
        show_dev_tools(browser);
    }
}
