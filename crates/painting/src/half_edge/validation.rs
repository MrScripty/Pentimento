//! Validation methods for HalfEdgeMesh.
//!
//! Provides comprehensive mesh validation including:
//! - Basic connectivity validation
//! - Manifold property checking
//! - Topology consistency verification

use super::types::HalfEdgeError;
use super::HalfEdgeMesh;

impl HalfEdgeMesh {
    /// Debug-only mesh connectivity check.
    ///
    /// Validates that:
    /// 1. All vertices with outgoing_half_edge point to valid, non-orphaned half-edges
    /// 2. All non-orphaned half-edges have consistent next/prev cycles
    /// 3. All twin relationships are symmetric
    ///
    /// Orphaned half-edges (face = None) are skipped - they are expected after collapse operations.
    #[cfg(debug_assertions)]
    pub fn validate_connectivity(&self) -> Result<(), String> {
        // Check all vertices with outgoing_half_edge point to valid half-edges
        for v in &self.vertices {
            if let Some(he_id) = v.outgoing_half_edge {
                let he = self
                    .half_edge(he_id)
                    .ok_or_else(|| format!("Vertex {:?}: missing half-edge {:?}", v.id, he_id))?;
                if he.origin != v.id {
                    return Err(format!(
                        "Vertex {:?}: outgoing edge {:?} has wrong origin {:?}",
                        v.id, he_id, he.origin
                    ));
                }
                if he.face.is_none() {
                    return Err(format!(
                        "Vertex {:?}: outgoing edge {:?} is orphaned (face = None)",
                        v.id, he_id
                    ));
                }
            }
        }

        // Check all half-edges with faces have consistent cycles
        for he in &self.half_edges {
            if he.face.is_none() {
                continue; // Skip orphaned half-edges
            }

            // next.prev should be self
            if let Some(next) = self.half_edge(he.next) {
                if next.prev != he.id {
                    return Err(format!(
                        "Half-edge {:?}: next.prev = {:?}, expected {:?}",
                        he.id, next.prev, he.id
                    ));
                }
            } else {
                return Err(format!(
                    "Half-edge {:?}: next {:?} doesn't exist",
                    he.id, he.next
                ));
            }

            // prev.next should be self
            if let Some(prev) = self.half_edge(he.prev) {
                if prev.next != he.id {
                    return Err(format!(
                        "Half-edge {:?}: prev.next = {:?}, expected {:?}",
                        he.id, prev.next, he.id
                    ));
                }
            } else {
                return Err(format!(
                    "Half-edge {:?}: prev {:?} doesn't exist",
                    he.id, he.prev
                ));
            }

            // twin.twin should be self (if twin exists)
            if let Some(twin_id) = he.twin {
                if let Some(twin) = self.half_edge(twin_id) {
                    if twin.twin != Some(he.id) {
                        return Err(format!(
                            "Half-edge {:?}: twin.twin = {:?}, expected Some({:?})",
                            he.id, twin.twin, he.id
                        ));
                    }
                } else {
                    return Err(format!(
                        "Half-edge {:?}: twin {:?} doesn't exist",
                        he.id, twin_id
                    ));
                }
            }
        }

        Ok(())
    }

    /// No-op in release builds.
    #[cfg(not(debug_assertions))]
    pub fn validate_connectivity(&self) -> Result<(), String> {
        Ok(())
    }

    /// Validate the mesh topology.
    ///
    /// Skips orphaned half-edges (those with face = None) which are expected
    /// after collapse operations.
    pub fn validate(&self) -> Result<(), HalfEdgeError> {
        // Check twin symmetry (only for non-orphaned half-edges)
        for he in &self.half_edges {
            // Skip orphaned half-edges
            if he.face.is_none() {
                continue;
            }

            if let Some(twin_id) = he.twin {
                let twin = self
                    .half_edge(twin_id)
                    .ok_or(HalfEdgeError::InvalidTopology("Invalid twin reference".into()))?;
                if twin.twin != Some(he.id) {
                    return Err(HalfEdgeError::InvalidTopology(
                        "Twin symmetry violated".into(),
                    ));
                }
            }
        }

        // Check next/prev cycle for each face
        for face in &self.faces {
            let start = face.half_edge;

            // Check if the face's starting half-edge is orphaned (shouldn't happen)
            if let Some(start_he) = self.half_edge(start) {
                if start_he.face.is_none() {
                    // This face was removed via collapse - skip it
                    continue;
                }
            }

            let mut current = start;
            let mut count = 0;

            loop {
                let he = self
                    .half_edge(current)
                    .ok_or(HalfEdgeError::InvalidTopology("Invalid half-edge".into()))?;

                if he.face != Some(face.id) {
                    return Err(HalfEdgeError::InvalidTopology(
                        "Half-edge face mismatch".into(),
                    ));
                }

                current = he.next;
                count += 1;

                if count > 1000 {
                    return Err(HalfEdgeError::InvalidTopology(
                        "Infinite loop in face".into(),
                    ));
                }

                if current == start {
                    break;
                }
            }

            if count < 3 {
                return Err(HalfEdgeError::InvalidTopology(
                    "Face has fewer than 3 edges".into(),
                ));
            }
        }

        Ok(())
    }

    // =========================================================================
    // Manifold Validation
    // =========================================================================

