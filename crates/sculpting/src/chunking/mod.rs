//! Mesh chunking system for localized GPU updates during sculpting.
//!
//! When tessellation changes mesh topology, rebuilding the entire vertex buffer
//! is expensive for large meshes. Chunking solves this by:
//!
//! 1. **Spatial partitioning**: Divide mesh into regions, each with its own buffer
//! 2. **Local updates**: Only rebuild the chunk being sculpted
//! 3. **Invisible to user**: All chunks behave as a single object
//! 4. **Merge on exit**: Chunks reunify when leaving sculpt mode
//!
//! # Architecture
//!
//! - [`MeshChunk`] - A spatial region of the mesh with its own half-edge data
//! - [`ChunkedMesh`] - Manages all chunks and coordinates updates
//! - [`BoundaryVertex`] - Tracks vertices shared between adjacent chunks

mod boundary;
pub mod merge;
pub mod partition;

pub use boundary::{
    BoundaryVertex, get_original_vertex_id, is_boundary_vertex, sync_vertex_position,
};
pub use merge::{MergeResult, merge_chunks, merge_two_chunks, rebalance_chunks};
pub use partition::{PartitionConfig, partition_mesh, split_chunk};

use crate::ChunkConfig;
use glam::{UVec3, Vec3};
use painting::half_edge::{HalfEdgeMesh, VertexId};
use std::collections::{HashMap, HashSet};

#[cfg(feature = "bevy")]
use bevy::prelude::*;

/// Unique identifier for a chunk within a ChunkedMesh.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkId(pub u32);

/// Axis-aligned bounding box for spatial queries.
#[derive(Debug, Clone, Copy)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    /// Create a new AABB from min/max corners.
    pub fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    /// Create an AABB that contains nothing (for accumulation).
    pub fn empty() -> Self {
        Self {
            min: Vec3::splat(f32::MAX),
            max: Vec3::splat(f32::MIN),
        }
    }

    /// Expand this AABB to include a point.
    pub fn include_point(&mut self, point: Vec3) {
        self.min = self.min.min(point);
        self.max = self.max.max(point);
    }

    /// Check if this AABB intersects a sphere.
    pub fn intersects_sphere(&self, center: Vec3, radius: f32) -> bool {
        // Find closest point on AABB to sphere center
        let closest = center.clamp(self.min, self.max);
        closest.distance_squared(center) <= radius * radius
    }

    /// Check if this AABB contains a point.
    pub fn contains_point(&self, point: Vec3) -> bool {
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
            && point.z >= self.min.z
            && point.z <= self.max.z
    }

    /// Get the center of this AABB.
    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    /// Get the size (extent) of this AABB.
    pub fn size(&self) -> Vec3 {
        self.max - self.min
    }

    /// Get the longest axis (0=x, 1=y, 2=z).
    pub fn longest_axis(&self) -> usize {
        let size = self.size();
        if size.x >= size.y && size.x >= size.z {
            0
        } else if size.y >= size.z {
            1
        } else {
            2
        }
    }
}

/// A spatial chunk of mesh data.
///
/// Each chunk contains a subset of the mesh's vertices and faces,
/// with its own half-edge topology for independent editing.
#[derive(Debug, Clone)]
pub struct MeshChunk {
    /// Unique identifier for this chunk.
    pub id: ChunkId,
    /// Bounding box of this chunk's geometry.
    pub bounds: Aabb,
    /// Half-edge mesh for this chunk's vertices/faces.
    pub mesh: HalfEdgeMesh,
    /// Map from this chunk's local vertex IDs to the original mesh vertex IDs.
    pub local_to_original: HashMap<VertexId, VertexId>,
    /// Map from original mesh vertex IDs to this chunk's local vertex IDs.
    pub original_to_local: HashMap<VertexId, VertexId>,
    /// Vertices shared with adjacent chunks.
    /// Maps local vertex ID to list of (chunk_id, their_local_vertex_id).
    pub boundary_vertices: HashMap<VertexId, Vec<BoundaryVertex>>,
    /// Whether vertex positions have changed (need GPU buffer update).
    pub dirty: bool,
    /// Whether topology has changed (need full buffer rebuild).
    pub topology_changed: bool,
}

