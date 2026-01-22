//! Shared scene setup for Pentimento
//!
//! This crate provides the common 3D scene setup used by both
//! the native Bevy app and the WASM Tauri build.

use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;

mod camera;
mod lighting;
#[cfg(feature = "selection")]
mod selection;
#[cfg(feature = "wireframe")]
mod wireframe;

pub use camera::{CameraControllerPlugin, MainCamera, OrbitCamera};
pub use lighting::{LightingPlugin, SceneLighting, SunLight};
#[cfg(feature = "selection")]
pub use selection::{Selectable, Selected, SelectionPlugin, SelectionState};
#[cfg(feature = "wireframe")]
pub use wireframe::{WireframeOverlayPlugin, WireframeSettings};

pub struct ScenePlugin;

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(CameraControllerPlugin);
        app.add_plugins(LightingPlugin);

        app.add_systems(Startup, setup_scene);

        #[cfg(feature = "selection")]
        app.add_plugins(SelectionPlugin);

        #[cfg(feature = "wireframe")]
        app.add_plugins(WireframeOverlayPlugin);
    }
}

/// Set up a basic PBR test scene
fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Camera with WebGL2-compatible tonemapping and orbit controls
    // TonyMcMapFace requires tonemapping_luts which needs zstd (not available in WASM)
    let orbit_camera = OrbitCamera::default();
    let camera_position = orbit_camera.calculate_position();
    commands.spawn((
        Camera3d::default(),
        Transform::from_translation(camera_position).looking_at(orbit_camera.target, Vec3::Y),
        Tonemapping::Reinhard,
        MainCamera,
        orbit_camera,
    ));

    // Sun lighting is handled by LightingPlugin

    // Ground plane
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(10.0, 10.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.3, 0.3, 0.3),
            perceptual_roughness: 0.8,
            ..default()
        })),
    ));

    // Test cube
    #[allow(unused_variables)]
    let cube = commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.8, 0.2, 0.2),
            metallic: 0.5,
            perceptual_roughness: 0.3,
            ..default()
        })),
        Transform::from_xyz(0.0, 0.5, 0.0),
        Name::new("Cube"),
    )).id();
    #[cfg(feature = "selection")]
    commands.entity(cube).insert(Selectable { id: "cube".to_string() });

    // Test sphere
    #[allow(unused_variables)]
    let sphere = commands.spawn((
        Mesh3d(meshes.add(Sphere::new(0.5).mesh().uv(32, 18))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.2, 0.6, 0.8),
            metallic: 0.9,
            perceptual_roughness: 0.1,
            ..default()
        })),
        Transform::from_xyz(2.0, 0.5, 0.0),
        Name::new("Sphere"),
    )).id();
    #[cfg(feature = "selection")]
    commands.entity(sphere).insert(Selectable { id: "sphere".to_string() });

    // Test torus
    #[allow(unused_variables)]
    let torus = commands.spawn((
        Mesh3d(meshes.add(Torus::new(0.3, 0.5))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.2, 0.8, 0.2),
            metallic: 0.3,
            perceptual_roughness: 0.5,
            ..default()
        })),
        Transform::from_xyz(-2.0, 0.5, 0.0),
        Name::new("Torus"),
    )).id();
    #[cfg(feature = "selection")]
    commands.entity(torus).insert(Selectable { id: "torus".to_string() });

    info!("Scene initialized with test objects");
}
