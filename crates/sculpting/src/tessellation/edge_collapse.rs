//! Edge collapse (simplification) algorithm.
//!
//! Collapsing an edge removes it by merging its two endpoints into one vertex.
//! This reduces mesh density when detail is no longer needed.
//!
//! # Safety Checks
//!
//! This module implements safety checks adapted from SculptGL (MIT License,
//! Copyright Stéphane Ginier) to prevent non-manifold geometry:
//!
//! 1. **Ring condition** - For each endpoint, ring_vertices.len() must equal
//!    ring_faces.len(). If they differ, the vertex is on a boundary.
//! 2. **Link condition** - Common neighbors must equal 2 (interior) or 1 (boundary).
//! 3. **Edge flip fallback** - When 3+ shared neighbors exist, suggest edge flip
//!    instead of collapse to avoid non-manifold topology.
//! 4. **Flip check** - Collapse must not flip any adjacent face normals.
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
//!
//! ## Reference
//!
//! Adapted from SculptGL Decimation.js (MIT License, Copyright Stéphane Ginier)
//! Source: <https://github.com/stephomi/sculptgl>

use glam::Vec3;
use painting::half_edge::{FaceId, HalfEdgeId, HalfEdgeMesh, VertexId};
use std::collections::HashSet;

// =============================================================================
// Collapse Check Types
// =============================================================================

/// Result of checking whether an edge can be collapsed.
///
/// This enum represents the three possible outcomes of the safety check:
/// - `Safe` - Edge can be collapsed, includes the optimal merge position
/// - `UseEdgeFlip` - Collapse would create non-manifold geometry, but an edge
///   flip operation can improve mesh quality instead
/// - `Rejected` - Edge cannot be collapsed or flipped, includes the reason
#[derive(Debug, Clone, PartialEq)]
pub enum CollapseCheck {
    /// Edge can be safely collapsed at the given position
    Safe(Vec3),
    /// Collapse would create non-manifold geometry (3+ shared neighbors),
    /// but an edge flip can improve mesh quality instead
    UseEdgeFlip,
    /// Edge cannot be collapsed, with the reason
    Rejected(CollapseRejection),
}

/// Reason why an edge collapse was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollapseRejection {
    /// Edge doesn't exist in the mesh
    InvalidEdge,
    /// Origin vertex is on a boundary (ring_vertices != ring_faces)
    OriginOnBoundary,
    /// Destination vertex is on a boundary (ring_vertices != ring_faces)
    DestinationOnBoundary,
    /// One of the opposite vertices (C or D) is on a boundary
    OppositeVertexOnBoundary,
    /// Link condition violated - wrong number of common neighbors
    LinkConditionFailed,
    /// Collapse would cause one or more face normals to flip
    WouldCauseFlip,
}

impl std::fmt::Display for CollapseRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidEdge => write!(f, "edge does not exist"),
            Self::OriginOnBoundary => write!(f, "origin vertex is on boundary"),
            Self::DestinationOnBoundary => write!(f, "destination vertex is on boundary"),
            Self::OppositeVertexOnBoundary => write!(f, "opposite vertex is on boundary"),
            Self::LinkConditionFailed => write!(f, "link condition violated"),
            Self::WouldCauseFlip => write!(f, "would cause face normal flip"),
        }
    }
}

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

// =============================================================================
// Ring Boundary Detection
// =============================================================================

/// Check if a vertex is on a "ring boundary" using SculptGL's pattern.
///
/// A vertex is considered on a ring boundary if its ring vertex count differs
/// from its ring face count. For interior vertices in a manifold mesh, these
/// counts should be equal.
///
/// # Reference
/// Adapted from SculptGL Decimation.js:
/// ```javascript
/// if (ring1.length !== tris1.length || ring2.length !== tris2.length)
///     return;  // vertices on the edge... we don't do anything
/// ```
fn is_ring_boundary_vertex(mesh: &HalfEdgeMesh, vertex_id: VertexId) -> bool {
    let ring_vertices = mesh.get_adjacent_vertices(vertex_id);
    let ring_faces = mesh.get_vertex_faces(vertex_id);

    // For interior vertices: ring_vertices.len() == ring_faces.len()
    // For boundary vertices: ring_vertices.len() != ring_faces.len()
    ring_vertices.len() != ring_faces.len()
}

// =============================================================================
// Safety Checks
// =============================================================================

