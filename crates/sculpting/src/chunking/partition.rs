//! Mesh partitioning into spatial chunks.
//!
//! This module handles the initial decomposition of a mesh into chunks
//! for sculpting. The partitioning strategy uses spatial subdivision
//! to create chunks of roughly equal face count.

use super::{boundary, Aabb, ChunkId, ChunkedMesh, MeshChunk};
use crate::ChunkConfig;
use glam::Vec3;
use painting::half_edge::{Face, FaceId, HalfEdgeMesh, Vertex, VertexId, HalfEdge, HalfEdgeId};
use std::collections::{HashMap, HashSet};

/// Configuration for mesh partitioning.
#[derive(Debug, Clone)]
pub struct PartitionConfig {
    /// Target faces per chunk.
    pub target_faces: usize,
    /// Minimum faces per chunk (won't subdivide below this).
    pub min_faces: usize,
    /// Maximum faces per chunk (will force subdivision above this).
    pub max_faces: usize,
}

impl Default for PartitionConfig {
    fn default() -> Self {
        Self {
            target_faces: 10000,
            min_faces: 5000,
            max_faces: 15000,
        }
    }
}

impl From<&ChunkConfig> for PartitionConfig {
    fn from(config: &ChunkConfig) -> Self {
        Self {
            target_faces: config.target_faces,
            min_faces: config.min_faces,
            max_faces: config.max_faces,
        }
    }
}

/// Partition a mesh into chunks for sculpting.
///
/// The mesh is recursively subdivided along its longest axis until
/// each partition has approximately `target_faces` faces.
pub fn partition_mesh(mesh: &HalfEdgeMesh, config: &PartitionConfig) -> ChunkedMesh {
    let mut chunked_mesh = ChunkedMesh::with_config(ChunkConfig {
        min_faces: config.min_faces,
        max_faces: config.max_faces,
        target_faces: config.target_faces,
    });

    // Initialize next_original_vertex_id to be one past the max original vertex ID.
    // This ensures tessellation-created vertices get globally unique IDs that
    // won't collide with original mesh vertices or each other across chunks.
    chunked_mesh.next_original_vertex_id = mesh.vertex_count() as u32;

    if mesh.face_count() <= config.max_faces {
        let chunk = create_chunk_from_faces(mesh, (0..mesh.face_count() as u32).map(FaceId).collect());
        chunked_mesh.add_chunk(chunk);
    } else {
        let all_faces: Vec<FaceId> = (0..mesh.face_count() as u32).map(FaceId).collect();
        recursive_partition(mesh, &all_faces, config, &mut chunked_mesh);
    }

    // Build boundary relationships
    boundary::build_boundary_relationships(&mut chunked_mesh);

    // Rebuild spatial grid
    chunked_mesh.rebuild_spatial_grid();

    chunked_mesh
}

/// Recursively partition a set of faces.
fn recursive_partition(
    mesh: &HalfEdgeMesh,
    faces: &[FaceId],
    config: &PartitionConfig,
    chunked_mesh: &mut ChunkedMesh,
) {
    // Base case: small enough to be a single chunk
    if faces.len() <= config.max_faces {
        let chunk = create_chunk_from_faces(mesh, faces.to_vec());
        chunked_mesh.add_chunk(chunk);
        return;
    }

    // Calculate bounding box of these faces
    let bounds = calculate_face_bounds(mesh, faces);

    // Split along longest axis
    let axis = bounds.longest_axis();
    let split_point = bounds.center()[axis];

    // Partition faces into two groups based on centroid
    let (left, right): (Vec<FaceId>, Vec<FaceId>) = faces.iter().partition(|&&face_id| {
        let centroid = calculate_face_centroid(mesh, face_id);
        centroid[axis] < split_point
    });

    // Handle degenerate cases (all faces on one side)
    if left.is_empty() || right.is_empty() {
        // Fall back to simple split by index
        let mid = faces.len() / 2;
        let left: Vec<FaceId> = faces[..mid].to_vec();
        let right: Vec<FaceId> = faces[mid..].to_vec();

        recursive_partition(mesh, &left, config, chunked_mesh);
        recursive_partition(mesh, &right, config, chunked_mesh);
    } else {
        recursive_partition(mesh, &left, config, chunked_mesh);
        recursive_partition(mesh, &right, config, chunked_mesh);
    }
}

