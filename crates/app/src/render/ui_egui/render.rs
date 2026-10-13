use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::EguiContexts;
use pentimento_egui_ui::show_root_ui;
use pentimento_ipc::{LayoutInfo, LayoutRegion, UiToBevy};

use super::resources::EguiFrontendState;

pub fn render_egui_ui(
    mut contexts: EguiContexts,
    mut frontend: ResMut<EguiFrontendState>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let Ok(window) = windows.single() else {
        return;
    };
    let frontend = &mut *frontend;
    let commands = show_root_ui(ctx, &mut frontend.snapshot, &mut frontend.runtime);
    // egui zoom scales points independently of the window's physical DPI.
    // Native input uses window logical coordinates, including stationary presses.
    let scale = ctx.pixels_per_point() / window.resolution.scale_factor();
    let regions = frontend
        .runtime
        .ui_regions()
        .iter()
        .enumerate()
        .map(|(index, rect)| LayoutRegion {
            id: format!("egui-{index}"),
            x: rect.min.x * scale,
            y: rect.min.y * scale,
            width: rect.width() * scale,
            height: rect.height() * scale,
            z_index: index as i32,
            accepts_keyboard: true,
        })
        .collect();
    frontend
        .pending_commands
        .push(UiToBevy::LayoutUpdate(LayoutInfo { regions }));
    frontend.pending_commands.extend(commands);
}