/// Perform comprehensive safety checks before collapsing an edge.
///
/// This function implements all the safety checks from SculptGL's decimation
/// algorithm to prevent non-manifold geometry:
///
/// 1. Validate edge exists
/// 2. Check ring boundary condition for both endpoints
/// 3. Check ring boundary condition for opposite vertices
/// 4. Check link condition (common neighbors count)
/// 5. If 3+ shared neighbors, suggest edge flip instead
/// 6. Check for face normal flipping
///
/// # Returns
/// - `CollapseCheck::Safe(position)` - Edge can be safely collapsed
/// - `CollapseCheck::UseEdgeFlip` - Use edge flip instead of collapse
/// - `CollapseCheck::Rejected(reason)` - Edge cannot be collapsed
pub fn can_collapse_edge_safe(mesh: &HalfEdgeMesh, edge_id: HalfEdgeId) -> CollapseCheck {
    // ===== PHASE 1: Validate edge exists =====
    let Some(he) = mesh.half_edge(edge_id) else {
        return CollapseCheck::Rejected(CollapseRejection::InvalidEdge);
    };

    let v0_id = he.origin;
    let Some(next_he) = mesh.half_edge(he.next) else {
        return CollapseCheck::Rejected(CollapseRejection::InvalidEdge);
    };
    let v1_id = next_he.origin;

    // ===== PHASE 2: Ring boundary check for edge endpoints =====
    // From SculptGL: if (ring1.length !== tris1.length || ring2.length !== tris2.length) return;
    if is_ring_boundary_vertex(mesh, v0_id) {
        return CollapseCheck::Rejected(CollapseRejection::OriginOnBoundary);
    }
    if is_ring_boundary_vertex(mesh, v1_id) {
        return CollapseCheck::Rejected(CollapseRejection::DestinationOnBoundary);
    }

    // ===== PHASE 3: Ring boundary check for opposite vertices =====
    // Get the third vertices of the adjacent faces (C and D in the diagram)
    let Some(prev_he) = mesh.half_edge(he.prev) else {
        return CollapseCheck::Rejected(CollapseRejection::InvalidEdge);
    };
    let v_opp1 = prev_he.origin; // Opposite vertex in primary face

    // Check opposite vertex in twin face if it exists
    if let Some(twin_id) = he.twin {
        if let Some(twin_he) = mesh.half_edge(twin_id) {
            if let Some(twin_prev) = mesh.half_edge(twin_he.prev) {
                let v_opp2 = twin_prev.origin;

                // From SculptGL: check opposite vertices too
                if is_ring_boundary_vertex(mesh, v_opp1) || is_ring_boundary_vertex(mesh, v_opp2) {
                    return CollapseCheck::Rejected(CollapseRejection::OppositeVertexOnBoundary);
                }
            }
        }
    } else if is_ring_boundary_vertex(mesh, v_opp1) {
        return CollapseCheck::Rejected(CollapseRejection::OppositeVertexOnBoundary);
    }

    // ===== PHASE 4: Get 1-ring neighbors and check link condition =====
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

    // ===== PHASE 5: Check for edge flip fallback =====
    // From SculptGL: if (Utils.intersectionArrays(ring1, ring2).length >= 3) { /* edge flip */ }
    if common.len() >= 3 {
        return CollapseCheck::UseEdgeFlip;
    }

    // ===== PHASE 6: Standard link condition =====
    let is_boundary = he.twin.is_none();
    let expected_common = if is_boundary { 1 } else { 2 };

    if common.len() != expected_common {
        return CollapseCheck::Rejected(CollapseRejection::LinkConditionFailed);
    }

    // ===== PHASE 7: Calculate collapse position and check for flips =====
    let Some(new_pos) = calculate_collapse_position(mesh, edge_id) else {
        return CollapseCheck::Rejected(CollapseRejection::InvalidEdge);
    };

    if would_cause_flip(mesh, edge_id, new_pos) {
        return CollapseCheck::Rejected(CollapseRejection::WouldCauseFlip);
    }

    CollapseCheck::Safe(new_pos)
}

/// Legacy compatibility wrapper - returns bool for simple checks.
///
/// For new code, prefer `can_collapse_edge_safe()` which provides
/// more detailed feedback including edge flip suggestions.
pub fn can_collapse_edge(mesh: &HalfEdgeMesh, edge_id: HalfEdgeId) -> bool {
    matches!(can_collapse_edge_safe(mesh, edge_id), CollapseCheck::Safe(_))
}

/// Collapse an edge by merging its endpoints.
///
/// The first vertex (origin of the half-edge) survives and is moved to
/// the optimal collapse position. The second vertex and the adjacent faces
/// are removed.
///
/// # Invariants Maintained
/// - Mesh remains manifold (no edges with 3+ faces)
/// - All vertices have valence >= 3
/// - Half-edge twin pointers remain symmetric
/// - Face traversal loops remain valid
///
/// # Preconditions
/// - Edge satisfies link condition (common neighbors == 2 for interior, 1 for boundary)
/// - Neither endpoint is a ring boundary vertex
/// - Neither opposite vertex is a ring boundary vertex
/// - Collapse would not cause face flipping
///
/// # Postconditions
/// - Face count reduced by exactly 2 (interior edge) or 1 (boundary edge)
/// - Vertex count reduced by exactly 1
/// - Edge count reduced by exactly 3 (interior) or 2 (boundary)
///
/// Returns None if the collapse would create invalid topology.
/// Use `can_collapse_edge_safe()` to get detailed rejection reasons.
pub fn collapse_edge(mesh: &mut HalfEdgeMesh, edge_id: HalfEdgeId) -> Option<CollapseResult> {
    // Use the comprehensive safety check
    let check = can_collapse_edge_safe(mesh, edge_id);

    match check {
        CollapseCheck::Safe(new_pos) => {
            // Get vertex IDs before the collapse
            let he = mesh.half_edge(edge_id)?;
            let v0_id = he.origin;
            let next_he = mesh.half_edge(he.next)?;
            let v1_id = next_he.origin;

            // Delegate to the HalfEdgeMesh's collapse_edge_topology method
            let removed_faces = mesh.collapse_edge_topology(edge_id)?;

            // Move surviving vertex to optimal position
            mesh.set_vertex_position(v0_id, new_pos);

            Some(CollapseResult {
                surviving_vertex: v0_id,
                removed_vertex: v1_id,
                removed_faces,
            })
        }
        CollapseCheck::UseEdgeFlip | CollapseCheck::Rejected(_) => None,
    }
}

