//! Windows-specific webview implementation using WebView2

use crate::error::WebviewError;
use pentimento_ipc::{KeyboardEvent, MouseEvent, UiToBevy};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Windows webview implementation using WebView2
pub struct WindowsWebview {
    // TODO: Implement Windows-specific webview
    _size: (u32, u32),
}

impl WindowsWebview {
    pub fn new(
        _html_content: &str,
        size: (u32, u32),
        _dirty: Arc<AtomicBool>,
        _from_ui_tx: mpsc::UnboundedSender<UiToBevy>,
    ) -> Result<Self, WebviewError> {
        // TODO: Implement WebView2 initialization
        // This would use the windows crate to create a hidden window
        // and initialize WebView2 for offscreen rendering

        Ok(Self { _size: size })
    }

    pub fn poll(&mut self) {
        // TODO: Windows message pump
    }

    pub fn capture(&self) -> Option<image::RgbaImage> {
        // TODO: Use WebView2 CapturePreview API
        None
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self._size = (width, height);
    }

    pub fn set_scale_factor(&mut self, _scale_factor: f64) {
        // TODO: Apply scaling once Windows WebView2 implementation is available.
    }

    pub fn inject_mouse(&mut self, _event: MouseEvent) {
        // TODO: Use SendInput or WebView2 input injection
    }

    pub fn inject_keyboard(&mut self, _event: KeyboardEvent) {
        // TODO: Use SendInput or WebView2 input injection
    }

    pub fn eval(&self, _js: &str) -> Result<(), WebviewError> {
        // TODO: Implement script evaluation
        Err(WebviewError::PlatformNotSupported)
    }
}
