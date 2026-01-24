//! Pentimento Dioxus UI
//!
//! Native Rust UI components using Dioxus, designed to replicate the Svelte UI.

mod app;
mod bridge;
mod components;
mod state;

pub use app::PentimentoApp;
pub use bridge::{DioxusBridge, DioxusBridgeHandle};
pub use state::UiState;
