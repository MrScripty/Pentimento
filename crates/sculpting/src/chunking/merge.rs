//! Chunk merging for reunifying mesh when exiting sculpt mode.
//!
//! When the user exits sculpt mode, all chunks are merged back into
//! a single unified mesh. Boundary vertices are welded to eliminate
//! duplicates.

use super::{boundary, ChunkId, ChunkedMesh};
use crate::ChunkConfig;
use glam::Vec3;
use painting::half_edge::{Face, FaceId, HalfEdge, HalfEdgeId, HalfEdgeMesh, Vertex, VertexId};
use std::collections::HashMap;

/// Result of merging chunks back into a unified mesh.
#[derive(Debug)]
pub struct MergeResult {
    /// The unified mesh.
    pub mesh: HalfEdgeMesh,
    /// Map from original vertex IDs to the unified mesh vertex IDs.
    pub vertex_mapping: HashMap<VertexId, VertexId>,
}

/// Merge all chunks back into a single unified mesh.
///
/// This function:
/// 1. Collects all unique vertices (welding boundary duplicates)
/// 2. Combines all faces from all chunks
/// 3. Rebuilds half-edge connectivity
pub fn merge_chunks(chunked_mesh: &ChunkedMesh) -> MergeResult {
    // Step 1: Collect unique vertices by original ID
    // Boundary vertices appear in multiple chunks but have the same original ID
    let mut original_vertices: HashMap<VertexId, Vec3> = HashMap::new();
    let mut original_normals: HashMap<VertexId, Vec3> = HashMap::new();
    let mut original_uvs: HashMap<VertexId, Option<glam::Vec2>> = HashMap::new();

    for chunk in chunked_mesh.chunks.values() {
        for vertex in chunk.mesh.vertices() {
            let local_id = vertex.id;
            if let Some(&original_id) = chunk.local_to_original.get(&local_id) {
                // For boundary vertices, we might visit them multiple times
                // Take the latest position (they should be synchronized)
                original_vertices.insert(original_id, vertex.position);
                original_normals.insert(original_id, vertex.normal);
                original_uvs.insert(original_id, vertex.uv);
            }
        }
    }

    // Step 2: Create new vertex array with consistent IDs
    // Sort by original ID to maintain deterministic ordering
    let mut sorted_original_ids: Vec<VertexId> = original_vertices.keys().copied().collect();
    sorted_original_ids.sort_by_key(|v| v.0);

    // Create mapping from original IDs to new unified IDs
    let mut original_to_unified: HashMap<VertexId, VertexId> = HashMap::new();
    let mut vertices: Vec<Vertex> = Vec::new();

    for (unified_idx, &original_id) in sorted_original_ids.iter().enumerate() {
        let unified_id = VertexId(unified_idx as u32);
        original_to_unified.insert(original_id, unified_id);

        vertices.push(Vertex {
            id: unified_id,
            position: original_vertices[&original_id],
            normal: original_normals[&original_id],
            uv: original_uvs[&original_id],
            outgoing_half_edge: None,
            source_index: original_id.0,
        });
    }

    // Step 3: Collect all faces and build half-edges
    let mut half_edges: Vec<HalfEdge> = Vec::new();
    let mut faces: Vec<Face> = Vec::new();
    let mut edge_map: HashMap<(VertexId, VertexId), HalfEdgeId> = HashMap::new();

    for chunk in chunked_mesh.chunks.values() {
        for face in chunk.mesh.faces() {
            // Get vertices in this face (in chunk-local IDs)
            let local_verts = chunk.mesh.get_face_vertices(face.id);

            // Map to unified IDs
            let unified_verts: Vec<VertexId> = local_verts
                .iter()
                .filter_map(|&local_id| {
                    chunk
                        .local_to_original
                        .get(&local_id)
                        .and_then(|original_id| original_to_unified.get(original_id).copied())
                })
                .collect();

            if unified_verts.len() != local_verts.len() {
                continue; // Skip malformed faces
            }

            let new_face_id = FaceId(faces.len() as u32);
            let base_he_idx = half_edges.len() as u32;
            let num_verts = unified_verts.len();

            for i in 0..num_verts {
                let he_id = HalfEdgeId(base_he_idx + i as u32);
                let origin = unified_verts[i];
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

                // Find twin half-edge
                let dest = unified_verts[next_idx];
                if let Some(&twin_id) = edge_map.get(&(dest, origin)) {
                    half_edges[he_id.0 as usize].twin = Some(twin_id);
                    half_edges[twin_id.0 as usize].twin = Some(he_id);
                }
                edge_map.insert((origin, dest), he_id);
            }

            faces.push(Face {
                id: new_face_id,
                half_edge: HalfEdgeId(base_he_idx),
                normal: face.normal,
            });
        }
    }

    let mesh = HalfEdgeMesh::from_raw(vertices, half_edges, faces, edge_map);

    MergeResult {
        mesh,
        vertex_mapping: original_to_unified,
    }
}

