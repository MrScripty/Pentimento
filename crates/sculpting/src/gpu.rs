//! GPU synchronization for sculpted mesh chunks.
//!
//! This module handles efficient updates to Bevy GPU buffers when chunks
//! are modified during sculpting. It supports two update modes:
//!
//! - **Full rebuild**: When topology changes (tessellation), rebuild the entire chunk mesh
//! - **Vertex patching**: When only positions/normals change, patch in-place (future optimization)
//!
//! Currently all updates use full rebuild for simplicity. Vertex patching can be
//! added as an optimization when profiling shows it's beneficial.

#[cfg(feature = "bevy")]
use bevy::prelude::*;
#[cfg(feature = "bevy")]
use bevy::asset::Assets;

use crate::chunking::{ChunkId, ChunkedMesh, MeshChunk};
use painting::half_edge::VertexId;
use std::collections::HashSet;

/// Tracks which vertices have been modified and need normal recalculation.
#[derive(Debug, Default)]
pub struct DirtyVertices {
    /// Set of vertex IDs that have been modified
    pub modified: HashSet<VertexId>,
}

impl DirtyVertices {
    /// Create a new empty dirty vertex tracker.
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark a vertex as modified.
    pub fn mark(&mut self, vertex_id: VertexId) {
        self.modified.insert(vertex_id);
    }

    /// Mark multiple vertices as modified.
    pub fn mark_all(&mut self, vertices: impl IntoIterator<Item = VertexId>) {
        self.modified.extend(vertices);
    }

    /// Clear all dirty flags.
    pub fn clear(&mut self) {
        self.modified.clear();
    }

    /// Check if any vertices are dirty.
    pub fn is_empty(&self) -> bool {
        self.modified.is_empty()
    }

    /// Get the number of dirty vertices.
    pub fn len(&self) -> usize {
        self.modified.len()
    }
}

/// Result of syncing chunks to GPU.
#[derive(Debug, Default)]
pub struct SyncResult {
    /// Number of chunks that were rebuilt (topology changed)
    pub chunks_rebuilt: usize,
    /// Number of chunks that were patched (position only)
    pub chunks_patched: usize,
    /// Number of chunks skipped (not dirty)
    pub chunks_skipped: usize,
}

/// Sync all dirty chunks to GPU.
///
/// This function iterates through all chunks and updates their corresponding
/// Bevy mesh assets. Chunks marked with `topology_changed` get a full rebuild,
/// while chunks that are just `dirty` could use vertex patching (currently
/// also does full rebuild for simplicity).
#[cfg(feature = "bevy")]
pub fn sync_chunks_to_gpu(
    chunked_mesh: &mut ChunkedMesh,
    meshes: &mut Assets<Mesh>,
    chunk_handles: &std::collections::HashMap<ChunkId, Handle<Mesh>>,
) -> SyncResult {
    let mut result = SyncResult::default();

    for (chunk_id, chunk) in chunked_mesh.chunks.iter_mut() {
        if !chunk.dirty && !chunk.topology_changed {
            result.chunks_skipped += 1;
            continue;
        }

        let Some(handle) = chunk_handles.get(chunk_id) else {
            // No handle for this chunk yet - skip
            continue;
        };

        if chunk.topology_changed {
            // Full rebuild - topology has changed
            if let Some(mesh) = meshes.get_mut(handle) {
                *mesh = chunk.mesh.to_bevy_mesh();
            }
            chunk.topology_changed = false;
            chunk.dirty = false;
            result.chunks_rebuilt += 1;
        } else if chunk.dirty {
            // For now, also do full rebuild for dirty chunks
            // Future optimization: patch vertex buffers in-place
            if let Some(mesh) = meshes.get_mut(handle) {
                *mesh = chunk.mesh.to_bevy_mesh();
            }
            chunk.dirty = false;
            result.chunks_patched += 1;
        }
    }

    result
}

/// Sync a single chunk to GPU.
#[cfg(feature = "bevy")]
pub fn sync_chunk_to_gpu(
    chunk: &mut MeshChunk,
    meshes: &mut Assets<Mesh>,
    handle: &Handle<Mesh>,
) -> bool {
    if !chunk.dirty && !chunk.topology_changed {
        return false;
    }

    if let Some(mesh) = meshes.get_mut(handle) {
        *mesh = chunk.mesh.to_bevy_mesh();
    }

    chunk.topology_changed = false;
    chunk.dirty = false;
    true
}

