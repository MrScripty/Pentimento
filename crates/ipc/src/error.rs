//! Error types for IPC operations.

/// Errors that can occur during IPC operations.
#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    #[error("Failed to serialize message: {0}")]
    Serialize(#[from] serde_json::Error),

    #[error("Invalid message format: {0}")]
    InvalidFormat(String),
}
