//! Surface ID (Cryptomatte) selection outline rendering
//!
//! Renders pixel-accurate orange outlines around selected 3D objects using
//! a Surface ID / Cryptomatte-style approach:
//! 1. ID Pass: Render selected objects to a texture with entity IDs as colors
//! 2. Edge Detection: Post-process shader finds ID boundaries
//! 3. Composite: Display outline via UI ImageNode (renders above webview)
//!
//! This approach is WebGL2-compatible for WASM builds.

use bevy::asset::embedded_asset;
use bevy::asset::RenderAssetUsages;
use bevy::camera::ClearColorConfig;
use bevy::camera::RenderTarget;
use bevy::camera::visibility::RenderLayers;
use bevy::picking::prelude::Pickable;
use bevy::prelude::*;
use bevy::render::extract_resource::ExtractResource;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};

mod edge_detection;
mod id_material;
mod outline_settings;

pub use id_material::{EntityIdMaterial, RenderToIdBuffer};
pub use outline_settings::OutlineSettings;

use crate::camera::MainCamera;
use crate::selection::Selected;
use edge_detection::EdgeDetectionPlugin;
use id_material::entity_to_color;

/// Resource holding the render targets for outline rendering
#[derive(Resource, Clone, ExtractResource)]
pub struct OutlineRenderTargets {
    /// Texture where entity IDs are rendered
    pub id_buffer: Handle<Image>,
    /// Texture where edge detection outputs the outline (composited via UI)
    pub outline_buffer: Handle<Image>,
}

/// Marker for the ID buffer camera
#[derive(Component)]
pub struct IdBufferCamera;

/// Marker for the outline UI overlay
#[derive(Component)]
pub struct OutlineOverlay;

/// Plugin for Surface ID selection outlines
pub struct OutlinePlugin;

impl Plugin for OutlinePlugin {
    fn build(&self, app: &mut App) {
        // Embed the entity ID shader
        embedded_asset!(app, "shaders/entity_id.wgsl");

        app.init_resource::<OutlineSettings>()
            .add_plugins(MaterialPlugin::<EntityIdMaterial>::default())
            .add_plugins(EdgeDetectionPlugin)
            .add_systems(Startup, (setup_outline_system, setup_outline_overlay).chain())
            .add_systems(
                Update,
                (
                    sync_id_camera_transform,
                    sync_id_mirror_transforms,
                    add_selected_to_id_buffer,
                    remove_deselected_from_id_buffer,
                    handle_window_resize,
                )
                    .chain(),
            );
    }
}

/// Initialize the outline system with render targets and ID camera
fn setup_outline_system(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut id_materials: ResMut<Assets<EntityIdMaterial>>,
    windows: Query<&Window>,
    main_camera: Query<(&Transform, &OrbitCamera), With<MainCamera>>,
) {
    let Ok(window) = windows.single() else {
        warn!("No window found for outline system setup");
        return;
    };

    let width = window.resolution.physical_width().max(1);
    let height = window.resolution.physical_height().max(1);

    // Create ID buffer render target (entity IDs as colors)
    let id_buffer = create_render_texture(width, height, TextureFormat::Rgba8Unorm, &mut images);

    // Create outline buffer render target (edge detection output, composited via UI)
    let outline_buffer = create_render_texture(width, height, TextureFormat::Rgba8UnormSrgb, &mut images);

    commands.insert_resource(OutlineRenderTargets {
        id_buffer: id_buffer.clone(),
        outline_buffer: outline_buffer.clone(),
    });

    // Get main camera transform for ID camera
    let main_transform = main_camera.single().map(|(t, _)| *t).unwrap_or_else(|_| {
        Transform::from_xyz(0.0, 5.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y)
    });

    // Spawn ID buffer camera (renders selected objects to ID texture)
    commands.spawn((
        Camera3d::default(),
        Camera {
            order: -1, // Render before main camera
            clear_color: ClearColorConfig::Custom(Color::srgba(0.0, 0.0, 0.0, 0.0)),
            ..default()
        },
        RenderTarget::Image(id_buffer.into()),
        main_transform,
        // Only render entities on layer 1 (selected objects)
        RenderLayers::layer(1),
        IdBufferCamera,
    ));

    info!("Surface ID outline system initialized ({}x{})", width, height);
}

/// Set up the UI overlay that displays the outline texture
fn setup_outline_overlay(mut commands: Commands, targets: Res<OutlineRenderTargets>) {
    // Spawn a fullscreen ImageNode that displays the outline buffer
    // This composites the outline over the 3D scene via the UI layer
    commands.spawn((
        ImageNode {
            image: targets.outline_buffer.clone(),
            ..default()
        },
        Node {
            width: Val::Vw(100.0),
            height: Val::Vh(100.0),
            position_type: PositionType::Absolute,
            left: Val::Px(0.0),
            top: Val::Px(0.0),
            ..default()
        },
        // High ZIndex to render above the 3D scene but below webview
        ZIndex(i32::MAX - 100),
        Pickable::IGNORE,
        OutlineOverlay,
    ));

    info!("Outline UI overlay initialized");
}

