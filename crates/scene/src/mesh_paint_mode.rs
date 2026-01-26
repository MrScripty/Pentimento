//! Mesh paint mode - 3D mesh painting with normal-based brush projection
//!
//! This module provides painting directly on 3D mesh surfaces. Unlike canvas plane
//! painting which projects from the camera view, mesh painting projects brushes
//! from surface normals to avoid distortion at oblique angles.
//!
//! # Architecture
//!
//! - `PaintableMesh` component marks meshes that can be painted on
//! - Ray-mesh intersection finds the hit point and triangle
//! - Vertex data (position, normal, UV, tangent) is interpolated using barycentric coords
//! - `MeshPaintEvent` messages are emitted for the painting system to process

use bevy::ecs::message::Message;
use bevy::input::mouse::MouseButton;
use bevy::mesh::{Indices, VertexAttributeValues};
use bevy::prelude::*;
use bevy::window::{CursorMoved, PrimaryWindow};

use painting::types::{MeshHit, MeshStorageMode};
use painting::projection::build_tangent_space;

use crate::camera::MainCamera;
use crate::paint_mode::{PaintMode, StrokeIdGenerator};

/// Component marking a mesh as paintable
#[derive(Component)]
pub struct PaintableMesh {
    /// Unique identifier for this mesh (used for stroke storage)
    pub mesh_id: u32,
    /// Storage mode (UV atlas or Ptex)
    pub storage_mode: MeshStorageMode,
}

/// Resource for generating unique mesh IDs
#[derive(Resource, Default)]
pub struct MeshIdGenerator {
    next_id: u32,
}

impl MeshIdGenerator {
    /// Generate the next unique mesh ID
    pub fn next(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

/// State for an in-progress mesh stroke
pub struct MeshStrokeState {
    /// Unique stroke identifier
    pub stroke_id: u64,
    /// Mesh ID this stroke is targeting
    pub mesh_id: u32,
    /// Timestamp when stroke started (milliseconds)
    pub start_time: u64,
    /// Last world-space position for delta calculation
    pub last_world_pos: Option<Vec3>,
    /// Last frame time for speed calculation
    pub last_time: f64,
}

/// Resource tracking mesh painting state
#[derive(Resource, Default)]
pub struct MeshPaintState {
    /// Current stroke state, if a mesh stroke is in progress
    pub current_stroke: Option<MeshStrokeState>,
    /// Entity of the mesh currently being painted
    pub active_mesh: Option<Entity>,
}

/// Message for mesh painting actions
#[derive(Message, Debug, Clone)]
pub enum MeshPaintEvent {
    /// A stroke has started on a mesh
    StrokeStart {
        /// The mesh entity being painted
        mesh_entity: Entity,
        /// Mesh ID
        mesh_id: u32,
        /// Hit data including position, normal, UV, etc.
        hit: MeshHit,
        /// Unique stroke ID
        stroke_id: u64,
    },
    /// Stroke continues with a new position
    StrokeMove {
        /// Hit data at the new position
        hit: MeshHit,
        /// Pressure value (0.0-1.0, defaults to 1.0 for mouse)
        pressure: f32,
        /// Speed in world units per second
        speed: f32,
    },
    /// Stroke has ended normally
    StrokeEnd,
    /// Stroke was cancelled
    StrokeCancel,
}

/// Plugin for mesh painting functionality
///
/// Note: This requires the `selection` feature to be enabled for mesh picking.
pub struct MeshPaintModePlugin;

impl Plugin for MeshPaintModePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MeshIdGenerator>()
            .init_resource::<MeshPaintState>()
            .add_message::<MeshPaintEvent>()
            .add_systems(Update, handle_mesh_paint_input);
    }
}