impl MeshChunk {
    /// Get the number of faces in this chunk.
    pub fn face_count(&self) -> usize {
        self.mesh.face_count()
    }

    /// Get the number of vertices in this chunk.
    pub fn vertex_count(&self) -> usize {
        self.mesh.vertex_count()
    }

    /// Mark this chunk as needing a GPU update.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Mark this chunk as having topology changes.
    pub fn mark_topology_changed(&mut self) {
        self.topology_changed = true;
        self.dirty = true;
    }

    /// Clear dirty flags after GPU sync.
    pub fn clear_dirty(&mut self) {
        self.dirty = false;
        self.topology_changed = false;
    }

    /// Update the bounding box from current vertex positions.
    pub fn recalculate_bounds(&mut self) {
        let mut bounds = Aabb::empty();
        for vertex in self.mesh.vertices() {
            bounds.include_point(vertex.position);
        }
        self.bounds = bounds;
    }
}

/// Manages all chunks for a sculpted mesh.
///
/// Coordinates chunk creation, updates, and merging.
#[derive(Debug, Clone)]
pub struct ChunkedMesh {
    /// All chunks indexed by ID.
    pub chunks: HashMap<ChunkId, MeshChunk>,
    /// Next available chunk ID.
    next_chunk_id: u32,
    /// Next available "original" vertex ID for tessellation-created vertices.
    /// This ensures globally unique IDs when registering new vertices from edge splits.
    /// Initialized to max_original_vertex_id + 1 during partitioning.
    pub next_original_vertex_id: u32,
    /// Overall bounding box of the entire mesh.
    pub bounds: Aabb,
    /// Configuration for chunk sizing.
    pub config: ChunkConfig,
    /// Spatial index for fast chunk lookup by position.
    /// Grid cell -> list of chunk IDs that overlap this cell.
    spatial_grid: HashMap<UVec3, Vec<ChunkId>>,
    /// Size of each spatial grid cell.
    grid_cell_size: f32,
}

impl ChunkedMesh {
    /// Compare all authoritative geometry, identities and allocation counters.
    /// Bounds, spatial indexes and GPU dirty flags are disposable derived state.
    pub fn same_authoritative_state(&self, other: &Self) -> bool {
        self.next_chunk_id == other.next_chunk_id
            && self.next_original_vertex_id == other.next_original_vertex_id
            && self.config == other.config
            && self.chunks.len() == other.chunks.len()
            && self.chunks.iter().all(|(id, chunk)| {
                other.chunks.get(id).is_some_and(|rhs| {
                    chunk.id == rhs.id
                        && chunk.mesh == rhs.mesh
                        && chunk.local_to_original == rhs.local_to_original
                        && chunk.original_to_local == rhs.original_to_local
                        && chunk.boundary_vertices == rhs.boundary_vertices
                })
            })
    }

