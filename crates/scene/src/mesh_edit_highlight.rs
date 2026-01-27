//! Selection highlighting for mesh edit mode
//!
//! Uses Bevy's gizmo system to render visual feedback for selected
//! vertices, edges, and faces during mesh editing.

use bevy::prelude::*;
use pentimento_ipc::EditMode;

use crate::edit_mode::EditModeState;
use crate::mesh_edit_mode::{EditableMesh, MeshEditState};

/// Plugin for mesh edit selection highlighting
pub struct MeshEditHighlightPlugin;

impl Plugin for MeshEditHighlightPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, render_mesh_edit_selection);
    }
}

/// Colors for selection highlighting
const VERTEX_COLOR: Color = Color::srgb(1.0, 0.5, 0.0); // Orange
const EDGE_COLOR: Color = Color::srgb(1.0, 0.7, 0.2); // Light orange
const FACE_COLOR: Color = Color::srgba(1.0, 0.5, 0.0, 0.3); // Transparent orange

/// Size of vertex points
const VERTEX_POINT_SIZE: f32 = 0.05;

/// Render selected vertices, edges, and faces using gizmos
fn render_mesh_edit_selection(
    edit_mode: Res<EditModeState>,
    mesh_edit_state: Res<MeshEditState>,
    editable_query: Query<(&EditableMesh, &GlobalTransform)>,
    mut gizmos: Gizmos,
) {
    // Only render in mesh edit mode
    if edit_mode.mode != EditMode::MeshEdit {
        return;
    }

    let Some(target_entity) = mesh_edit_state.target_entity else {
        return;
    };

    let Ok((editable, transform)) = editable_query.get(target_entity) else {
        return;
    };

    let mesh = &editable.half_edge_mesh;

    // Render selected vertices as small spheres
    for vertex_id in &mesh_edit_state.selected_vertices {
        if let Some(vertex) = mesh.vertex(*vertex_id) {
            let world_pos = transform.transform_point(vertex.position);
            // Draw a cross at the vertex position
            gizmos.sphere(world_pos, VERTEX_POINT_SIZE, VERTEX_COLOR);
        }
    }

    // Render selected edges as lines
    for he_id in &mesh_edit_state.selected_edges {
        if let Some(he) = mesh.half_edge(*he_id) {
            let origin = mesh.vertex(he.origin);
            let dest_id = mesh.get_half_edge_dest(*he_id);

            if let (Some(v0), Some(dest)) = (origin, dest_id) {
                if let Some(v1) = mesh.vertex(dest) {
                    let p0 = transform.transform_point(v0.position);
                    let p1 = transform.transform_point(v1.position);
                    gizmos.line(p0, p1, EDGE_COLOR);
                }
            }
        }
    }

    // Render selected faces as filled triangles
    // Note: Bevy gizmos don't support filled triangles, so we draw the edges
    for face_id in &mesh_edit_state.selected_faces {
        let verts = mesh.get_face_vertices(*face_id);
        if verts.len() < 3 {
            continue;
        }

        // Get world positions
        let positions: Vec<Vec3> = verts
            .iter()
            .filter_map(|vid| mesh.vertex(*vid))
            .map(|v| transform.transform_point(v.position))
            .collect();

        if positions.len() < 3 {
            continue;
        }

        // Draw face outline
        for i in 0..positions.len() {
            let next = (i + 1) % positions.len();
            gizmos.line(positions[i], positions[next], FACE_COLOR);
        }

        // Draw a small marker at the face center
        let center: Vec3 = positions.iter().sum::<Vec3>() / positions.len() as f32;
        let normal = if let Some(face) = mesh.face(*face_id) {
            transform.affine().transform_vector3(face.normal)
        } else {
            Vec3::Y
        };

        // Draw a small normal indicator
        gizmos.line(center, center + normal * 0.1, FACE_COLOR);
    }
}