/// Recalculate normals for vertices that have been modified.
///
/// This updates vertex normals based on the average of adjacent face normals.
/// Should be called after deformation but before GPU sync.
pub fn recalculate_normals_for_dirty(chunk: &mut MeshChunk, dirty: &DirtyVertices) {
    use glam::Vec3;

    for &vertex_id in &dirty.modified {
        // Get all faces adjacent to this vertex
        let face_ids = chunk.mesh.get_vertex_faces(vertex_id);
        if face_ids.is_empty() {
            continue;
        }

        // Calculate average face normal
        let mut normal_sum = Vec3::ZERO;
        for face_id in face_ids {
            if let Some(face) = chunk.mesh.face(face_id) {
                normal_sum += face.normal;
            }
        }

        let averaged = normal_sum.normalize_or_zero();
        if let Some(vertex) = chunk.mesh.vertex_mut(vertex_id) {
            vertex.normal = averaged;
        }
    }
}

/// Recalculate face normals for faces that contain modified vertices.
///
/// This should be called before `recalculate_normals_for_dirty` to ensure
/// face normals are up-to-date.
pub fn recalculate_face_normals_for_dirty(chunk: &mut MeshChunk, dirty: &DirtyVertices) {
    use glam::Vec3;
    use std::collections::HashSet;

    // Collect all faces that need updating
    let mut dirty_faces: HashSet<painting::half_edge::FaceId> = HashSet::new();
    for &vertex_id in &dirty.modified {
        let face_ids = chunk.mesh.get_vertex_faces(vertex_id);
        dirty_faces.extend(face_ids);
    }

    // Recalculate each dirty face's normal
    for face_id in dirty_faces {
        let face_verts = chunk.mesh.get_face_vertices(face_id);
        if face_verts.len() < 3 {
            continue;
        }

        // Get vertex positions
        let positions: Vec<Vec3> = face_verts
            .iter()
            .filter_map(|&vid| chunk.mesh.vertex(vid).map(|v| v.position))
            .collect();

        if positions.len() < 3 {
            continue;
        }

        // Calculate face normal from first three vertices
        let v0 = positions[0];
        let v1 = positions[1];
        let v2 = positions[2];
        let edge1 = v1 - v0;
        let edge2 = v2 - v0;
        let normal = edge1.cross(edge2).normalize_or_zero();

        // Update face normal
        if let Some(face) = chunk.mesh.face_mut(face_id) {
            face.normal = normal;
        }
    }
}

/// Full normal recalculation pipeline for a chunk after deformation.
///
/// This updates both face normals and vertex normals for affected geometry.
pub fn update_normals_after_deformation(chunk: &mut MeshChunk, dirty: &DirtyVertices) {
    // First update face normals
    recalculate_face_normals_for_dirty(chunk, dirty);
    // Then update vertex normals based on new face normals
    recalculate_normals_for_dirty(chunk, dirty);
}

/// Create Bevy mesh handles for all chunks in a chunked mesh.
#[cfg(feature = "bevy")]
pub fn create_chunk_meshes(
    chunked_mesh: &ChunkedMesh,
    meshes: &mut Assets<Mesh>,
) -> std::collections::HashMap<ChunkId, Handle<Mesh>> {
    let mut handles = std::collections::HashMap::new();

    for (chunk_id, chunk) in &chunked_mesh.chunks {
        let bevy_mesh = chunk.mesh.to_bevy_mesh();
        let handle = meshes.add(bevy_mesh);
        handles.insert(*chunk_id, handle);
    }

    handles
}

/// Remove chunk meshes from assets.
#[cfg(feature = "bevy")]
pub fn remove_chunk_meshes(
    chunk_handles: &std::collections::HashMap<ChunkId, Handle<Mesh>>,
    meshes: &mut Assets<Mesh>,
) {
    for handle in chunk_handles.values() {
        meshes.remove(handle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dirty_vertices() {
        let mut dirty = DirtyVertices::new();
        assert!(dirty.is_empty());

        dirty.mark(VertexId(0));
        dirty.mark(VertexId(1));
        assert_eq!(dirty.len(), 2);

        dirty.mark(VertexId(0)); // Duplicate
        assert_eq!(dirty.len(), 2);

        dirty.clear();
        assert!(dirty.is_empty());
    }

    #[test]
    fn test_dirty_vertices_mark_all() {
        let mut dirty = DirtyVertices::new();
        dirty.mark_all([VertexId(0), VertexId(1), VertexId(2)]);
        assert_eq!(dirty.len(), 3);
    }
}
