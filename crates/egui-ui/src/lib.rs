//! Pentimento egui UI
//!
//! Immediate-mode native UI components for the experimental Bevy + egui
//! frontend path.

mod app;
mod controls;
mod paint_panel;
mod presets;
mod project_dialog;
mod sculpt_panel;
mod state;
#[cfg(test)]
mod tests;
mod uv_layer_panel;

pub use app::show_root_ui;
pub use egui;
pub use state::{EguiUiRuntime, EguiUiSnapshot, apply_bevy_message};
