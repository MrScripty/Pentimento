//! Half-edge mesh data structure for mesh editing operations
//!
//! Provides topology information (vertex-face adjacency, edge loops, etc.)
//! that is not available in a simple triangle soup representation.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

/// Type-safe vertex identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VertexId(pub u32);

/// Type-safe half-edge identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HalfEdgeId(pub u32);

/// Type-safe face identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FaceId(pub u32);

/// A vertex in the half-edge mesh
#[derive(Debug, Clone)]
pub struct Vertex {
    pub id: VertexId,
    pub position: Vec3,
    pub normal: Vec3,
    pub uv: Option<Vec2>,
    /// One outgoing half-edge from this vertex (arbitrary choice if multiple)
    pub outgoing_half_edge: Option<HalfEdgeId>,
    /// Original index in the source Bevy mesh (for attribute mapping)
    pub source_index: u32,
}

/// A half-edge in the mesh
///
/// Each edge in the mesh is represented by two half-edges pointing in opposite
/// directions. Half-edges store connectivity information for traversing the mesh.
#[derive(Debug, Clone)]
pub struct HalfEdge {
    pub id: HalfEdgeId,
    /// The vertex this half-edge originates from
    pub origin: VertexId,
    /// The opposite half-edge (None for boundary edges)
    pub twin: Option<HalfEdgeId>,
    /// The next half-edge around the face (counter-clockwise)
    pub next: HalfEdgeId,
    /// The previous half-edge around the face (counter-clockwise)
    pub prev: HalfEdgeId,
    /// The face this half-edge borders (None for boundary half-edges)
    pub face: Option<FaceId>,
}

/// A face (polygon) in the mesh
#[derive(Debug, Clone)]
pub struct Face {
    pub id: FaceId,
    /// One half-edge on the boundary of this face
    pub half_edge: HalfEdgeId,
    /// Cached face normal
    pub normal: Vec3,
}

/// Errors that can occur during half-edge mesh operations
#[derive(Debug, thiserror::Error)]
pub enum HalfEdgeError {
    #[error("Mesh has no position attribute")]
    NoPositions,
    #[error("Mesh has no indices")]
    NoIndices,
    #[error("Invalid mesh topology: {0}")]
    InvalidTopology(String),
    #[error("Non-manifold edge detected")]
    NonManifoldEdge,
}

/// Half-edge mesh data structure
///
/// Provides efficient topology queries for mesh editing operations.
#[derive(Debug, Clone)]
pub struct HalfEdgeMesh {
    vertices: Vec<Vertex>,
    half_edges: Vec<HalfEdge>,
    faces: Vec<Face>,
    /// Map from (origin, destination) vertex pair to half-edge
    edge_map: HashMap<(VertexId, VertexId), HalfEdgeId>,
}

impl HalfEdgeMesh {
    /// Build a half-edge mesh from a Bevy mesh
    ///
    /// The mesh must have position attributes and triangle indices.
    pub fn from_bevy_mesh(mesh: &Mesh) -> Result<Self, HalfEdgeError> {
        // Extract positions
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|attr| attr.as_float3())
            .ok_or(HalfEdgeError::NoPositions)?;

        // Extract normals (generate if missing)
        let normals: Vec<[f32; 3]> = mesh
            .attribute(Mesh::ATTRIBUTE_NORMAL)
            .and_then(|attr| attr.as_float3())
            .map(|n| n.to_vec())
            .unwrap_or_else(|| vec![[0.0, 1.0, 0.0]; positions.len()]);

        // Extract UVs (optional)
        let uvs: Option<Vec<[f32; 2]>> = mesh
            .attribute(Mesh::ATTRIBUTE_UV_0)
            .and_then(|attr| match attr {
                VertexAttributeValues::Float32x2(v) => Some(v.clone()),
                _ => None,
            });

        // Extract indices
        let indices: Vec<u32> = match mesh.indices() {
            Some(Indices::U16(idx)) => idx.iter().map(|&i| i as u32).collect(),
            Some(Indices::U32(idx)) => idx.to_vec(),
            None => return Err(HalfEdgeError::NoIndices),
        };

