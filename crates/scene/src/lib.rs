//! Shared scene setup for Pentimento
//!
//! This crate provides the common 3D scene setup used by both
//! the native Bevy app and the WASM Tauri build.

use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;
use pentimento_ipc::BevyToUi;

mod add_object;
mod ambient_occlusion;
mod camera;
mod canvas_plane;
mod edit_mode;
mod gizmo;
mod lighting;
mod paint_mode;
mod painting_system;
#[cfg(feature = "mesh_painting")]
mod mesh_paint_mode;
#[cfg(feature = "mesh_painting")]
mod mesh_painting_system;
#[cfg(feature = "selection")]
mod outline;
#[cfg(feature = "selection")]
mod selection;
#[cfg(feature = "wireframe")]
mod wireframe;

pub use add_object::{AddObjectEvent, AddObjectPlugin};
pub use ambient_occlusion::{AmbientOcclusionPlugin, SceneAmbientOcclusion};
pub use camera::{CameraControllerPlugin, MainCamera, OrbitCamera};
pub use edit_mode::{EditModeEvent, EditModePlugin, EditModeState};
pub use canvas_plane::{
    ActiveCanvasPlane, CanvasMaterialUpdated, CanvasPlane, CanvasPlaneEvent,
    CanvasPlaneIdGenerator, CanvasPlanePlugin,
};
pub use gizmo::{GizmoPlugin, GizmoState};
pub use lighting::{LightingPlugin, SceneLighting, SunLight};
pub use paint_mode::{PaintEvent, PaintMode, PaintModePlugin, StrokeIdGenerator, StrokeState};
pub use painting_system::{CanvasTexture, PaintingResource, PaintingSystemPlugin};
#[cfg(feature = "mesh_painting")]
pub use mesh_paint_mode::{
    MeshIdGenerator, MeshPaintEvent, MeshPaintModePlugin, MeshPaintState, PaintableMesh,
};
#[cfg(feature = "mesh_painting")]
pub use mesh_painting_system::{
    MeshPaintTexture, MeshPaintingResource, MeshPaintingSystemPlugin,
};
#[cfg(feature = "selection")]
pub use outline::{OutlineCamera, OutlinePlugin};
#[cfg(feature = "selection")]
pub use selection::{Selectable, Selected, SelectionPlugin, SelectionState};
#[cfg(feature = "wireframe")]
pub use wireframe::{WireframeOverlayPlugin, WireframeSettings};

/// Resource for queuing messages to send to the UI
/// The rendering layer (app crate) should drain this and send to the webview
#[derive(Resource, Default)]
pub struct OutboundUiMessages {
    pub messages: Vec<BevyToUi>,
}

impl OutboundUiMessages {
    /// Queue a message to be sent to the UI
    pub fn send(&mut self, msg: BevyToUi) {
        self.messages.push(msg);
    }

    /// Take all queued messages, leaving the queue empty
    pub fn drain(&mut self) -> Vec<BevyToUi> {
        std::mem::take(&mut self.messages)
    }
}

pub struct ScenePlugin;

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OutboundUiMessages>();

        app.add_plugins(CameraControllerPlugin);
        app.add_plugins(LightingPlugin);
        app.add_plugins(AmbientOcclusionPlugin);
        app.add_plugins(AddObjectPlugin);
        app.add_plugins(EditModePlugin);
        app.add_plugins(GizmoPlugin);
        app.add_plugins(CanvasPlanePlugin);
        app.add_plugins(PaintModePlugin);
        app.add_plugins(PaintingSystemPlugin);

        app.add_systems(Startup, setup_scene);

        #[cfg(feature = "mesh_painting")]
        {
            app.add_plugins(MeshPaintModePlugin);
            app.add_plugins(MeshPaintingSystemPlugin);
        }

        #[cfg(feature = "selection")]
        {
            app.add_plugins(SelectionPlugin);
            app.add_plugins(OutlinePlugin);
        }

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
    let mut camera_entity = commands.spawn((
        Camera3d::default(),
        Transform::from_translation(camera_position).looking_at(orbit_camera.target, Vec3::Y),
        Tonemapping::Reinhard,
        MainCamera,
        orbit_camera,
    ));
    #[cfg(feature = "selection")]
    camera_entity.insert(OutlineCamera);

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
    #[cfg(feature = "mesh_painting")]
    commands.entity(cube).insert(PaintableMesh {
        mesh_id: 0,
        storage_mode: painting::types::MeshStorageMode::Ptex { face_resolution: 32 },
    });

    // Test sphere (has UVs from .uv() call)
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
    #[cfg(feature = "mesh_painting")]
    commands.entity(sphere).insert(PaintableMesh {
        mesh_id: 1,
        storage_mode: painting::types::MeshStorageMode::UvAtlas { resolution: (512, 512) },
    });

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
    #[cfg(feature = "mesh_painting")]
    commands.entity(torus).insert(PaintableMesh {
        mesh_id: 2,
        storage_mode: painting::types::MeshStorageMode::Ptex { face_resolution: 32 },
    });

    info!("Scene initialized with test objects");
}
