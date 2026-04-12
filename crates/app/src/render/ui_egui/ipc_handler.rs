use bevy::prelude::*;
use pentimento_egui_ui::apply_bevy_message;
use pentimento_scene::OutboundUiMessages;

use super::resources::EguiFrontendState;

pub fn sync_bevy_messages(
    mut outbound: ResMut<OutboundUiMessages>,
    mut frontend: ResMut<EguiFrontendState>,
) {
    for message in outbound.drain() {
        apply_bevy_message(&mut frontend.snapshot, message);
    }
}
