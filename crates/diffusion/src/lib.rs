//! Diffusion texture streaming for Pentimento
//!
//! Supports both local GPU inference (via candle) and remote server streaming.

mod remote;

#[cfg(feature = "local")]
mod local;

pub use remote::RemoteDiffusion;

#[cfg(feature = "local")]
pub use local::LocalDiffusion;

use pentimento_ipc::DiffusionRequest;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DiffusionError {
    #[error("Connection failed: {0}")]
    Connection(String),

    #[error("Generation failed: {0}")]
    Generation(String),

    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    #[error("Cancelled")]
    Cancelled,
}

/// Progress callback type
pub type ProgressCallback = Box<dyn Fn(f32, Option<&image::RgbaImage>) + Send + Sync>;

/// Trait for diffusion backends
#[allow(async_fn_in_trait)]
pub trait DiffusionBackend {
    /// Generate an image from the given request
    async fn generate(
        &mut self,
        request: DiffusionRequest,
        on_progress: Option<ProgressCallback>,
    ) -> Result<image::RgbaImage, DiffusionError>;

    /// Cancel the current generation
    fn cancel(&mut self);

    /// Check if currently generating
    fn is_generating(&self) -> bool;
}