/// Calculate the bounding box of a set of faces.
fn calculate_face_bounds(mesh: &HalfEdgeMesh, faces: &[FaceId]) -> Aabb {
    let mut bounds = Aabb::empty();

    for &face_id in faces {
        let verts = mesh.get_face_vertices(face_id);
        for vid in verts {
            if let Some(v) = mesh.vertex(vid) {
                bounds.include_point(v.position);
            }
        }
    }

    bounds
}

/// Calculate the centroid of a face.
fn calculate_face_centroid(mesh: &HalfEdgeMesh, face_id: FaceId) -> Vec3 {
    let verts = mesh.get_face_vertices(face_id);
    if verts.is_empty() {
        return Vec3::ZERO;
    }

    let sum: Vec3 = verts
        .iter()
        .filter_map(|vid| mesh.vertex(*vid).map(|v| v.position))
        .sum();

    sum / verts.len() as f32
}

/// Create a chunk from a subset of faces in the original mesh.
fn create_chunk_from_faces(mesh: &HalfEdgeMesh, face_ids: Vec<FaceId>) -> MeshChunk {
    // Collect all vertices used by these faces
    let mut used_vertices: HashSet<VertexId> = HashSet::new();
    for &face_id in &face_ids {
        for vid in mesh.get_face_vertices(face_id) {
            used_vertices.insert(vid);
        }
    }

    // Create mapping from original to local vertex IDs
    let mut original_to_local: HashMap<VertexId, VertexId> = HashMap::new();
    let mut local_to_original: HashMap<VertexId, VertexId> = HashMap::new();

    let sorted_vertices: Vec<VertexId> = {
        let mut v: Vec<_> = used_vertices.into_iter().collect();
        v.sort_by_key(|v| v.0);
        v
    };

    for (local_idx, &original_id) in sorted_vertices.iter().enumerate() {
        let local_id = VertexId(local_idx as u32);
        original_to_local.insert(original_id, local_id);
        local_to_original.insert(local_id, original_id);
    }

    // Build new half-edge mesh for this chunk
    let chunk_mesh = build_chunk_mesh(mesh, &face_ids, &original_to_local);

    // Calculate bounds
    let mut bounds = Aabb::empty();
    for vertex in chunk_mesh.vertices() {
        bounds.include_point(vertex.position);
    }

    MeshChunk {
        id: ChunkId(0), // Will be assigned by ChunkedMesh::add_chunk
        bounds,
        mesh: chunk_mesh,
        local_to_original,
        original_to_local,
        boundary_vertices: HashMap::new(), // Built later
        dirty: false,
        topology_changed: false,
    }
}