/// Create a render target texture
fn create_render_texture(
    width: u32,
    height: u32,
    format: TextureFormat,
    images: &mut Assets<Image>,
) -> Handle<Image> {
    let size = Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };

    let mut image = Image::new_fill(
        size,
        TextureDimension::D2,
        &[0, 0, 0, 0],
        format,
        RenderAssetUsages::RENDER_WORLD,
    );

    image.texture_descriptor.usage =
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;

    images.add(image)
}

/// Sync ID camera transform with main camera
fn sync_id_camera_transform(
    main_camera: Query<&Transform, (With<MainCamera>, Without<IdBufferCamera>)>,
    mut id_camera: Query<&mut Transform, With<IdBufferCamera>>,
) {
    let Ok(main_transform) = main_camera.single() else {
        return;
    };
    let Ok(mut id_transform) = id_camera.single_mut() else {
        return;
    };

    *id_transform = *main_transform;
}

/// When an entity is selected, set up ID buffer rendering
fn add_selected_to_id_buffer(
    mut commands: Commands,
    mut id_materials: ResMut<Assets<EntityIdMaterial>>,
    added_selected: Query<(Entity, &Mesh3d), Added<Selected>>,
    meshes: Res<Assets<Mesh>>,
) {
    for (entity, mesh_handle) in added_selected.iter() {
        let entity_color = entity_to_color(entity);

        // Create ID material for this entity
        let id_material = id_materials.add(EntityIdMaterial {
            entity_id: id_material::EntityIdUniform { entity_color },
        });

        // Clone the mesh for the ID pass rendering
        // We need a separate entity on layer 1 with the ID material
        if let Some(mesh) = meshes.get(&mesh_handle.0) {
            commands.spawn((
                Mesh3d(mesh_handle.0.clone()),
                MeshMaterial3d(id_material),
                // Will be synced with the original entity's transform
                Transform::default(),
                GlobalTransform::default(),
                // Only visible to ID camera
                RenderLayers::layer(1),
                RenderToIdBuffer { entity_color },
                // Track which entity this is for
                IdBufferMirror { source: entity },
                Pickable::IGNORE,
            ));

            info!(
                "Added entity {:?} to ID buffer with color {:?}",
                entity, entity_color
            );
        }
    }
}

/// Component linking an ID buffer mirror to its source entity
#[derive(Component)]
pub struct IdBufferMirror {
    pub source: Entity,
}

/// Update ID buffer mirror transforms to match their source entities
fn sync_id_mirror_transforms(
    source_query: Query<&GlobalTransform, With<Selected>>,
    mut mirror_query: Query<(&IdBufferMirror, &mut Transform)>,
) {
    for (mirror, mut transform) in mirror_query.iter_mut() {
        if let Ok(source_transform) = source_query.get(mirror.source) {
            // Copy the global transform as local (since mirror has no parent)
            let (scale, rotation, translation) = source_transform.to_scale_rotation_translation();
            transform.translation = translation;
            transform.rotation = rotation;
            transform.scale = scale;
        }
    }
}

/// Remove ID buffer entities when their source is deselected
fn remove_deselected_from_id_buffer(
    mut commands: Commands,
    mirror_query: Query<(Entity, &IdBufferMirror)>,
    selected_query: Query<&Selected>,
) {
    for (mirror_entity, mirror) in mirror_query.iter() {
        // If source entity no longer has Selected component, remove the mirror
        if selected_query.get(mirror.source).is_err() {
            commands.entity(mirror_entity).despawn();
            info!(
                "Removed ID buffer mirror for deselected entity {:?}",
                mirror.source
            );
        }
    }
}

/// Handle window resize by recreating render targets
fn handle_window_resize(
    mut commands: Commands,
    windows: Query<&Window, Changed<Window>>,
    mut images: ResMut<Assets<Image>>,
    targets: Option<ResMut<OutlineRenderTargets>>,
    id_camera: Query<Entity, With<IdBufferCamera>>,
    mut overlay_query: Query<&mut ImageNode, With<OutlineOverlay>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };

    let Some(mut targets) = targets else {
        return;
    };

    let width = window.resolution.physical_width().max(1);
    let height = window.resolution.physical_height().max(1);

    // Check if resize is needed
    if let Some(id_image) = images.get(&targets.id_buffer) {
        if id_image.width() == width && id_image.height() == height {
            return;
        }
    }

    // Create new ID buffer
    let new_id_buffer = create_render_texture(width, height, TextureFormat::Rgba8Unorm, &mut images);

    // Create new outline buffer
    let new_outline_buffer =
        create_render_texture(width, height, TextureFormat::Rgba8UnormSrgb, &mut images);

    // Update camera target component
    if let Ok(camera_entity) = id_camera.single() {
        commands
            .entity(camera_entity)
            .insert(RenderTarget::Image(new_id_buffer.clone().into()));
    }

    // Update UI overlay image
    if let Ok(mut image_node) = overlay_query.single_mut() {
        image_node.image = new_outline_buffer.clone();
    }

    // Remove old textures
    images.remove(&targets.id_buffer);
    images.remove(&targets.outline_buffer);

    targets.id_buffer = new_id_buffer;
    targets.outline_buffer = new_outline_buffer;

    info!("Resized outline render targets to {}x{}", width, height);
}

// Re-export OrbitCamera for setup
use crate::camera::OrbitCamera;
