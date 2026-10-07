//! Construction methods for HalfEdgeMesh.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use std::collections::HashMap;

use super::HalfEdgeMesh;
use super::types::{Face, FaceId, HalfEdge, HalfEdgeError, HalfEdgeId, Vertex, VertexId};

impl HalfEdgeMesh {
    /// Build a half-edge mesh from a Bevy mesh
    ///
    /// The mesh must have position attributes and triangle indices.
    pub fn from_bevy_mesh(mesh: &Mesh) -> Result<Self, HalfEdgeError> {
        Self::import_bevy_mesh(mesh, false)
    }

    /// Import a sculpt surface, explicitly joining position-quantized seam vertices.
    ///
    /// Uses the legacy 1e-6 object-space quantization. UVs remain on face corners.
    /// A weld which collapses a triangle or produces a non-manifold surface fails;
    /// callers may retry the connectivity-preserving importer on the original mesh.
    pub fn from_bevy_mesh_welded(mesh: &Mesh) -> Result<Self, HalfEdgeError> {
        Self::import_bevy_mesh(mesh, true)
    }

    fn import_bevy_mesh(mesh: &Mesh, weld: bool) -> Result<Self, HalfEdgeError> {
        if mesh.primitive_topology() != PrimitiveTopology::TriangleList {
            return Err(HalfEdgeError::InvalidTopology(
                "Expected triangle list".into(),
            ));
        }
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
        let uvs: Option<Vec<[f32; 2]>> =
            mesh.attribute(Mesh::ATTRIBUTE_UV_0)
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

        // Validate before any attribute or index access. Import must never panic or
        // silently remove artist geometry to repair malformed input.
        if normals.len() != positions.len()
            || uvs.as_ref().is_some_and(|uv| uv.len() != positions.len())
            || positions.iter().flatten().any(|x| !x.is_finite())
            || normals.iter().flatten().any(|x| !x.is_finite())
            || uvs
                .as_ref()
                .is_some_and(|uv| uv.iter().flatten().any(|x| !x.is_finite()))
        {
            return Err(HalfEdgeError::InvalidTopology(
                "Invalid vertex attributes".into(),
            ));
        }
        if indices.iter().any(|&i| i as usize >= positions.len()) {
            return Err(HalfEdgeError::InvalidTopology(
                "Index outside position array".into(),
            ));
        }
        let source_indices = indices;
        let mut positions_to_canonical = HashMap::new();
        let canonical_map: Vec<u32> = positions
            .iter()
            .enumerate()
            .map(|(i, pos)| {
                if weld {
                    let key = pos.map(|x| (x * 1_000_000.0) as i64);
                    *positions_to_canonical.entry(key).or_insert(i as u32)
                } else {
                    i as u32
                }
            })
            .collect();
        let indices: Vec<u32> = source_indices
            .iter()
            .map(|&i| canonical_map[i as usize])
            .collect();
        if indices
            .chunks_exact(3)
            .any(|tri| tri[0] == tri[1] || tri[1] == tri[2] || tri[2] == tri[0])
        {
            return Err(HalfEdgeError::InvalidTopology(
                "Triangle has repeated vertices (possibly after welding)".into(),
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
                corner_uv: uvs
                    .as_ref()
                    .map(|uv| Vec2::from_array(uv[source_indices[tri_idx * 3] as usize])),
                twin: None,
                next: he1_id,
                prev: he2_id,
                face: Some(face_id),
            });

            // Half-edge 1: v1 -> v2
            half_edges.push(HalfEdge {
                id: he1_id,
                origin: v1,
                corner_uv: uvs
                    .as_ref()
                    .map(|uv| Vec2::from_array(uv[source_indices[tri_idx * 3 + 1] as usize])),
                twin: None,
                next: he2_id,
                prev: he0_id,
                face: Some(face_id),
            });

            // Half-edge 2: v2 -> v0
            half_edges.push(HalfEdge {
                id: he2_id,
                origin: v2,
                corner_uv: uvs
                    .as_ref()
                    .map(|uv| Vec2::from_array(uv[source_indices[tri_idx * 3 + 2] as usize])),
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
            for (he_id, (origin, dest)) in
                [(he0_id, (v0, v1)), (he1_id, (v1, v2)), (he2_id, (v2, v0))]
            {
                if edge_map.contains_key(&(origin, dest)) {
                    return Err(HalfEdgeError::NonManifoldEdge);
                }
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
            let area_normal = (p1 - p0).cross(p2 - p0);
            let normal = area_normal.normalize_or_zero();
            if !area_normal.is_finite() || normal == Vec3::ZERO {
                return Err(HalfEdgeError::InvalidTopology(
                    "Degenerate or non-finite triangle geometry".into(),
                ));
            }

            faces.push(Face {
                id: face_id,
                half_edge: he0_id,
                normal,
            });
        }

        let result = Self {
            vertices,
            half_edges,
            faces,
            edge_map,
        };
        result
            .check_manifold()
            .map_err(|error| HalfEdgeError::InvalidTopology(error.to_string()))?;
        Ok(result)
    }

    /// Create a HalfEdgeMesh from raw components.
    ///
    /// This constructor is used by the sculpting crate to build meshes
    /// from chunk data during partitioning and merging operations.
    pub fn from_raw(
        vertices: Vec<Vertex>,
        half_edges: Vec<HalfEdge>,
        faces: Vec<Face>,
        edge_map: HashMap<(VertexId, VertexId), HalfEdgeId>,
    ) -> Self {
        Self {
            vertices,
            half_edges,
            faces,
            edge_map,
        }
    }

    /// Convert back to a Bevy mesh
    pub fn to_bevy_mesh(&self) -> Mesh {
        self.to_bevy_mesh_with_vertex_map().0
    }

    /// Export face corners plus the topological vertex for every emitted render
    /// vertex. Position-only updates must patch all copies at a UV seam.
    pub fn to_bevy_mesh_with_vertex_map(&self) -> (Mesh, Vec<VertexId>) {
        let mut render_to_vertex = Vec::new();
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
            for he_id in self.get_face_half_edges(face.id) {
                let corner = &self.half_edges[he_id.0 as usize];
                let v = &self.vertices[corner.origin.0 as usize];
                render_to_vertex.push(v.id);
                positions.push(v.position.to_array());
                normals.push(v.normal.to_array());
                uvs.push(corner.corner_uv.or(v.uv).unwrap_or(Vec2::ZERO).to_array());
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
        (mesh, render_to_vertex)
    }
}
