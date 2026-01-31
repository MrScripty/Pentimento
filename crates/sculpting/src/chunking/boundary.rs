//! Boundary vertex tracking and synchronization.
//!
//! When a mesh is partitioned into chunks, vertices at chunk boundaries are
//! duplicated in each adjacent chunk. This module tracks these relationships
//! and ensures consistent positions and normals across chunk boundaries.

use super::{ChunkId, ChunkedMesh};
use glam::Vec3;
use painting::half_edge::VertexId;

/// Reference to a vertex in another chunk that shares the same position.
#[derive(Debug, Clone, Copy)]
pub struct BoundaryVertex {
    /// The chunk containing the mirrored vertex.
    pub chunk_id: ChunkId,
    /// The vertex ID in that chunk.
    pub vertex_id: VertexId,
    /// Original vertex ID from the source mesh (before chunking).
    pub original_vertex_id: VertexId,
}

impl BoundaryVertex {
    /// Create a new boundary vertex reference.
    pub fn new(chunk_id: ChunkId, vertex_id: VertexId, original_vertex_id: VertexId) -> Self {
        Self {
            chunk_id,
            vertex_id,
            original_vertex_id,
        }
    }
}

/// Build boundary vertex relationships between chunks.
///
/// This function analyzes all chunks and identifies vertices that are
/// shared (have the same original vertex ID) between adjacent chunks.
pub fn build_boundary_relationships(chunked_mesh: &mut ChunkedMesh) {
    // First, build a map of original_vertex_id -> [(chunk_id, local_vertex_id)]
    let mut original_to_chunks: std::collections::HashMap<
        VertexId,
        Vec<(ChunkId, VertexId)>,
    > = std::collections::HashMap::new();

    for (&chunk_id, chunk) in &chunked_mesh.chunks {
        for (&local_id, &original_id) in &chunk.local_to_original {
            original_to_chunks
                .entry(original_id)
                .or_default()
                .push((chunk_id, local_id));
        }
    }

    // Clear existing boundary info
    for chunk in chunked_mesh.chunks.values_mut() {
        chunk.boundary_vertices.clear();
    }

    // For each original vertex that appears in multiple chunks, record boundary relationships
    for (original_id, locations) in original_to_chunks.into_iter() {
        let locations: Vec<(ChunkId, VertexId)> = locations;
        if locations.len() <= 1 {
            // Not shared between chunks
            continue;
        }

        // For each occurrence, link to all other occurrences
        for &(chunk_id, local_id) in &locations {
            let others: Vec<BoundaryVertex> = locations
                .iter()
                .filter(|&&(cid, _)| cid != chunk_id)
                .map(|&(other_chunk_id, other_local_id)| {
                    BoundaryVertex::new(other_chunk_id, other_local_id, original_id)
                })
                .collect();

            if let Some(chunk) = chunked_mesh.chunks.get_mut(&chunk_id) {
                chunk.boundary_vertices.insert(local_id, others);
            }
        }
    }
}

/// Recalculate normals for boundary vertices.
///
/// Boundary vertex normals must account for faces from all adjacent chunks
/// to prevent visible seams at chunk boundaries.
pub fn recalculate_boundary_normals(chunked_mesh: &mut ChunkedMesh) {
    // Collect all boundary vertex positions and their contributing face normals
    let mut boundary_normals: std::collections::HashMap<VertexId, Vec<Vec3>> =
        std::collections::HashMap::new();

    // First pass: gather all face normals for boundary vertices
    for chunk in chunked_mesh.chunks.values() {
        for (&local_id, _) in &chunk.boundary_vertices {
            // Get original vertex ID for grouping
            let original_id = match chunk.local_to_original.get(&local_id) {
                Some(&id) => id,
                None => continue,
            };

            // Get face normals for this vertex in this chunk
            let face_ids = chunk.mesh.get_vertex_faces(local_id);
            for face_id in face_ids {
                if let Some(face) = chunk.mesh.face(face_id) {
                    boundary_normals
                        .entry(original_id)
                        .or_default()
                        .push(face.normal);
                }
            }
        }
    }

    // Second pass: compute averaged normals and apply to all instances
    for (original_id, normals) in boundary_normals.into_iter() {
        let normals: Vec<Vec3> = normals;
        if normals.is_empty() {
            continue;
        }

        // Average the normals
        let sum: Vec3 = normals.iter().copied().sum();
        let averaged = sum.normalize_or_zero();

        // Apply to all chunks containing this boundary vertex
        for chunk in chunked_mesh.chunks.values_mut() {
            if let Some(&local_id) = chunk.original_to_local.get(&original_id) {
                if let Some(vertex) = chunk.mesh.vertex_mut(local_id) {
                    vertex.normal = averaged;
                }
            }
        }
    }
}

/// Synchronize a single vertex position across all chunks.
///
/// This is a convenience function for when you've modified a vertex
/// and need to ensure consistency across chunk boundaries.
pub fn sync_vertex_position(
    chunked_mesh: &mut ChunkedMesh,
    chunk_id: ChunkId,
    local_vertex_id: VertexId,
    new_position: Vec3,
) {
    // First, set in the source chunk
    if let Some(chunk) = chunked_mesh.chunks.get_mut(&chunk_id) {
        chunk.mesh.set_vertex_position(local_vertex_id, new_position);
        chunk.mark_dirty();
    }

    // Then sync to neighbors
    chunked_mesh.sync_boundary_vertex(chunk_id, local_vertex_id, new_position);
}

/// Check if a vertex is on a chunk boundary.
pub fn is_boundary_vertex(chunked_mesh: &ChunkedMesh, chunk_id: ChunkId, local_vertex_id: VertexId) -> bool {
    chunked_mesh
        .chunks
        .get(&chunk_id)
        .map(|chunk| chunk.boundary_vertices.contains_key(&local_vertex_id))
        .unwrap_or(false)
}

/// Get the original vertex ID for a local vertex in a chunk.
pub fn get_original_vertex_id(
    chunked_mesh: &ChunkedMesh,
    chunk_id: ChunkId,
    local_vertex_id: VertexId,
) -> Option<VertexId> {
    chunked_mesh
        .chunks
        .get(&chunk_id)
        .and_then(|chunk| chunk.local_to_original.get(&local_vertex_id).copied())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_boundary_vertex_creation() {
        let bv = BoundaryVertex::new(ChunkId(1), VertexId(5), VertexId(100));
        assert_eq!(bv.chunk_id, ChunkId(1));
        assert_eq!(bv.vertex_id, VertexId(5));
        assert_eq!(bv.original_vertex_id, VertexId(100));
    }
}
