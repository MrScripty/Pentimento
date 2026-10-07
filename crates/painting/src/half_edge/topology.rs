//! Topology query methods for HalfEdgeMesh.

use std::collections::HashSet;

use super::HalfEdgeMesh;
use super::types::{Face, FaceId, HalfEdge, HalfEdgeId, Vertex, VertexId};

impl HalfEdgeMesh {
    // ========================================================================
    // Accessors
    // ========================================================================

    /// Get vertex by ID
    pub fn vertex(&self, id: VertexId) -> Option<&Vertex> {
        self.vertices.get(id.0 as usize)
    }

    /// Get mutable vertex by ID
    pub fn vertex_mut(&mut self, id: VertexId) -> Option<&mut Vertex> {
        self.vertices.get_mut(id.0 as usize)
    }

    /// Get half-edge by ID
    pub fn half_edge(&self, id: HalfEdgeId) -> Option<&HalfEdge> {
        self.half_edges.get(id.0 as usize)
    }

    /// Get face by ID
    pub fn face(&self, id: FaceId) -> Option<&Face> {
        self.faces.get(id.0 as usize)
    }

    /// Get mutable face by ID
    pub fn face_mut(&mut self, id: FaceId) -> Option<&mut Face> {
        self.faces.get_mut(id.0 as usize)
    }

    /// Get all vertices
    pub fn vertices(&self) -> &[Vertex] {
        &self.vertices
    }

    /// Get all half-edges
    pub fn half_edges(&self) -> &[HalfEdge] {
        &self.half_edges
    }

    /// Get all faces
    pub fn faces(&self) -> &[Face] {
        &self.faces
    }

    /// Number of vertices
    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    /// Number of faces
    pub fn face_count(&self) -> usize {
        self.faces.len()
    }

    /// Number of edges (each edge has two half-edges, boundary edges have one)
    pub fn edge_count(&self) -> usize {
        // Count half-edges with twins + boundary half-edges
        let paired = self
            .half_edges
            .iter()
            .filter(|he| he.twin.is_some())
            .count();
        let boundary = self
            .half_edges
            .iter()
            .filter(|he| he.twin.is_none())
            .count();
        paired / 2 + boundary
    }

    // ========================================================================
    // Topology Queries
    // ========================================================================

    /// Traverse both sides of a vertex fan. A boundary can occur on either
    /// side of the arbitrary outgoing seed; it must not truncate the other side.
    pub(crate) fn vertex_half_edges(&self, vertex_id: VertexId) -> Vec<HalfEdgeId> {
        let Some(start) = self.vertex(vertex_id).and_then(|v| v.outgoing_half_edge) else {
            return Vec::new();
        };
        let mut pending = vec![start];
        let mut visited = HashSet::new();
        let mut result = Vec::new();
        while let Some(id) = pending.pop() {
            if !visited.insert(id) {
                continue;
            }
            let Some(edge) = self.half_edge(id) else {
                continue;
            };
            if edge.origin != vertex_id || edge.face.is_none() {
                continue;
            }
            result.push(id);
            if let Some(other) = self.half_edge(edge.prev).and_then(|prev| prev.twin) {
                pending.push(other);
            }
            if let Some(other) = edge
                .twin
                .and_then(|twin| self.half_edge(twin))
                .map(|twin| twin.next)
            {
                pending.push(other);
            }
        }
        result
    }

    /// Get all faces in the vertex's connected fan, including both boundary sides.
    pub fn get_vertex_faces(&self, vertex_id: VertexId) -> Vec<FaceId> {
        self.vertex_half_edges(vertex_id)
            .iter()
            .filter_map(|&id| self.half_edge(id)?.face)
            .collect()
    }

    /// Get all neighbors in the vertex's connected fan, including the incoming
    /// boundary neighbor which has no outgoing half-edge from this vertex.
    pub fn get_adjacent_vertices(&self, vertex_id: VertexId) -> Vec<VertexId> {
        let mut neighbors = Vec::new();
        let mut seen = HashSet::new();
        for id in self.vertex_half_edges(vertex_id) {
            let edge = &self.half_edges[id.0 as usize];
            for adjacent in [edge.next, edge.prev] {
                if let Some(other) = self.half_edge(adjacent)
                    && seen.insert(other.origin)
                {
                    neighbors.push(other.origin);
                }
            }
        }
        neighbors
    }

    /// UV for a face corner. Legacy/raw meshes may store only per-vertex UVs.
    pub fn corner_uv(&self, edge: HalfEdgeId) -> Option<bevy::prelude::Vec2> {
        let edge = self.half_edge(edge)?;
        edge.corner_uv.or_else(|| self.vertex(edge.origin)?.uv)
    }

