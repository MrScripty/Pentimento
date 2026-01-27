//! Topology query methods for HalfEdgeMesh.

use std::collections::HashSet;

use super::types::{Face, FaceId, HalfEdge, HalfEdgeId, Vertex, VertexId};
use super::HalfEdgeMesh;

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
        let paired = self.half_edges.iter().filter(|he| he.twin.is_some()).count();
        let boundary = self.half_edges.iter().filter(|he| he.twin.is_none()).count();
        paired / 2 + boundary
    }

    // ========================================================================
    // Topology Queries
    // ========================================================================

    /// Get all faces adjacent to a vertex
    pub fn get_vertex_faces(&self, vertex_id: VertexId) -> Vec<FaceId> {
        let mut faces = Vec::new();
        let vertex = match self.vertex(vertex_id) {
            Some(v) => v,
            None => return faces,
        };

        let start_he = match vertex.outgoing_half_edge {
            Some(he) => he,
            None => return faces,
        };

        // Walk around the vertex using twin/prev
        let mut current = start_he;
        let mut visited = HashSet::new();

        loop {
            if visited.contains(&current) {
                break;
            }
            visited.insert(current);

            if let Some(he) = self.half_edge(current) {
                if let Some(face_id) = he.face {
                    if !faces.contains(&face_id) {
                        faces.push(face_id);
                    }
                }

                // Move to next outgoing half-edge from this vertex
                // Go to prev, then to twin
                let prev = self.half_edge(he.prev);
                if let Some(prev_he) = prev {
                    if let Some(twin) = prev_he.twin {
                        current = twin;
                    } else {
                        break; // Boundary
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

        faces
    }

    /// Get all vertices adjacent to a vertex (connected by an edge)
    pub fn get_adjacent_vertices(&self, vertex_id: VertexId) -> Vec<VertexId> {
        let mut neighbors = Vec::new();
        let vertex = match self.vertex(vertex_id) {
            Some(v) => v,
            None => return neighbors,
        };

        let start_he = match vertex.outgoing_half_edge {
            Some(he) => he,
            None => return neighbors,
        };

        let mut current = start_he;
        let mut visited = HashSet::new();

        loop {
            if visited.contains(&current) {
                break;
            }
            visited.insert(current);

            if let Some(he) = self.half_edge(current) {
                // The destination vertex is the origin of the next half-edge
                if let Some(next_he) = self.half_edge(he.next) {
                    if !neighbors.contains(&next_he.origin) {
                        neighbors.push(next_he.origin);
                    }
                }

                // Move around the vertex
                let prev = self.half_edge(he.prev);
                if let Some(prev_he) = prev {
                    if let Some(twin) = prev_he.twin {
                        current = twin;
                    } else {
                        break;
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

        neighbors
    }

    /// Get the vertices of a face in order
    pub fn get_face_vertices(&self, face_id: FaceId) -> Vec<VertexId> {
        let mut vertices = Vec::new();
        let face = match self.face(face_id) {
            Some(f) => f,
            None => return vertices,
        };

        let start_he = face.half_edge;
        let mut current = start_he;

        loop {
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
        let mut edges = Vec::new();
        let face = match self.face(face_id) {
            Some(f) => f,
            None => return edges,
        };

        let start_he = face.half_edge;
        let mut current = start_he;

        loop {
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