    /// Validate snapshot-owned identities and counters, including unused vertices.
    pub(crate) fn validate_snapshot_identity(&self) -> Result<(), crate::SafetyError> {
        let invalid =
            || crate::SafetyError::Topology("invalid snapshot identities or counters".into());
        for (id, chunk) in &self.chunks {
            if *id != chunk.id
                || id.0 >= self.next_chunk_id
                || chunk.local_to_original.len() != chunk.mesh.vertex_count()
                || chunk.original_to_local.len() != chunk.mesh.vertex_count()
            {
                return Err(invalid());
            }
            for vertex in chunk.mesh.vertices() {
                if !vertex.position.is_finite()
                    || !vertex.normal.is_finite()
                    || vertex.uv.is_some_and(|uv| !uv.is_finite())
                {
                    return Err(invalid());
                }
                let original = chunk
                    .local_to_original
                    .get(&vertex.id)
                    .ok_or_else(invalid)?;
                if original.0 >= self.next_original_vertex_id
                    || chunk.original_to_local.get(original) != Some(&vertex.id)
                {
                    return Err(invalid());
                }
            }
            for edge in chunk.mesh.half_edges() {
                if edge.corner_uv.is_some_and(|uv| !uv.is_finite()) {
                    return Err(invalid());
                }
            }
            for (local, peers) in &chunk.boundary_vertices {
                let original = chunk.local_to_original.get(local).ok_or_else(invalid)?;
                for peer in peers {
                    let neighbor = self.chunks.get(&peer.chunk_id).ok_or_else(invalid)?;
                    if peer.original_vertex_id != *original
                        || neighbor.local_to_original.get(&peer.vertex_id) != Some(original)
                        || !neighbor
                            .boundary_vertices
                            .get(&peer.vertex_id)
                            .is_some_and(|back| {
                                back.iter().any(|p| {
                                    p.chunk_id == *id
                                        && p.vertex_id == *local
                                        && p.original_vertex_id == *original
                                })
                            })
                    {
                        return Err(invalid());
                    }
                }
            }
        }
        // Completeness matters as well as reciprocity: a missing seam relation
        // would otherwise pass a loop over only the references that remain.
        let mut canonical = self.clone();
        canonical.rebuild_boundary_relationships();
        for (id, chunk) in &self.chunks {
            let expected = &canonical.chunks[id].boundary_vertices;
            if chunk.boundary_vertices.len() != expected.len() {
                return Err(invalid());
            }
            for (local, peers) in expected {
                let mut want: Vec<_> = peers
                    .iter()
                    .map(|p| (p.chunk_id.0, p.vertex_id.0, p.original_vertex_id.0))
                    .collect();
                let mut got: Vec<_> = chunk
                    .boundary_vertices
                    .get(local)
                    .ok_or_else(invalid)?
                    .iter()
                    .map(|p| (p.chunk_id.0, p.vertex_id.0, p.original_vertex_id.0))
                    .collect();
                want.sort_unstable();
                got.sort_unstable();
                if want != got {
                    return Err(invalid());
                }
            }
            if chunk
                .mesh
                .faces()
                .iter()
                .any(|face| !face.normal.is_finite())
            {
                return Err(invalid());
            }
        }
        Ok(())
    }

    /// Charge every owned allocation, including private spatial lookup storage.
    pub fn retained_bytes(&self) -> usize {
        fn table<K, V>(map: &HashMap<K, V>) -> usize {
            map.capacity()
                .saturating_mul(4 * (std::mem::size_of::<(K, V)>() + 1))
                .saturating_add(64)
        }
        let mut bytes = std::mem::size_of::<Self>()
            .saturating_add(table(&self.chunks))
            .saturating_add(table(&self.spatial_grid));
        for ids in self.spatial_grid.values() {
            bytes = bytes.saturating_add(
                ids.capacity()
                    .saturating_mul(std::mem::size_of::<ChunkId>()),
            );
        }
        for chunk in self.chunks.values() {
            bytes = bytes
                .saturating_add(chunk.mesh.retained_bytes())
                .saturating_add(table(&chunk.local_to_original))
                .saturating_add(table(&chunk.original_to_local))
                .saturating_add(table(&chunk.boundary_vertices));
            for peers in chunk.boundary_vertices.values() {
                bytes = bytes.saturating_add(
                    peers
                        .capacity()
                        .saturating_mul(std::mem::size_of::<BoundaryVertex>()),
                );
            }
        }
        bytes
    }

    /// Create a new empty chunked mesh with default configuration.
    pub fn new() -> Self {
        Self::with_config(ChunkConfig::default())
    }

    /// Create a new empty chunked mesh with custom configuration.
    pub fn with_config(config: ChunkConfig) -> Self {
        Self {
            chunks: HashMap::new(),
            next_chunk_id: 0,
            next_original_vertex_id: 0, // Will be set during partitioning
            bounds: Aabb::empty(),
            config,
            spatial_grid: HashMap::new(),
            grid_cell_size: 1.0, // Will be adjusted during partitioning
        }
    }

    /// Get the total number of chunks.
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// Get the total number of vertices across all chunks.
    pub fn total_vertex_count(&self) -> usize {
        self.chunks.values().map(|c| c.vertex_count()).sum()
    }

    /// Get the total number of faces across all chunks.
    pub fn total_face_count(&self) -> usize {
        self.chunks.values().map(|c| c.face_count()).sum()
    }

