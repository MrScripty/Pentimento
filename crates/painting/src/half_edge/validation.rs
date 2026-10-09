//! Validation derives incidence from every live face, independently of ring walks.

use std::collections::{HashMap, HashSet};

use super::HalfEdgeMesh;
use super::types::{FaceId, HalfEdgeError, HalfEdgeId, VertexId};

impl HalfEdgeMesh {
    /// Inspect all live face cycles without modifying the mesh. Tombstones left
    /// by a collapse are admitted; malformed live geometry is never discarded.
    pub(crate) fn live_face_cycles(&self) -> Result<Vec<(FaceId, Vec<HalfEdgeId>)>, HalfEdgeError> {
        let invalid = |reason: &str| HalfEdgeError::InvalidTopology(reason.into());
        let mut cycles = Vec::new();
        let mut visited = HashSet::new();
        let mut directed = HashSet::new();
        let mut facets = HashSet::new();
        for (i, face) in self.faces.iter().enumerate() {
            if face.id != FaceId(i as u32) {
                return Err(invalid("Face ID/index mismatch"));
            }
            let first = self
                .half_edge(face.half_edge)
                .ok_or_else(|| invalid("Invalid face start"))?;
            if first.face.is_none() {
                continue;
            }
            let mut edges = Vec::new();
            let mut vertices = HashSet::new();
            let mut current = face.half_edge;
            loop {
                let edge = self
                    .half_edge(current)
                    .ok_or_else(|| invalid("Invalid face half-edge"))?;
                if edge.id != current || edge.face != Some(face.id) || !visited.insert(current) {
                    return Err(invalid("Inconsistent or repeated face half-edge"));
                }
                if self.vertex(edge.origin).is_none_or(|v| v.id != edge.origin)
                    || !vertices.insert(edge.origin)
                {
                    return Err(invalid("Invalid or repeated face vertex"));
                }
                let next = self
                    .half_edge(edge.next)
                    .ok_or_else(|| invalid("Invalid next edge"))?;
                let prev = self
                    .half_edge(edge.prev)
                    .ok_or_else(|| invalid("Invalid previous edge"))?;
                if next.prev != current || prev.next != current {
                    return Err(invalid("Broken next/previous cycle"));
                }
                if !directed.insert((edge.origin, next.origin)) {
                    return Err(HalfEdgeError::NonManifoldEdge);
                }
                edges.push(current);
                current = edge.next;
                if current == face.half_edge {
                    break;
                }
            }
            if edges.len() < 3 {
                return Err(invalid("Face has fewer than three vertices"));
            }
            // Editable faces are unique facets, not overlapping copies of the
            // same facet with opposite winding. Such a two-cell surface has
            // parallel link incidences and cannot be represented after a split.
            let mut facet: Vec<_> = vertices.into_iter().collect();
            facet.sort_by_key(|id| id.0);
            if !facets.insert(facet) {
                return Err(invalid(
                    "Duplicate face vertices (including reversed winding)",
                ));
            }
            cycles.push((face.id, edges));
        }
        for (i, edge) in self.half_edges.iter().enumerate() {
            if edge.id != HalfEdgeId(i as u32) {
                return Err(invalid("Half-edge ID/index mismatch"));
            }
            if edge.face.is_some() && !visited.contains(&edge.id) {
                return Err(invalid("Live half-edge is not reachable from its face"));
            }
        }
        Ok(cycles)
    }

