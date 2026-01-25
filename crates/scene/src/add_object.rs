//! Add object system for creating new primitives in the scene
//!
//! Handles spawning new mesh objects via the AddObjectEvent.

use bevy::ecs::message::Message;
use bevy::prelude::*;
use pentimento_ipc::{AddObjectRequest, PrimitiveType};

#[cfg(feature = "selection")]
use crate::selection::Selectable;

/// Event/Message for adding new objects to the scene
#[derive(Message)]
pub struct AddObjectEvent(pub AddObjectRequest);

/// Counter for generating unique object IDs
#[derive(Resource, Default)]
struct ObjectCounter(u32);

/// Plugin for adding objects to the scene
pub struct AddObjectPlugin;

impl Plugin for AddObjectPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ObjectCounter>()
            .add_message::<AddObjectEvent>()
            .add_systems(Update, handle_add_object_event);
    }
}

/// Handle add object events by spawning appropriate meshes
fn handle_add_object_event(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut events: MessageReader<AddObjectEvent>,
    mut counter: ResMut<ObjectCounter>,
) {
    for event in events.read() {
        let request = &event.0;

        // Generate unique ID
        counter.0 += 1;
        let id = format!("object_{}", counter.0);

        // Determine position
        let position = request
            .position
            .map(Vec3::from_array)
            .unwrap_or(Vec3::new(0.0, 0.5, 0.0)); // Slightly above ground by default

        // Generate name
        let name = request.name.clone().unwrap_or_else(|| {
            format!("{:?}", request.primitive_type)
        });

        // Create mesh based on primitive type
        let mesh = match request.primitive_type {
            PrimitiveType::Cube => meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
            PrimitiveType::Sphere => meshes.add(Sphere::new(0.5).mesh().uv(32, 18)),
            PrimitiveType::Cylinder => meshes.add(Cylinder::new(0.5, 1.0)),
            PrimitiveType::Plane => meshes.add(Plane3d::default().mesh().size(2.0, 2.0)),
            PrimitiveType::Torus => meshes.add(Torus::new(0.3, 0.5)),
            PrimitiveType::Cone => meshes.add(Cone::new(0.5, 1.0)),
            PrimitiveType::Capsule => meshes.add(Capsule3d::new(0.25, 0.5)),
        };

        // Default gray material
        let material = materials.add(StandardMaterial {
            base_color: Color::srgb(0.7, 0.7, 0.7),
            metallic: 0.0,
            perceptual_roughness: 0.5,
            ..default()
        });

        // Spawn the entity
        #[allow(unused_variables)]
        let entity = commands
            .spawn((
                Mesh3d(mesh),
                MeshMaterial3d(material),
                Transform::from_translation(position),
                Name::new(name.clone()),
            ))
            .id();

        // Add Selectable component if selection feature is enabled
        #[cfg(feature = "selection")]
        commands.entity(entity).insert(Selectable { id: id.clone() });

        info!("Added object '{}' (id: {}) at {:?}", name, id, position);
    }
}