/// Handle mesh painting input
fn handle_mesh_paint_input(
    mouse_button: Res<ButtonInput<MouseButton>>,
    windows: Query<(Entity, &Window), With<PrimaryWindow>>,
    mut cursor_events: MessageReader<CursorMoved>,
    camera_query: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    mesh_query: Query<(Entity, &PaintableMesh, &Mesh3d, &GlobalTransform)>,
    meshes: Res<Assets<Mesh>>,
    paint_mode: Res<PaintMode>,
    mut mesh_paint_state: ResMut<MeshPaintState>,
    mut stroke_id_gen: ResMut<StrokeIdGenerator>,
    mut mesh_paint_events: MessageWriter<MeshPaintEvent>,
    time: Res<Time>,
) {
    // Only process if paint mode is active
    if !paint_mode.active {
        return;
    }

    // Get camera for ray casting
    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };

    // Get window for cursor position
    let Ok((window_entity, window)) = windows.single() else {
        return;
    };

    // Collect cursor positions from this frame
    let cursor_positions: Vec<Vec2> = cursor_events
        .read()
        .filter(|e| e.window == window_entity)
        .map(|e| e.position)
        .collect();

    let current_time = time.elapsed_secs_f64();

    // Handle stroke start
    if mouse_button.just_pressed(MouseButton::Left) {
        let cursor_pos = cursor_positions.last().copied().or_else(|| window.cursor_position());
        if let Some(cursor_pos) = cursor_pos {
            if let Some(ray) = camera.viewport_to_world(camera_transform, cursor_pos).ok() {
                // Find closest mesh hit
                if let Some((entity, paintable, hit)) =
                    find_closest_mesh_hit(&ray, &mesh_query, &meshes)
                {
                    let stroke_id = stroke_id_gen.next();
                    let mesh_id = paintable.mesh_id;

                    mesh_paint_state.current_stroke = Some(MeshStrokeState {
                        stroke_id,
                        mesh_id,
                        start_time: (current_time * 1000.0) as u64,
                        last_world_pos: Some(hit.world_pos),
                        last_time: current_time,
                    });
                    mesh_paint_state.active_mesh = Some(entity);

                    mesh_paint_events.write(MeshPaintEvent::StrokeStart {
                        mesh_entity: entity,
                        mesh_id,
                        hit,
                        stroke_id,
                    });

                    info!("Mesh stroke started on mesh_id={}", mesh_id);
                }
            }
        }
    } else if mouse_button.pressed(MouseButton::Left) {
        // Continue stroke - extract active_mesh before borrowing current_stroke
        let active_entity = mesh_paint_state.active_mesh;

        if let (Some(active_entity), Some(ref mut stroke_state)) =
            (active_entity, mesh_paint_state.current_stroke.as_mut())
        {
            let positions_to_process: Vec<Vec2> = if !cursor_positions.is_empty() {
                cursor_positions
            } else if let Some(pos) = window.cursor_position() {
                vec![pos]
            } else {
                vec![]
            };

            for cursor_pos in positions_to_process {
                if let Some(ray) = camera.viewport_to_world(camera_transform, cursor_pos).ok() {
                    // Only hit test against the active mesh
                    if let Ok((_entity, _, mesh_handle, mesh_transform)) =
                        mesh_query.get(active_entity)
                    {
                        if let Some(mesh) = meshes.get(&mesh_handle.0) {
                            if let Some(hit) =
                                ray_mesh_intersection(&ray, mesh, mesh_transform)
                            {
                                let speed =
                                    if let Some(last_pos) = stroke_state.last_world_pos {
                                        let distance = hit.world_pos.distance(last_pos);
                                        let dt = (current_time - stroke_state.last_time) as f32;
                                        if dt > 0.0 {
                                            distance / dt
                                        } else {
                                            0.0
                                        }
                                    } else {
                                        0.0
                                    };

                                stroke_state.last_world_pos = Some(hit.world_pos);
                                stroke_state.last_time = current_time;

                                mesh_paint_events.write(MeshPaintEvent::StrokeMove {
                                    hit,
                                    pressure: 1.0,
                                    speed,
                                });
                            }
                        }
                    }
                }
            }
        }
    } else if mouse_button.just_released(MouseButton::Left) {
        // End stroke
        if mesh_paint_state.current_stroke.is_some() {
            mesh_paint_events.write(MeshPaintEvent::StrokeEnd);
            mesh_paint_state.current_stroke = None;
            mesh_paint_state.active_mesh = None;
            info!("Mesh stroke ended");
        }
    }
}

/// Find the closest paintable mesh hit by a ray
fn find_closest_mesh_hit<'a>(
    ray: &Ray3d,
    mesh_query: &'a Query<(Entity, &PaintableMesh, &Mesh3d, &GlobalTransform)>,
    meshes: &Assets<Mesh>,
) -> Option<(Entity, &'a PaintableMesh, MeshHit)> {
    let mut closest: Option<(Entity, &PaintableMesh, MeshHit, f32)> = None;

    for (entity, paintable, mesh_handle, transform) in mesh_query.iter() {
        let Some(mesh) = meshes.get(&mesh_handle.0) else {
            continue;
        };

        if let Some(hit) = ray_mesh_intersection(ray, mesh, transform) {
            let distance = ray.origin.distance(hit.world_pos);
            if closest.is_none() || distance < closest.as_ref().unwrap().3 {
                closest = Some((entity, paintable, hit, distance));
            }
        }
    }

    closest.map(|(e, p, h, _)| (e, p, h))
}