/// Merge two adjacent chunks into one.
///
/// Used for chunk rebalancing when chunks become too small.
pub fn merge_two_chunks(
    chunked_mesh: &mut ChunkedMesh,
    chunk_a: ChunkId,
    chunk_b: ChunkId,
) -> Option<ChunkId> {
    // Remove both chunks
    let a = chunked_mesh.remove_chunk(chunk_a)?;
    let b = chunked_mesh.remove_chunk(chunk_b)?;

    // Combine original-to-local mappings
    // First, collect all unique original vertex IDs
    let mut all_originals: HashMap<VertexId, Vec3> = HashMap::new();
    let mut all_normals: HashMap<VertexId, Vec3> = HashMap::new();
    let mut all_uvs: HashMap<VertexId, Option<glam::Vec2>> = HashMap::new();

    for (local_id, &original_id) in &a.local_to_original {
        if let Some(v) = a.mesh.vertex(*local_id) {
            all_originals.insert(original_id, v.position);
            all_normals.insert(original_id, v.normal);
            all_uvs.insert(original_id, v.uv);
        }
    }

    for (local_id, &original_id) in &b.local_to_original {
        if let Some(v) = b.mesh.vertex(*local_id) {
            // For shared boundary vertices, prefer chunk B's position
            // (they should be synchronized anyway)
            all_originals.insert(original_id, v.position);
            all_normals.insert(original_id, v.normal);
            all_uvs.insert(original_id, v.uv);
        }
    }

    // Create new vertex array
    let mut sorted_originals: Vec<VertexId> = all_originals.keys().copied().collect();
    sorted_originals.sort_by_key(|v| v.0);

    let mut original_to_new: HashMap<VertexId, VertexId> = HashMap::new();
    let mut new_to_original: HashMap<VertexId, VertexId> = HashMap::new();
    let mut vertices: Vec<Vertex> = Vec::new();

    for (new_idx, &original_id) in sorted_originals.iter().enumerate() {
        let new_id = VertexId(new_idx as u32);
        original_to_new.insert(original_id, new_id);
        new_to_original.insert(new_id, original_id);

        vertices.push(Vertex {
            id: new_id,
            position: all_originals[&original_id],
            normal: all_normals[&original_id],
            uv: all_uvs[&original_id],
            outgoing_half_edge: None,
            source_index: original_id.0,
        });
    }

    // Collect all faces from both chunks
    let mut half_edges: Vec<HalfEdge> = Vec::new();
    let mut faces: Vec<Face> = Vec::new();
    let mut edge_map: HashMap<(VertexId, VertexId), HalfEdgeId> = HashMap::new();

    for chunk in [&a, &b] {
        for face in chunk.mesh.faces() {
            let local_verts = chunk.mesh.get_face_vertices(face.id);

            let new_verts: Vec<VertexId> = local_verts
                .iter()
                .filter_map(|&local_id| {
                    chunk
                        .local_to_original
                        .get(&local_id)
                        .and_then(|original_id| original_to_new.get(original_id).copied())
                })
                .collect();

            if new_verts.len() != local_verts.len() {
                continue;
            }

            let new_face_id = FaceId(faces.len() as u32);
            let base_he_idx = half_edges.len() as u32;
            let num_verts = new_verts.len();

            for i in 0..num_verts {
                let he_id = HalfEdgeId(base_he_idx + i as u32);
                let origin = new_verts[i];
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

                if vertices[origin.0 as usize].outgoing_half_edge.is_none() {
                    vertices[origin.0 as usize].outgoing_half_edge = Some(he_id);
                }

                let dest = new_verts[next_idx];
                if let Some(&twin_id) = edge_map.get(&(dest, origin)) {
                    half_edges[he_id.0 as usize].twin = Some(twin_id);
                    half_edges[twin_id.0 as usize].twin = Some(he_id);
                }
                edge_map.insert((origin, dest), he_id);
            }

            faces.push(Face {
                id: new_face_id,
                half_edge: HalfEdgeId(base_he_idx),
                normal: face.normal,
            });
        }
    }

    let mesh = HalfEdgeMesh::from_raw(vertices, half_edges, faces, edge_map);

    // Calculate bounds
    let mut bounds = super::Aabb::empty();
    for vertex in mesh.vertices() {
        bounds.include_point(vertex.position);
    }

    let merged_chunk = super::MeshChunk {
        id: ChunkId(0),
        bounds,
        mesh,
        local_to_original: new_to_original,
        original_to_local: original_to_new,
        boundary_vertices: HashMap::new(),
        dirty: true,
        topology_changed: true,
    };

    let merged_id = chunked_mesh.add_chunk(merged_chunk);

    // Rebuild boundary relationships
    boundary::build_boundary_relationships(chunked_mesh);

    Some(merged_id)
}

