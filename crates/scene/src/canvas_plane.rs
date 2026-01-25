//! CanvasPlane entity and management
//!
//! A CanvasPlane is a planar surface that can be painted on.
//! It has a resolution (up to 1048x1048), a unique ID, and can be selected
//! for painting. When a plane is active, the camera can be locked to it
//! using the Tab key.

use bevy::ecs::message::Message;
use bevy::prelude::*;

use crate::painting_system::CanvasTexture;

/// Component marking an entity as a paintable canvas plane
#[derive(Component)]
pub struct CanvasPlane {
    /// Unique ID for this plane (used in StrokeHeader.space_id)
    pub plane_id: u32,
    /// Resolution width in pixels (max 1048)
    pub width: u32,
    /// Resolution height in pixels (max 1048)
    pub height: u32,
    /// Whether this plane is currently selected for painting
    pub active: bool,
}

impl CanvasPlane {
    /// Maximum resolution for a canvas plane
    pub const MAX_RESOLUTION: u32 = 1048;

    /// Create a new canvas plane with the given ID and resolution
    pub fn new(plane_id: u32, width: u32, height: u32) -> Self {
        Self {
            plane_id,
            width: width.min(Self::MAX_RESOLUTION),
            height: height.min(Self::MAX_RESOLUTION),
            active: false,
        }
    }
}

/// Resource tracking the currently active canvas plane
#[derive(Resource, Default)]
pub struct ActiveCanvasPlane {
    /// The entity of the currently active canvas plane, if any
    pub entity: Option<Entity>,
    /// Whether the camera is locked to the active plane
    pub camera_locked: bool,
}

/// Resource for generating unique plane IDs
#[derive(Resource, Default)]
pub struct CanvasPlaneIdGenerator {
    next_id: u32,
}

impl CanvasPlaneIdGenerator {
    /// Generate the next unique plane ID
    pub fn next(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

/// Message for canvas plane actions
#[derive(Message, Debug, Clone)]
pub enum CanvasPlaneEvent {
    /// Create a new canvas plane at position with given dimensions
    Create {
        position: Vec3,
        width: u32,
        height: u32,
    },
    /// Select a canvas plane for painting
    Select(Entity),
    /// Deselect the current canvas plane
    Deselect,
    /// Toggle camera lock (Tab key)
    ToggleCameraLock,
}

/// Plugin for CanvasPlane entities
pub struct CanvasPlanePlugin;

impl Plugin for CanvasPlanePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveCanvasPlane>()
            .init_resource::<CanvasPlaneIdGenerator>()
            .add_message::<CanvasPlaneEvent>()
            .add_systems(
                Update,
                (
                    handle_canvas_plane_events,
                    handle_camera_lock_input.after(handle_canvas_plane_events),
                    update_canvas_materials,
                ),
            );
    }
}

/// Handle canvas plane events (create, select, deselect, toggle camera lock)
fn handle_canvas_plane_events(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut events: MessageReader<CanvasPlaneEvent>,
    mut active_plane: ResMut<ActiveCanvasPlane>,
    mut id_generator: ResMut<CanvasPlaneIdGenerator>,
    mut canvas_query: Query<&mut CanvasPlane>,
) {
    for event in events.read() {
        match event {
            CanvasPlaneEvent::Create {
                position,
                width,
                height,
            } => {
                let plane_id = id_generator.next();

                // Calculate plane size based on resolution (1 unit = 100 pixels for reasonable scale)
                let scale_factor = 0.01;
                let plane_width = *width as f32 * scale_factor;
                let plane_height = *height as f32 * scale_factor;

                // Create a quad mesh for the canvas plane
                let mesh = Plane3d::default()
                    .mesh()
                    .size(plane_width, plane_height)
                    .build();

                // Placeholder material - white with some transparency to indicate paintable area
                let material = StandardMaterial {
                    base_color: Color::srgba(0.95, 0.95, 0.95, 0.9),
                    alpha_mode: AlphaMode::Blend,
                    unlit: true,
                    double_sided: true,
                    ..default()
                };

                let entity = commands
                    .spawn((
                        Mesh3d(meshes.add(mesh)),
                        MeshMaterial3d(materials.add(material)),
                        Transform::from_translation(*position),
                        CanvasPlane::new(plane_id, *width, *height),
                        Name::new(format!("CanvasPlane_{}", plane_id)),
                    ))
                    .id();

                info!(
                    "Created canvas plane {} at {:?} with resolution {}x{}",
                    plane_id, position, width, height
                );

                // Automatically select the newly created plane
                active_plane.entity = Some(entity);
                if let Ok(mut plane) = canvas_query.get_mut(entity) {
                    plane.active = true;
                }
            }
            CanvasPlaneEvent::Select(entity) => {
                // Deactivate previous plane
                if let Some(prev_entity) = active_plane.entity {
                    if let Ok(mut prev_plane) = canvas_query.get_mut(prev_entity) {
                        prev_plane.active = false;
                    }
                }

                // Activate new plane
                if let Ok(mut plane) = canvas_query.get_mut(*entity) {
                    plane.active = true;
                    active_plane.entity = Some(*entity);
                    info!("Selected canvas plane {}", plane.plane_id);
                }
            }
            CanvasPlaneEvent::Deselect => {
                // Deactivate current plane
                if let Some(prev_entity) = active_plane.entity {
                    if let Ok(mut prev_plane) = canvas_query.get_mut(prev_entity) {
                        prev_plane.active = false;
                    }
                }
                active_plane.entity = None;
                active_plane.camera_locked = false;
                info!("Deselected canvas plane");
            }
            CanvasPlaneEvent::ToggleCameraLock => {
                if active_plane.entity.is_some() {
                    active_plane.camera_locked = !active_plane.camera_locked;
                    info!(
                        "Camera lock {}",
                        if active_plane.camera_locked {
                            "enabled"
                        } else {
                            "disabled"
                        }
                    );
                }
            }
        }
    }
}

/// Handle Tab key input to toggle camera lock when a plane is selected
fn handle_camera_lock_input(
    key_input: Res<ButtonInput<KeyCode>>,
    active_plane: Res<ActiveCanvasPlane>,
    mut events: MessageWriter<CanvasPlaneEvent>,
) {
    // Tab key toggles camera lock when a plane is selected
    if key_input.just_pressed(KeyCode::Tab) && active_plane.entity.is_some() {
        events.write(CanvasPlaneEvent::ToggleCameraLock);
    }
}

/// Marker component indicating the material has been updated with the canvas texture
#[derive(Component)]
pub struct CanvasMaterialUpdated;

/// Update canvas plane materials to use the painting texture
fn update_canvas_materials(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    query: Query<
        (Entity, &CanvasTexture, &MeshMaterial3d<StandardMaterial>),
        Without<CanvasMaterialUpdated>,
    >,
) {
    for (entity, canvas_texture, mesh_material) in query.iter() {
        // Update the material to use the canvas texture
        if let Some(material) = materials.get_mut(&mesh_material.0) {
            material.base_color_texture = Some(canvas_texture.image_handle.clone());
            material.base_color = Color::WHITE;
            material.alpha_mode = AlphaMode::Opaque;
            material.unlit = true;
            material.double_sided = true;

            // Mark as updated to avoid processing again
            commands.entity(entity).insert(CanvasMaterialUpdated);

            info!("Updated canvas plane material with painting texture");
        }
    }
}
