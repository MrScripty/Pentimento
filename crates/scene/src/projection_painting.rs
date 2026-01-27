//! Projection painting system - projects canvas paint onto 3D meshes.
//!
//! This module provides the core systems for projection painting:
//! - Raycasting from canvas pixels to scene geometry
//! - Applying projected paint to mesh textures
//! - Managing projection target textures and GPU uploads

use bevy::asset::RenderAssetUsages;
use bevy::math::{Vec2, Vec3};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::mesh::{Indices, VertexAttributeValues};
use std::collections::HashMap;

use painting::{
    projection::{canvas_uv_to_ray, pixel_to_canvas_uv, project_brush_size_to_depth, CanvasPlaneParams},
    raycast::{raycast_mesh, MeshRaycastData},
    projection_target::{ProjectionTargetStorage, UvAtlasTarget},
    BlendMode,
};

use crate::camera::MainCamera;
use crate::canvas_plane::{ActiveCanvasPlane, CanvasPlane};
use crate::painting_system::PaintingResource;
use crate::projection_mode::{ProjectionEvent, ProjectionMode, ProjectionTarget};

/// Resource holding projection targets for each mesh
#[derive(Resource, Default)]
pub struct ProjectionTargets {
    /// Mapping from mesh entity to projection target
    targets: HashMap<Entity, UvAtlasTarget>,
    /// Mapping from mesh entity to GPU texture handle
    textures: HashMap<Entity, Handle<Image>>,
}

impl ProjectionTargets {
    /// Get or create a projection target for a mesh
    pub fn get_or_create(&mut self, entity: Entity, resolution: (u32, u32)) -> &mut UvAtlasTarget {
        self.targets.entry(entity).or_insert_with(|| {
            let mut target = UvAtlasTarget::new(resolution.0, resolution.1);
            // Initialize with transparent
            target.clear([0.0, 0.0, 0.0, 0.0]);
            target
        })
    }

    /// Get a projection target for a mesh
    pub fn get(&self, entity: Entity) -> Option<&UvAtlasTarget> {
        self.targets.get(&entity)
    }

    /// Get a mutable projection target for a mesh
    pub fn get_mut(&mut self, entity: Entity) -> Option<&mut UvAtlasTarget> {
        self.targets.get_mut(&entity)
    }

    /// Set the texture handle for a mesh
    pub fn set_texture(&mut self, entity: Entity, handle: Handle<Image>) {
        self.textures.insert(entity, handle);
    }

    /// Get the texture handle for a mesh
    pub fn get_texture(&self, entity: Entity) -> Option<&Handle<Image>> {
        self.textures.get(&entity)
    }

    /// Iterate over all targets
    pub fn iter(&self) -> impl Iterator<Item = (Entity, &UvAtlasTarget)> {
        self.targets.iter().map(|(e, t)| (*e, t))
    }

    /// Iterate over all targets mutably
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (Entity, &mut UvAtlasTarget)> {
        self.targets.iter_mut().map(|(e, t)| (*e, t))
    }
}

/// Cached mesh data for raycasting
#[derive(Resource, Default)]
pub struct MeshRaycastCache {
    /// Cached mesh data by entity
    cache: HashMap<Entity, MeshRaycastData>,
}

impl MeshRaycastCache {
    /// Get or build raycast data for a mesh
    pub fn get_or_build(
        &mut self,
        entity: Entity,
        mesh: &Mesh,
        _transform: &GlobalTransform,
    ) -> Option<&MeshRaycastData> {
        if !self.cache.contains_key(&entity) {
            if let Some(data) = extract_mesh_raycast_data(mesh) {
                self.cache.insert(entity, data);
            }
        }
        self.cache.get(&entity)
    }

    /// Invalidate cache for an entity (call when mesh changes)
    pub fn invalidate(&mut self, entity: Entity) {
        self.cache.remove(&entity);
    }
}

/// Extract raycast data from a Bevy Mesh
fn extract_mesh_raycast_data(mesh: &Mesh) -> Option<MeshRaycastData> {
    // Get positions
    let positions: Vec<Vec3> = match mesh.attribute(Mesh::ATTRIBUTE_POSITION)? {
        VertexAttributeValues::Float32x3(pos) => pos
            .iter()
            .map(|p| Vec3::new(p[0], p[1], p[2]))
            .collect(),
        _ => return None,
    };

    // Get indices
    let indices = match mesh.indices()? {
        Indices::U16(idx) => idx.iter().map(|i| *i as u32).collect(),
        Indices::U32(idx) => idx.clone(),
    };

    // Get normals
    let normals = match mesh.attribute(Mesh::ATTRIBUTE_NORMAL) {
        Some(VertexAttributeValues::Float32x3(norm)) => norm
            .iter()
            .map(|n| Vec3::new(n[0], n[1], n[2]))
            .collect(),
        _ => {
            // Generate flat normals if not present
            vec![Vec3::Y; positions.len()]
        }
    };

    // Get UVs
    let uvs = match mesh.attribute(Mesh::ATTRIBUTE_UV_0) {
        Some(VertexAttributeValues::Float32x2(uv)) => uv
            .iter()
            .map(|u| Vec2::new(u[0], u[1]))
            .collect(),
        _ => Vec::new(),
    };

    // Get tangents
    let tangents = match mesh.attribute(Mesh::ATTRIBUTE_TANGENT) {
        Some(VertexAttributeValues::Float32x4(tang)) => tang
            .iter()
            .map(|t| Vec3::new(t[0], t[1], t[2]))
            .collect(),
        _ => Vec::new(),
    };

    Some(MeshRaycastData {
        positions,
        indices,
        normals,
        uvs,
        tangents,
    })
}

