//! Object selection system
//!
//! Provides click-to-select functionality for 3D objects.
//! Uses Bevy's built-in MeshPickingPlugin for raycasting.
//! Outline rendering is handled by the separate outline module.

use bevy::picking::prelude::*;
use bevy::prelude::*;

/// Marker component for selectable objects
#[derive(Component)]
pub struct Selectable {
    /// Unique identifier for this object
    pub id: String,
}

/// Marker component for currently selected objects
#[derive(Component)]
pub struct Selected;

/// Resource tracking current selection
#[derive(Resource, Default)]
pub struct SelectionState {
    /// IDs of currently selected objects
    pub selected_ids: Vec<String>,
}

/// Plugin for object selection
pub struct SelectionPlugin;

impl Plugin for SelectionPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MeshPickingPlugin)
            .init_resource::<SelectionState>()
            .add_systems(Update, handle_click_selection);
    }
}

/// Handle click events for selection using Pointer events
fn handle_click_selection(
    mut commands: Commands,
    key_input: Res<ButtonInput<KeyCode>>,
    mut selection: ResMut<SelectionState>,
    mut click_events: MessageReader<Pointer<Click>>,
    selected_query: Query<(Entity, &Selectable), With<Selected>>,
    all_selectable: Query<(Entity, &Selectable)>,
) {
    let shift_held =
        key_input.pressed(KeyCode::ShiftLeft) || key_input.pressed(KeyCode::ShiftRight);

    // Process click events from the picking system
    for event in click_events.read() {
        // Only handle left clicks
        if event.button != PointerButton::Primary {
            continue;
        }

        // Check if the clicked entity is selectable
        if let Ok((entity, selectable)) = all_selectable.get(event.entity) {
            let id = selectable.id.clone();
            let already_selected = selected_query.get(entity).is_ok();

            if shift_held {
                // Toggle selection
                if already_selected {
                    commands.entity(entity).remove::<Selected>();
                    selection.selected_ids.retain(|s| s != &id);
                } else {
                    commands.entity(entity).insert(Selected);
                    selection.selected_ids.push(id);
                }
            } else {
                // Single select - clear others first
                for (selected_entity, _) in selected_query.iter() {
                    if selected_entity != entity {
                        commands.entity(selected_entity).remove::<Selected>();
                    }
                }
                selection.selected_ids.clear();

                if !already_selected {
                    commands.entity(entity).insert(Selected);
                }
                selection.selected_ids.push(id);
            }
        } else {
            // Clicked on non-selectable or empty space - deselect all
            if !shift_held {
                for (entity, _) in selected_query.iter() {
                    commands.entity(entity).remove::<Selected>();
                }
                selection.selected_ids.clear();
            }
        }
    }
}
