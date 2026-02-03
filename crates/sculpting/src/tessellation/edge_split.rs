//! Edge split (subdivision) algorithm.
//!
//! Splitting an edge creates a new vertex at the midpoint and subdivides
//! adjacent triangles. This increases mesh density for finer detail.
//!
//! ## Curvature-Aware Positioning
//!
//! Instead of placing the new vertex at the simple midpoint, this module
//! supports curvature-aware positioning that preserves surface curvature:
//!
//! 1. Calculate the angle between the vertex normals at the edge endpoints
//! 2. Offset the midpoint along the averaged normal proportionally to the angle
//!
//! This prevents flattening of curved surfaces during subdivision.
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
//! The new vertex M is created at the (optionally curvature-offset) midpoint of edge AB.
//! Each triangle (ABC, ABD) becomes two triangles (AMC, MBC, AMD, MBD).
//!
//! ## Reference
//!
//! Curvature-aware positioning adapted from SculptGL Subdivision.js
//! (MIT License, Copyright Stéphane Ginier)

use glam::{Vec2, Vec3};
use painting::half_edge::{FaceId, HalfEdgeId, HalfEdgeMesh, VertexId};

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
    // Delegate to the HalfEdgeMesh's split_edge_topology method
    let (new_vertex, new_faces) = mesh.split_edge_topology(edge_id)?;

    Some(SplitResult {
        new_vertex,
        new_faces,
        new_half_edges: Vec::new(), // Not tracked separately
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
/// For curvature-preserving splits, use `calculate_curvature_aware_split_position`.
pub fn calculate_split_position(mesh: &HalfEdgeMesh, edge_id: HalfEdgeId) -> Option<Vec3> {
    let he = mesh.half_edge(edge_id)?;
    let v0 = mesh.vertex(he.origin)?;

    let next_he = mesh.half_edge(he.next)?;
    let v1 = mesh.vertex(next_he.origin)?;

    Some((v0.position + v1.position) * 0.5)
}

/// Calculate curvature-aware split position for preserving surface curvature.
///
/// Instead of placing the new vertex at the simple midpoint, this function
/// offsets it along the averaged normal based on the angle between the
/// endpoint normals. This preserves surface curvature instead of flattening it.
///
/// # Algorithm
///
/// 1. Calculate midpoint of edge AB
/// 2. Compute averaged normal: (n0 + n1) / 2
/// 3. Calculate angle between n0 and n1: θ = acos(n0 · n1)
/// 4. Calculate offset: θ * 0.12 * edge_length
/// 5. Determine direction based on edge-normal relationship
/// 6. New position = midpoint + averaged_normal * offset
///
/// # Reference
///
/// Adapted from SculptGL Subdivision.js:
/// ```javascript
/// var dot = n1x * n2x + n1y * n2y + n1z * n2z;
/// var angle = Math.acos(dot);
/// var offset = angle * 0.12 * edgeLength;
/// // Direction based on edge/normal relationship
/// if ((edgex * (n1x - n2x) + edgey * (n1y - n2y) + edgez * (n1z - n2z)) < 0)
///     offset = -offset;
/// vAr[id] = (v1x + v2x) * 0.5 + n1n2x * offset;
/// ```
pub fn calculate_curvature_aware_split_position(
    mesh: &HalfEdgeMesh,
    edge_id: HalfEdgeId,
) -> Option<Vec3> {
    let he = mesh.half_edge(edge_id)?;
    let v0 = mesh.vertex(he.origin)?;

    let next_he = mesh.half_edge(he.next)?;
    let v1 = mesh.vertex(next_he.origin)?;

    // Get positions and normals
    let p0 = v0.position;
    let p1 = v1.position;
    let n0 = v0.normal.normalize_or_zero();
    let n1 = v1.normal.normalize_or_zero();

    // Calculate midpoint
    let midpoint = (p0 + p1) * 0.5;

    // Calculate averaged normal
    let avg_normal = n0 + n1;
    let avg_normal_len_sq = avg_normal.length_squared();

    // If normals are opposite or zero, fall back to linear midpoint
    if avg_normal_len_sq < 0.001 {
        return Some(midpoint);
    }

    // Calculate angle between normals
    let dot = n0.dot(n1).clamp(-1.0, 1.0);
    let angle = dot.acos();

    // If angle is very small, just use midpoint
    if angle < 0.01 {
        return Some(midpoint);
    }

    // Calculate edge vector and length
    let edge = p0 - p1;
    let edge_length = edge.length();

    // Calculate offset proportional to angle and edge length
    // The 0.12 factor comes from SculptGL - empirically tuned for good results
    const CURVATURE_FACTOR: f32 = 0.12;
    let mut offset = angle * CURVATURE_FACTOR * edge_length;

    // Normalize the offset by the averaged normal length
    offset /= avg_normal_len_sq.sqrt();

    // Determine direction: if edge aligns with normal difference, flip offset
    // This ensures we offset in the correct direction to preserve curvature
    let normal_diff = n0 - n1;
    if edge.dot(normal_diff) < 0.0 {
        offset = -offset;
    }

    // Calculate final position
    let new_position = midpoint + avg_normal * offset;

    Some(new_position)
}

/// Split an edge with curvature-aware positioning.
///
/// This is a convenience wrapper that uses curvature-aware positioning
/// instead of simple midpoint positioning.
pub fn split_edge_curvature_aware(
    mesh: &mut HalfEdgeMesh,
    edge_id: HalfEdgeId,
) -> Option<SplitResult> {
    // Calculate the curvature-aware position first
    let new_pos = calculate_curvature_aware_split_position(mesh, edge_id)?;

    // Perform the standard split
    let (new_vertex, new_faces) = mesh.split_edge_topology(edge_id)?;

    // Update the new vertex position to the curvature-aware position
    mesh.set_vertex_position(new_vertex, new_pos);

    // The split already set an interpolated normal from the edge endpoints,
    // which is a reasonable approximation for curvature-aware splits.

    Some(SplitResult {
        new_vertex,
        new_faces,
        new_half_edges: Vec::new(),
    })
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