/// Build a HalfEdgeMesh from a subset of faces.
fn build_chunk_mesh(
    source: &HalfEdgeMesh,
    face_ids: &[FaceId],
    vertex_map: &HashMap<VertexId, VertexId>,
) -> HalfEdgeMesh {
    // Create vertices
    let mut vertices: Vec<Vertex> = Vec::new();
    let mut sorted_mappings: Vec<_> = vertex_map.iter().collect();
    sorted_mappings.sort_by_key(|(_, local)| local.0);

    for &(&original_id, &local_id) in &sorted_mappings {
        let source_vertex = source.vertex(original_id).unwrap();
        vertices.push(Vertex {
            id: local_id,
            position: source_vertex.position,
            normal: source_vertex.normal,
            uv: source_vertex.uv,
            outgoing_half_edge: None, // Will be set when building half-edges
            source_index: source_vertex.source_index,
        });
    }

    // Create faces and half-edges
    let mut half_edges: Vec<HalfEdge> = Vec::new();
    let mut faces: Vec<Face> = Vec::new();
    let mut edge_map: HashMap<(VertexId, VertexId), HalfEdgeId> = HashMap::new();

    for &original_face_id in face_ids.iter() {
        let source_face = match source.face(original_face_id) {
            Some(f) => f,
            None => continue,
        };

        let original_verts = source.get_face_vertices(original_face_id);
        if original_verts.len() < 3 {
            continue;
        }

        // Map to local vertex IDs
        let local_verts: Vec<VertexId> = original_verts
            .iter()
            .filter_map(|v| vertex_map.get(v).copied())
            .collect();

        if local_verts.len() != original_verts.len() {
            continue; // Some vertices not in this chunk
        }

        // Use faces.len() to ensure the face ID matches its array index
        let new_face_id = FaceId(faces.len() as u32);

        // Create half-edges for this face
        let base_he_idx = half_edges.len() as u32;
        let num_verts = local_verts.len();

        for i in 0..num_verts {
            let he_id = HalfEdgeId(base_he_idx + i as u32);
            let origin = local_verts[i];
            let next_idx = (i + 1) % num_verts;
            let prev_idx = (i + num_verts - 1) % num_verts;

            half_edges.push(HalfEdge {
                id: he_id,
                origin,
                twin: None,
                next: HalfEdgeId(base_he_idx + next_idx as u32),
                prev: HalfEdgeId(base_he_idx + prev_idx as u32),
                face: Some(new_face_id),
            });

            // Set outgoing half-edge for vertex
            if vertices[origin.0 as usize].outgoing_half_edge.is_none() {
                vertices[origin.0 as usize].outgoing_half_edge = Some(he_id);
            }

            // Add to edge map for twin finding
            let dest = local_verts[next_idx];
            if let Some(&twin_id) = edge_map.get(&(dest, origin)) {
                half_edges[he_id.0 as usize].twin = Some(twin_id);
                half_edges[twin_id.0 as usize].twin = Some(he_id);
            }
            // Check for duplicate edge before inserting
            if let Some(&existing_he) = edge_map.get(&(origin, dest)) {
                tracing::warn!(
                    "build_chunk_mesh: DUPLICATE EDGE detected! edge ({:?} -> {:?}) already has HE {:?}, \
                     now creating HE {:?}. Source faces may share the same edge.",
                    origin, dest, existing_he, he_id
                );
            }
            edge_map.insert((origin, dest), he_id);
        }

        faces.push(Face {
            id: new_face_id,
            half_edge: HalfEdgeId(base_he_idx),
            normal: source_face.normal,
        });
    }

    HalfEdgeMesh::from_raw(vertices, half_edges, faces, edge_map)
}

/// Split an oversized chunk along its longest axis.
///
/// Returns the IDs of the two new chunks (or None if split wasn't possible).
pub fn split_chunk(
    chunked_mesh: &mut ChunkedMesh,
    chunk_id: ChunkId,
) -> Option<(ChunkId, ChunkId)> {
    // Get chunk data
    let chunk = chunked_mesh.remove_chunk(chunk_id)?;

    // If already small enough, just re-add it
    if chunk.face_count() <= chunked_mesh.config.max_faces {
        let id = chunked_mesh.add_chunk(chunk);
        return Some((id, id));
    }

    let mesh = &chunk.mesh;
    let all_faces: Vec<FaceId> = (0..mesh.face_count() as u32).map(FaceId).collect();

    // Calculate split point
    let axis = chunk.bounds.longest_axis();
    let split_point = chunk.bounds.center()[axis];

    // Partition faces
    let (left_faces, right_faces): (Vec<FaceId>, Vec<FaceId>) =
        all_faces.into_iter().partition(|&face_id| {
            let centroid = calculate_face_centroid(mesh, face_id);
            centroid[axis] < split_point
        });

    // Create two new chunks
    let left_chunk = create_chunk_from_faces_local(
        mesh,
        left_faces,
        &chunk.local_to_original,
    );
    let right_chunk = create_chunk_from_faces_local(
        mesh,
        right_faces,
        &chunk.local_to_original,
    );

    let left_id = chunked_mesh.add_chunk(left_chunk);
    let right_id = chunked_mesh.add_chunk(right_chunk);

    // Rebuild boundary relationships
    boundary::build_boundary_relationships(chunked_mesh);

    Some((left_id, right_id))
}

