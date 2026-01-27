//! Sub-object selection for mesh edit mode
//!
//! Handles clicking to select vertices, edges, or faces within a mesh.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use painting::half_edge::{FaceId, HalfEdgeId, HalfEdgeMesh, VertexId};
use pentimento_ipc::{BevyToUi, EditMode, MeshSelectionMode};

use crate::camera::MainCamera;
use crate::edit_mode::EditModeState;
use crate::mesh_edit_mode::{EditableMesh, MeshEditState};
use crate::OutboundUiMessages;

/// Result of a sub-object raycast
#[derive(Debug, Clone)]
pub struct SubObjectHit {
    /// Type of element hit
    pub hit_type: SubObjectHitType,
    /// World position of the hit
    pub world_position: Vec3,
    /// Distance from ray origin
    pub distance: f32,
}

/// Type of sub-object hit
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubObjectHitType {
    Vertex(VertexId),
    Edge(HalfEdgeId),
    Face(FaceId),
}

/// Threshold for vertex selection (in world units, scaled by distance)
const VERTEX_SELECT_THRESHOLD: f32 = 0.1;
/// Threshold for edge selection (in world units, scaled by distance)
const EDGE_SELECT_THRESHOLD: f32 = 0.05;

/// Plugin for sub-object selection
pub struct MeshEditSelectionPlugin;

impl Plugin for MeshEditSelectionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, handle_sub_object_click);
    }
}

/// Handle mouse clicks for sub-object selection
fn handle_sub_object_click(
    mouse_button: Res<ButtonInput<MouseButton>>,
    key_input: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera_query: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    edit_mode: Res<EditModeState>,
    mut mesh_edit_state: ResMut<MeshEditState>,
    editable_query: Query<(&EditableMesh, &GlobalTransform)>,
    mut outbound: ResMut<OutboundUiMessages>,
) {
    // Only handle in mesh edit mode
    if edit_mode.mode != EditMode::MeshEdit {
        return;
    }

    // Only handle left click
    if !mouse_button.just_pressed(MouseButton::Left) {
        return;
    }

    let Some(target_entity) = mesh_edit_state.target_entity else {
        return;
    };

    let Ok((editable, mesh_transform)) = editable_query.get(target_entity) else {
        return;
    };

    let Ok(window) = windows.single() else {
        return;
    };

    let Ok((camera, camera_transform)) = camera_query.single() else {
        return;
    };

    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };

    // Get ray from camera through cursor
    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_pos) else {
        return;
    };

    let shift_held =
        key_input.pressed(KeyCode::ShiftLeft) || key_input.pressed(KeyCode::ShiftRight);

    // Perform raycast and selection
    if let Some(hit) = raycast_sub_object(
        &ray,
        &editable.half_edge_mesh,
        mesh_transform,
        mesh_edit_state.selection_mode,
    ) {
        handle_selection_hit(&mut mesh_edit_state, hit, shift_held);
        send_selection_update(&mesh_edit_state, &mut outbound);
    } else if !shift_held {
        // Click on empty space without shift - deselect all
        mesh_edit_state.clear_selection();
        send_selection_update(&mesh_edit_state, &mut outbound);
    }
}

