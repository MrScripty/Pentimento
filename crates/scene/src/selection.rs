//! Object selection system
//!
//! Provides click-to-select functionality for 3D objects.
//! Uses Bevy's built-in MeshPickingPlugin for raycasting.
//! Outline rendering is handled by the separate outline module.

use bevy::picking::prelude::*;
use bevy::prelude::*;

use crate::frontend_input::FrontendInputBlockState;
use crate::paint_mode::PaintMode;

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
    mut click_events: MessageReader<PointerClick>,
    selected_query: Query<(Entity, &Selectable), With<Selected>>,
    all_selectable: Query<(Entity, &Selectable)>,
    paint_mode: Res<PaintMode>,
    edit_mode: Res<crate::EditModeState>,
    input_blocks: Res<FrontendInputBlockState>,
) {
    if input_blocks.blocks_pointer() {
        click_events.clear();
        return;
    }

    // A brush owns its target until explicit exit. Discard these clicks rather
    // than replaying them as selection changes once the brush mode ends.
    if paint_mode.active || edit_mode.mode == pentimento_ipc::EditMode::Sculpt {
        click_events.clear();
        return;
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::camera::NormalizedRenderTarget;
    use bevy::picking::{
        backend::HitData,
        pointer::{Location, PointerId},
    };

    #[test]
    fn sculpt_keeps_selection_and_does_not_replay_blocked_click_after_exit() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<SelectionState>()
            .init_resource::<PaintMode>()
            .init_resource::<FrontendInputBlockState>()
            .insert_resource(crate::EditModeState {
                mode: pentimento_ipc::EditMode::Sculpt,
                target_entity: None,
            })
            .add_message::<PointerClick>()
            .add_systems(Update, handle_click_selection);
        let selected = app
            .world_mut()
            .spawn((
                Selected,
                Selectable {
                    id: "sphere".into(),
                },
            ))
            .id();
        let empty = app.world_mut().spawn_empty().id();
        app.world_mut()
            .resource_mut::<SelectionState>()
            .selected_ids
            .push("sphere".into());
        app.world_mut()
            .resource_mut::<crate::EditModeState>()
            .target_entity = Some(selected);
        let click = PointerClick {
            entity: empty,
            pointer: Pointer::new(
                PointerId::Mouse,
                Location {
                    target: NormalizedRenderTarget::None {
                        width: 100,
                        height: 100,
                    },
                    position: Vec2::splat(10.0),
                },
            ),
            button: PointerButton::Primary,
            hit: HitData::new(empty, 1.0, None, None),
            duration: std::time::Duration::from_millis(50),
            count: 1,
        };
        app.world_mut().write_message(click.clone());
        app.update();
        assert!(app.world().get::<Selected>(selected).is_some());
        assert_eq!(
            app.world().resource::<SelectionState>().selected_ids,
            ["sphere"]
        );
        app.world_mut().resource_mut::<crate::EditModeState>().mode =
            pentimento_ipc::EditMode::None;
        app.update();
        assert!(app.world().get::<Selected>(selected).is_some());
        // A fresh ordinary-mode click is still allowed to deselect.
        app.world_mut().write_message(click);
        app.update();
        assert!(app.world().get::<Selected>(selected).is_none());
        assert!(
            app.world()
                .resource::<SelectionState>()
                .selected_ids
                .is_empty()
        );
    }
}
