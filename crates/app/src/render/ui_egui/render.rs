use bevy::prelude::*;
use bevy_egui::EguiContexts;
use pentimento_egui_ui::show_root_ui;

use super::resources::EguiFrontendState;

pub fn render_egui_ui(mut contexts: EguiContexts, mut frontend: ResMut<EguiFrontendState>) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    let snapshot = frontend.snapshot.clone();
    let commands = show_root_ui(ctx, &snapshot, &mut frontend.runtime);
    frontend.pending_commands.extend(commands);
}
