//! Sculpting pipeline orchestration.
//!
//! This module coordinates the complete sculpting workflow:
//! 1. Brush input → dab generation
//! 2. Dab → vertex deformation
//! 3. Deformation → tessellation (if enabled)
//! 4. Post-stroke → chunk rebalancing
//! 5. Dirty tracking → GPU sync
//!
//! The pipeline ensures deterministic operation for undo/redo via dab replay.

use crate::brush::{BrushInput, BrushPreset, DabResult, SculptBrushEngine};
use crate::budget::VertexBudget;
use crate::chunking::{ChunkId, ChunkedMesh, MeshChunk};
use crate::deformation::{apply_autosmooth, apply_deformation, DabInfo};
use crate::gpu::{update_normals_after_deformation, DirtyVertices};
use crate::spatial::{Aabb as SpatialAabb, VertexOctree};
use crate::tessellation::{
    tessellate_at_brush, tessellate_at_brush_budget, ScreenSpaceConfig, TessellationStats,
};
use crate::types::{ChunkConfig, SculptStrokePacket, TessellationConfig, TessellationMode};
use glam::Vec3;
use painting::half_edge::VertexId;
use std::collections::{HashMap, HashSet};
use tracing::{debug, error, trace};

/// Result of processing a single dab through the pipeline.
#[derive(Debug, Default)]
pub struct DabProcessResult {
    /// Number of vertices modified by deformation.
    pub vertices_modified: usize,
    /// Chunks that were modified.
    pub chunks_affected: Vec<ChunkId>,
    /// Tessellation statistics (if tessellation was applied).
    pub tessellation: Option<TessellationStats>,
}

/// Result of ending a stroke.
#[derive(Debug, Default)]
pub struct StrokeEndResult {
    /// Completed stroke packets for recording/sync.
    pub packets: Vec<SculptStrokePacket>,
    /// Chunks that were split during rebalancing.
    pub chunks_split: usize,
    /// Chunks that were merged during rebalancing.
    pub chunks_merged: usize,
}

/// Configuration for the sculpting pipeline.
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// Whether to apply tessellation during sculpting.
    pub tessellation_enabled: bool,
    /// Tessellation parameters.
    pub tessellation_config: TessellationConfig,
    /// Chunk sizing parameters.
    pub chunk_config: ChunkConfig,
    /// Whether to rebalance chunks after each stroke.
    pub rebalance_after_stroke: bool,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            // Tessellation now enabled by default after implementing:
            // - Comprehensive edge collapse safety checks (ring boundary, edge flip)
            // - Curvature-aware split positioning
            // - Mesh quality validation
            // See tessellation module for implementation details.
            tessellation_enabled: true,
            tessellation_config: TessellationConfig::default(),
            chunk_config: ChunkConfig::default(),
            // Chunk rebalancing enabled - works with improved tessellation
            rebalance_after_stroke: true,
        }
    }
}

/// State tracked during an active stroke.
#[derive(Debug, Clone)]
struct ActiveStrokeState {
    /// All chunks modified during this stroke.
    affected_chunks: HashSet<ChunkId>,
    /// Previous dab position for stroke direction calculation.
    last_dab_position: Option<Vec3>,
    /// First dab position (for grab brush).
    first_dab_position: Option<Vec3>,
}

impl Default for ActiveStrokeState {
    fn default() -> Self {
        Self {
            affected_chunks: HashSet::new(),
            last_dab_position: None,
            first_dab_position: None,
        }
    }
}

/// The sculpting pipeline orchestrates brush → deform → tessellate → sync.
///
/// This struct coordinates all sculpting operations and ensures proper ordering
/// of operations for deterministic results.
#[derive(Debug)]
pub struct SculptingPipeline {
    /// Brush engine for dab generation.
    pub brush_engine: SculptBrushEngine,
    /// Pipeline configuration.
    pub config: PipelineConfig,
    /// Current screen-space configuration (updated per frame, used in ScreenSpace mode).
    screen_config: ScreenSpaceConfig,
    /// Global vertex budget (used in BudgetCurvature mode).
    pub budget: VertexBudget,
    /// State for the active stroke.
    active_stroke_state: Option<ActiveStrokeState>,
    /// Per-chunk octrees for spatial queries (lazily built).
    chunk_octrees: HashMap<ChunkId, VertexOctree>,
}