    /// Get a chunk by ID.
    pub fn get_chunk(&self, id: ChunkId) -> Option<&MeshChunk> {
        self.chunks.get(&id)
    }

    /// Get a mutable chunk by ID.
    pub fn get_chunk_mut(&mut self, id: ChunkId) -> Option<&mut MeshChunk> {
        self.chunks.get_mut(&id)
    }

    /// Allocate a new chunk ID.
    fn allocate_chunk_id(&mut self) -> ChunkId {
        let id = ChunkId(self.next_chunk_id);
        self.next_chunk_id += 1;
        id
    }

    /// Allocate a globally unique "original" vertex ID for tessellation-created vertices.
    ///
    /// This ensures that vertices created by edge splits in different chunks
    /// don't collide when chunks are later merged.
    pub fn allocate_original_vertex_id(&mut self) -> VertexId {
        let id = VertexId(self.next_original_vertex_id);
        self.next_original_vertex_id += 1;
        id
    }

    /// Add a chunk to the mesh.
    pub fn add_chunk(&mut self, mut chunk: MeshChunk) -> ChunkId {
        let id = self.allocate_chunk_id();
        chunk.id = id;

        // Update overall bounds
        self.bounds.include_point(chunk.bounds.min);
        self.bounds.include_point(chunk.bounds.max);

        // Add to spatial grid
        self.add_to_spatial_grid(id, &chunk.bounds);

        self.chunks.insert(id, chunk);
        id
    }

    /// Remove a chunk from the mesh.
    pub fn remove_chunk(&mut self, id: ChunkId) -> Option<MeshChunk> {
        if let Some(chunk) = self.chunks.remove(&id) {
            self.remove_from_spatial_grid(id, &chunk.bounds);
            Some(chunk)
        } else {
            None
        }
    }

    /// Find all chunks that intersect a sphere (for brush queries).
    pub fn chunks_intersecting_sphere(&self, center: Vec3, radius: f32) -> Vec<ChunkId> {
        // Early exit if no chunks or invalid bounds
        if self.chunks.is_empty() || !self.bounds_valid() {
            return Vec::new();
        }

        let mut result = HashSet::new();

        // Get grid cells that the sphere overlaps
        let min_cell = self.world_to_grid(center - Vec3::splat(radius));
        let max_cell = self.world_to_grid(center + Vec3::splat(radius));

        // Safety check: limit loop iterations to prevent OOM from pathological cases
        const MAX_CELLS_PER_AXIS: u32 = 100;
        let x_range = (max_cell.x.saturating_sub(min_cell.x)).min(MAX_CELLS_PER_AXIS);
        let y_range = (max_cell.y.saturating_sub(min_cell.y)).min(MAX_CELLS_PER_AXIS);
        let z_range = (max_cell.z.saturating_sub(min_cell.z)).min(MAX_CELLS_PER_AXIS);

        for dx in 0..=x_range {
            for dy in 0..=y_range {
                for dz in 0..=z_range {
                    let cell = UVec3::new(
                        min_cell.x.saturating_add(dx),
                        min_cell.y.saturating_add(dy),
                        min_cell.z.saturating_add(dz),
                    );
                    if let Some(chunks) = self.spatial_grid.get(&cell) {
                        for &chunk_id in chunks {
                            if let Some(chunk) = self.chunks.get(&chunk_id) {
                                if chunk.bounds.intersects_sphere(center, radius) {
                                    result.insert(chunk_id);
                                }
                            }
                        }
                    }
                }
            }
        }

        result.into_iter().collect()
    }

    /// Find the chunk containing a point (returns first match).
    pub fn chunk_containing_point(&self, point: Vec3) -> Option<ChunkId> {
        let cell = self.world_to_grid(point);
        if let Some(chunks) = self.spatial_grid.get(&cell) {
            for &chunk_id in chunks {
                if let Some(chunk) = self.chunks.get(&chunk_id) {
                    if chunk.bounds.contains_point(point) {
                        return Some(chunk_id);
                    }
                }
            }
        }
        None
    }

    /// Get all chunks that are marked dirty.
    pub fn dirty_chunks(&self) -> Vec<ChunkId> {
        self.chunks
            .iter()
            .filter(|(_, c)| c.dirty)
            .map(|(&id, _)| id)
            .collect()
    }

