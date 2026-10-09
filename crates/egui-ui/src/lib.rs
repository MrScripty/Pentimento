//! Pentimento egui UI
//!
//! Immediate-mode native UI components for the experimental Bevy + egui
//! frontend path.

mod app;
mod state;

pub use app::show_root_ui;
pub use egui;
pub use state::{EguiUiRuntime, EguiUiSnapshot, apply_bevy_message};