/// Attempt to collapse an edge, with edge flip fallback.
///
/// This function provides the full tessellation workflow:
/// 1. If edge can be safely collapsed, collapse it
/// 2. If edge flip is suggested (3+ shared neighbors), perform the flip
/// 3. Otherwise, return None
///
/// # Returns
/// - `Some(CollapseOrFlipResult::Collapsed(result))` - Edge was collapsed
/// - `Some(CollapseOrFlipResult::Flipped)` - Edge was flipped instead
/// - `None` - Neither operation was possible
pub fn collapse_or_flip_edge(
    mesh: &mut HalfEdgeMesh,
    edge_id: HalfEdgeId,
) -> Option<CollapseOrFlipResult> {
    let check = can_collapse_edge_safe(mesh, edge_id);

    match check {
        CollapseCheck::Safe(new_pos) => {
            let he = mesh.half_edge(edge_id)?;
            let v0_id = he.origin;
            let next_he = mesh.half_edge(he.next)?;
            let v1_id = next_he.origin;

            let removed_faces = mesh.collapse_edge_topology(edge_id)?;
            mesh.set_vertex_position(v0_id, new_pos);

            Some(CollapseOrFlipResult::Collapsed(CollapseResult {
                surviving_vertex: v0_id,
                removed_vertex: v1_id,
                removed_faces,
            }))
        }
        CollapseCheck::UseEdgeFlip => {
            if mesh.flip_edge_topology(edge_id) {
                Some(CollapseOrFlipResult::Flipped)
            } else {
                None
            }
        }
        CollapseCheck::Rejected(_) => None,
    }
}

/// Result of attempting to collapse or flip an edge.
#[derive(Debug, Clone)]
pub enum CollapseOrFlipResult {
    /// Edge was successfully collapsed
    Collapsed(CollapseResult),
    /// Edge was flipped instead of collapsed (due to 3+ shared neighbors)
    Flipped,
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

    #[test]
    fn test_collapse_check_variants() {
        // Test Safe variant
        let safe = CollapseCheck::Safe(Vec3::new(0.5, 0.0, 0.0));
        assert!(matches!(safe, CollapseCheck::Safe(_)));

        // Test UseEdgeFlip variant
        let flip = CollapseCheck::UseEdgeFlip;
        assert!(matches!(flip, CollapseCheck::UseEdgeFlip));

        // Test Rejected variants
        let rejected = CollapseCheck::Rejected(CollapseRejection::OriginOnBoundary);
        assert!(matches!(
            rejected,
            CollapseCheck::Rejected(CollapseRejection::OriginOnBoundary)
        ));
    }

    #[test]
    fn test_collapse_rejection_display() {
        assert_eq!(
            CollapseRejection::InvalidEdge.to_string(),
            "edge does not exist"
        );
        assert_eq!(
            CollapseRejection::OriginOnBoundary.to_string(),
            "origin vertex is on boundary"
        );
        assert_eq!(
            CollapseRejection::DestinationOnBoundary.to_string(),
            "destination vertex is on boundary"
        );
        assert_eq!(
            CollapseRejection::OppositeVertexOnBoundary.to_string(),
            "opposite vertex is on boundary"
        );
        assert_eq!(
            CollapseRejection::LinkConditionFailed.to_string(),
            "link condition violated"
        );
        assert_eq!(
            CollapseRejection::WouldCauseFlip.to_string(),
            "would cause face normal flip"
        );
    }

    #[test]
    fn test_collapse_or_flip_result_structure() {
        let collapsed = CollapseOrFlipResult::Collapsed(CollapseResult {
            surviving_vertex: VertexId(0),
            removed_vertex: VertexId(1),
            removed_faces: vec![FaceId(0)],
        });
        assert!(matches!(collapsed, CollapseOrFlipResult::Collapsed(_)));

        let flipped = CollapseOrFlipResult::Flipped;
        assert!(matches!(flipped, CollapseOrFlipResult::Flipped));
    }
}
