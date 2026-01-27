//! CanvasPlane entity and management
//!
//! A CanvasPlane is a planar surface that can be painted on.
//! It has a resolution (up to 1048x1048), a unique ID, and can be selected
//! for painting. When a plane is active, the camera can be locked to it
//! using the Tab key.

use bevy::ecs::message::Message;
use bevy::prelude::*;
use pentimento_ipc::{BevyToUi, EditMode};

use crate::camera::{MainCamera, OrbitCamera};
use crate::paint_mode::PaintMode;
use crate::painting_system::CanvasTexture;
use crate::OutboundUiMessages;
#[cfg(feature = "selection")]
use crate::selection::{Selectable, Selected};

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
    /// World-space width of the plane (for UV calculation)
    pub world_width: f32,
    /// World-space height of the plane (for UV calculation)
    pub world_height: f32,
    /// Camera position when paint mode was entered (for returning to paint view)
    pub paint_camera_pos: Option<Vec3>,
    /// Camera target (plane center) when paint mode was entered
    pub paint_camera_target: Option<Vec3>,
}

impl CanvasPlane {
    /// Maximum resolution for a canvas plane
    pub const MAX_RESOLUTION: u32 = 1048;

    /// Create a new canvas plane with the given ID, resolution, and world dimensions
    pub fn new(
        plane_id: u32,
        width: u32,
        height: u32,
        world_width: f32,
        world_height: f32,
    ) -> Self {
        Self {
            plane_id,
            width: width.min(Self::MAX_RESOLUTION),
            height: height.min(Self::MAX_RESOLUTION),
            active: false,
            world_width,
            world_height,
            paint_camera_pos: None,
            paint_camera_target: None,
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
    /// Create a canvas plane in front of the camera and enter paint mode
    CreateInFrontOfCamera { width: u32, height: u32 },
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

        #[cfg(feature = "selection")]
        app.add_systems(
            Update,
            sync_active_plane_with_selection.after(handle_canvas_plane_events),
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
    camera_query: Query<&GlobalTransform, With<MainCamera>>,
    mut orbit_camera_query: Query<&mut OrbitCamera>,
    mut paint_mode: ResMut<PaintMode>,
    mut outbound: ResMut<OutboundUiMessages>,
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
                        CanvasPlane::new(plane_id, *width, *height, plane_width, plane_height),
                        Name::new(format!("CanvasPlane_{}", plane_id)),
                    ))
                    .id();

                #[cfg(feature = "selection")]
                commands.entity(entity).insert(Selectable {
                    id: format!("canvas_{}", plane_id),
                });

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
            CanvasPlaneEvent::CreateInFrontOfCamera { width, height } => {
                // Get camera position and forward direction
                let Ok(camera_transform) = camera_query.single() else {
                    warn!("No main camera found for CreateInFrontOfCamera");
                    continue;
                };

                let camera_pos = camera_transform.translation();
                let forward = camera_transform.forward();

                // Position the plane in front of the camera
                let distance = 2.0;
                let plane_pos = camera_pos + *forward * distance;

                let plane_id = id_generator.next();

                // Calculate plane size to fill camera's field of view
                // Default Bevy FOV is 45 degrees (vertical)
                let fov = std::f32::consts::FRAC_PI_4;
                let half_height = (fov / 2.0).tan() * distance;
                // Use canvas aspect ratio for the plane
                let aspect_ratio = *width as f32 / *height as f32;
                let half_width = half_height * aspect_ratio;
                let plane_width = half_width * 2.0;
                let plane_height = half_height * 2.0;

                // Create a vertical quad mesh (Rectangle is in XY plane)
                let mesh = Rectangle::new(plane_width, plane_height);

                // Fully transparent material - only the painted texture will show
                let material = StandardMaterial {
                    base_color: Color::srgba(1.0, 1.0, 1.0, 0.0),
                    alpha_mode: AlphaMode::Blend,
                    unlit: true,
                    double_sided: true,
                    ..default()
                };

                // Create transform facing the camera
                // Rectangle mesh is in XY plane with front face at +Z
                // looking_at makes -Z point toward target, so we rotate 180° around Y
                // to make the +Z face (front) point toward the camera
                let mut transform =
                    Transform::from_translation(plane_pos).looking_at(camera_pos, Vec3::Y);
                transform.rotate_local_y(std::f32::consts::PI);

                // Create canvas plane with world dimensions
                let mut canvas_plane = CanvasPlane::new(
                    plane_id,
                    *width,
                    *height,
                    plane_width,
                    plane_height,
                );
                // Store camera position for returning to paint view
                canvas_plane.paint_camera_pos = Some(camera_pos);
                canvas_plane.paint_camera_target = Some(plane_pos);

                let entity = commands
                    .spawn((
                        Mesh3d(meshes.add(mesh)),
                        MeshMaterial3d(materials.add(material)),
                        transform,
                        canvas_plane,
                        Name::new(format!("CanvasPlane_{}", plane_id)),
                    ))
                    .id();

                #[cfg(feature = "selection")]
                commands.entity(entity).insert(Selectable {
                    id: format!("canvas_{}", plane_id),
                });

                info!(
                    "Created canvas plane {} in front of camera at {:?} with resolution {}x{}",
                    plane_id, plane_pos, width, height
                );

                // Auto-select and lock camera
                active_plane.entity = Some(entity);
                active_plane.camera_locked = true;

                // Enable paint mode and notify UI
                paint_mode.active = true;
                outbound.send(BevyToUi::EditModeChanged { mode: EditMode::Paint });
                info!("Entered paint mode with camera locked");
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
                if let Some(plane_entity) = active_plane.entity {
                    let was_locked = active_plane.camera_locked;
                    active_plane.camera_locked = !was_locked;

                    // When unlocking, deactivate paint mode and notify UI
                    if was_locked && !active_plane.camera_locked {
                        paint_mode.active = false;
                        outbound.send(BevyToUi::EditModeChanged { mode: EditMode::None });
                        info!("Camera lock disabled, exiting paint mode");
                    } else if !was_locked && active_plane.camera_locked {
                        // When locking, restore camera to paint position and activate paint mode
                        if let Ok(canvas_plane) = canvas_query.get(plane_entity) {
                            if let (Some(cam_pos), Some(cam_target)) =
                                (canvas_plane.paint_camera_pos, canvas_plane.paint_camera_target)
                            {
                                // Update OrbitCamera to match stored position
                                for mut orbit in orbit_camera_query.iter_mut() {
                                    orbit.target = cam_target;
                                    // Calculate distance, yaw, pitch from position
                                    let offset = cam_pos - cam_target;
                                    orbit.distance = offset.length();
                                    orbit.yaw = offset.x.atan2(offset.z);
                                    orbit.pitch = (offset.y / orbit.distance).asin();
                                    info!(
                                        "Restored camera: distance={}, yaw={}, pitch={}",
                                        orbit.distance, orbit.yaw, orbit.pitch
                                    );
                                }
                            }
                        }
                        paint_mode.active = true;
                        outbound.send(BevyToUi::EditModeChanged { mode: EditMode::Paint });
                        info!("Camera lock enabled, entering paint mode");
                    }
                }
            }
        }
    }
}