/// Perform ray-mesh intersection and return hit data
///
/// This performs a brute-force triangle intersection test. For large meshes,
/// a BVH acceleration structure would be more efficient.
fn ray_mesh_intersection(
    ray: &Ray3d,
    mesh: &Mesh,
    transform: &GlobalTransform,
) -> Option<MeshHit> {
    // Get vertex positions
    let positions = match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
        Some(VertexAttributeValues::Float32x3(v)) => v,
        _ => return None,
    };

    // Get indices (required for triangle iteration)
    let indices = match mesh.indices() {
        Some(Indices::U32(i)) => i.iter().map(|&x| x as usize).collect::<Vec<_>>(),
        Some(Indices::U16(i)) => i.iter().map(|&x| x as usize).collect::<Vec<_>>(),
        None => return None,
    };

    // Get optional vertex attributes
    let normals = match mesh.attribute(Mesh::ATTRIBUTE_NORMAL) {
        Some(VertexAttributeValues::Float32x3(v)) => Some(v),
        _ => None,
    };

    let uvs = match mesh.attribute(Mesh::ATTRIBUTE_UV_0) {
        Some(VertexAttributeValues::Float32x2(v)) => Some(v),
        _ => None,
    };

    let tangents = match mesh.attribute(Mesh::ATTRIBUTE_TANGENT) {
        Some(VertexAttributeValues::Float32x4(v)) => Some(v),
        _ => None,
    };

    // Transform ray to local space for intersection
    let inv_transform = transform.affine().inverse();
    let local_ray_origin = inv_transform.transform_point3(ray.origin);
    let local_ray_dir = inv_transform.transform_vector3(*ray.direction).normalize();

    let mut closest_hit: Option<(f32, u32, Vec3)> = None; // (t, face_id, barycentric)

    // Iterate through triangles
    for (face_id, triangle) in indices.chunks(3).enumerate() {
        if triangle.len() != 3 {
            continue;
        }

        let i0 = triangle[0];
        let i1 = triangle[1];
        let i2 = triangle[2];

        let v0 = Vec3::from(positions[i0]);
        let v1 = Vec3::from(positions[i1]);
        let v2 = Vec3::from(positions[i2]);

        // Möller–Trumbore ray-triangle intersection
        if let Some((t, u, v)) = ray_triangle_intersection(
            local_ray_origin,
            local_ray_dir,
            v0,
            v1,
            v2,
        ) {
            if t > 0.0 && (closest_hit.is_none() || t < closest_hit.as_ref().unwrap().0) {
                let w = 1.0 - u - v;
                closest_hit = Some((t, face_id as u32, Vec3::new(w, u, v)));
            }
        }
    }

    let (_t, face_id, barycentric) = closest_hit?;

    // Get triangle vertex indices
    let base_idx = face_id as usize * 3;
    let i0 = indices[base_idx];
    let i1 = indices[base_idx + 1];
    let i2 = indices[base_idx + 2];

    // Interpolate position in local space
    let v0 = Vec3::from(positions[i0]);
    let v1 = Vec3::from(positions[i1]);
    let v2 = Vec3::from(positions[i2]);
    let local_pos = v0 * barycentric.x + v1 * barycentric.y + v2 * barycentric.z;

    // Transform to world space
    let world_pos = transform.transform_point(local_pos);

    // Interpolate and transform normal
    let normal = if let Some(normals) = normals {
        let n0 = Vec3::from(normals[i0]);
        let n1 = Vec3::from(normals[i1]);
        let n2 = Vec3::from(normals[i2]);
        let local_normal = (n0 * barycentric.x + n1 * barycentric.y + n2 * barycentric.z).normalize();
        // Transform normal (use rotation only, not scale)
        (transform.rotation() * local_normal).normalize()
    } else {
        // Compute face normal from triangle edges
        let edge1 = v1 - v0;
        let edge2 = v2 - v0;
        let local_normal = edge1.cross(edge2).normalize();
        (transform.rotation() * local_normal).normalize()
    };

    // Interpolate UV if available
    let uv = uvs.map(|uvs| {
        let uv0 = Vec2::from(uvs[i0]);
        let uv1 = Vec2::from(uvs[i1]);
        let uv2 = Vec2::from(uvs[i2]);
        uv0 * barycentric.x + uv1 * barycentric.y + uv2 * barycentric.z
    });

    // Build tangent space
    let (tangent, bitangent) = if let Some(tangents) = tangents {
        let t0 = Vec4::from(tangents[i0]);
        let t1 = Vec4::from(tangents[i1]);
        let t2 = Vec4::from(tangents[i2]);
        let local_tangent4 = t0 * barycentric.x + t1 * barycentric.y + t2 * barycentric.z;
        let local_tangent = Vec3::new(local_tangent4.x, local_tangent4.y, local_tangent4.z).normalize();
        let world_tangent = (transform.rotation() * local_tangent).normalize();
        let bitangent = normal.cross(world_tangent) * local_tangent4.w;
        (world_tangent, bitangent.normalize())
    } else {
        // Generate tangent from UV gradient or arbitrary
        let (t, b, _) = build_tangent_space(normal, None);
        (t, b)
    };

    Some(MeshHit {
        world_pos,
        face_id,
        barycentric,
        normal,
        tangent,
        bitangent,
        uv,
    })
}

