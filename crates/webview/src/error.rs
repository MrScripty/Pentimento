//! Error types for the webview crate

use thiserror::Error;

#[derive(Debug, Error)]
pub enum WebviewError {
    #[error("Failed to initialize GTK: {0}")]
    GtkInit(String),

    #[error("Failed to create webview: {0}")]
    WebviewCreate(String),

    #[error("Failed to create window: {0}")]
    WindowCreate(String),

    #[error("Failed to capture framebuffer: {0}")]
    Capture(String),

    #[error("Failed to evaluate JavaScript: {0}")]
    EvalScript(String),

    #[error("IPC channel closed")]
    ChannelClosed,

    #[error("Platform not supported")]
    PlatformNotSupported,
}