    /// Get all chunks that have topology changes.
    pub fn topology_changed_chunks(&self) -> Vec<ChunkId> {
        self.chunks
            .iter()
            .filter(|(_, c)| c.topology_changed)
            .map(|(&id, _)| id)
            .collect()
    }

    /// Check if bounds are valid (not empty or inverted).
    fn bounds_valid(&self) -> bool {
        self.bounds.min.x <= self.bounds.max.x
            && self.bounds.min.y <= self.bounds.max.y
            && self.bounds.min.z <= self.bounds.max.z
            && self.bounds.min.x.is_finite()
            && self.bounds.max.x.is_finite()
            && self.bounds.min.y.is_finite()
            && self.bounds.max.y.is_finite()
            && self.bounds.min.z.is_finite()
            && self.bounds.max.z.is_finite()
            && self.grid_cell_size > 0.0
            && self.grid_cell_size.is_finite()
    }

    /// Convert world position to grid cell coordinates.
    fn world_to_grid(&self, pos: Vec3) -> UVec3 {
        // Guard against invalid bounds - return zero to prevent pathological loops
        if !self.bounds_valid() {
            return UVec3::ZERO;
        }

        let offset = pos - self.bounds.min;
        let cell = offset / self.grid_cell_size;

        // Clamp to reasonable maximum (prevents billion-iteration loops)
        const MAX_GRID_CELLS: u32 = 1000;
        UVec3::new(
            (cell.x.max(0.0).min(MAX_GRID_CELLS as f32)) as u32,
            (cell.y.max(0.0).min(MAX_GRID_CELLS as f32)) as u32,
            (cell.z.max(0.0).min(MAX_GRID_CELLS as f32)) as u32,
        )
    }

    /// Add a chunk to the spatial grid.
    fn add_to_spatial_grid(&mut self, id: ChunkId, bounds: &Aabb) {
        // Skip if bounds are invalid
        if !self.bounds_valid() {
            return;
        }

        let min_cell = self.world_to_grid(bounds.min);
        let max_cell = self.world_to_grid(bounds.max);

        // Safety limit on iterations
        const MAX_CELLS_PER_AXIS: u32 = 100;
        let x_range = (max_cell.x.saturating_sub(min_cell.x)).min(MAX_CELLS_PER_AXIS);
        let y_range = (max_cell.y.saturating_sub(min_cell.y)).min(MAX_CELLS_PER_AXIS);
        let z_range = (max_cell.z.saturating_sub(min_cell.z)).min(MAX_CELLS_PER_AXIS);

        for dx in 0..=x_range {
            for dy in 0..=y_range {
                for dz in 0..=z_range {
                    let cell = UVec3::new(
                        min_cell.x.saturating_add(dx),
                        min_cell.y.saturating_add(dy),
                        min_cell.z.saturating_add(dz),
                    );
                    self.spatial_grid.entry(cell).or_default().push(id);
                }
            }
        }
    }

    /// Remove a chunk from the spatial grid.
    fn remove_from_spatial_grid(&mut self, id: ChunkId, bounds: &Aabb) {
        // Skip if bounds are invalid
        if !self.bounds_valid() {
            return;
        }

        let min_cell = self.world_to_grid(bounds.min);
        let max_cell = self.world_to_grid(bounds.max);

        // Safety limit on iterations
        const MAX_CELLS_PER_AXIS: u32 = 100;
        let x_range = (max_cell.x.saturating_sub(min_cell.x)).min(MAX_CELLS_PER_AXIS);
        let y_range = (max_cell.y.saturating_sub(min_cell.y)).min(MAX_CELLS_PER_AXIS);
        let z_range = (max_cell.z.saturating_sub(min_cell.z)).min(MAX_CELLS_PER_AXIS);

        for dx in 0..=x_range {
            for dy in 0..=y_range {
                for dz in 0..=z_range {
                    let cell = UVec3::new(
                        min_cell.x.saturating_add(dx),
                        min_cell.y.saturating_add(dy),
                        min_cell.z.saturating_add(dz),
                    );
                    if let Some(chunks) = self.spatial_grid.get_mut(&cell) {
                        chunks.retain(|&c| c != id);
                    }
                }
            }
        }
    }