/// Möller–Trumbore ray-triangle intersection algorithm
///
/// Returns (t, u, v) where t is the ray parameter and (u, v) are barycentric coordinates.
/// The third barycentric coordinate w = 1 - u - v.
fn ray_triangle_intersection(
    ray_origin: Vec3,
    ray_dir: Vec3,
    v0: Vec3,
    v1: Vec3,
    v2: Vec3,
) -> Option<(f32, f32, f32)> {
    const EPSILON: f32 = 1e-8;

    let edge1 = v1 - v0;
    let edge2 = v2 - v0;

    let h = ray_dir.cross(edge2);
    let a = edge1.dot(h);

    // Ray is parallel to triangle
    if a.abs() < EPSILON {
        return None;
    }

    let f = 1.0 / a;
    let s = ray_origin - v0;
    let u = f * s.dot(h);

    // Intersection outside triangle
    if !(0.0..=1.0).contains(&u) {
        return None;
    }

    let q = s.cross(edge1);
    let v = f * ray_dir.dot(q);

    // Intersection outside triangle
    if v < 0.0 || u + v > 1.0 {
        return None;
    }

    let t = f * edge2.dot(q);

    if t > EPSILON {
        Some((t, u, v))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ray_triangle_intersection_hit() {
        // Triangle in XY plane at z=0
        let v0 = Vec3::new(0.0, 0.0, 0.0);
        let v1 = Vec3::new(1.0, 0.0, 0.0);
        let v2 = Vec3::new(0.0, 1.0, 0.0);

        // Ray from z=1 pointing down, hitting center of triangle
        let origin = Vec3::new(0.25, 0.25, 1.0);
        let dir = Vec3::new(0.0, 0.0, -1.0);

        let result = ray_triangle_intersection(origin, dir, v0, v1, v2);
        assert!(result.is_some());

        let (t, u, v) = result.unwrap();
        assert!((t - 1.0).abs() < 0.001); // Should hit at z=0
        assert!(u >= 0.0 && u <= 1.0);
        assert!(v >= 0.0 && v <= 1.0);
        assert!(u + v <= 1.0);
    }

    #[test]
    fn test_ray_triangle_intersection_miss() {
        // Triangle in XY plane at z=0
        let v0 = Vec3::new(0.0, 0.0, 0.0);
        let v1 = Vec3::new(1.0, 0.0, 0.0);
        let v2 = Vec3::new(0.0, 1.0, 0.0);

        // Ray that misses the triangle
        let origin = Vec3::new(2.0, 2.0, 1.0);
        let dir = Vec3::new(0.0, 0.0, -1.0);

        let result = ray_triangle_intersection(origin, dir, v0, v1, v2);
        assert!(result.is_none());
    }

    #[test]
    fn test_ray_triangle_intersection_parallel() {
        // Triangle in XY plane at z=0
        let v0 = Vec3::new(0.0, 0.0, 0.0);
        let v1 = Vec3::new(1.0, 0.0, 0.0);
        let v2 = Vec3::new(0.0, 1.0, 0.0);

        // Ray parallel to triangle
        let origin = Vec3::new(0.25, 0.25, 1.0);
        let dir = Vec3::new(1.0, 0.0, 0.0);

        let result = ray_triangle_intersection(origin, dir, v0, v1, v2);
        assert!(result.is_none());
    }
}