impl SculptingPipeline {
    /// Create a new sculpting pipeline with the given brush preset.
    pub fn new(brush_preset: BrushPreset) -> Self {
        Self {
            brush_engine: SculptBrushEngine::new(brush_preset),
            config: PipelineConfig::default(),
            screen_config: ScreenSpaceConfig::default(),
            budget: VertexBudget::default(),
            active_stroke_state: None,
            chunk_octrees: HashMap::new(),
        }
    }

    /// Create a pipeline with custom configuration.
    pub fn with_config(brush_preset: BrushPreset, config: PipelineConfig) -> Self {
        Self {
            brush_engine: SculptBrushEngine::new(brush_preset),
            config,
            screen_config: ScreenSpaceConfig::default(),
            budget: VertexBudget::default(),
            active_stroke_state: None,
            chunk_octrees: HashMap::new(),
        }
    }

    /// Update the screen-space configuration (call when camera changes).
    /// Used in `ScreenSpace` tessellation mode.
    pub fn update_screen_config(&mut self, screen_config: ScreenSpaceConfig) {
        self.screen_config = screen_config;
    }

    /// Update the vertex budget from pixel coverage data.
    /// Used in `BudgetCurvature` tessellation mode.
    pub fn update_budget_from_coverage(&mut self, pixel_coverage: u32) {
        let vpp = self.config.tessellation_config.vertices_per_pixel;
        self.budget.update_max(pixel_coverage, vpp);
    }

    /// Set the brush preset.
    pub fn set_brush_preset(&mut self, preset: BrushPreset) {
        self.brush_engine.preset = preset;
    }

    /// Get the current brush preset.
    pub fn brush_preset(&self) -> &BrushPreset {
        &self.brush_engine.preset
    }

    /// Begin a new stroke.
    ///
    /// Returns the stroke ID.
    pub fn begin_stroke(&mut self, mesh_id: u32, input: BrushInput) -> u64 {
        self.active_stroke_state = Some(ActiveStrokeState {
            affected_chunks: HashSet::new(),
            last_dab_position: Some(input.position),
            first_dab_position: Some(input.position),
        });

        self.brush_engine.begin_stroke(mesh_id, input)
    }

    /// Process a brush input and apply deformation to the chunked mesh.
    ///
    /// This is the main entry point for sculpting during a stroke.
    pub fn process_input(
        &mut self,
        input: BrushInput,
        chunked_mesh: &mut ChunkedMesh,
    ) -> DabProcessResult {
        debug!("process_input: START pos={:?}", input.position);
        let mut result = DabProcessResult::default();

        // Generate dabs from brush input
        debug!("process_input: generating dabs");
        let dabs = self.brush_engine.update_stroke(input);
        debug!("process_input: generated {} dabs", dabs.len());

        // Get stroke state info for direction calculation (copy what we need)
        let (last_pos, first_pos) = match &self.active_stroke_state {
            Some(state) => (state.last_dab_position, state.first_dab_position),
            None => return result,
        };

        // Process each dab
        for dab in dabs {
            let dab_result = self.apply_dab_internal(
                &dab,
                last_pos,
                first_pos,
                chunked_mesh,
            );

            result.vertices_modified += dab_result.vertices_modified;
            result.chunks_affected.extend(dab_result.chunks_affected.clone());

            // Accumulate tessellation stats
            if let Some(tess) = dab_result.tessellation {
                let existing = result.tessellation.get_or_insert(TessellationStats::default());
                existing.edges_split += tess.edges_split;
                existing.edges_collapsed += tess.edges_collapsed;
            }

            // Track affected chunks
            if let Some(state) = &mut self.active_stroke_state {
                for chunk_id in &dab_result.chunks_affected {
                    state.affected_chunks.insert(*chunk_id);
                }
            }
        }

        // Update last dab position
        if let Some(state) = &mut self.active_stroke_state {
            state.last_dab_position = Some(input.position);
        }

        result
    }