/// Create a chunk from faces in a local (chunk) mesh, preserving original mappings.
fn create_chunk_from_faces_local(
    mesh: &HalfEdgeMesh,
    face_ids: Vec<FaceId>,
    parent_local_to_original: &HashMap<VertexId, VertexId>,
) -> MeshChunk {
    // Collect vertices used by these faces (in the local mesh)
    let mut used_local_vertices: HashSet<VertexId> = HashSet::new();
    for &face_id in &face_ids {
        for vid in mesh.get_face_vertices(face_id) {
            used_local_vertices.insert(vid);
        }
    }

    // Create new local vertex mapping
    let mut new_to_parent: HashMap<VertexId, VertexId> = HashMap::new();
    let mut parent_to_new: HashMap<VertexId, VertexId> = HashMap::new();

    let sorted_vertices: Vec<VertexId> = {
        let mut v: Vec<_> = used_local_vertices.into_iter().collect();
        v.sort_by_key(|v| v.0);
        v
    };

    for (new_idx, &parent_local_id) in sorted_vertices.iter().enumerate() {
        let new_id = VertexId(new_idx as u32);
        new_to_parent.insert(new_id, parent_local_id);
        parent_to_new.insert(parent_local_id, new_id);
    }

    // Map new local IDs to original IDs
    let mut local_to_original: HashMap<VertexId, VertexId> = HashMap::new();
    let mut original_to_local: HashMap<VertexId, VertexId> = HashMap::new();

    for (&new_id, &parent_local_id) in &new_to_parent {
        if let Some(&original_id) = parent_local_to_original.get(&parent_local_id) {
            local_to_original.insert(new_id, original_id);
            original_to_local.insert(original_id, new_id);
        }
    }

    // Build chunk mesh
    let chunk_mesh = build_chunk_mesh(mesh, &face_ids, &parent_to_new);

    // Calculate bounds
    let mut bounds = Aabb::empty();
    for vertex in chunk_mesh.vertices() {
        bounds.include_point(vertex.position);
    }

    MeshChunk {
        id: ChunkId(0),
        bounds,
        mesh: chunk_mesh,
        local_to_original,
        original_to_local,
        boundary_vertices: HashMap::new(),
        dirty: false,
        topology_changed: false,
    }
}

#[cfg(all(test, feature = "bevy"))]
mod tests {
    use super::*;
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::{Indices, PrimitiveTopology};
    use bevy::prelude::*;

    fn create_test_mesh(num_quads: usize) -> Mesh {
        // Create a grid of quads
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut indices = Vec::new();

        let grid_size = (num_quads as f32).sqrt().ceil() as usize;

        for y in 0..=grid_size {
            for x in 0..=grid_size {
                positions.push([x as f32, 0.0, y as f32]);
                normals.push([0.0, 1.0, 0.0]);
            }
        }

        for y in 0..grid_size {
            for x in 0..grid_size {
                let v0 = (y * (grid_size + 1) + x) as u32;
                let v1 = v0 + 1;
                let v2 = v0 + (grid_size + 1) as u32;
                let v3 = v2 + 1;

                // Two triangles per quad
                indices.extend_from_slice(&[v0, v2, v1, v1, v2, v3]);
            }
        }

        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        mesh.insert_indices(Indices::U32(indices));
        mesh
    }

    #[test]
    fn test_partition_small_mesh() {
        let mesh = create_test_mesh(100); // 200 triangles
        let he_mesh = HalfEdgeMesh::from_bevy_mesh(&mesh).unwrap();

        let config = PartitionConfig {
            target_faces: 1000,
            min_faces: 500,
            max_faces: 2000,
        };

        let chunked = partition_mesh(&he_mesh, &config);

        // Small mesh should be a single chunk
        assert_eq!(chunked.chunk_count(), 1);
        assert_eq!(chunked.total_face_count(), he_mesh.face_count());
    }

    #[test]
    fn test_partition_large_mesh() {
        let mesh = create_test_mesh(10000); // 20000 triangles
        let he_mesh = HalfEdgeMesh::from_bevy_mesh(&mesh).unwrap();

        let config = PartitionConfig {
            target_faces: 5000,
            min_faces: 2000,
            max_faces: 8000,
        };

        let chunked = partition_mesh(&he_mesh, &config);

        // Should be split into multiple chunks
        assert!(chunked.chunk_count() > 1);

        // Total faces should match
        assert_eq!(chunked.total_face_count(), he_mesh.face_count());

        // Each chunk should be within bounds
        for (_, chunk) in &chunked.chunks {
            assert!(chunk.face_count() <= config.max_faces);
        }
    }
}