/// Plugin for projection painting systems
pub struct ProjectionPaintingPlugin;

impl Plugin for ProjectionPaintingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ProjectionTargets>()
            .init_resource::<MeshRaycastCache>()
            .add_systems(
                Update,
                (
                    handle_projection_events,
                    setup_projection_textures,
                    live_projection_system,
                )
                    .chain(),
            );
    }
}

/// Handle projection mode events (project to scene, etc.)
fn handle_projection_events(
    mut events: MessageReader<ProjectionEvent>,
    mut projection_mode: ResMut<ProjectionMode>,
    painting_res: Res<PaintingResource>,
    active_plane: Res<ActiveCanvasPlane>,
    canvas_query: Query<(&CanvasPlane, &GlobalTransform)>,
    mesh_query: Query<(Entity, &Mesh3d, &GlobalTransform), With<ProjectionTarget>>,
    meshes: Res<Assets<Mesh>>,
    camera_query: Query<&GlobalTransform, With<MainCamera>>,
    mut targets: ResMut<ProjectionTargets>,
    mut mesh_cache: ResMut<MeshRaycastCache>,
) {
    for event in events.read() {
        match event {
            ProjectionEvent::ProjectToScene => {
                // One-shot projection of entire canvas
                project_canvas_to_scene(
                    &painting_res,
                    &active_plane,
                    &canvas_query,
                    &mesh_query,
                    &meshes,
                    &camera_query,
                    &mut targets,
                    &mut mesh_cache,
                );
            }
            ProjectionEvent::SetLiveProjection { enabled } => {
                projection_mode.live_projection = *enabled;
                projection_mode.enabled = *enabled;
            }
            _ => {}
        }
    }
}

/// Project the entire canvas to scene meshes
fn project_canvas_to_scene(
    painting_res: &PaintingResource,
    active_plane: &ActiveCanvasPlane,
    canvas_query: &Query<(&CanvasPlane, &GlobalTransform)>,
    mesh_query: &Query<(Entity, &Mesh3d, &GlobalTransform), With<ProjectionTarget>>,
    meshes: &Assets<Mesh>,
    camera_query: &Query<&GlobalTransform, With<MainCamera>>,
    targets: &mut ProjectionTargets,
    mesh_cache: &mut MeshRaycastCache,
) {
    let Some(plane_entity) = active_plane.entity else {
        return;
    };

    let Ok((canvas_plane, canvas_transform)) = canvas_query.get(plane_entity) else {
        return;
    };

    let Ok(camera_transform) = camera_query.single() else {
        return;
    };

    let Some(pipeline) = painting_res.get_pipeline(canvas_plane.plane_id) else {
        return;
    };

    let camera_pos = camera_transform.translation();
    let canvas_center = canvas_transform.translation();
    let canvas_right = canvas_transform.right().as_vec3();
    let canvas_up = canvas_transform.up().as_vec3();

    let canvas_params = CanvasPlaneParams {
        resolution: (canvas_plane.width, canvas_plane.height),
        world_size: (canvas_plane.world_width, canvas_plane.world_height),
    };

    let camera_to_canvas_dist = (canvas_center - camera_pos).length();

    info!(
        "Projecting canvas {}x{} to scene",
        canvas_plane.width, canvas_plane.height
    );

    // Iterate over all canvas pixels
    for py in 0..canvas_plane.height {
        for px in 0..canvas_plane.width {
            // Get the pixel color from the canvas
            let Some(pixel) = pipeline.get_pixel(px, py) else {
                continue;
            };

            // Skip transparent pixels
            if pixel[3] < 0.01 {
                continue;
            }

            // Convert pixel to UV
            let canvas_uv = pixel_to_canvas_uv((px, py), (canvas_plane.width, canvas_plane.height));

            // Create ray from camera through canvas point
            let (ray_origin, ray_dir) = canvas_uv_to_ray(
                camera_pos,
                canvas_uv,
                canvas_center,
                canvas_right,
                canvas_up,
                &canvas_params,
            );

            // Find nearest mesh hit
            let mut nearest_hit: Option<(Entity, painting::MeshHit, f32)> = None;

            for (entity, mesh_handle, mesh_transform) in mesh_query.iter() {
                let Some(mesh) = meshes.get(&mesh_handle.0) else {
                    continue;
                };

                let Some(mesh_data) = mesh_cache.get_or_build(entity, mesh, mesh_transform) else {
                    continue;
                };

                // Transform ray to mesh local space
                let inv_transform = mesh_transform.affine().inverse();
                let local_origin = inv_transform.transform_point3(ray_origin);
                let local_dir = inv_transform.transform_vector3(ray_dir).normalize();

                if let Some(hit) = raycast_mesh(local_origin, local_dir, mesh_data) {
                    let world_hit_pos = mesh_transform.transform_point(hit.world_pos);
                    let dist = (world_hit_pos - ray_origin).length();

                    let dominated = match &nearest_hit {
                        Some((_, _, prev_dist)) => dist >= *prev_dist,
                        None => false,
                    };

                    if !dominated {
                        // Transform hit back to world space
                        let mut world_hit = hit;
                        world_hit.world_pos = world_hit_pos;
                        // Transform normal to world space (use world_hit since hit was moved)
                        let local_normal = world_hit.normal;
                        world_hit.normal = mesh_transform
                            .affine()
                            .transform_vector3(local_normal)
                            .normalize();

                        nearest_hit = Some((entity, world_hit, dist));
                    }
                }
            }

            // Apply paint to nearest hit
            if let Some((entity, hit, dist)) = nearest_hit {
                // Get or create target for this mesh
                let target = targets.get_or_create(entity, (512, 512)); // Default resolution

                if let Some(tex_coord) = target.hit_to_tex_coord(&hit) {
                    // Calculate projected brush size based on depth
                    let world_radius =
                        project_brush_size_to_depth(0.5, &canvas_params, camera_to_canvas_dist, dist);

                    // Convert world radius to texture pixels
                    let (tex_w, _tex_h) = target.resolution();
                    let _tex_radius = world_radius * (tex_w as f32 / canvas_params.world_size.0);

                    // Apply as a small dab (single pixel projection)
                    target.apply_projected_pixel(tex_coord, pixel, 1.0, BlendMode::Normal);
                }
            }
        }
    }

    info!("Projection complete");
}

