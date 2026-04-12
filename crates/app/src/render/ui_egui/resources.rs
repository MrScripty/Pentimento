use bevy::prelude::*;
use pentimento_egui_ui::{EguiUiRuntime, EguiUiSnapshot};
use pentimento_ipc::UiToBevy;

/// Bevy-owned wrapper around the egui frontend snapshot and runtime state.
#[derive(Resource, Default)]
pub struct EguiFrontendState {
    pub snapshot: EguiUiSnapshot,
    pub runtime: EguiUiRuntime,
    pub pending_commands: Vec<UiToBevy>,
}
