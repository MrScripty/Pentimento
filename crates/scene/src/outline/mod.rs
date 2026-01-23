//! Screen-space selection outline rendering
//!
//! Renders orange outlines around selected 3D objects using UI-based rendering.
//! Projects 3D object bounds to screen space and draws outlines as UI elements,
//! ensuring visibility even when a fullscreen UI texture covers the 3D scene.

use bevy::color::palettes::css::ORANGE;
use bevy::prelude::*;
use bevy::camera::primitives::Aabb;

use crate::camera::MainCamera;
use crate::selection::Selected;

/// Marker component for the outline UI container
#[derive(Component)]
pub struct OutlineContainer;

/// Component storing the entity this outline is for
#[derive(Component)]
pub struct OutlineFor {
    pub entity: Entity,
}

/// Plugin for screen-space selection outlines
pub struct OutlinePlugin;

impl Plugin for OutlinePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_outline_container);
        app.add_systems(
            Update,
            (
                spawn_outlines_for_selected,
                remove_outlines_for_deselected,
                update_outline_positions,
            )
                .chain(),
        );
    }
}

/// Create a container node for outline UI elements
fn setup_outline_container(mut commands: Commands) {
    commands.spawn((
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            ..default()
        },
        // Render on top of everything including other UI
        ZIndex(i32::MAX),
        // Don't block picking
        bevy::picking::prelude::Pickable::IGNORE,
        OutlineContainer,
    ));
}

/// Spawn UI outline nodes for newly selected entities
fn spawn_outlines_for_selected(
    mut commands: Commands,
    container_query: Query<Entity, With<OutlineContainer>>,
    selected_query: Query<Entity, Added<Selected>>,
    existing_outlines: Query<&OutlineFor>,
) {
    let Ok(container) = container_query.single() else {
        return;
    };

    for entity in selected_query.iter() {
        // Check if outline already exists for this entity
        let already_has_outline = existing_outlines
            .iter()
            .any(|outline_for| outline_for.entity == entity);

        if already_has_outline {
            continue;
        }

        // Spawn outline UI node as child of container
        let outline_entity = commands
            .spawn((
                // Invisible node that will be positioned based on 3D object bounds
                Node {
                    position_type: PositionType::Absolute,
                    border: UiRect::all(Val::Px(2.0)),
                    ..default()
                },
                BorderColor::all(Color::from(ORANGE)),
                BackgroundColor(Color::NONE),
                OutlineFor { entity },
                bevy::picking::prelude::Pickable::IGNORE,
            ))
            .id();

        commands.entity(container).add_child(outline_entity);

        info!("Created UI outline for selected entity {:?}", entity);
    }
}

/// Remove outline UI nodes for deselected entities
fn remove_outlines_for_deselected(
    mut commands: Commands,
    outline_query: Query<(Entity, &OutlineFor)>,
    selected_query: Query<&Selected>,
) {
    for (outline_entity, outline_for) in outline_query.iter() {
        // If the target entity no longer has Selected component, remove the outline
        if selected_query.get(outline_for.entity).is_err() {
            commands.entity(outline_entity).despawn();
            info!(
                "Removed UI outline for deselected entity {:?}",
                outline_for.entity
            );
        }
    }
}

/// Update outline positions based on 3D object screen-space bounds
fn update_outline_positions(
    mut outline_query: Query<(&mut Node, &OutlineFor)>,
    transform_query: Query<(&GlobalTransform, &Aabb)>,
    camera_query: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
) {
    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };

    for (mut node, outline_for) in outline_query.iter_mut() {
        let Ok((global_transform, aabb)) = transform_query.get(outline_for.entity) else {
            // Entity doesn't exist or doesn't have required components
            continue;
        };

        // Calculate screen-space bounding box
        if let Some(screen_rect) =
            calculate_screen_bounds(aabb, global_transform, camera, camera_transform)
        {
            node.left = Val::Px(screen_rect.min.x);
            node.top = Val::Px(screen_rect.min.y);
            node.width = Val::Px(screen_rect.width());
            node.height = Val::Px(screen_rect.height());
        }
    }
}

/// Calculate the screen-space bounding rectangle for an AABB
fn calculate_screen_bounds(
    aabb: &Aabb,
    global_transform: &GlobalTransform,
    camera: &Camera,
    camera_transform: &GlobalTransform,
) -> Option<Rect> {
    // Get the 8 corners of the AABB in local space
    let min = aabb.min();
    let max = aabb.max();

    let corners = [
        Vec3::new(min.x, min.y, min.z),
        Vec3::new(max.x, min.y, min.z),
        Vec3::new(min.x, max.y, min.z),
        Vec3::new(max.x, max.y, min.z),
        Vec3::new(min.x, min.y, max.z),
        Vec3::new(max.x, min.y, max.z),
        Vec3::new(min.x, max.y, max.z),
        Vec3::new(max.x, max.y, max.z),
    ];

    // Transform corners to world space and project to screen
    let mut min_screen = Vec2::new(f32::MAX, f32::MAX);
    let mut max_screen = Vec2::new(f32::MIN, f32::MIN);
    let mut any_visible = false;

    for corner in corners {
        let world_pos = global_transform.transform_point(corner);

        // Project to screen space (viewport coordinates)
        if let Some(ndc) = camera.world_to_ndc(camera_transform, world_pos) {
            // Only consider points in front of camera
            if ndc.z >= 0.0 && ndc.z <= 1.0 {
                if let Some(viewport) = camera.logical_viewport_rect() {
                    let screen_pos = Vec2::new(
                        (ndc.x + 1.0) * 0.5 * viewport.width() + viewport.min.x,
                        (1.0 - ndc.y) * 0.5 * viewport.height() + viewport.min.y,
                    );

                    min_screen = min_screen.min(screen_pos);
                    max_screen = max_screen.max(screen_pos);
                    any_visible = true;
                }
            }
        }
    }

    if any_visible {
        // Add padding for the border
        let padding = 4.0;
        Some(Rect::new(
            min_screen.x - padding,
            min_screen.y - padding,
            max_screen.x + padding,
            max_screen.y + padding,
        ))
    } else {
        None
    }
}