/// Setup textures for projection targets
fn setup_projection_textures(
    _commands: Commands,
    mut images: ResMut<Assets<Image>>,
    _materials: ResMut<Assets<StandardMaterial>>,
    mut targets: ResMut<ProjectionTargets>,
    query: Query<(Entity, &ProjectionTarget, &MeshMaterial3d<StandardMaterial>), Changed<ProjectionTarget>>,
) {
    for (entity, proj_target, _material_handle) in query.iter() {
        // Get resolution from storage mode
        let resolution = match proj_target.storage_mode {
            painting::MeshStorageMode::UvAtlas { resolution } => resolution,
            painting::MeshStorageMode::Ptex { face_resolution } => {
                (face_resolution * 8, face_resolution * 8) // Approximate
            }
        };

        // Create projection target if not exists
        let _target = targets.get_or_create(entity, resolution);

        // Create GPU texture if not exists
        if targets.get_texture(entity).is_none() {
            let (width, height) = resolution;

            // Create transparent RGBA8 texture
            let data = vec![0u8; (width * height * 4) as usize];

            let mut image = Image::new(
                Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                data,
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
            );

            image.texture_descriptor.usage =
                TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::COPY_SRC;

            let handle = images.add(image);
            targets.set_texture(entity, handle.clone());

            info!("Created projection texture for entity {:?} ({}x{})", entity, width, height);
        }
    }
}

/// Live projection system - projects paint in real-time as user strokes
fn live_projection_system(
    projection_mode: Res<ProjectionMode>,
    painting_res: Res<PaintingResource>,
    active_plane: Res<ActiveCanvasPlane>,
    canvas_query: Query<(&CanvasPlane, &GlobalTransform)>,
    _mesh_query: Query<(Entity, &Mesh3d, &GlobalTransform), With<ProjectionTarget>>,
    _meshes: Res<Assets<Mesh>>,
    _camera_query: Query<&GlobalTransform, With<MainCamera>>,
    _targets: ResMut<ProjectionTargets>,
    _mesh_cache: ResMut<MeshRaycastCache>,
) {
    // Only run if live projection is enabled
    if !projection_mode.live_projection {
        return;
    }

    let Some(plane_entity) = active_plane.entity else {
        return;
    };

    let Ok((canvas_plane, _canvas_transform)) = canvas_query.get(plane_entity) else {
        return;
    };

    let Some(pipeline) = painting_res.get_pipeline(canvas_plane.plane_id) else {
        return;
    };

    // Only process if there are dirty tiles (new paint)
    if !pipeline.has_dirty_tiles() {
        return;
    }

    // For live projection, we project dirty regions rather than the whole canvas
    // This is more efficient for real-time updates
    // (Full implementation would iterate dirty tiles and project only those pixels)

    // For now, trigger a full projection when there's activity
    // A more optimized version would track which pixels changed
}
