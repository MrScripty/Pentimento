//! Edge collapse (simplification) algorithm.
//!
//! Collapsing an edge removes it by merging its two endpoints into one vertex.
//! This reduces mesh density when detail is no longer needed.
//!
//! ## Link Condition
//!
//! An edge can only be collapsed if it satisfies the "link condition":
//! the set of vertices adjacent to both endpoints must have exactly 2 vertices
//! (the two vertices of the triangles sharing the edge).
//!
//! This prevents topology changes like creating non-manifold geometry.
//!
//! ## Algorithm
//!
//! For an edge AB shared by two triangles:
//! ```text
//!     Before:              After:
//!        C                    C
//!       /|\                  / \
//!      / | \                /   \
//!     /  |  \              /     \
//!    A---+---B    ->      M-------+
//!     \  |  /              \     /
//!      \ | /                \   /
//!       \|/                  \ /
//!        D                    D
//! ```
//!
//! Vertices A and B are merged into M (typically at the midpoint).
//! The two triangles sharing edge AB are removed.

use glam::Vec3;
use painting::half_edge::{FaceId, HalfEdgeId, HalfEdgeMesh, VertexId};
use std::collections::HashSet;

/// Result of collapsing an edge.
#[derive(Debug, Clone)]
pub struct CollapseResult {
    /// The vertex that remains after collapse (one of the original endpoints)
    pub surviving_vertex: VertexId,
    /// The vertex that was removed
    pub removed_vertex: VertexId,
    /// Faces that were removed by the collapse
    pub removed_faces: Vec<FaceId>,
}

/// Check if an edge can be collapsed without creating invalid topology.
///
/// This checks the "link condition": the intersection of the 1-ring neighborhoods
/// of the two endpoints should only contain the two vertices opposite the edge
/// (for an interior edge) or one vertex (for a boundary edge).
pub fn can_collapse_edge(mesh: &HalfEdgeMesh, edge_id: HalfEdgeId) -> bool {
    let Some(he) = mesh.half_edge(edge_id) else {
        return false;
    };

    let v0_id = he.origin;

    // Get destination vertex
    let Some(next_he) = mesh.half_edge(he.next) else {
        return false;
    };
    let v1_id = next_he.origin;

    // Get 1-ring neighbors of each vertex
    let v0_neighbors: HashSet<VertexId> = mesh
        .get_adjacent_vertices(v0_id)
        .into_iter()
        .collect();

    let v1_neighbors: HashSet<VertexId> = mesh
        .get_adjacent_vertices(v1_id)
        .into_iter()
        .collect();

    // Find common neighbors (excluding v0 and v1 themselves)
    let common: HashSet<VertexId> = v0_neighbors
        .intersection(&v1_neighbors)
        .copied()
        .filter(|&v| v != v0_id && v != v1_id)
        .collect();

    // Link condition: for an interior edge, there should be exactly 2 common vertices
    // (the vertices of the two adjacent triangles opposite the edge)
    // For a boundary edge, there should be exactly 1 common vertex

    let is_boundary = he.twin.is_none();

    if is_boundary {
        common.len() == 1
    } else {
        common.len() == 2
    }
}

