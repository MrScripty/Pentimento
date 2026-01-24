//! Pentimento Dioxus UI
//!
//! Native Rust UI components using Dioxus, designed to replicate the Svelte UI.
//!
//! # Rendering Architecture
//!
//! This crate provides two rendering approaches:
//!
//! 1. **Dioxus Components** (`app`, `components`) - Reactive UI components using Dioxus
//! 2. **Vello Renderer** (`renderer`) - GPU-accelerated 2D rendering using Vello
//!
//! For Bevy integration with zero-copy GPU texture sharing, use the `VelloRenderer`
//! which accepts an external wgpu Device from Bevy.

mod app;
mod bridge;
mod components;
mod renderer;
mod state;

pub use app::PentimentoApp;
pub use bridge::{DioxusBridge, DioxusBridgeHandle};
pub use renderer::{SharedVelloRenderer, UiRenderState, VelloRenderer, VelloRendererError};
pub use state::UiState;

// Re-export types needed for the renderer interface
pub use vello::kurbo;
pub use vello::peniko;
pub use vello::{AaConfig, RenderParams, Scene};
pub use wgpu;
