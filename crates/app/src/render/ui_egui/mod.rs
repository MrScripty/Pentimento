//! egui frontend integration for Pentimento.

mod ipc_handler;
mod render;
mod resources;

use bevy::prelude::*;
use bevy_egui::input::EguiWantsInput;
use bevy_egui::{
    EguiGlobalSettings, EguiPlugin, EguiPostUpdateSet, EguiPrimaryContextPass, PrimaryEguiContext,
};
use pentimento_scene::FrontendInputBlockState;

use ipc_handler::sync_bevy_messages;
use render::render_egui_ui;
use resources::EguiFrontendState;

use crate::render::ui_commands::dispatch_ui_commands;

pub struct EguiRenderPlugin;

impl Plugin for EguiRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EguiPlugin::default())
            .insert_resource(EguiGlobalSettings {
                auto_create_primary_context: false,
                ..default()
            })
            .init_resource::<EguiFrontendState>()
            .add_systems(Startup, setup_egui_overlay_camera)
            .add_systems(
                EguiPrimaryContextPass,
                (
                    sync_bevy_messages,
                    render_egui_ui,
                    drain_pending_egui_commands,
                )
                    .chain(),
            )
            .add_systems(
                PostUpdate,
                sync_egui_input_blocks.after(EguiPostUpdateSet::ProcessOutput),
            );

        info!("Render plugin initialized with EGUI mode (experimental native UI)");
    }
}

fn setup_egui_overlay_camera(mut commands: Commands) {
    commands.spawn((
        PrimaryEguiContext,
        Camera2d,
        Camera {
            order: 100,
            clear_color: bevy::camera::ClearColorConfig::None,
            ..default()
        },
    ));
}

fn drain_pending_egui_commands(world: &mut World) {
    let commands = {
        let Some(mut frontend) = world.get_resource_mut::<EguiFrontendState>() else {
            return;
        };

        if frontend.pending_commands.is_empty() {
            return;
        }

        std::mem::take(&mut frontend.pending_commands)
    };

    dispatch_ui_commands(world, commands);
}

fn sync_egui_input_blocks(
    egui_wants_input: Res<EguiWantsInput>,
    mut input_blocks: ResMut<FrontendInputBlockState>,
) {
    input_blocks.block_pointer = egui_wants_input.wants_any_pointer_input();
    input_blocks.block_keyboard = egui_wants_input.wants_any_keyboard_input();
}