    /// End the current stroke and perform post-stroke processing.
    ///
    /// This triggers chunk rebalancing if configured.
    pub fn end_stroke(&mut self, chunked_mesh: &mut ChunkedMesh) -> StrokeEndResult {
        let mut result = StrokeEndResult::default();

        // Get stroke packets from brush engine
        if let Some(packets) = self.brush_engine.end_stroke() {
            result.packets = packets;
        }

        // Perform chunk rebalancing if enabled
        if self.config.rebalance_after_stroke {
            let (split, merged) = self.rebalance_chunks(chunked_mesh);
            result.chunks_split = split;
            result.chunks_merged = merged;
        }

        // Clear stroke state
        self.active_stroke_state = None;

        result
    }

    /// Cancel the current stroke without applying final operations.
    pub fn cancel_stroke(&mut self) {
        self.brush_engine.end_stroke();
        self.active_stroke_state = None;
    }

    /// Apply a single dab to the chunked mesh (internal implementation).
    ///
    /// Uses a tessellation-first approach to ensure geometry exists before deformation:
    /// 1. **Pass 1**: Tessellate all affected chunks (creates vertices for brush to deform)
    /// 2. **Boundary sync**: Synchronize positions across chunks
    /// 3. **Pass 2**: Deform vertices + update normals in all affected chunks
    /// 4. **Final sync**: Synchronize final positions
    ///
    /// This ordering ensures that when long edges pass through the brush area,
    /// they are split BEFORE deformation so the new vertices receive the brush effect.
    /// Previously, tessellation ran after deformation, causing new vertices to be
    /// placed on the un-deformed surface (creating dents/discontinuities).
    fn apply_dab_internal(
        &mut self,
        dab: &DabResult,
        last_dab_position: Option<Vec3>,
        first_dab_position: Option<Vec3>,
        chunked_mesh: &mut ChunkedMesh,
    ) -> DabProcessResult {
        debug!(
            "SCULPT DAB: faces={}, vertices={}, chunks={}",
            chunked_mesh.total_face_count(),
            chunked_mesh.total_vertex_count(),
            chunked_mesh.chunk_count()
        );
        trace!("apply_dab_internal: START brush_center={:?}", dab.position);
        let mut result = DabProcessResult::default();

        let brush_center = dab.position;
        let brush_radius = dab.radius;
        let influence_radius = brush_radius * 1.5;

        // Calculate stroke direction for grab/crease brushes
        let stroke_direction = last_dab_position
            .map(|last| (dab.position - last).normalize_or_zero())
            .unwrap_or(Vec3::Z);

        // Calculate stroke delta for grab brush
        let stroke_delta = first_dab_position
            .map(|first| dab.position - first)
            .unwrap_or(Vec3::ZERO);

        // Find affected chunks
        debug!("apply_dab_internal: finding chunks in sphere");
        let affected_chunk_ids = chunked_mesh.chunks_intersecting_sphere(brush_center, influence_radius);
        debug!("apply_dab_internal: found {} affected chunks", affected_chunk_ids.len());
        trace!("apply_dab_internal: found {} affected chunks", affected_chunk_ids.len());

        // Extract the next_original_vertex_id counter to avoid borrow conflicts.
        // We'll write it back after the loop. This counter is used to assign globally
        // unique IDs to tessellation-created vertices, preventing ID collisions during chunk merges.
        let mut next_original_vertex_id = chunked_mesh.next_original_vertex_id;

        // Pre-compute total vertex count for budget mode (avoids borrow conflict inside chunk loop)
        if self.config.tessellation_config.mode == TessellationMode::BudgetCurvature {
            self.budget.update_current(chunked_mesh.total_vertex_count());
        }

        // ===== PASS 1: TESSELLATE all affected chunks FIRST =====
        // Tessellation runs before deformation so that when long edges pass through
        // the brush area (e.g., cube face diagonals), they are split and the new
        // vertices exist before the brush tries to deform them. Without this,
        // new vertices from pass-through edge splits would be placed on the
        // un-deformed surface, creating dents and discontinuities.
        if self.config.tessellation_enabled {
            for &chunk_id in &affected_chunk_ids {
                let chunk = match chunked_mesh.get_chunk_mut(chunk_id) {
                    Some(c) => c,
                    None => continue,
                };

                // Validate mesh BEFORE tessellation in debug builds.
                // Gated behind env var to allow skipping during interactive testing:
                //   PENTIMENTO_SKIP_MESH_VALIDATION=1 cargo run
                #[cfg(debug_assertions)]
                if std::env::var("PENTIMENTO_SKIP_MESH_VALIDATION").is_err() {
                    if let Err(e) = chunk.mesh.validate_connectivity() {
                        error!("MESH CORRUPT BEFORE tessellation: {}", e);
                        panic!("Mesh corrupted before tessellation - bug is in chunk split: {}", e);
                    }
                }

                let tess_start = std::time::Instant::now();
                debug!(
                    "apply_dab_internal: starting tessellation (faces={}, verts={})",
                    chunk.mesh.face_count(),
                    chunk.mesh.vertex_count()
                );
                let tess_stats = match self.config.tessellation_config.mode {
                    TessellationMode::BudgetCurvature => {
                        tessellate_at_brush_budget(
                            chunk,
                            brush_center,
                            brush_radius,
                            &self.config.tessellation_config,
                            &mut self.budget,
                            &mut next_original_vertex_id,
                        )
                    }
                    TessellationMode::ScreenSpace => tessellate_at_brush(
                        chunk,
                        brush_center,
                        brush_radius,
                        &self.config.tessellation_config,
                        &self.screen_config,
                        &mut next_original_vertex_id,
                    ),
                };
                debug!(
                    "apply_dab_internal: tessellation done in {:?} - split={}, collapsed={}, faces={}",
                    tess_start.elapsed(),
                    tess_stats.edges_split,
                    tess_stats.edges_collapsed,
                    chunk.mesh.face_count(),
                );

                // Validate mesh after tessellation in debug builds
                #[cfg(debug_assertions)]
                if std::env::var("PENTIMENTO_SKIP_MESH_VALIDATION").is_err() {
                    if let Err(e) = chunk.mesh.validate_connectivity() {
                        error!("MESH CORRUPTION after tessellation: {}", e);
                        panic!("Mesh corrupted by tessellation: {}", e);
                    }
                }

                debug!(
                    "SCULPT TESS: split={}, collapsed={}, chunk_faces={}",
                    tess_stats.edges_split,
                    tess_stats.edges_collapsed,
                    chunk.mesh.face_count()
                );

                if tess_stats.edges_split > 0 || tess_stats.edges_collapsed > 0 {
                    chunk.mark_topology_changed();

                    // CRITICAL: Recalculate normals after tessellation changed topology.
                    // New faces inherit the original face's normal which is now wrong,
                    // and new vertices only have interpolated normals that don't match
                    // the actual post-split geometry.
                    trace!("apply_dab_internal: recalculating normals after tessellation");
                    let tessellated_vertices: HashSet<VertexId> = chunk
                        .mesh
                        .vertices()
                        .iter()
                        .filter(|v| {
                            v.position.distance_squared(brush_center)
                                <= (brush_radius * 1.5).powi(2)
                        })
                        .map(|v| v.id)
                        .collect();
                    let tess_dirty = DirtyVertices {
                        modified: tessellated_vertices,
                    };
                    update_normals_after_deformation(chunk, &tess_dirty);
                }

                let existing = result.tessellation.get_or_insert(TessellationStats::default());
                existing.edges_split += tess_stats.edges_split;
                existing.edges_collapsed += tess_stats.edges_collapsed;

                // Track this chunk as affected (tessellation happened)
                if tess_stats.edges_split > 0 || tess_stats.edges_collapsed > 0 {
                    result.chunks_affected.push(chunk_id);
                    // Invalidate octree since topology changed
                    self.chunk_octrees.remove(&chunk_id);
                }
            }
        }

        // ===== BOUNDARY SYNC between tessellation and deformation =====
        // Synchronize boundary vertex positions after tessellation so all chunks
        // see consistent positions before deformation.
        if !result.chunks_affected.is_empty() {
            trace!("apply_dab_internal: post-tessellation boundary sync");
            self.sync_boundary_vertices(chunked_mesh, &result.chunks_affected);
        }

        // ===== PASS 2: DEFORM all affected chunks =====
        // Now deformation operates on the refined mesh, including any new vertices
        // created by splitting pass-through edges.
        for &chunk_id in &affected_chunk_ids {
            trace!("apply_dab_internal: deforming chunk {:?}", chunk_id);
            let chunk = match chunked_mesh.get_chunk_mut(chunk_id) {
                Some(c) => c,
                None => continue,
            };

            // Rebuild octree since tessellation may have added vertices
            self.chunk_octrees.remove(&chunk_id);
            self.ensure_octree(chunk_id, chunk);

            // Query vertices in brush radius (now includes new vertices from tessellation)
            let octree = self.chunk_octrees.get(&chunk_id).unwrap();
            let affected_vertices: Vec<VertexId> = octree
                .query_sphere(brush_center, brush_radius)
                .into_iter()
                .collect();

            if affected_vertices.is_empty() {
                continue;
            }

            // Create dab info for deformation
            let dab_info = DabInfo {
                position: brush_center,
                radius: brush_radius,
                strength: dab.strength,
                normal: dab.normal,
            };

            // Apply deformation
            let falloff = self.brush_engine.preset.falloff;
            let deformation_type = self.brush_engine.preset.deformation_type;

            let _displacements = apply_deformation(
                &mut chunk.mesh,
                &affected_vertices,
                &dab_info,
                deformation_type,
                falloff,
                Some(stroke_direction),
                Some(stroke_delta),
            );

            // Auto-smooth to dampen high-frequency dab ripples
            let autosmooth = self.brush_engine.preset.autosmooth;
            if autosmooth > 0.0 && !affected_vertices.is_empty() {
                apply_autosmooth(
                    &mut chunk.mesh,
                    &affected_vertices,
                    &dab_info,
                    falloff,
                    autosmooth,
                );
            }

            result.vertices_modified += affected_vertices.len();
            if !result.chunks_affected.contains(&chunk_id) {
                result.chunks_affected.push(chunk_id);
            }

            // Mark chunk dirty and track affected vertices
            chunk.mark_dirty();

            // Update normals for affected region
            let dirty = DirtyVertices {
                modified: affected_vertices.into_iter().collect(),
            };
            update_normals_after_deformation(chunk, &dirty);

            // Invalidate octree (positions changed)
            self.chunk_octrees.remove(&chunk_id);
        }

        // Write back the updated vertex ID counter to the chunked mesh
        chunked_mesh.next_original_vertex_id = next_original_vertex_id;

        // Final boundary sync: propagate deformation positions across chunks
        trace!("apply_dab_internal: final boundary sync");
        self.sync_boundary_vertices(chunked_mesh, &result.chunks_affected);

        trace!("apply_dab_internal: END");
        result
    }

