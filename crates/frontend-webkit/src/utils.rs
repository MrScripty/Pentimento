//! Utility functions for the WebKit backend

use pentimento_frontend_core::FrontendError;

use crate::WebKitBackend;

impl WebKitBackend {
    /// Evaluate JavaScript in the webview
    pub fn eval(&self, js: &str) -> Result<(), FrontendError> {
        self.webview
            .evaluate_script(js)
            .map_err(|e| FrontendError::Backend(format!("Failed to evaluate script: {}", e)))
    }

    /// Set the device scale factor for HiDPI rendering
    pub fn set_scale_factor(&mut self, scale_factor: f64) {
        if scale_factor > 0.0 {
            self.scale_factor = scale_factor;
        }
    }

    /// Get the logical size (physical size divided by scale factor)
    pub(crate) fn logical_size(&self) -> (u32, u32) {
        let scale = if self.scale_factor > 0.0 {
            self.scale_factor
        } else {
            1.0
        };
        (
            ((self.size.0 as f64) / scale).round().max(1.0) as u32,
            ((self.size.1 as f64) / scale).round().max(1.0) as u32,
        )
    }
}