/// Handle Tab key input to toggle camera lock when a plane is selected
///
/// Tab behavior is context-aware:
/// - In Paint mode or when no mesh is in edit mode: toggle camera lock for canvas
/// - In MeshEdit mode: handled by mesh_edit_mode.rs
fn handle_camera_lock_input(
    key_input: Res<ButtonInput<KeyCode>>,
    active_plane: Res<ActiveCanvasPlane>,
    edit_mode: Res<crate::edit_mode::EditModeState>,
    mut events: MessageWriter<CanvasPlaneEvent>,
) {
    // Tab key toggles camera lock when a plane is selected AND we're not in mesh edit mode
    // (mesh_edit_mode.rs handles Tab for entering/exiting mesh edit mode)
    if key_input.just_pressed(KeyCode::Tab)
        && active_plane.entity.is_some()
        && edit_mode.mode != EditMode::MeshEdit
    {
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
            // Keep alpha blending so transparent parts of the texture are see-through
            material.alpha_mode = AlphaMode::Blend;
            material.unlit = true;
            material.double_sided = true;

            // Mark as updated to avoid processing again
            commands.entity(entity).insert(CanvasMaterialUpdated);

            info!("Updated canvas plane material with painting texture");
        }
    }
}

/// Sync ActiveCanvasPlane with the selection system
///
/// When a canvas plane is selected (via the normal selection system),
/// update ActiveCanvasPlane to point to it. When a canvas plane is
/// deselected, clear ActiveCanvasPlane.
#[cfg(feature = "selection")]
fn sync_active_plane_with_selection(
    mut active_plane: ResMut<ActiveCanvasPlane>,
    added_selected: Query<Entity, (With<CanvasPlane>, Added<Selected>)>,
    mut removed_selected: RemovedComponents<Selected>,
    canvas_query: Query<Entity, With<CanvasPlane>>,
) {
    // When a canvas plane is selected, make it the active plane
    for entity in added_selected.iter() {
        active_plane.entity = Some(entity);
        active_plane.camera_locked = false; // Not locked until Tab pressed
        info!("Canvas plane selected via click, set as active plane");
    }

    // When a canvas plane is deselected, clear active plane if it matches
    for entity in removed_selected.read() {
        // Check if this entity is a canvas plane and is the active one
        if canvas_query.get(entity).is_ok() && active_plane.entity == Some(entity) {
            active_plane.entity = None;
            active_plane.camera_locked = false;
            info!("Canvas plane deselected, cleared active plane");
        }
    }
}