    /// Validate live connectivity in every build, including outgoing references,
    /// edge ownership, complete twin pairing and face next/previous cycles.
    pub fn validate(&self) -> Result<(), HalfEdgeError> {
        let cycles = self.live_face_cycles()?;
        let mut owners = HashMap::new();
        let mut incident = HashSet::new();
        for (_, edges) in cycles {
            for id in edges {
                let edge = &self.half_edges[id.0 as usize];
                owners.insert(
                    (edge.origin, self.half_edges[edge.next.0 as usize].origin),
                    id,
                );
                incident.insert(edge.origin);
            }
        }
        if owners != self.edge_map {
            return Err(HalfEdgeError::InvalidTopology(
                "Edge map does not match live faces".into(),
            ));
        }
        for (&(origin, dest), &id) in &owners {
            if self.half_edges[id.0 as usize].twin != owners.get(&(dest, origin)).copied() {
                return Err(HalfEdgeError::InvalidTopology(
                    "Twin pairing does not match live incidence".into(),
                ));
            }
        }
        for (i, vertex) in self.vertices.iter().enumerate() {
            if vertex.id != VertexId(i as u32) {
                return Err(HalfEdgeError::InvalidTopology(
                    "Vertex ID/index mismatch".into(),
                ));
            }
            let valid_outgoing = vertex
                .outgoing_half_edge
                .and_then(|id| self.half_edge(id))
                .is_some_and(|edge| edge.origin == vertex.id && edge.face.is_some());
            if incident.contains(&vertex.id) != valid_outgoing
                || (!incident.contains(&vertex.id) && vertex.outgoing_half_edge.is_some())
            {
                return Err(HalfEdgeError::InvalidTopology(
                    "Invalid outgoing half-edge".into(),
                ));
            }
        }
        Ok(())
    }

    pub fn validate_connectivity(&self) -> Result<(), String> {
        self.validate().map_err(|error| error.to_string())
    }

    /// A manifold vertex has one connected link: a cycle in the interior or a
    /// path on the boundary. Self-intersection is a separate geometric property.
    pub fn check_manifold(&self) -> Result<(), ManifoldError> {
        self.validate()
            .map_err(|error| ManifoldError::InvalidTopology(error.to_string()))?;
        let cycles = self
            .live_face_cycles()
            .map_err(|error| ManifoldError::InvalidTopology(error.to_string()))?;
        // Each face contributes an undirected edge (previous, next) to the link
        // of each corner. Build from all faces, never from an arbitrary fan seed.
        let mut links: HashMap<VertexId, HashMap<VertexId, Vec<VertexId>>> = HashMap::new();
        let mut face_counts: HashMap<VertexId, usize> = HashMap::new();
        for (_, edges) in cycles {
            for id in edges {
                let edge = &self.half_edges[id.0 as usize];
                let prev = self.half_edges[edge.prev.0 as usize].origin;
                let next = self.half_edges[edge.next.0 as usize].origin;
                let link = links.entry(edge.origin).or_default();
                link.entry(prev).or_default().push(next);
                link.entry(next).or_default().push(prev);
                *face_counts.entry(edge.origin).or_default() += 1;
            }
        }
        for (vertex, link) in links {
            let ends = link
                .values()
                .filter(|neighbors| neighbors.len() == 1)
                .count();
            let is_boundary = ends != 0;
            let mut visited = HashSet::new();
            let mut stack = vec![*link.keys().next().unwrap()];
            while let Some(neighbor) = stack.pop() {
                if visited.insert(neighbor) {
                    stack.extend(link[&neighbor].iter().copied());
                }
            }
            if visited.len() != link.len()
                || (ends != 0 && ends != 2)
                || link
                    .values()
                    .any(|neighbors| neighbors.is_empty() || neighbors.len() > 2)
            {
                return Err(ManifoldError::NonManifoldVertex {
                    vertex_id: vertex,
                    ring_vertices: link.len(),
                    ring_faces: face_counts[&vertex],
                    is_boundary,
                });
            }
        }
        Ok(())
    }

    /// Compatibility predicate. This now checks complete incidence; sampling two
    /// traversals of the same fan cannot establish manifoldness.
    pub fn is_likely_manifold(&self) -> bool {
        self.check_manifold().is_ok()
    }
}

/// Error types for manifold validation.
#[derive(Debug, Clone, PartialEq)]
pub enum ManifoldError {
    /// Live face connectivity is malformed.
    InvalidTopology(String),
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
            Self::InvalidTopology(reason) => write!(f, "Invalid topology: {reason}"),
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
