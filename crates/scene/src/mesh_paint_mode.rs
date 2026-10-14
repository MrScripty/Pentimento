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
use bevy::ecs::message::MessageCursor;
use bevy::input::mouse::MouseButton;
use bevy::mesh::{Indices, VertexAttributeValues};
use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowEvent};

use painting::projection::build_tangent_space;
use painting::types::{MeshHit, MeshStorageMode};

use crate::camera::MainCamera;
use crate::frontend_input::FrontendInputBlockState;
use crate::paint_mode::{PaintMode, StrokeIdGenerator};

/// Component marking a mesh as paintable
#[derive(Component, Clone, Copy)]
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
    pub(crate) fn document_next_id(&self) -> u32 {
        self.next_id
    }
    pub(crate) fn from_document(next_id: u32) -> Self {
        Self { next_id }
    }
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
    /// Pressure-bearing native contact; legacy mouse Start remains full pressure.
    StrokeStartWithPressure {
        mesh_entity: Entity,
        mesh_id: u32,
        hit: MeshHit,
        stroke_id: u64,
        pressure: f32,
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
    StrokeBreak,
    History {
        mesh_entity: Entity,
        redo: bool,
    },
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
            .add_systems(
                Update,
                handle_mesh_paint_input
                    .after(crate::paint_mode::handle_paint_mode_toggle)
                    .after(crate::mesh_painting_system::sync_mesh_paint_owners)
                    .before(crate::mesh_painting_system::process_mesh_paint_events),
            );
    }
}

#[derive(Default)]
struct DirectPointer {
    reader: MessageCursor<WindowEvent>,
    cursor: Option<Vec2>,
    window: Option<Entity>,
    generation: u64,
    touch: Option<u64>,
    pressure: f32,
    keys: ButtonInput<KeyCode>,
}
/// Consume the actual app-arbitrated WindowEvent stream, including native contact
/// identity and force. Never synthesize scene mouse buttons from touch input.
pub(crate) fn handle_mesh_paint_input(world: &mut World) {
    world
        .run_system_cached(collect_mesh_paint_input)
        .expect("DirectUV input resources available");
}