/// Perform raycast against mesh to find sub-object hit
fn raycast_sub_object(
    ray: &Ray3d,
    mesh: &HalfEdgeMesh,
    transform: &GlobalTransform,
    selection_mode: MeshSelectionMode,
) -> Option<SubObjectHit> {
    // Transform ray to local space
    let inv_transform = transform.affine().inverse();
    let local_origin = inv_transform.transform_point3(ray.origin);
    let local_dir = inv_transform.transform_vector3(*ray.direction).normalize();

    // Find face intersection first
    let mut closest_hit: Option<(FaceId, Vec3, f32, Vec3)> = None; // (face_id, hit_pos, distance, barycentric)

    for face in mesh.faces() {
        let verts = mesh.get_face_vertices(face.id);
        if verts.len() < 3 {
            continue;
        }

        // Get triangle vertices
        let v0 = mesh.vertex(verts[0])?.position;
        let v1 = mesh.vertex(verts[1])?.position;
        let v2 = mesh.vertex(verts[2])?.position;

        // Möller-Trumbore intersection
        if let Some((t, u, v)) = ray_triangle_intersection(local_origin, local_dir, v0, v1, v2) {
            if t > 0.0 {
                if closest_hit.is_none() || t < closest_hit.as_ref()?.2 {
                    let hit_pos = local_origin + local_dir * t;
                    let bary = Vec3::new(1.0 - u - v, u, v);
                    closest_hit = Some((face.id, hit_pos, t, bary));
                }
            }
        }
    }

    let (face_id, local_hit_pos, distance, bary) = closest_hit?;
    let world_hit_pos = transform.transform_point(local_hit_pos);

    // Based on selection mode, determine what was actually hit
    let hit_type = match selection_mode {
        MeshSelectionMode::Face => SubObjectHitType::Face(face_id),
        MeshSelectionMode::Vertex => {
            // Find closest vertex to hit point
            let verts = mesh.get_face_vertices(face_id);
            let mut closest_vert = verts[0];
            let mut closest_dist = f32::MAX;

            for vid in &verts {
                if let Some(v) = mesh.vertex(*vid) {
                    let dist = v.position.distance(local_hit_pos);
                    if dist < closest_dist {
                        closest_dist = dist;
                        closest_vert = *vid;
                    }
                }
            }

            // Check if close enough to count as vertex selection
            let threshold = VERTEX_SELECT_THRESHOLD * (distance * 0.1_f32).max(0.1_f32);
            if closest_dist < threshold {
                SubObjectHitType::Vertex(closest_vert)
            } else {
                // Fall back to face if not close enough to any vertex
                SubObjectHitType::Face(face_id)
            }
        }
        MeshSelectionMode::Edge => {
            // Find closest edge to hit point
            let edges = mesh.get_face_half_edges(face_id);
            let mut closest_edge = edges[0];
            let mut closest_dist = f32::MAX;

            for he_id in &edges {
                if let Some(he) = mesh.half_edge(*he_id) {
                    if let (Some(v0), Some(dest)) =
                        (mesh.vertex(he.origin), mesh.get_half_edge_dest(*he_id))
                    {
                        if let Some(v1) = mesh.vertex(dest) {
                            let dist = point_to_line_segment_distance(
                                local_hit_pos,
                                v0.position,
                                v1.position,
                            );
                            if dist < closest_dist {
                                closest_dist = dist;
                                closest_edge = *he_id;
                            }
                        }
                    }
                }
            }

            // Check if close enough to count as edge selection
            let threshold = EDGE_SELECT_THRESHOLD * (distance * 0.1_f32).max(0.1_f32);
            if closest_dist < threshold {
                SubObjectHitType::Edge(closest_edge)
            } else {
                // Fall back to face if not close enough to any edge
                SubObjectHitType::Face(face_id)
            }
        }
    };

    Some(SubObjectHit {
        hit_type,
        world_position: world_hit_pos,
        distance,
    })
}

/// Möller-Trumbore ray-triangle intersection
/// Returns (t, u, v) where t is distance along ray, u and v are barycentric coords
fn ray_triangle_intersection(
    origin: Vec3,
    dir: Vec3,
    v0: Vec3,
    v1: Vec3,
    v2: Vec3,
) -> Option<(f32, f32, f32)> {
    let epsilon = 1e-6;

    let edge1 = v1 - v0;
    let edge2 = v2 - v0;

    let h = dir.cross(edge2);
    let a = edge1.dot(h);

    if a.abs() < epsilon {
        return None; // Ray parallel to triangle
    }

    let f = 1.0 / a;
    let s = origin - v0;
    let u = f * s.dot(h);

    if !(0.0..=1.0).contains(&u) {
        return None;
    }

    let q = s.cross(edge1);
    let v = f * dir.dot(q);

    if v < 0.0 || u + v > 1.0 {
        return None;
    }

    let t = f * edge2.dot(q);

    if t > epsilon {
        Some((t, u, v))
    } else {
        None
    }
}

/// Calculate distance from point to line segment
fn point_to_line_segment_distance(point: Vec3, line_start: Vec3, line_end: Vec3) -> f32 {
    let line = line_end - line_start;
    let line_len_sq = line.length_squared();

    if line_len_sq < 1e-10 {
        return point.distance(line_start);
    }

    let t = ((point - line_start).dot(line) / line_len_sq).clamp(0.0, 1.0);
    let projection = line_start + line * t;
    point.distance(projection)
}

/// Handle a selection hit
fn handle_selection_hit(state: &mut MeshEditState, hit: SubObjectHit, shift_held: bool) {
    match hit.hit_type {
        SubObjectHitType::Vertex(vid) => {
            if shift_held {
                // Toggle selection
                if state.selected_vertices.contains(&vid) {
                    state.selected_vertices.remove(&vid);
                } else {
                    state.selected_vertices.insert(vid);
                }
            } else {
                // Single select
                state.clear_selection();
                state.selected_vertices.insert(vid);
            }
        }
        SubObjectHitType::Edge(heid) => {
            if shift_held {
                if state.selected_edges.contains(&heid) {
                    state.selected_edges.remove(&heid);
                } else {
                    state.selected_edges.insert(heid);
                }
            } else {
                state.clear_selection();
                state.selected_edges.insert(heid);
            }
        }
        SubObjectHitType::Face(fid) => {
            if shift_held {
                if state.selected_faces.contains(&fid) {
                    state.selected_faces.remove(&fid);
                } else {
                    state.selected_faces.insert(fid);
                }
            } else {
                state.clear_selection();
                state.selected_faces.insert(fid);
            }
        }
    }
}

/// Send selection update to UI
fn send_selection_update(state: &MeshEditState, outbound: &mut OutboundUiMessages) {
    outbound.send(BevyToUi::MeshEditSelectionChanged {
        vertex_count: state.selected_vertices.len(),
        edge_count: state.selected_edges.len(),
        face_count: state.selected_faces.len(),
    });
}