/// Collapse an edge by merging its endpoints.
///
/// The first vertex (origin of the half-edge) survives and is moved to
/// the midpoint. The second vertex and the adjacent faces are removed.
///
/// Returns None if the collapse would create invalid topology.
pub fn collapse_edge(mesh: &mut HalfEdgeMesh, edge_id: HalfEdgeId) -> Option<CollapseResult> {
    if !can_collapse_edge(mesh, edge_id) {
        return None;
    }

    let he = mesh.half_edge(edge_id)?;
    let v0_id = he.origin;
    let twin_id = he.twin;
    let face_id = he.face;

    // Get destination vertex
    let next_he = mesh.half_edge(he.next)?;
    let v1_id = next_he.origin;

    // Calculate midpoint position for the surviving vertex
    let v0 = mesh.vertex(v0_id)?;
    let v1 = mesh.vertex(v1_id)?;
    let mid_pos = (v0.position + v1.position) * 0.5;
    let mid_normal = (v0.normal + v1.normal).normalize_or_zero();

    // Collect faces to remove
    let mut removed_faces = Vec::new();
    if let Some(fid) = face_id {
        removed_faces.push(fid);
    }
    if let Some(tid) = twin_id {
        if let Some(twin_he) = mesh.half_edge(tid) {
            if let Some(twin_face_id) = twin_he.face {
                removed_faces.push(twin_face_id);
            }
        }
    }

    // TODO: Implement the actual collapse operation
    // This requires:
    // 1. Move v0 to midpoint
    // 2. Redirect all half-edges pointing to v1 to point to v0
    // 3. Remove the faces sharing the collapsed edge
    // 4. Update all half-edge connectivity
    // 5. Remove v1 from the vertex list (or mark as invalid)

    // For now, just move the surviving vertex to midpoint
    // Full implementation requires mesh mutation support
    mesh.set_vertex_position(v0_id, mid_pos);
    if let Some(v) = mesh.vertex_mut(v0_id) {
        v.normal = mid_normal;
    }

    Some(CollapseResult {
        surviving_vertex: v0_id,
        removed_vertex: v1_id,
        removed_faces,
    })
}

/// Calculate the optimal position for the vertex after collapse.
///
/// Uses midpoint by default, but could be extended to use
/// quadric error metrics (QEM) for better quality.
pub fn calculate_collapse_position(mesh: &HalfEdgeMesh, edge_id: HalfEdgeId) -> Option<Vec3> {
    let he = mesh.half_edge(edge_id)?;
    let v0 = mesh.vertex(he.origin)?;

    let next_he = mesh.half_edge(he.next)?;
    let v1 = mesh.vertex(next_he.origin)?;

    Some((v0.position + v1.position) * 0.5)
}

/// Check if collapsing an edge would cause face flipping.
///
/// Face flipping occurs when a face's normal reverses direction after
/// a vertex move, which can cause rendering artifacts.
pub fn would_cause_flip(mesh: &HalfEdgeMesh, edge_id: HalfEdgeId, new_pos: Vec3) -> bool {
    let Some(he) = mesh.half_edge(edge_id) else {
        return true;
    };

    let v0_id = he.origin;

    // Check all faces adjacent to v0
    let faces = mesh.get_vertex_faces(v0_id);

    for face_id in faces {
        let face_verts = mesh.get_face_vertices(face_id);
        if face_verts.len() < 3 {
            continue;
        }

        // Get current positions
        let positions: Vec<Vec3> = face_verts
            .iter()
            .filter_map(|&vid| mesh.vertex(vid).map(|v| v.position))
            .collect();

        if positions.len() < 3 {
            continue;
        }

        // Calculate current normal
        let e1 = positions[1] - positions[0];
        let e2 = positions[2] - positions[0];
        let current_normal = e1.cross(e2);

        // Calculate new positions (replacing v0 with new_pos)
        let new_positions: Vec<Vec3> = face_verts
            .iter()
            .filter_map(|&vid| {
                if vid == v0_id {
                    Some(new_pos)
                } else {
                    mesh.vertex(vid).map(|v| v.position)
                }
            })
            .collect();

        if new_positions.len() < 3 {
            continue;
        }

        // Calculate new normal
        let e1_new = new_positions[1] - new_positions[0];
        let e2_new = new_positions[2] - new_positions[0];
        let new_normal = e1_new.cross(e2_new);

        // Check if normal flipped (dot product negative)
        if current_normal.dot(new_normal) < 0.0 {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collapse_result_structure() {
        let result = CollapseResult {
            surviving_vertex: VertexId(0),
            removed_vertex: VertexId(1),
            removed_faces: vec![FaceId(0), FaceId(1)],
        };

        assert_eq!(result.surviving_vertex, VertexId(0));
        assert_eq!(result.removed_vertex, VertexId(1));
        assert_eq!(result.removed_faces.len(), 2);
    }
}