fn collect_mesh_paint_input(world: &mut World, mut previous: Local<DirectPointer>) {
    use bevy::input::touch::TouchPhase;
    let raw: Vec<_> = world
        .get_resource::<Messages<WindowEvent>>()
        .map(|m| previous.reader.read(m).cloned().collect())
        .unwrap_or_default();
    if crate::frontend_input::native_scene_managed(world)
        && !world.contains_resource::<crate::frontend_input::NativePointerSegment>()
    {
        return;
    }
    let Some((window, focused, current_cursor)) = world
        .query_filtered::<(Entity, &Window), With<PrimaryWindow>>()
        .iter(world)
        .next()
        .map(|(e, w)| (e, w.focused, w.cursor_position()))
    else {
        return;
    };
    let generation = world
        .get_resource::<crate::project::ProjectState>()
        .map_or(0, |p| p.generation);
    if previous.window != Some(window) || previous.generation != generation {
        previous.cursor = None;
        previous.touch = None;
        previous.keys.reset_all();
        previous.window = Some(window);
        previous.generation = generation;
    }
    let arbitrated = world
        .get_resource::<crate::FrontendScenePointerInput>()
        .and_then(|s| s.events(window))
        .map(|e| e.to_vec());
    let segment = world.get_resource::<crate::frontend_input::NativePointerSegment>();
    let using_arbitration = segment.is_some() || arbitrated.is_some();
    let segment_native = segment.is_some();
    let finish = segment.is_none_or(|segment| segment.finish);
    let focused = segment.map_or(focused, |segment| segment.focused);
    let current_cursor = segment.map_or(current_cursor, |segment| segment.cursor);
    let left_down = segment.map_or_else(
        || {
            world
                .resource::<ButtonInput<MouseButton>>()
                .pressed(MouseButton::Left)
        },
        |segment| segment.left_down,
    );
    let batch = segment.map_or_else(
        || arbitrated.unwrap_or_else(|| raw.clone()),
        |segment| segment.events.clone(),
    );
    if segment_native {
        if segment.is_some_and(|segment| segment.reset_direct_contact) {
            previous.touch = None;
            previous.pressure = 1.;
        }
        previous.cursor = current_cursor;
    }
    let blocks = *world.resource::<FrontendInputBlockState>();
    // Retain chronological modifiers across frames: focus loss clears Bevy's
    // final ButtonInput before Update, even for a valid shortcut before the loss.
    let mut keys = previous.keys.clone();
    for event in &raw {
        match event {
            WindowEvent::KeyboardInput(e) if e.window == window => {
                if e.state.is_pressed() {
                    previous.keys.press(e.key_code);
                } else {
                    previous.keys.release(e.key_code);
                }
            }
            WindowEvent::KeyboardFocusLost(_) => previous.keys.reset_all(),
            WindowEvent::WindowFocused(e) if e.window == window && !e.focused => {
                previous.keys.reset_all()
            }
            _ => {}
        }
    }
    let active = world
        .get_resource::<PaintMode>()
        .is_some_and(|p| p.active && p.target == pentimento_ipc::PaintTarget::DirectUv);
    if !active {
        if crate::direct_uv_tool::is_direct(world) {
            crate::direct_uv_tool::restore_canvas(world);
        }
        previous.cursor = None;
        previous.touch = None;
        close_direct(world, true);
        return;
    }
    if !using_arbitration && blocks.blocks_pointer() {
        close_direct(world, previous.touch.is_some());
        previous.cursor = None;
        return;
    }
    if previous.cursor.is_none()
        && !batch
            .iter()
            .any(|e| matches!(e, WindowEvent::CursorMoved(_)))
    {
        previous.cursor = current_cursor.filter(|p| p.is_finite());
    }
    let first_focus = batch.iter().find_map(|e| match e {
        WindowEvent::WindowFocused(e) if e.window == window => Some(e.focused),
        _ => None,
    });
    if !focused && first_focus.is_none() {
        close_direct(world, previous.touch.is_some());
        previous.cursor = None;
        return;
    }
    let mut focus_lost = first_focus.map_or(!focused, |f| f);
    for event in batch {
        match event {
            WindowEvent::CursorMoved(e) if e.window == window && previous.touch.is_none() => {
                previous.cursor = (!focus_lost && e.position.is_finite()).then_some(e.position);
                if let Some(position) = previous.cursor {
                    move_direct(world, position, 1.);
                }
            }
            WindowEvent::MouseButtonInput(e)
                if e.window == window
                    && e.button == MouseButton::Left
                    && previous.touch.is_none() =>
            {
                if e.state.is_pressed() && !focus_lost {
                    if let Some(position) = previous.cursor {
                        start_direct(world, position, 1.);
                    }
                } else if !e.state.is_pressed() {
                    close_direct(world, false);
                }
            }
            WindowEvent::TouchInput(e) if e.window == window => {
                if e.phase == TouchPhase::Started {
                    if previous.touch.is_some()
                        || world.resource::<MeshPaintState>().current_stroke.is_some()
                        || focus_lost
                    {
                        continue;
                    }
                    previous.touch = Some(e.id);
                }
                if previous.touch != Some(e.id) {
                    continue;
                }
                if e.phase == TouchPhase::Canceled {
                    close_direct(world, true);
                    previous.touch = None;
                    previous.cursor = None;
                    continue;
                }
                let pressure = if e.phase == TouchPhase::Ended && e.force.is_none() {
                    Some(previous.pressure)
                } else {
                    crate::touch_pressure(e.force)
                };
                if !e.position.is_finite() || pressure.is_none() {
                    close_direct(world, true);
                    previous.cursor = None;
                    if e.phase == TouchPhase::Ended {
                        previous.touch = None;
                    }
                    continue;
                }
                let pressure = pressure.unwrap();
                previous.pressure = pressure;
                previous.cursor = Some(e.position);
                match e.phase {
                    TouchPhase::Started => start_direct(world, e.position, pressure),
                    TouchPhase::Moved => move_direct(world, e.position, pressure),
                    TouchPhase::Ended => {
                        move_direct(world, e.position, pressure);
                        close_direct(world, false);
                        previous.touch = None;
                        previous.cursor = None;
                    }
                    TouchPhase::Canceled => unreachable!(),
                }
            }
            WindowEvent::WindowFocused(e) if e.window == window => {
                focus_lost = !e.focused;
                if focus_lost {
                    close_direct(world, previous.touch.is_some());
                    previous.touch = None;
                    previous.cursor = None;
                    keys.reset_all();
                }
            }
            WindowEvent::KeyboardInput(e) if e.window == window => {
                if e.state.is_pressed() {
                    keys.press(e.key_code);
                } else {
                    keys.release(e.key_code);
                }
                if e.state.is_pressed()
                    && !e.repeat
                    && !focus_lost
                    && (segment_native || !blocks.blocks_keyboard())
                {
                    if e.key_code == KeyCode::Escape {
                        crate::direct_uv_tool::cancel(world);
                    }
                    if e.key_code == KeyCode::KeyZ
                        && !(world.contains_resource::<crate::NativeSceneHistoryOwner>()
                            && world
                                .get_resource::<crate::FrontendSceneKeyboardInput>()
                                .is_some_and(|input| {
                                    input.events(crate::project_generation(world)).is_some()
                                }))
                        && (keys.pressed(KeyCode::ControlLeft)
                            || keys.pressed(KeyCode::ControlRight))
                    {
                        if let Some(entity) = world.resource::<PaintMode>().direct_target {
                            world.write_message(MeshPaintEvent::History {
                                mesh_entity: entity,
                                redo: keys.pressed(KeyCode::ShiftLeft)
                                    || keys.pressed(KeyCode::ShiftRight),
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }
    if finish && previous.touch.is_none() && !left_down {
        close_direct(world, false);
    }
    if !focused {
        previous.cursor = None;
    }
}
fn close_direct(world: &mut World, cancel: bool) {
    if world
        .resource_mut::<MeshPaintState>()
        .current_stroke
        .take()
        .is_some()
    {
        world.write_message(if cancel {
            MeshPaintEvent::StrokeCancel
        } else {
            MeshPaintEvent::StrokeEnd
        });
    }
    world.resource_mut::<MeshPaintState>().active_mesh = None;
}
fn move_direct(world: &mut World, position: Vec2, pressure: f32) {
    if world.resource::<MeshPaintState>().current_stroke.is_none() {
        return;
    }
    let owner = world.resource::<MeshPaintState>().active_mesh;
    if let Some((_, _, hit)) = nearest(world, position).filter(|(e, _, _)| Some(*e) == owner) {
        world.write_message(MeshPaintEvent::StrokeMove {
            hit,
            pressure,
            speed: 0.,
        });
    } else {
        world.write_message(MeshPaintEvent::StrokeBreak);
    }
}
fn start_direct(world: &mut World, position: Vec2, pressure: f32) {
    if world.resource::<MeshPaintState>().current_stroke.is_some() {
        return;
    }
    let Some((entity, paintable, hit)) = nearest(world, position) else {
        return;
    };
    let Some(p) = paintable.filter(|p| matches!(p.storage_mode, MeshStorageMode::UvAtlas { .. }))
    else {
        crate::direct_uv_tool::error(
            world,
            "The visible surface is not a supported DirectUV receiver. PTex painting is unavailable.",
        );
        return;
    };
    if world
        .get_resource::<PaintMode>()
        .and_then(|m| m.direct_target)
        .is_some_and(|selected| {
            selected != entity
                && world
                    .resource::<crate::MeshPaintingResource>()
                    .shared_id_for_entity(selected)
                    .is_some()
        })
    {
        crate::direct_uv_tool::error(
            world,
            "Paint on the explicitly selected UV receiver, or select another receiver first.",
        );
        return;
    }
    if !crate::direct_uv_tool::admitted(world, entity) {
        crate::direct_uv_tool::error(
            world,
            "This DirectUV receiver is not ready or its paint ownership changed. Reopen or choose another supported receiver.",
        );
        return;
    }
    if let Some(layers) = world
        .resource::<crate::MeshPaintingResource>()
        .uv_layers(p.mesh_id)
    {
        if let Err(message) = layers.paintable() {
            crate::direct_uv_tool::error(world, message);
            return;
        }
    }
    let stroke_id = world.resource_mut::<StrokeIdGenerator>().next();
    let time = world.resource::<Time>().elapsed_secs_f64();
    world.resource_mut::<MeshPaintState>().active_mesh = Some(entity);
    world.resource_mut::<MeshPaintState>().current_stroke = Some(MeshStrokeState {
        stroke_id,
        mesh_id: p.mesh_id,
        start_time: (time * 1000.) as u64,
        last_world_pos: Some(hit.world_pos),
        last_time: time,
    });
    let mut mode = world.resource_mut::<PaintMode>();
    mode.direct_target = Some(entity);
    mode.target_notice = None;
    world.write_message(MeshPaintEvent::StrokeStartWithPressure {
        mesh_entity: entity,
        mesh_id: p.mesh_id,
        hit,
        stroke_id,
        pressure,
    });
}
/// Nearest visible triangle is the occluder even if it cannot receive paint.
fn nearest(world: &mut World, cursor: Vec2) -> Option<(Entity, Option<PaintableMesh>, MeshHit)> {
    let (camera, transform) = world
        .query_filtered::<(&Camera, &GlobalTransform), With<MainCamera>>()
        .iter(world)
        .next()
        .map(|(c, t)| (c.clone(), *t))?;
    let ray = camera.viewport_to_world(&transform, cursor).ok()?;
    let candidates: Vec<_> = world
        .query::<(
            Entity,
            &Mesh3d,
            &GlobalTransform,
            Option<&PaintableMesh>,
            Option<&Visibility>,
            Option<&InheritedVisibility>,
            Option<&MeshMaterial3d<StandardMaterial>>,
        )>()
        .iter(world)
        .filter(|(_, _, _, _, v, inherited, _)| {
            v.is_none_or(|v| *v != Visibility::Hidden) && inherited.is_none_or(|v| v.get())
        })
        .map(|(e, m, t, p, _, _, material)| {
            (
                e,
                m.0.clone(),
                *t,
                p.copied(),
                material.map(|m| m.0.clone()),
            )
        })
        .collect();
    let meshes = world.resource::<Assets<Mesh>>();
    let materials = world.resource::<Assets<StandardMaterial>>();
    candidates
        .into_iter()
        .filter_map(|(entity, mesh, transform, p, material)| {
            let mesh = meshes.get(&mesh)?;
            let cull = material
                .as_ref()
                .and_then(|h| materials.get(h))
                .and_then(|m| m.cull_mode);
            let hit = ray_mesh_intersection_culled(&ray, mesh, &transform, cull)?;
            Some((ray.origin.distance(hit.world_pos), entity, p, hit))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, e, p, h)| (e, p, h))
}

/// Perform ray-mesh intersection and return hit data
///
/// This performs a brute-force triangle intersection test. For large meshes,
/// a BVH acceleration structure would be more efficient.
pub fn ray_mesh_intersection(
    ray: &Ray3d,
    mesh: &Mesh,
    transform: &GlobalTransform,
) -> Option<MeshHit> {
    ray_mesh_intersection_culled(ray, mesh, transform, None)
}
fn ray_mesh_intersection_culled(
    ray: &Ray3d,
    mesh: &Mesh,
    transform: &GlobalTransform,
    cull: Option<bevy::render::render_resource::Face>,
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
        None => (0..positions.len()).collect(),
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
    if !transform.to_matrix().is_finite() || transform.to_matrix().determinant() == 0.0 {
        return None;
    }
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

        let v0 = Vec3::from(*positions.get(i0)?);
        let v1 = Vec3::from(*positions.get(i1)?);
        let v2 = Vec3::from(*positions.get(i2)?);
        if !v0.is_finite() || !v1.is_finite() || !v2.is_finite() {
            return None;
        }
        let a = transform.transform_point(v0);
        let b = transform.transform_point(v1);
        let c = transform.transform_point(v2);
        let front = (b - a).cross(c - a).dot(*ray.direction) < 0.0;
        if matches!(cull, Some(bevy::render::render_resource::Face::Back)) && !front
            || matches!(cull, Some(bevy::render::render_resource::Face::Front)) && front
        {
            continue;
        }

        // Möller–Trumbore ray-triangle intersection
        if let Some((t, u, v)) =
            ray_triangle_intersection(local_ray_origin, local_ray_dir, v0, v1, v2)
        {
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
        let n0 = Vec3::from(*normals.get(i0)?);
        let n1 = Vec3::from(*normals.get(i1)?);
        let n2 = Vec3::from(*normals.get(i2)?);
        let local_normal =
            (n0 * barycentric.x + n1 * barycentric.y + n2 * barycentric.z).normalize();
        // Transform normal (use rotation only, not scale)
        transform
            .to_matrix()
            .inverse()
            .transpose()
            .transform_vector3(local_normal)
            .normalize()
    } else {
        // Compute face normal from triangle edges
        let edge1 = v1 - v0;
        let edge2 = v2 - v0;
        let local_normal = edge1.cross(edge2).normalize();
        transform
            .to_matrix()
            .inverse()
            .transpose()
            .transform_vector3(local_normal)
            .normalize()
    };

    // Interpolate UV if available
    let uv = uvs.and_then(|uvs| {
        let uv0 = Vec2::from(*uvs.get(i0)?);
        let uv1 = Vec2::from(*uvs.get(i1)?);
        let uv2 = Vec2::from(*uvs.get(i2)?);
        Some(uv0 * barycentric.x + uv1 * barycentric.y + uv2 * barycentric.z)
    });

    // Build tangent space
    let (tangent, bitangent) = if let Some(tangents) = tangents {
        let t0 = Vec4::from(*tangents.get(i0)?);
        let t1 = Vec4::from(*tangents.get(i1)?);
        let t2 = Vec4::from(*tangents.get(i2)?);
        let local_tangent4 = t0 * barycentric.x + t1 * barycentric.y + t2 * barycentric.z;
        let local_tangent =
            Vec3::new(local_tangent4.x, local_tangent4.y, local_tangent4.z).normalize();
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

    if t > EPSILON { Some((t, u, v)) } else { None }
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