    /// Rebuild the spatial grid after bulk operations.
    pub fn rebuild_spatial_grid(&mut self) {
        self.spatial_grid.clear();

        // Recalculate overall bounds
        self.bounds = Aabb::empty();
        for chunk in self.chunks.values() {
            self.bounds.include_point(chunk.bounds.min);
            self.bounds.include_point(chunk.bounds.max);
        }

        // Adjust grid cell size based on mesh bounds
        let mesh_size = self.bounds.size();
        let max_dim = mesh_size.x.max(mesh_size.y).max(mesh_size.z);
        self.grid_cell_size = (max_dim / 10.0).max(1.0);

        // Re-add all chunks
        let chunk_ids: Vec<_> = self.chunks.keys().copied().collect();
        for id in chunk_ids {
            let bounds = self.chunks[&id].bounds;
            self.add_to_spatial_grid(id, &bounds);
        }
    }

    /// Refresh both sides of every boundary reference after local compaction.
    /// Global IDs survive topology edits; cached neighbor-local IDs do not.
    pub fn rebuild_boundary_relationships(&mut self) {
        boundary::build_boundary_relationships(self);
    }

    /// Synchronize a vertex position change to all chunks sharing this boundary vertex.
    ///
    /// Call this after modifying a vertex that may be shared across chunk boundaries.
    pub fn sync_boundary_vertex(
        &mut self,
        chunk_id: ChunkId,
        local_vertex_id: VertexId,
        new_position: Vec3,
    ) {
        // Get the boundary info for this vertex
        let boundary_refs: Vec<BoundaryVertex> = {
            let chunk = match self.chunks.get(&chunk_id) {
                Some(c) => c,
                None => return,
            };
            match chunk.boundary_vertices.get(&local_vertex_id) {
                Some(refs) => refs.clone(),
                None => return,
            }
        };

        // Update in neighboring chunks
        for boundary_ref in boundary_refs {
            if let Some(neighbor) = self.chunks.get_mut(&boundary_ref.chunk_id) {
                if let Some(&local_id) = neighbor
                    .original_to_local
                    .get(&boundary_ref.original_vertex_id)
                {
                    neighbor.mesh.set_vertex_position(local_id, new_position);
                    neighbor.mark_dirty();
                }
            }
        }
    }

    /// Recalculate normals for all dirty chunks.
    pub fn recalculate_normals(&mut self) {
        for chunk in self.chunks.values_mut() {
            if chunk.dirty {
                chunk.mesh.recalculate_face_normals();
                chunk.mesh.recalculate_vertex_normals();
            }
        }
    }

    /// Recalculate boundary vertex normals across chunk edges.
    ///
    /// This ensures seamless normals at chunk boundaries by averaging
    /// normals from faces in adjacent chunks.
    pub fn recalculate_boundary_normals(&mut self) {
        boundary::recalculate_boundary_normals(self);
    }
}

impl Default for ChunkedMesh {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aabb_intersects_sphere() {
        let aabb = Aabb::new(Vec3::ZERO, Vec3::ONE);

        // Sphere at center should intersect
        assert!(aabb.intersects_sphere(Vec3::splat(0.5), 0.1));

        // Sphere outside should not intersect
        assert!(!aabb.intersects_sphere(Vec3::splat(3.0), 0.5));

        // Sphere touching edge should intersect
        assert!(aabb.intersects_sphere(Vec3::new(1.5, 0.5, 0.5), 0.5));
    }

    #[test]
    fn test_aabb_longest_axis() {
        let aabb = Aabb::new(Vec3::ZERO, Vec3::new(10.0, 5.0, 2.0));
        assert_eq!(aabb.longest_axis(), 0); // X is longest

        let aabb = Aabb::new(Vec3::ZERO, Vec3::new(2.0, 10.0, 5.0));
        assert_eq!(aabb.longest_axis(), 1); // Y is longest

        let aabb = Aabb::new(Vec3::ZERO, Vec3::new(2.0, 5.0, 10.0));
        assert_eq!(aabb.longest_axis(), 2); // Z is longest
    }
}