    /// Whether a paired edge joins different UV charts.
    pub fn is_uv_seam_edge(&self, id: HalfEdgeId) -> bool {
        let Some(edge) = self.half_edge(id) else {
            return true;
        };
        let Some(twin) = edge.twin.and_then(|id| self.half_edge(id)) else {
            return false;
        };
        self.corner_uv(id) != self.corner_uv(twin.next)
            || self.corner_uv(edge.next) != self.corner_uv(twin.id)
    }

    pub fn is_uv_seam_vertex(&self, vertex: VertexId) -> bool {
        self.vertex_half_edges(vertex).iter().any(|&id| {
            self.is_uv_seam_edge(id)
                || self
                    .half_edge(id)
                    .is_some_and(|edge| self.is_uv_seam_edge(edge.prev))
        })
    }

    /// Full simplicial link condition for a triangle edge. Shared link edges
    /// matter as well as neighbor counts (a tetrahedron is the minimal example).
    pub fn satisfies_collapse_link(&self, id: HalfEdgeId) -> bool {
        let Some(edge) = self.half_edge(id).filter(|edge| edge.face.is_some()) else {
            return false;
        };
        let Some(dest) = self.get_half_edge_dest(id) else {
            return false;
        };
        let origin = edge.origin;
        if origin == dest {
            return false;
        }
        let mut edge_link = HashSet::new();
        let mut link_vertices = [HashSet::new(), HashSet::new()];
        let mut link_edges = [HashSet::new(), HashSet::new()];
        for (i, vertex) in [origin, dest].into_iter().enumerate() {
            for face in self.get_vertex_faces(vertex) {
                let vertices = self.get_face_vertices(face);
                if vertices.len() != 3 {
                    return false;
                }
                let opposite: Vec<_> = vertices.into_iter().filter(|&v| v != vertex).collect();
                if opposite.len() != 2 {
                    return false;
                }
                link_vertices[i].extend(opposite.iter().copied());
                let (a, b) = (opposite[0], opposite[1]);
                link_edges[i].insert(if a.0 < b.0 { (a, b) } else { (b, a) });
                if opposite.contains(&origin) || opposite.contains(&dest) {
                    edge_link.extend(opposite.into_iter().filter(|&v| v != origin && v != dest));
                }
            }
        }
        let common: HashSet<_> = link_vertices[0]
            .intersection(&link_vertices[1])
            .copied()
            .collect();
        let expected = if edge.twin.is_some() { 2 } else { 1 };
        edge_link.len() == expected
            && common == edge_link
            && link_edges[0].is_disjoint(&link_edges[1])
    }

    /// Get the vertices of a face in order
    pub fn get_face_vertices(&self, face_id: FaceId) -> Vec<VertexId> {
        // Maximum edges per face - prevents infinite loops on corrupted mesh
        const MAX_FACE_EDGES: usize = 100;

        let mut vertices = Vec::new();
        let face = match self.face(face_id) {
            Some(f) => f,
            None => return vertices,
        };

        let start_he = face.half_edge;

        // CONSISTENCY CHECK: Verify face's half_edge actually belongs to this face
        if let Some(he) = self.half_edge(start_he) {
            if he.face != Some(face_id) {
                // Check if this is expected (face was collapsed) vs unexpected (topology bug)
                if he.face.is_none() {
                    // Face was "removed" by edge collapse - half-edges are orphaned (face = None)
                    // This is expected; the face should be skipped during mesh iteration
                    // Log at trace level since this is normal during tessellation
                    tracing::trace!(
                        "Face {:?} was removed by collapse (half_edge {:?} is orphaned)",
                        face_id,
                        start_he
                    );
                } else {
                    // Half-edge belongs to a different face - this is a topology bug
                    tracing::warn!(
                        "Face {:?} has stale half_edge {:?} (belongs to face {:?}). \
                         This indicates a topology bug in split_edge_topology().",
                        face_id,
                        start_he,
                        he.face
                    );
                }
                return vertices; // Return empty rather than garbage
            }
        } else {
            tracing::warn!("Face {:?} has invalid half_edge {:?}", face_id, start_he);
            return vertices;
        }

        let mut current = start_he;
        let mut iterations = 0;

        loop {
            iterations += 1;
            if iterations > MAX_FACE_EDGES {
                tracing::warn!(
                    "Face {:?} traversal exceeded {} iterations, possible mesh corruption",
                    face_id,
                    MAX_FACE_EDGES
                );
                break;
            }

            if let Some(he) = self.half_edge(current) {
                vertices.push(he.origin);
                current = he.next;
            } else {
                break;
            }

            if current == start_he {
                break;
            }
        }

        vertices
    }

