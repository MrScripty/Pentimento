//! Pentimento Dioxus UI
//!
//! Native Rust UI components using Dioxus, designed to replicate the Svelte UI.
//!
//! # Rendering Architecture
//!
//! This crate provides the following components:
//!
//! 1. **Dioxus Components** (`app`, `components`) - Reactive UI components using Dioxus
//! 2. **Blitz Document** (`document`) - DOM/CSS/layout integration via Blitz
//! 3. **Vello Renderer** (`renderer`) - GPU-accelerated 2D rendering using Vello
//!
//! ## Zero-Copy Rendering Pipeline
//!
//! For Bevy integration with zero-copy GPU texture sharing:
//!
//! 1. Create a `BlitzDocument` with your UI components
//! 2. Call `document.poll()` each frame to process state changes
//! 3. Call `document.paint_to_scene(&mut scene)` to generate Vello draw commands
//! 4. Use `SharedVelloRenderer` to render the scene to Bevy's GpuImage

mod app;
mod bridge;
mod components;
mod document;
mod renderer;
mod state;

pub use app::PentimentoApp;
pub use bridge::{DioxusBridge, DioxusBridgeHandle};
pub use document::BlitzDocument;
pub use renderer::{SharedVelloRenderer, UiRenderState, VelloRenderer, VelloRendererError};
pub use state::UiState;

// Re-export types needed for the renderer interface
pub use vello::kurbo;
pub use vello::peniko;
pub use vello::{AaConfig, RenderParams, Scene};
pub use wgpu;

// Re-export Blitz event types for input handling
pub use blitz_traits::events::{
    BlitzPointerId, BlitzPointerEvent, BlitzWheelDelta, BlitzWheelEvent, MouseEventButton,
    MouseEventButtons, PointerCoords, PointerDetails, UiEvent,
};
pub use keyboard_types::Modifiers as BlitzModifiers;