/// Rebalance chunks after sculpting operations.
///
/// This function:
/// 1. Splits chunks that exceed max_faces
/// 2. Merges adjacent chunks that are below min_faces
pub fn rebalance_chunks(chunked_mesh: &mut ChunkedMesh) {
    let config = chunked_mesh.config.clone();

    // Phase 1: Split oversized chunks
    loop {
        let oversized: Vec<ChunkId> = chunked_mesh
            .chunks
            .iter()
            .filter(|(_, c)| c.face_count() > config.max_faces)
            .map(|(&id, _)| id)
            .collect();

        if oversized.is_empty() {
            break;
        }

        for chunk_id in oversized {
            super::partition::split_chunk(chunked_mesh, chunk_id);
        }
    }

    // Phase 2: Merge undersized adjacent chunks
    loop {
        let merge_pair = find_mergeable_pair(chunked_mesh, &config);
        if let Some((a, b)) = merge_pair {
            merge_two_chunks(chunked_mesh, a, b);
        } else {
            break;
        }
    }

    // Rebuild spatial grid after rebalancing
    chunked_mesh.rebuild_spatial_grid();
}

/// Find a pair of adjacent chunks that can be merged.
fn find_mergeable_pair(chunked_mesh: &ChunkedMesh, config: &ChunkConfig) -> Option<(ChunkId, ChunkId)> {
    for (&id, chunk) in &chunked_mesh.chunks {
        if chunk.face_count() >= config.min_faces {
            continue;
        }

        // Find adjacent chunks (those sharing boundary vertices)
        let neighbor_ids: std::collections::HashSet<ChunkId> = chunk
            .boundary_vertices
            .values()
            .flatten()
            .map(|bv| bv.chunk_id)
            .collect();

        // Find smallest neighbor that we can merge with
        let mut best_neighbor: Option<(ChunkId, usize)> = None;

        for neighbor_id in neighbor_ids {
            if let Some(neighbor) = chunked_mesh.chunks.get(&neighbor_id) {
                let combined = chunk.face_count() + neighbor.face_count();
                if combined <= config.max_faces {
                    let count = neighbor.face_count();
                    if best_neighbor.map_or(true, |(_, best_count)| count < best_count) {
                        best_neighbor = Some((neighbor_id, count));
                    }
                }
            }
        }

        if let Some((neighbor_id, _)) = best_neighbor {
            return Some((id, neighbor_id));
        }
    }

    None
}

#[cfg(all(test, feature = "bevy"))]
mod tests {
    use super::*;
    use super::super::partition::{partition_mesh, PartitionConfig};
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::{Indices, PrimitiveTopology};
    use bevy::prelude::*;

    fn create_test_mesh(num_quads: usize) -> Mesh {
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
    fn test_merge_chunks() {
        let mesh = create_test_mesh(100);
        let he_mesh = HalfEdgeMesh::from_bevy_mesh(&mesh).unwrap();

        // Partition into small chunks
        let config = PartitionConfig {
            target_faces: 50,
            min_faces: 20,
            max_faces: 100,
        };

        let chunked = partition_mesh(&he_mesh, &config);

        // Merge back
        let result = merge_chunks(&chunked);

        // Should have same number of faces
        assert_eq!(result.mesh.face_count(), he_mesh.face_count());
    }
}