        if indices.len() % 3 != 0 {
            return Err(HalfEdgeError::InvalidTopology(
                "Index count not divisible by 3".to_string(),
            ));
        }

        // Create vertices
        let mut vertices: Vec<Vertex> = positions
            .iter()
            .enumerate()
            .map(|(i, pos)| Vertex {
                id: VertexId(i as u32),
                position: Vec3::from_array(*pos),
                normal: Vec3::from_array(normals[i]),
                uv: uvs.as_ref().map(|u| Vec2::from_array(u[i])),
                outgoing_half_edge: None,
                source_index: i as u32,
            })
            .collect();

        let mut half_edges: Vec<HalfEdge> = Vec::new();
        let mut faces: Vec<Face> = Vec::new();
        let mut edge_map: HashMap<(VertexId, VertexId), HalfEdgeId> = HashMap::new();

        // Process each triangle
        let num_triangles = indices.len() / 3;
        for tri_idx in 0..num_triangles {
            let i0 = indices[tri_idx * 3] as usize;
            let i1 = indices[tri_idx * 3 + 1] as usize;
            let i2 = indices[tri_idx * 3 + 2] as usize;

            let v0 = VertexId(i0 as u32);
            let v1 = VertexId(i1 as u32);
            let v2 = VertexId(i2 as u32);

            let face_id = FaceId(faces.len() as u32);

            // Create three half-edges for this triangle
            let he0_id = HalfEdgeId(half_edges.len() as u32);
            let he1_id = HalfEdgeId(half_edges.len() as u32 + 1);
            let he2_id = HalfEdgeId(half_edges.len() as u32 + 2);

            // Half-edge 0: v0 -> v1
            half_edges.push(HalfEdge {
                id: he0_id,
                origin: v0,
                twin: None,
                next: he1_id,
                prev: he2_id,
                face: Some(face_id),
            });

            // Half-edge 1: v1 -> v2
            half_edges.push(HalfEdge {
                id: he1_id,
                origin: v1,
                twin: None,
                next: he2_id,
                prev: he0_id,
                face: Some(face_id),
            });

            // Half-edge 2: v2 -> v0
            half_edges.push(HalfEdge {
                id: he2_id,
                origin: v2,
                twin: None,
                next: he0_id,
                prev: he1_id,
                face: Some(face_id),
            });

            // Set outgoing half-edges for vertices
            if vertices[i0].outgoing_half_edge.is_none() {
                vertices[i0].outgoing_half_edge = Some(he0_id);
            }
            if vertices[i1].outgoing_half_edge.is_none() {
                vertices[i1].outgoing_half_edge = Some(he1_id);
            }
            if vertices[i2].outgoing_half_edge.is_none() {
                vertices[i2].outgoing_half_edge = Some(he2_id);
            }

            // Add to edge map and try to find twins
            for (he_id, (origin, dest)) in [
                (he0_id, (v0, v1)),
                (he1_id, (v1, v2)),
                (he2_id, (v2, v0)),
            ] {
                // Check if the opposite half-edge exists
                if let Some(&twin_id) = edge_map.get(&(dest, origin)) {
                    // Link twins
                    half_edges[he_id.0 as usize].twin = Some(twin_id);
                    half_edges[twin_id.0 as usize].twin = Some(he_id);
                }
                edge_map.insert((origin, dest), he_id);
            }

            // Calculate face normal
            let p0 = vertices[i0].position;
            let p1 = vertices[i1].position;
            let p2 = vertices[i2].position;
            let normal = (p1 - p0).cross(p2 - p0).normalize_or_zero();

            faces.push(Face {
                id: face_id,
                half_edge: he0_id,
                normal,
            });
        }

