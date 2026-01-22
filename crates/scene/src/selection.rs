//! Object selection and outline rendering
//!
//! Provides click-to-select functionality with orange outline highlighting.
//! Uses Bevy's built-in MeshPickingPlugin for raycasting.

use bevy::color::palettes::css::ORANGE;
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

/// Component linking an object to its outline entity
#[derive(Component)]
pub struct OutlineTarget {
    /// Entity of the outline mesh
    pub outline_entity: Entity,
}

/// Marker component for outline meshes
#[derive(Component)]
pub struct OutlineMesh;

/// Plugin for selection and outline rendering
pub struct SelectionPlugin;

impl Plugin for SelectionPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MeshPickingPlugin)
            .init_resource::<SelectionState>()
            .add_systems(Update, handle_click_selection)
            .add_systems(Update, spawn_outline_for_selected)
            .add_systems(Update, remove_outline_for_deselected)
            .add_systems(Update, update_outline_transforms);
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

/// Spawn outline mesh for newly selected entities
fn spawn_outline_for_selected(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    query: Query<(Entity, &Mesh3d, &Transform), (Added<Selected>, Without<OutlineTarget>)>,
) {
    // Create outline material (orange, unlit-like appearance)
    let outline_material = materials.add(StandardMaterial {
        base_color: Color::from(ORANGE),
        emissive: LinearRgba::new(1.0, 0.5, 0.0, 1.0),
        unlit: true,
        cull_mode: Some(bevy::render::render_resource::Face::Front), // Render back faces for outline
        ..default()
    });

    for (entity, mesh_handle, transform) in query.iter() {
        // Get the original mesh (use meshes ResMut for reading)
        let Some(original_mesh) = meshes.get(&mesh_handle.0) else {
            continue;
        };

        // Clone the mesh for the outline
        let outline_mesh = original_mesh.clone();

        // Spawn outline entity slightly scaled up
        let outline_scale = 1.03; // 3% larger
        let outline_transform = Transform {
            translation: transform.translation,
            rotation: transform.rotation,
            scale: transform.scale * outline_scale,
        };

        let outline_entity = commands
            .spawn((
                Mesh3d(meshes.add(outline_mesh)),
                MeshMaterial3d(outline_material.clone()),
                outline_transform,
                OutlineMesh,
                // Don't make outline pickable
                Pickable::IGNORE,
            ))
            .id();

        // Link the original entity to its outline
        commands
            .entity(entity)
            .insert(OutlineTarget { outline_entity });

        info!("Created outline for selected entity {:?}", entity);
    }
}

/// Remove outline when entity is deselected
fn remove_outline_for_deselected(
    mut commands: Commands,
    query: Query<(Entity, &OutlineTarget), Without<Selected>>,
) {
    for (entity, outline_target) in query.iter() {
        // Despawn the outline entity
        commands.entity(outline_target.outline_entity).despawn();

        // Remove the OutlineTarget component
        commands.entity(entity).remove::<OutlineTarget>();

        info!("Removed outline for deselected entity {:?}", entity);
    }
}

/// Keep outline transforms in sync with their targets
fn update_outline_transforms(
    query: Query<(&Transform, &OutlineTarget), (With<Selected>, Changed<Transform>)>,
    mut outline_query: Query<&mut Transform, (With<OutlineMesh>, Without<Selected>)>,
) {
    let outline_scale = 1.03;

    for (transform, outline_target) in query.iter() {
        if let Ok(mut outline_transform) = outline_query.get_mut(outline_target.outline_entity) {
            outline_transform.translation = transform.translation;
            outline_transform.rotation = transform.rotation;
            outline_transform.scale = transform.scale * outline_scale;
        }
    }
}