    /// Get the half-edges forming the boundary of a face
    pub fn get_face_half_edges(&self, face_id: FaceId) -> Vec<HalfEdgeId> {
        // Maximum edges per face - prevents infinite loops on corrupted mesh
        const MAX_FACE_EDGES: usize = 100;

        let mut edges = Vec::new();
        let face = match self.face(face_id) {
            Some(f) => f,
            None => return edges,
        };

        let start_he = face.half_edge;

        // CONSISTENCY CHECK: Verify face's half_edge actually belongs to this face
        if let Some(he) = self.half_edge(start_he) {
            if he.face != Some(face_id) {
                // Check if this is expected (face was collapsed) vs unexpected (topology bug)
                if he.face.is_none() {
                    // Face was removed by collapse - expected during tessellation
                    tracing::trace!(
                        "get_face_half_edges: Face {:?} was removed by collapse",
                        face_id
                    );
                } else {
                    // Half-edge belongs to different face - topology bug
                    tracing::warn!(
                        "get_face_half_edges: Face {:?} has stale half_edge {:?} (belongs to face {:?})",
                        face_id,
                        start_he,
                        he.face
                    );
                }
                return edges;
            }
        } else {
            return edges;
        }

        let mut current = start_he;
        let mut iterations = 0;

        loop {
            iterations += 1;
            if iterations > MAX_FACE_EDGES {
                tracing::warn!(
                    "Face {:?} half-edge traversal exceeded {} iterations, possible mesh corruption",
                    face_id,
                    MAX_FACE_EDGES
                );
                break;
            }

            edges.push(current);
            if let Some(he) = self.half_edge(current) {
                current = he.next;
            } else {
                break;
            }

            if current == start_he {
                break;
            }
        }

        edges
    }

    /// Get the two faces adjacent to an edge (via half-edge)
    /// Returns (face of this half-edge, face of twin half-edge)
    pub fn get_edge_faces(&self, he_id: HalfEdgeId) -> (Option<FaceId>, Option<FaceId>) {
        let he = match self.half_edge(he_id) {
            Some(h) => h,
            None => return (None, None),
        };

        let face1 = he.face;
        let face2 = he.twin.and_then(|twin| self.half_edge(twin)?.face);

        (face1, face2)
    }

    /// Get the destination vertex of a half-edge
    pub fn get_half_edge_dest(&self, he_id: HalfEdgeId) -> Option<VertexId> {
        let he = self.half_edge(he_id)?;
        let next = self.half_edge(he.next)?;
        Some(next.origin)
    }

    /// Find a half-edge by its origin and destination vertices
    pub fn find_half_edge(&self, from: VertexId, to: VertexId) -> Option<HalfEdgeId> {
        self.edge_map.get(&(from, to)).copied()
    }

    /// Check if a half-edge is on the boundary (has no twin)
    pub fn is_boundary_edge(&self, he_id: HalfEdgeId) -> bool {
        self.half_edge(he_id)
            .map(|he| he.twin.is_none())
            .unwrap_or(true)
    }

    /// Check if a face is still valid (not orphaned by edge collapse).
    ///
    /// A face is considered invalid if its `half_edge` pointer references a
    /// half-edge that no longer belongs to this face (typically because the
    /// face was removed by edge collapse but the Face struct remains in the array).
    pub fn is_face_valid(&self, face_id: FaceId) -> bool {
        if let Some(face) = self.face(face_id) {
            if let Some(he) = self.half_edge(face.half_edge) {
                return he.face == Some(face_id);
            }
        }
        false
    }

    /// Check if a vertex is on the boundary
    pub fn is_boundary_vertex(&self, vertex_id: VertexId) -> bool {
        let vertex = match self.vertex(vertex_id) {
            Some(v) => v,
            None => return false,
        };

        let start_he = match vertex.outgoing_half_edge {
            Some(he) => he,
            None => return true, // Isolated vertex
        };

        // Check if any outgoing half-edge is a boundary
        let mut current = start_he;
        let mut visited = HashSet::new();

        loop {
            if visited.contains(&current) {
                break;
            }
            visited.insert(current);

            if let Some(he) = self.half_edge(current) {
                if he.twin.is_none() {
                    return true;
                }

                let prev = self.half_edge(he.prev);
                if let Some(prev_he) = prev {
                    if let Some(twin) = prev_he.twin {
                        current = twin;
                    } else {
                        return true;
                    }
                } else {
                    break;
                }
            } else {
                break;
            }

            if current == start_he {
                break;
            }
        }

        false
    }
}