        Ok(Self {
            vertices,
            half_edges,
            faces,
            edge_map,
        })
    }

    /// Convert back to a Bevy mesh
    pub fn to_bevy_mesh(&self) -> Mesh {
        let mut positions: Vec<[f32; 3]> = Vec::new();
        let mut normals: Vec<[f32; 3]> = Vec::new();
        let mut uvs: Vec<[f32; 2]> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();

        // For each face, emit triangle vertices
        // Note: This creates duplicate vertices per face (not shared)
        // A more sophisticated approach could detect shared vertices
        for face in &self.faces {
            let face_verts = self.get_face_vertices(face.id);
            if face_verts.len() < 3 {
                continue;
            }

            // Triangulate the face (fan triangulation for convex faces)
            let base_idx = positions.len() as u32;
            for vid in &face_verts {
                let v = &self.vertices[vid.0 as usize];
                positions.push(v.position.to_array());
                normals.push(v.normal.to_array());
                uvs.push(v.uv.unwrap_or(Vec2::ZERO).to_array());
            }

            // Fan triangulation
            for i in 1..(face_verts.len() - 1) {
                indices.push(base_idx);
                indices.push(base_idx + i as u32);
                indices.push(base_idx + i as u32 + 1);
            }
        }

        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        mesh.insert_indices(Indices::U32(indices));
        mesh
    }

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

    // ========================================================================
    // Modification
    // ========================================================================

    /// Set the position of a vertex
    pub fn set_vertex_position(&mut self, vertex_id: VertexId, position: Vec3) {
        if let Some(v) = self.vertex_mut(vertex_id) {
            v.position = position;
        }
    }

    /// Update face normals after vertex positions change
    pub fn recalculate_face_normals(&mut self) {
        for i in 0..self.faces.len() {
            let verts = self.get_face_vertices(FaceId(i as u32));
            if verts.len() >= 3 {
                let p0 = self.vertices[verts[0].0 as usize].position;
                let p1 = self.vertices[verts[1].0 as usize].position;
                let p2 = self.vertices[verts[2].0 as usize].position;
                let normal = (p1 - p0).cross(p2 - p0).normalize_or_zero();
                self.faces[i].normal = normal;
            }
        }
    }

    /// Recalculate vertex normals from adjacent face normals
    pub fn recalculate_vertex_normals(&mut self) {
        for i in 0..self.vertices.len() {
            let faces = self.get_vertex_faces(VertexId(i as u32));
            if faces.is_empty() {
                continue;
            }

            let mut normal = Vec3::ZERO;
            for fid in &faces {
                if let Some(f) = self.face(*fid) {
                    normal += f.normal;
                }
            }
            self.vertices[i].normal = normal.normalize_or_zero();
        }
    }

    // ========================================================================
    // Validation
    // ========================================================================

    /// Validate the mesh topology
    pub fn validate(&self) -> Result<(), HalfEdgeError> {
        // Check twin symmetry
        for he in &self.half_edges {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_triangle_mesh() -> Mesh {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_POSITION,
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.5, 1.0, 0.0],
            ],
        );
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_NORMAL,
            vec![
                [0.0, 0.0, 1.0],
                [0.0, 0.0, 1.0],
                [0.0, 0.0, 1.0],
            ],
        );
        mesh.insert_indices(Indices::U32(vec![0, 1, 2]));
        mesh
    }

    #[test]
    fn test_from_bevy_mesh_single_triangle() {
        let mesh = create_test_triangle_mesh();
        let he_mesh = HalfEdgeMesh::from_bevy_mesh(&mesh).unwrap();

        assert_eq!(he_mesh.vertex_count(), 3);
        assert_eq!(he_mesh.face_count(), 1);
        assert!(he_mesh.validate().is_ok());
    }

    #[test]
    fn test_face_vertices() {
        let mesh = create_test_triangle_mesh();
        let he_mesh = HalfEdgeMesh::from_bevy_mesh(&mesh).unwrap();

        let verts = he_mesh.get_face_vertices(FaceId(0));
        assert_eq!(verts.len(), 3);
    }

    #[test]
    fn test_vertex_faces() {
        let mesh = create_test_triangle_mesh();
        let he_mesh = HalfEdgeMesh::from_bevy_mesh(&mesh).unwrap();

        let faces = he_mesh.get_vertex_faces(VertexId(0));
        assert_eq!(faces.len(), 1);
        assert_eq!(faces[0], FaceId(0));
    }
}