    /// Check if the mesh is manifold.
    ///
    /// A mesh is manifold if:
    /// 1. Every edge is shared by exactly 1 or 2 faces (boundary or interior)
    /// 2. Every vertex has a single continuous ring of faces around it
    /// 3. No self-intersections (not checked here - would require spatial queries)
    ///
    /// Returns a detailed error if the mesh is not manifold.
    pub fn check_manifold(&self) -> Result<(), ManifoldError> {
        // Check 1: Every edge has at most 2 adjacent faces
        for he in &self.half_edges {
            if he.face.is_none() {
                continue; // Skip orphaned half-edges
            }

            // If twin exists, it should point back to us
            if let Some(twin_id) = he.twin {
                if let Some(twin) = self.half_edge(twin_id) {
                    if twin.twin != Some(he.id) {
                        return Err(ManifoldError::NonManifoldEdge {
                            edge_id: he.id,
                            reason: "twin symmetry broken".to_string(),
                        });
                    }
                    // Check that twin points in opposite direction
                    let he_dest = self.get_half_edge_dest(he.id);
                    if he_dest != Some(twin.origin) {
                        return Err(ManifoldError::NonManifoldEdge {
                            edge_id: he.id,
                            reason: "twin direction mismatch".to_string(),
                        });
                    }
                }
            }
        }

        // Check 2: Every vertex has a continuous ring of faces
        // This is checked by ensuring ring_vertices.len() matches ring_faces.len()
        // for interior vertices
        for vertex in &self.vertices {
            if vertex.outgoing_half_edge.is_none() {
                continue; // Skip orphaned vertices
            }

            let ring_vertices = self.get_adjacent_vertices(vertex.id);
            let ring_faces = self.get_vertex_faces(vertex.id);

            // For interior vertices: ring_vertices == ring_faces
            // For boundary vertices: ring_vertices == ring_faces + 1
            let is_boundary = self.is_boundary_vertex(vertex.id);

            if !ring_vertices.is_empty() && !ring_faces.is_empty() {
                let expected_diff = if is_boundary { 1 } else { 0 };
                let actual_diff = ring_vertices.len().saturating_sub(ring_faces.len());

                if actual_diff != expected_diff && actual_diff != 0 {
                    return Err(ManifoldError::NonManifoldVertex {
                        vertex_id: vertex.id,
                        ring_vertices: ring_vertices.len(),
                        ring_faces: ring_faces.len(),
                        is_boundary,
                    });
                }
            }
        }

        // Check 3: All vertices have valence >= 3 (except isolated vertices)
        for vertex in &self.vertices {
            if vertex.outgoing_half_edge.is_none() {
                continue;
            }

            let valence = self.get_adjacent_vertices(vertex.id).len();
            if valence > 0 && valence < 3 {
                return Err(ManifoldError::InvalidValence {
                    vertex_id: vertex.id,
                    valence,
                });
            }
        }

        Ok(())
    }

    /// Quick check if the mesh appears to be manifold (fast heuristic).
    ///
    /// This is faster than `check_manifold()` but may miss some issues.
    /// Good for runtime checks during tessellation.
    pub fn is_likely_manifold(&self) -> bool {
        // Quick check: sample a few vertices for ring condition
        let sample_size = (self.vertices.len() / 10).max(5).min(self.vertices.len());

        for i in 0..sample_size {
            let vertex = &self.vertices[i];
            if vertex.outgoing_half_edge.is_none() {
                continue;
            }

            let ring_v = self.get_adjacent_vertices(vertex.id).len();
            let ring_f = self.get_vertex_faces(vertex.id).len();

            // Interior vertex should have ring_v == ring_f
            // Boundary vertex should have ring_v == ring_f + 1
            if ring_v > 0 && ring_f > 0 {
                let diff = ring_v.saturating_sub(ring_f);
                if diff > 1 {
                    return false;
                }
            }
        }

        true
    }
}

/// Error types for manifold validation.
#[derive(Debug, Clone, PartialEq)]
pub enum ManifoldError {
    /// An edge is shared by more than 2 faces
    NonManifoldEdge {
        edge_id: super::types::HalfEdgeId,
        reason: String,
    },
    /// A vertex has a broken ring structure
    NonManifoldVertex {
        vertex_id: super::types::VertexId,
        ring_vertices: usize,
        ring_faces: usize,
        is_boundary: bool,
    },
    /// A vertex has valence less than 3
    InvalidValence {
        vertex_id: super::types::VertexId,
        valence: usize,
    },
}

impl std::fmt::Display for ManifoldError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonManifoldEdge { edge_id, reason } => {
                write!(f, "Non-manifold edge {:?}: {}", edge_id, reason)
            }
            Self::NonManifoldVertex {
                vertex_id,
                ring_vertices,
                ring_faces,
                is_boundary,
            } => {
                write!(
                    f,
                    "Non-manifold vertex {:?}: {} ring vertices, {} ring faces (boundary={})",
                    vertex_id, ring_vertices, ring_faces, is_boundary
                )
            }
            Self::InvalidValence { vertex_id, valence } => {
                write!(
                    f,
                    "Invalid valence at vertex {:?}: {} < 3",
                    vertex_id, valence
                )
            }
        }
    }
}

impl std::error::Error for ManifoldError {}
