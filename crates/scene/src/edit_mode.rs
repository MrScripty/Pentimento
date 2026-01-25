//! Edit mode state management
//!
//! Tracks whether the user is in a special editing mode (paint, sculpt, etc.)
//! and coordinates between UI and backend state.

use bevy::ecs::message::Message;
use bevy::prelude::*;
use pentimento_ipc::EditMode;

/// Resource tracking the current edit mode
#[derive(Resource, Default)]
pub struct EditModeState {
    /// Current edit mode
    pub mode: EditMode,
    /// Entity being edited (e.g., canvas plane in paint mode)
    pub target_entity: Option<Entity>,
}

/// Message for edit mode changes (internal)
#[derive(Message, Debug, Clone)]
pub enum EditModeEvent {
    /// Enter a specific edit mode
    Enter { mode: EditMode, target: Option<Entity> },
    /// Exit current edit mode
    Exit,
}

pub struct EditModePlugin;

impl Plugin for EditModePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EditModeState>()
            .add_message::<EditModeEvent>()
            .add_systems(Update, handle_edit_mode_events);
    }
}

/// Handle edit mode transitions
fn handle_edit_mode_events(
    mut events: MessageReader<EditModeEvent>,
    mut state: ResMut<EditModeState>,
    // TODO: Add sender for BevyToUi messages when IPC is wired up
) {
    for event in events.read() {
        match event {
            EditModeEvent::Enter { mode, target } => {
                state.mode = *mode;
                state.target_entity = *target;
                info!("Entered {:?} mode", mode);
                // TODO: Send BevyToUi::EditModeChanged to UI
            }
            EditModeEvent::Exit => {
                info!("Exited {:?} mode", state.mode);
                state.mode = EditMode::None;
                state.target_entity = None;
                // TODO: Send BevyToUi::EditModeChanged to UI
            }
        }
    }
}