    /// Ensure an octree exists for the given chunk.
    fn ensure_octree(&mut self, chunk_id: ChunkId, chunk: &MeshChunk) {
        if self.chunk_octrees.contains_key(&chunk_id) {
            return;
        }

        // Convert chunking::Aabb to spatial::Aabb
        let spatial_bounds = SpatialAabb::new(chunk.bounds.min, chunk.bounds.max);
        let mut octree = VertexOctree::new(spatial_bounds);
        for vertex in chunk.mesh.vertices() {
            octree.insert(vertex.id, vertex.position);
        }
        self.chunk_octrees.insert(chunk_id, octree);
    }

    /// Sync boundary vertices between affected chunks.
    fn sync_boundary_vertices(&mut self, chunked_mesh: &mut ChunkedMesh, affected_chunks: &[ChunkId]) {
        // For each affected chunk, sync all boundary vertices
        for &chunk_id in affected_chunks {
            let boundary_updates: Vec<(VertexId, Vec3)> = {
                let chunk = match chunked_mesh.get_chunk(chunk_id) {
                    Some(c) => c,
                    None => continue,
                };

                chunk
                    .boundary_vertices
                    .keys()
                    .filter_map(|&vid| {
                        chunk.mesh.vertex(vid).map(|v| (vid, v.position))
                    })
                    .collect()
            };

            for (local_vid, position) in boundary_updates {
                chunked_mesh.sync_boundary_vertex(chunk_id, local_vid, position);
            }
        }

        // Recalculate boundary normals
        chunked_mesh.recalculate_boundary_normals();

        // Verify boundary consistency in debug builds
        #[cfg(debug_assertions)]
        if std::env::var("PENTIMENTO_SKIP_MESH_VALIDATION").is_err() {
            Self::verify_boundary_consistency(chunked_mesh);
        }
    }

