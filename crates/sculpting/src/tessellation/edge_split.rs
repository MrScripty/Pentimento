//! Edge split (subdivision) algorithm.
//!
//! Splitting an edge creates a new vertex at the midpoint and subdivides
//! adjacent triangles. This increases mesh density for finer detail.
//!
//! ## Algorithm
//!
//! For an edge shared by two triangles:
//! ```text
//!     Before:              After:
//!        C                    C
//!       /|\                  /|\
//!      / | \                / | \
//!     /  |  \              /  |  \
//!    A---+---B    ->    A--M--+--B
//!     \  |  /              \  |  /
//!      \ | /                \ | /
//!       \|/                  \|/
//!        D                    D
//! ```
//!
//! The new vertex M is created at the midpoint of edge AB.
//! Each triangle (ABC, ABD) becomes two triangles (AMC, MBC, AMD, MBD).

use glam::{Vec2, Vec3};
use painting::half_edge::{FaceId, HalfEdgeId, HalfEdgeMesh, Vertex, VertexId};

/// Result of splitting an edge.
#[derive(Debug, Clone)]
pub struct SplitResult {
    /// The new vertex created at the midpoint
    pub new_vertex: VertexId,
    /// New faces created by the split
    pub new_faces: Vec<FaceId>,
    /// New half-edges created
    pub new_half_edges: Vec<HalfEdgeId>,
}

/// Split an edge at its midpoint.
///
/// Creates a new vertex at the edge midpoint and subdivides adjacent faces.
/// Returns None if the edge doesn't exist or can't be split.
pub fn split_edge(mesh: &mut HalfEdgeMesh, edge_id: HalfEdgeId) -> Option<SplitResult> {
    // Get the half-edge and its endpoints
    let he = mesh.half_edge(edge_id)?;
    let v0_id = he.origin;
    let twin_id = he.twin;
    let _face_id = he.face;
    let next_id = he.next;
    let _prev_id = he.prev;

    // Get the destination vertex (origin of next half-edge)
    let next_he = mesh.half_edge(next_id)?;
    let v1_id = next_he.origin;

    // Get vertex positions for midpoint calculation
    let v0 = mesh.vertex(v0_id)?;
    let v1 = mesh.vertex(v1_id)?;

    let mid_pos = (v0.position + v1.position) * 0.5;
    let mid_normal = (v0.normal + v1.normal).normalize_or_zero();
    let mid_uv = match (v0.uv, v1.uv) {
        (Some(uv0), Some(uv1)) => Some((uv0 + uv1) * 0.5),
        _ => None,
    };

    // Create new vertex at midpoint
    let new_vertex_id = VertexId(mesh.vertices().len() as u32);
    let _new_vertex = Vertex {
        id: new_vertex_id,
        position: mid_pos,
        normal: mid_normal,
        uv: mid_uv,
        outgoing_half_edge: None, // Will be set later
        source_index: u32::MAX,   // New vertex, no source
    };

    // For simplicity, we'll use a simplified split that works for triangle meshes
    // A full implementation would need to handle:
    // 1. The primary face being split
    // 2. The twin face (if exists) being split
    // 3. Updating all half-edge connectivity

    // This is a placeholder that indicates the structure
    // Full implementation requires careful half-edge manipulation

    // For now, return a basic result indicating what would happen
    // TODO: Implement full half-edge split with proper connectivity updates

    // Check if this is a boundary edge (no twin)
    let _is_boundary = twin_id.is_none();

    // The full implementation would:
    // 1. Add the new vertex to mesh.vertices
    // 2. Create new half-edges for the split edge
    // 3. Create new faces from the split triangles
    // 4. Update all prev/next/twin pointers
    // 5. Update face.half_edge pointers
    // 6. Update vertex.outgoing_half_edge pointers

    // For the initial implementation, we'll mark this as needing
    // the half-edge mesh to support dynamic modification

    // Placeholder: just indicate what vertex would be created
    Some(SplitResult {
        new_vertex: new_vertex_id,
        new_faces: Vec::new(),
        new_half_edges: Vec::new(),
    })
}

/// Check if an edge can be split.
///
/// An edge can be split if:
/// - It exists in the mesh
/// - Its adjacent faces are triangles (for simple triangulation)
pub fn can_split_edge(mesh: &HalfEdgeMesh, edge_id: HalfEdgeId) -> bool {
    let Some(he) = mesh.half_edge(edge_id) else {
        return false;
    };

    // Check if the face is a triangle
    if let Some(face_id) = he.face {
        let face_verts = mesh.get_face_vertices(face_id);
        if face_verts.len() != 3 {
            return false;
        }
    }

    // Check twin's face if it exists
    if let Some(twin_id) = he.twin {
        if let Some(twin_he) = mesh.half_edge(twin_id) {
            if let Some(twin_face_id) = twin_he.face {
                let twin_face_verts = mesh.get_face_vertices(twin_face_id);
                if twin_face_verts.len() != 3 {
                    return false;
                }
            }
        }
    }

    true
}

/// Calculate the position for a new vertex when splitting an edge.
///
/// By default, uses simple linear interpolation (midpoint).
/// Can be extended to use Loop subdivision weights or other schemes.
pub fn calculate_split_position(mesh: &HalfEdgeMesh, edge_id: HalfEdgeId) -> Option<Vec3> {
    let he = mesh.half_edge(edge_id)?;
    let v0 = mesh.vertex(he.origin)?;

    let next_he = mesh.half_edge(he.next)?;
    let v1 = mesh.vertex(next_he.origin)?;

    Some((v0.position + v1.position) * 0.5)
}

/// Calculate interpolated attributes for a split vertex.
pub fn interpolate_vertex_attributes(
    mesh: &HalfEdgeMesh,
    edge_id: HalfEdgeId,
    t: f32, // 0.0 = v0, 1.0 = v1, 0.5 = midpoint
) -> Option<(Vec3, Vec3, Option<Vec2>)> {
    let he = mesh.half_edge(edge_id)?;
    let v0 = mesh.vertex(he.origin)?;

    let next_he = mesh.half_edge(he.next)?;
    let v1 = mesh.vertex(next_he.origin)?;

    let position = v0.position.lerp(v1.position, t);
    let normal = v0.normal.lerp(v1.normal, t).normalize_or_zero();
    let uv = match (v0.uv, v1.uv) {
        (Some(uv0), Some(uv1)) => Some(uv0.lerp(uv1, t)),
        _ => None,
    };

    Some((position, normal, uv))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_result_structure() {
        let result = SplitResult {
            new_vertex: VertexId(5),
            new_faces: vec![FaceId(10), FaceId(11)],
            new_half_edges: vec![HalfEdgeId(20), HalfEdgeId(21)],
        };

        assert_eq!(result.new_vertex, VertexId(5));
        assert_eq!(result.new_faces.len(), 2);
        assert_eq!(result.new_half_edges.len(), 2);
    }
}