    /// Verify that boundary vertices have consistent original IDs and positions
    /// across all chunks that share them. Logs errors for any inconsistencies.
    #[cfg(debug_assertions)]
    fn verify_boundary_consistency(chunked_mesh: &ChunkedMesh) {
        for (&chunk_id, chunk) in &chunked_mesh.chunks {
            for (&local_id, refs) in &chunk.boundary_vertices {
                let Some(&original_id) = chunk.local_to_original.get(&local_id) else {
                    error!(
                        "BOUNDARY CONSISTENCY: vertex {:?} in chunk {:?} has no \
                         original ID mapping! This will cause mesh tearing.",
                        local_id, chunk_id
                    );
                    continue;
                };
                let our_pos = chunk.mesh.vertex(local_id).map(|v| v.position);
                for bref in refs {
                    // Verify the neighbor chunk has the same original ID
                    if let Some(neighbor) = chunked_mesh.chunks.get(&bref.chunk_id) {
                        if let Some(&neighbor_original) =
                            neighbor.local_to_original.get(&bref.vertex_id)
                        {
                            if neighbor_original != original_id {
                                error!(
                                    "BOUNDARY CONSISTENCY: original ID mismatch! \
                                     chunk {:?} vertex {:?} has original={:?}, but \
                                     neighbor chunk {:?} vertex {:?} has original={:?}",
                                    chunk_id, local_id, original_id,
                                    bref.chunk_id, bref.vertex_id, neighbor_original
                                );
                            }
                        }
                        // Verify positions are synchronized
                        let their_pos =
                            neighbor.mesh.vertex(bref.vertex_id).map(|v| v.position);
                        if let (Some(ours), Some(theirs)) = (our_pos, their_pos) {
                            let dist = ours.distance(theirs);
                            if dist > 1e-5 {
                                debug!(
                                    "BOUNDARY CONSISTENCY: position desync for \
                                     original={:?}: chunk {:?}={:?} vs chunk {:?}={:?} \
                                     (delta={})",
                                    original_id, chunk_id, ours,
                                    bref.chunk_id, theirs, dist
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    /// Rebalance chunks after a stroke ends.
    ///
    /// Returns (chunks_split, chunks_merged).
    fn rebalance_chunks(&mut self, chunked_mesh: &mut ChunkedMesh) -> (usize, usize) {
        let config = &self.config.chunk_config;

        let mut chunks_split = 0;
        let mut chunks_merged = 0;

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
                if crate::chunking::partition::split_chunk(chunked_mesh, chunk_id).is_some() {
                    chunks_split += 1;
                    // Invalidate octree for split chunks
                    self.chunk_octrees.remove(&chunk_id);
                }
            }
        }

        // Phase 2: Merge undersized adjacent chunks
        loop {
            let merge_pair = self.find_mergeable_pair(chunked_mesh, config);
            if let Some((a, b)) = merge_pair {
                if crate::chunking::merge::merge_two_chunks(chunked_mesh, a, b).is_some() {
                    chunks_merged += 1;
                    // Invalidate octrees for merged chunks
                    self.chunk_octrees.remove(&a);
                    self.chunk_octrees.remove(&b);
                }
            } else {
                break;
            }
        }

        // Rebuild spatial grid if chunks changed
        if chunks_split > 0 || chunks_merged > 0 {
            chunked_mesh.rebuild_spatial_grid();
        }

        (chunks_split, chunks_merged)
    }

    /// Find a pair of adjacent chunks that can be merged.
    fn find_mergeable_pair(
        &self,
        chunked_mesh: &ChunkedMesh,
        config: &ChunkConfig,
    ) -> Option<(ChunkId, ChunkId)> {
        for (&id, chunk) in &chunked_mesh.chunks {
            if chunk.face_count() >= config.min_faces {
                continue;
            }

            // Find adjacent chunks (those sharing boundary vertices)
            let neighbor_ids: HashSet<ChunkId> = chunk
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

    /// Clear all cached octrees.
    ///
    /// Call this when the mesh changes outside of the pipeline.
    pub fn invalidate_caches(&mut self) {
        self.chunk_octrees.clear();
    }

    /// Check if a stroke is currently active.
    pub fn is_stroke_active(&self) -> bool {
        self.active_stroke_state.is_some()
    }
}

/// Re-export the standalone rebalance function for direct use.
pub use crate::chunking::merge::rebalance_chunks;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_config_default() {
        let config = PipelineConfig::default();
        // Tessellation now enabled by default with comprehensive safety checks
        assert!(config.tessellation_enabled);
        assert!(config.rebalance_after_stroke);
        assert_eq!(config.tessellation_config.target_pixels, 6.0);
        assert_eq!(config.chunk_config.target_faces, 10000);
        // Edge collapse is enabled with safety checks
        assert!(config.tessellation_config.collapse_enabled);
    }

    #[test]
    fn test_pipeline_creation() {
        let preset = BrushPreset::default();
        let pipeline = SculptingPipeline::new(preset);
        assert!(!pipeline.is_stroke_active());
    }

    #[test]
    fn test_dab_process_result_default() {
        let result = DabProcessResult::default();
        assert_eq!(result.vertices_modified, 0);
        assert!(result.chunks_affected.is_empty());
        assert!(result.tessellation.is_none());
    }

    #[test]
    fn test_stroke_end_result_default() {
        let result = StrokeEndResult::default();
        assert!(result.packets.is_empty());
        assert_eq!(result.chunks_split, 0);
        assert_eq!(result.chunks_merged, 0);
    }

    #[test]
    fn test_pipeline_stroke_lifecycle() {
        let preset = BrushPreset::default();
        let mut pipeline = SculptingPipeline::new(preset);

        assert!(!pipeline.is_stroke_active());

        // Begin stroke
        let input = BrushInput {
            position: Vec3::new(0.0, 0.0, 0.0),
            normal: Vec3::Y,
            pressure: 1.0,
            timestamp_ms: 0,
        };
        let stroke_id = pipeline.begin_stroke(1, input);
        assert!(stroke_id == 0);
        assert!(pipeline.is_stroke_active());

        // Cancel stroke
        pipeline.cancel_stroke();
        assert!(!pipeline.is_stroke_active());
    }

    #[test]
    fn test_pipeline_brush_preset_change() {
        let preset = BrushPreset::push();
        let mut pipeline = SculptingPipeline::new(preset);

        assert_eq!(pipeline.brush_preset().name, "Push");

        // Change to smooth brush
        pipeline.set_brush_preset(BrushPreset::smooth());
        assert_eq!(pipeline.brush_preset().name, "Smooth");
    }

    #[test]
    fn test_pipeline_with_custom_config() {
        let preset = BrushPreset::default();
        let config = PipelineConfig {
            tessellation_enabled: false,
            rebalance_after_stroke: false,
            ..Default::default()
        };

        let pipeline = SculptingPipeline::with_config(preset, config);
        assert!(!pipeline.config.tessellation_enabled);
        assert!(!pipeline.config.rebalance_after_stroke);
    }

    #[test]
    fn test_pipeline_invalidate_caches() {
        let preset = BrushPreset::default();
        let mut pipeline = SculptingPipeline::new(preset);

        // Caches should be empty initially
        assert!(pipeline.chunk_octrees.is_empty());

        // Invalidate shouldn't panic
        pipeline.invalidate_caches();
        assert!(pipeline.chunk_octrees.is_empty());
    }

    #[test]
    fn test_rebalance_chunks_empty_mesh() {
        let mut chunked_mesh = ChunkedMesh::new();
        // Rebalancing an empty mesh should not panic
        rebalance_chunks(&mut chunked_mesh);
        assert_eq!(chunked_mesh.chunk_count(), 0);
    }
}
