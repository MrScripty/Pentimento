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
use crate::deformation::{DabInfo, SmoothingPass, apply_chunked_smoothing, apply_deformation};
use crate::gpu::{
    DirtyVertices, recalculate_face_normals_for_dirty, recalculate_normals_for_dirty,
    update_normals_after_deformation,
};
use crate::history::{HistoryError, HistorySnapshot, HistoryStatus, RecordOutcome, SculptHistory};
use crate::safety::{GeometryWitness, SafetyError, Surface};
use crate::spatial::{Aabb as SpatialAabb, VertexOctree};
use crate::tessellation::{
    ScreenSpaceConfig, TessellationStats, tessellate_at_brush_budget_checked,
    tessellate_at_brush_checked,
};
use crate::types::{
    ChunkConfig, DeformationType, SculptStrokePacket, TessellationConfig, TessellationMode,
};
use glam::Vec3;
use painting::half_edge::VertexId;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tracing::{debug, error, trace};

/// Result of processing a single dab through the pipeline.
#[derive(Debug, Default)]
pub struct DabProcessResult {
    /// A rejected dab rolls the complete active stroke back to its initial mesh.
    pub rejected: Option<SafetyError>,
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
    /// Rejected strokes never produce replay/sync packets.
    pub rejected: Option<SafetyError>,
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

/// Opaque, complete geometry endpoint. Live camera/brush policy is not restored.
#[derive(Debug, Clone)]
pub struct SculptSnapshot {
    mesh_id: u32,
    mesh: ChunkedMesh,
}

impl HistorySnapshot for SculptSnapshot {
    fn same_state(&self, other: &Self) -> bool {
        self.mesh_id == other.mesh_id && self.mesh.same_authoritative_state(&other.mesh)
    }
    fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>().saturating_add(self.mesh.retained_bytes())
    }
}

impl SculptSnapshot {
    fn capture(mesh_id: u32, mesh: &ChunkedMesh) -> Self {
        Self {
            mesh_id,
            mesh: mesh.clone(),
        }
    }
    fn validate(&self) -> Result<(), SafetyError> {
        self.mesh.validate_snapshot_identity()?;
        Surface::read(&self.mesh)?.validate()
    }
    /// Validate the complete detached target afresh with the current guard, then
    /// rebuild derived indexes before any live geometry/history cursor changes.
    fn prepare(&self) -> Result<ChunkedMesh, SafetyError> {
        self.validate()?;
        let mut mesh = self.mesh.clone();
        for chunk in mesh.chunks.values_mut() {
            chunk.recalculate_bounds();
            chunk.mark_topology_changed();
        }
        mesh.rebuild_spatial_grid();
        Ok(mesh)
    }
}

/// State tracked during an active stroke.
#[derive(Debug, Clone)]
struct ActiveStrokeState {
    mesh_id: u32,
    checkpoint: Option<ChunkedMesh>,
    admitted_surface: Option<Surface>,
    admitted_witness: Option<Arc<GeometryWitness>>,
    budget_checkpoint: Option<VertexBudget>,
    rejected: Option<SafetyError>,
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
            mesh_id: 0,
            checkpoint: None,
            admitted_surface: None,
            admitted_witness: None,
            budget_checkpoint: None,
            rejected: None,
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
    /// Last admitted geometry across strokes, keyed to the explicit mesh owner.
    /// Incoming mesh content is compared to it; this is never a blind validity flag.
    last_admitted_surface: Option<(u32, Surface, Arc<GeometryWitness>)>,
    history: SculptHistory<SculptSnapshot>,
    history_owner: Option<u32>,
    history_notice: Option<String>,
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
            last_admitted_surface: None,
            history: SculptHistory::default(),
            history_owner: None,
            history_notice: None,
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
            last_admitted_surface: None,
            history: SculptHistory::default(),
            history_owner: None,
            history_notice: None,
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
        // A second begin must never discard the owning rollback checkpoint.
        if self.active_stroke_state.is_some() {
            return 0;
        }
        if self.history_owner != Some(mesh_id) {
            self.history.clear();
            self.history_owner = Some(mesh_id);
        }
        self.history_notice = None;
        self.active_stroke_state = Some(ActiveStrokeState {
            mesh_id,
            checkpoint: None,
            admitted_surface: None,
            admitted_witness: None,
            budget_checkpoint: None,
            rejected: None,
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

        let Some(state) = &self.active_stroke_state else {
            return result;
        };
        if let Some(error) = &state.rejected {
            result.rejected = Some(error.clone());
            return result;
        }
        if state.checkpoint.is_none() {
            let mesh_id = state.mesh_id;
            let baseline = SculptSnapshot::capture(mesh_id, chunked_mesh);
            if self
                .history
                .check_current::<SafetyError>(&baseline)
                .is_err()
            {
                return self.reject_stroke(
                    chunked_mesh,
                    SafetyError::Topology(
                        "history baseline changed outside the sculpt transaction".into(),
                    ),
                );
            }
            let cached = self
                .last_admitted_surface
                .as_ref()
                .filter(|(owner, _, _)| *owner == mesh_id);
            let (surface, witness) = if let Some((_, previous, witness)) =
                cached.filter(|(_, _, witness)| witness.matches(chunked_mesh))
            {
                (previous.clone(), witness.clone())
            } else {
                let surface = match Surface::read(chunked_mesh).and_then(|surface| {
                    match cached {
                        Some((_, previous, _)) => surface.validate_change(previous)?,
                        None => surface.validate()?,
                    }
                    Ok(surface)
                }) {
                    Ok(surface) => surface,
                    Err(error) => return self.reject_stroke(chunked_mesh, error),
                };
                (surface, Arc::new(GeometryWitness::capture(chunked_mesh)))
            };
            let state = self.active_stroke_state.as_mut().unwrap();
            state.checkpoint = Some(chunked_mesh.clone());
            state.admitted_surface = Some(surface);
            state.admitted_witness = Some(witness);
            state.budget_checkpoint = Some(self.budget.clone());
        } else if let Err(error) = self.admit_current_mesh(chunked_mesh) {
            return self.reject_stroke(chunked_mesh, error);
        }

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
        let mut previous_dab = last_pos;
        for dab in dabs {
            let dab_result = self.apply_dab_internal(&dab, previous_dab, first_pos, chunked_mesh);
            previous_dab = Some(dab.position);

            if dab_result.rejected.is_some() {
                return dab_result;
            }
            result.vertices_modified += dab_result.vertices_modified;
            result
                .chunks_affected
                .extend(dab_result.chunks_affected.clone());

            // Accumulate tessellation stats
            if let Some(tess) = dab_result.tessellation {
                let existing = result
                    .tessellation
                    .get_or_insert(TessellationStats::default());
                existing.edges_split += tess.edges_split;
                existing.edges_collapsed += tess.edges_collapsed;
                existing.edges_flipped += tess.edges_flipped;
            }

            // Track affected chunks
            if let Some(state) = &mut self.active_stroke_state {
                for chunk_id in &dab_result.chunks_affected {
                    state.affected_chunks.insert(*chunk_id);
                }
            }
        }

        // Track the last emitted dab, not a sub-spacing input that emitted none.
        if let Some(state) = &mut self.active_stroke_state {
            state.last_dab_position = previous_dab;
        }

        result
    }

    /// End the current stroke and perform post-stroke processing.
    ///
    /// This triggers chunk rebalancing if configured.
    pub fn end_stroke(&mut self, chunked_mesh: &mut ChunkedMesh) -> StrokeEndResult {
        let mut result = StrokeEndResult::default();

        if self
            .active_stroke_state
            .as_ref()
            .is_some_and(|state| state.checkpoint.is_some() && state.rejected.is_none())
        {
            if let Err(error) = self.admit_current_mesh(chunked_mesh) {
                self.reject_stroke(chunked_mesh, error);
            }
        }

        if self
            .active_stroke_state
            .as_ref()
            .is_some_and(|state| state.checkpoint.is_some() && state.rejected.is_none())
            && self.config.rebalance_after_stroke
        {
            match self.rebalance_chunks(chunked_mesh) {
                Ok((split, merged)) => {
                    result.chunks_split = split;
                    result.chunks_merged = merged;
                }
                Err(error) => {
                    self.reject_stroke(chunked_mesh, error);
                }
            }
        }
        if let Some(state) = &self.active_stroke_state {
            if state.rejected.is_none() {
                if let Some(before) = &state.checkpoint {
                    let before = SculptSnapshot::capture(state.mesh_id, before);
                    let after = SculptSnapshot::capture(state.mesh_id, chunked_mesh);
                    match self
                        .history
                        .record_accepted(before, after, SculptSnapshot::validate)
                    {
                        Ok(RecordOutcome::NoChange) => {
                            self.brush_engine.cancel_stroke();
                        }
                        Ok(RecordOutcome::AcceptedWithoutUndo { .. }) => {
                            self.history_notice = Some(
                                "This stroke exceeds the local history limit and cannot be undone."
                                    .into(),
                            );
                        }
                        Ok(RecordOutcome::Recorded { .. }) => {}
                        Err(error) => {
                            self.reject_stroke(
                                chunked_mesh,
                                SafetyError::Topology(format!(
                                    "history acceptance failed: {error:?}"
                                )),
                            );
                        }
                    }
                } else {
                    // Input without an admitted dab must not emit a replay packet.
                    self.brush_engine.cancel_stroke();
                }
            }
        }
        result.rejected = self
            .active_stroke_state
            .as_ref()
            .and_then(|state| state.rejected.clone());
        if let Some(packets) = self.brush_engine.end_stroke() {
            if result.rejected.is_none() {
                result.packets = packets;
            }
        }
        if result.rejected.is_none() && (result.chunks_split > 0 || result.chunks_merged > 0) {
            if let Some(state) = &self.active_stroke_state {
                self.last_admitted_surface = Surface::read(chunked_mesh).ok().map(|surface| {
                    (
                        state.mesh_id,
                        surface,
                        Arc::new(GeometryWitness::capture(chunked_mesh)),
                    )
                });
            }
        }
        self.active_stroke_state = None;

        result
    }

    /// The public mesh can be edited by other callers between input events.
    /// Reuse a prior admission only after exact equality, including hidden
    /// half-edge maps. This also covers zero-dab inputs and stroke completion.
    fn admit_current_mesh(&mut self, mesh: &ChunkedMesh) -> Result<(), SafetyError> {
        let state = self
            .active_stroke_state
            .as_ref()
            .ok_or_else(|| SafetyError::Topology("missing stroke state".into()))?;
        if state
            .admitted_witness
            .as_ref()
            .is_some_and(|witness| witness.matches(mesh))
        {
            return Ok(());
        }
        let previous = state
            .admitted_surface
            .as_ref()
            .ok_or_else(|| SafetyError::Topology("missing admitted stroke state".into()))?;
        let surface = Surface::read(mesh)?;
        surface.validate_change(previous)?;
        surface.validate_orientation(previous)?;
        let witness = Arc::new(GeometryWitness::capture(mesh));
        let state = self.active_stroke_state.as_mut().unwrap();
        self.last_admitted_surface = Some((state.mesh_id, surface.clone(), witness.clone()));
        state.admitted_surface = Some(surface);
        state.admitted_witness = Some(witness);
        Ok(())
    }

    fn reject_stroke(&mut self, mesh: &mut ChunkedMesh, error: SafetyError) -> DabProcessResult {
        tracing::warn!("Sculpt stroke rejected; preserving pre-stroke geometry: {error}");
        if let Some(state) = &mut self.active_stroke_state {
            if let Some(checkpoint) = state.checkpoint.take() {
                *mesh = checkpoint;
                self.last_admitted_surface = Surface::read(mesh).ok().map(|surface| {
                    (
                        state.mesh_id,
                        surface,
                        Arc::new(GeometryWitness::capture(mesh)),
                    )
                });
                // Earlier dabs may already have been uploaded. A rollback is a
                // full topology upload, including vertices removed by this stroke.
                for chunk in mesh.chunks.values_mut() {
                    chunk.mark_topology_changed();
                }
            }
            if let Some(budget) = state.budget_checkpoint.take() {
                self.budget = budget;
            }
            state.rejected = Some(error.clone());
        }
        self.chunk_octrees.clear();
        DabProcessResult {
            rejected: Some(error),
            chunks_affected: mesh.chunks.keys().copied().collect(),
            ..Default::default()
        }
    }

    /// Restore the complete pre-stroke checkpoint, including topology and IDs.
    /// Cancellation emits no replay packet and never changes history stacks.
    pub fn cancel_stroke(&mut self, mesh: &mut ChunkedMesh) {
        self.brush_engine.cancel_stroke();
        if let Some(state) = self.active_stroke_state.take() {
            if let Some(checkpoint) = state.checkpoint {
                *mesh = checkpoint;
                for chunk in mesh.chunks.values_mut() {
                    chunk.mark_topology_changed();
                }
            }
            if let Some(budget) = state.budget_checkpoint {
                self.budget = budget;
            }
        }
        self.chunk_octrees.clear();
        self.last_admitted_surface = None;
    }

    pub fn history_status(&self) -> HistoryStatus {
        self.history.status()
    }
    pub fn history_notice(&self) -> Option<&str> {
        self.history_notice.as_deref()
    }

    /// Admit an explicitly supplied external edit as a fresh session baseline.
    ///
    /// Finish or cancel the active transaction first. This operation refuses
    /// active (including rejected) strokes and validates the complete mesh with
    /// the current safety policy before releasing any history or cache state.
    /// It never changes geometry or emits replay packets. A failed reset leaves
    /// history, notices, and admissions unchanged.
    pub fn reset_history(
        &mut self,
        mesh_id: u32,
        mesh: &mut ChunkedMesh,
        notice: Option<String>,
    ) -> Result<(), SafetyError> {
        if self.stroke_active() {
            return Err(SafetyError::Topology(
                "finish or cancel the active stroke before resetting history".into(),
            ));
        }
        mesh.validate_snapshot_identity()?;
        let surface = Surface::read(mesh)?;
        surface.validate()?;
        let witness = Arc::new(GeometryWitness::capture(mesh));
        // External edits may leave derived bounds and spatial lookup stale.
        // Prepare a detached replacement before publishing any state.
        let mut prepared = mesh.clone();
        for chunk in prepared.chunks.values_mut() {
            chunk.recalculate_bounds();
            chunk.mark_topology_changed();
        }
        prepared.rebuild_spatial_grid();

        *mesh = prepared;
        self.chunk_octrees.clear();
        self.history.clear();
        self.history_owner = Some(mesh_id);
        self.history_notice = notice;
        self.last_admitted_surface = Some((mesh_id, surface, witness));
        self.budget.update_current(mesh.total_vertex_count());
        Ok(())
    }
    pub fn stroke_active(&self) -> bool {
        self.active_stroke_state.is_some()
    }

    /// Active transactions are blocked: callers may explicitly cancel first.
    pub fn restore_history(
        &mut self,
        mesh: &mut ChunkedMesh,
        redo: bool,
    ) -> Result<bool, HistoryError<SafetyError>> {
        if self.stroke_active() {
            return Err(HistoryError::Validation(SafetyError::Topology(
                "finish or cancel the active stroke before history".into(),
            )));
        }
        let Some(owner) = self.history_owner else {
            return Ok(false);
        };
        let current = SculptSnapshot::capture(owner, mesh);
        let replace = |expected: &SculptSnapshot, target: &SculptSnapshot| {
            if !expected.mesh.same_authoritative_state(mesh) {
                return Err(SafetyError::Topology(
                    "live mesh changed before history replacement".into(),
                ));
            }
            let candidate = target.prepare()?;
            *mesh = candidate;
            Ok(())
        };
        let restored = if redo {
            self.history.redo_replace(&current, replace)?
        } else {
            self.history.undo_replace(&current, replace)?
        };
        if restored {
            self.chunk_octrees.clear();
            self.last_admitted_surface = None;
            // Preserve the current camera's budget policy while rebuilding count.
            self.budget.update_current(mesh.total_vertex_count());
            self.history_notice = None;
        }
        Ok(restored)
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
        _first_dab_position: Option<Vec3>,
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
        let stroke_delta = last_dab_position
            .map(|previous| dab.position - previous)
            .unwrap_or(Vec3::ZERO);

        // Find affected chunks
        debug!("apply_dab_internal: finding chunks in sphere");
        let mut affected_chunk_ids =
            chunked_mesh.chunks_intersecting_sphere(brush_center, influence_radius);
        affected_chunk_ids.sort_by_key(|id| id.0);
        debug!(
            "apply_dab_internal: found {} affected chunks",
            affected_chunk_ids.len()
        );
        trace!(
            "apply_dab_internal: found {} affected chunks",
            affected_chunk_ids.len()
        );

        // Extract the next_original_vertex_id counter to avoid borrow conflicts.
        // We'll write it back after the loop. This counter is used to assign globally
        // unique IDs to tessellation-created vertices, preventing ID collisions during chunk merges.
        let mut next_original_vertex_id = chunked_mesh.next_original_vertex_id;

        // Pre-compute total vertex count for budget mode (avoids borrow conflict inside chunk loop)
        if self.config.tessellation_config.mode == TessellationMode::BudgetCurvature {
            self.budget
                .update_current(chunked_mesh.total_vertex_count());
        }

        // ===== PASS 1: TESSELLATE all affected chunks FIRST =====
        // Tessellation runs before deformation so that when long edges pass through
        // the brush area (e.g., cube face diagonals), they are split and the new
        // vertices exist before the brush tries to deform them. Without this,
        // new vertices from pass-through edge splits would be placed on the
        // un-deformed surface, creating dents and discontinuities.
        // Incoming admission ran once for this process_input call. No external
        // caller can mutate the exclusively borrowed mesh between these dabs.
        let committed_before = self
            .active_stroke_state
            .as_ref()
            .and_then(|state| state.admitted_surface.clone())
            .expect("active stroke admitted before dabs");
        let mut candidate_surface = committed_before.clone();

        // Adaptive work is bounded per dab. Local mutation checks run after
        // each edit; one global collision check admits the whole transaction.
        // Dense meshes refine progressively instead of blocking on many edits.
        let mut edit_budget = (10_000 / chunked_mesh.total_face_count().max(1)).clamp(1, 4);
        if self.config.tessellation_enabled {
            for &chunk_id in &affected_chunk_ids {
                if edit_budget == 0 {
                    break;
                }
                let mut before = candidate_surface.chunk(chunk_id);
                let mut safety_error = None;
                let mut check = |chunk: &MeshChunk| {
                    if safety_error.is_some() {
                        return false;
                    }
                    match Surface::read_chunk(chunk).and_then(|surface| {
                        surface.validate_orientation(&before)?;
                        Ok(surface)
                    }) {
                        Ok(surface) => {
                            before = surface;
                            true
                        }
                        Err(error) => {
                            safety_error = Some(error);
                            false
                        }
                    }
                };
                let chunk = match chunked_mesh.get_chunk_mut(chunk_id) {
                    Some(c) => c,
                    None => continue,
                };

                let tess_start = std::time::Instant::now();
                debug!(
                    "apply_dab_internal: starting tessellation (faces={}, verts={})",
                    chunk.mesh.face_count(),
                    chunk.mesh.vertex_count()
                );
                let tess_stats = match self.config.tessellation_config.mode {
                    TessellationMode::BudgetCurvature => tessellate_at_brush_budget_checked(
                        chunk,
                        brush_center,
                        brush_radius,
                        &self.config.tessellation_config,
                        &mut self.budget,
                        &mut next_original_vertex_id,
                        &mut check,
                        &mut edit_budget,
                    ),
                    TessellationMode::ScreenSpace => tessellate_at_brush_checked(
                        chunk,
                        brush_center,
                        brush_radius,
                        &self.config.tessellation_config,
                        &self.screen_config,
                        &mut next_original_vertex_id,
                        &mut check,
                        &mut edit_budget,
                    ),
                };
                if let Some(error) = safety_error {
                    return self.reject_stroke(chunked_mesh, error);
                }
                candidate_surface.replace_chunk_geometry(chunk_id, before);
                let chunk = chunked_mesh.get_chunk_mut(chunk_id).unwrap();
                debug!(
                    "apply_dab_internal: tessellation done in {:?} - split={}, collapsed={}, faces={}",
                    tess_start.elapsed(),
                    tess_stats.edges_split,
                    tess_stats.edges_collapsed,
                    chunk.mesh.face_count(),
                );

                debug!(
                    "SCULPT TESS: split={}, collapsed={}, chunk_faces={}",
                    tess_stats.edges_split,
                    tess_stats.edges_collapsed,
                    chunk.mesh.face_count()
                );

                if tess_stats.edges_split > 0
                    || tess_stats.edges_collapsed > 0
                    || tess_stats.edges_flipped > 0
                {
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

                let existing = result
                    .tessellation
                    .get_or_insert(TessellationStats::default());
                existing.edges_split += tess_stats.edges_split;
                existing.edges_collapsed += tess_stats.edges_collapsed;
                existing.edges_flipped += tess_stats.edges_flipped;

                // Track this chunk as affected (tessellation happened)
                if tess_stats.edges_split > 0
                    || tess_stats.edges_collapsed > 0
                    || tess_stats.edges_flipped > 0
                {
                    result.chunks_affected.push(chunk_id);
                    // Invalidate octree since topology changed
                    self.chunk_octrees.remove(&chunk_id);
                }
            }
        }

        // Compaction changes local IDs. Refresh every neighbor's reverse
        // reference before synchronization can write to an unrelated vertex.
        if !result.chunks_affected.is_empty() {
            chunked_mesh.rebuild_boundary_relationships();
        }

        // ===== BOUNDARY SYNC between tessellation and deformation =====
        // Synchronize boundary vertex positions after tessellation so all chunks
        // see consistent positions before deformation.
        if !result.chunks_affected.is_empty() {
            trace!("apply_dab_internal: post-tessellation boundary sync");
            self.sync_boundary_vertices(chunked_mesh, &result.chunks_affected);
        }

        let before_deformation = match candidate_surface.deformed(chunked_mesh) {
            Ok(surface) => surface,
            Err(error) => return self.reject_stroke(chunked_mesh, error),
        };

        // ===== PASS 2: DEFORM all affected chunks =====
        // Now deformation operates on the refined mesh, including any new vertices
        // created by splitting pass-through edges.
        let dab_info = DabInfo {
            position: brush_center,
            radius: brush_radius,
            strength: dab.strength,
            normal: dab.normal,
            hardness: self.brush_engine.preset.hardness,
        };
        let falloff = self.brush_engine.preset.falloff;
        let deformation_type = self.brush_engine.preset.deformation_type;
        let autosmooth = self.brush_engine.preset.autosmooth;
        let mut smoothing_vertices = HashMap::new();
        let mut modified_vertices = HashMap::<ChunkId, HashSet<VertexId>>::new();
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

            // Smooth requires a complete one-ring spanning all chunk copies.
            // Other primary tools retain their existing local deformation.
            let mut modified = HashSet::new();
            if deformation_type != DeformationType::Smooth {
                let displacements = apply_deformation(
                    &mut chunk.mesh,
                    &affected_vertices,
                    &dab_info,
                    deformation_type,
                    falloff,
                    Some(stroke_direction),
                    Some(stroke_delta),
                );
                modified.extend(displacements.into_iter().filter_map(|(vertex, before)| {
                    (chunk.mesh.vertex(vertex).unwrap().position != before).then_some(vertex)
                }));
            }

            if deformation_type == DeformationType::Smooth || autosmooth > 0.0 {
                smoothing_vertices.insert(chunk_id, affected_vertices.clone());
            }

            // Query membership alone is not a deformation. Refreshing normals
            // on an untouched sphere would create a false commit and clear redo.
            if modified.is_empty() {
                continue;
            }
            modified_vertices
                .entry(chunk_id)
                .or_default()
                .extend(&modified);
            if !result.chunks_affected.contains(&chunk_id) {
                result.chunks_affected.push(chunk_id);
            }

            // Mark chunk dirty and track affected vertices
            chunk.mark_dirty();

            // Update normals for affected region
            Self::update_changed_normals(chunk, modified);

            // Invalidate octree (positions changed)
            self.chunk_octrees.remove(&chunk_id);
        }

        // Autosmooth reads a coherent snapshot of the primary deformation,
        // including shared vertices changed by neighboring chunks.
        if autosmooth > 0.0 && deformation_type != DeformationType::Smooth {
            self.sync_boundary_vertices(chunked_mesh, &result.chunks_affected);
        }
        let mut smoothing_passes = Vec::with_capacity(2);
        if deformation_type == DeformationType::Smooth {
            smoothing_passes.push(SmoothingPass::Smooth);
        }
        if autosmooth > 0.0 {
            smoothing_passes.push(SmoothingPass::Autosmooth(autosmooth));
        }
        for pass in smoothing_passes {
            let changed = apply_chunked_smoothing(
                chunked_mesh,
                &smoothing_vertices,
                &dab_info,
                falloff,
                pass,
            );
            for (chunk_id, vertices) in changed {
                modified_vertices
                    .entry(chunk_id)
                    .or_default()
                    .extend(&vertices);
                let chunk = chunked_mesh.get_chunk_mut(chunk_id).unwrap();
                chunk.mark_dirty();
                Self::update_changed_normals(chunk, vertices.into_iter().collect());
                if !result.chunks_affected.contains(&chunk_id) {
                    result.chunks_affected.push(chunk_id);
                }
                self.chunk_octrees.remove(&chunk_id);
            }
        }
        result.vertices_modified = modified_vertices.values().map(HashSet::len).sum();

        // Write back the updated vertex ID counter to the chunked mesh
        chunked_mesh.next_original_vertex_id = next_original_vertex_id;

        // Final boundary sync: propagate deformation positions across chunks
        trace!("apply_dab_internal: final boundary sync");
        self.sync_boundary_vertices(chunked_mesh, &result.chunks_affected);

        let validation = before_deformation
            .deformed(chunked_mesh)
            .and_then(|surface| {
                surface.validate_change(&committed_before)?;
                surface.validate_motion(&before_deformation)?;
                Ok(surface)
            });
        match validation {
            Ok(surface) => {
                if let Some(state) = &mut self.active_stroke_state {
                    let witness = Arc::new(GeometryWitness::capture(chunked_mesh));
                    self.last_admitted_surface =
                        Some((state.mesh_id, surface.clone(), witness.clone()));
                    state.admitted_surface = Some(surface);
                    state.admitted_witness = Some(witness);
                }
            }
            Err(error) => return self.reject_stroke(chunked_mesh, error),
        }
        // Spatial membership must follow accepted deformation, including seam
        // neighbors moved by synchronization, rather than retaining stale AABBs.
        for chunk in chunked_mesh.chunks.values_mut() {
            chunk.recalculate_bounds();
        }
        chunked_mesh.rebuild_spatial_grid();

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

    /// A changed face changes the normal of every incident vertex, including
    /// vertices outside the brush query and vertices with zero displacement.
    fn update_changed_normals(chunk: &mut MeshChunk, modified: HashSet<VertexId>) {
        let changed = DirtyVertices { modified };
        recalculate_face_normals_for_dirty(chunk, &changed);
        let mut normal_vertices = changed.modified.clone();
        for &vertex in &changed.modified {
            for face in chunk.mesh.get_vertex_faces(vertex) {
                normal_vertices.extend(chunk.mesh.get_face_vertices(face));
            }
        }
        recalculate_normals_for_dirty(
            chunk,
            &DirtyVertices {
                modified: normal_vertices,
            },
        );
    }

    /// Sync boundary vertices between affected chunks.
    fn sync_boundary_vertices(
        &mut self,
        chunked_mesh: &mut ChunkedMesh,
        affected_chunks: &[ChunkId],
    ) {
        // For each affected chunk, sync all boundary vertices
        if affected_chunks.is_empty() {
            return;
        }
        for &chunk_id in affected_chunks {
            let boundary_updates: Vec<(VertexId, Vec3)> = {
                let chunk = match chunked_mesh.get_chunk(chunk_id) {
                    Some(c) => c,
                    None => continue,
                };

                chunk
                    .boundary_vertices
                    .keys()
                    .filter_map(|&vid| chunk.mesh.vertex(vid).map(|v| (vid, v.position)))
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
                                    chunk_id,
                                    local_id,
                                    original_id,
                                    bref.chunk_id,
                                    bref.vertex_id,
                                    neighbor_original
                                );
                            }
                        }
                        // Verify positions are synchronized
                        let their_pos = neighbor.mesh.vertex(bref.vertex_id).map(|v| v.position);
                        if let (Some(ours), Some(theirs)) = (our_pos, their_pos) {
                            let dist = ours.distance(theirs);
                            if dist > 1e-5 {
                                debug!(
                                    "BOUNDARY CONSISTENCY: position desync for \
                                     original={:?}: chunk {:?}={:?} vs chunk {:?}={:?} \
                                     (delta={})",
                                    original_id, chunk_id, ours, bref.chunk_id, theirs, dist
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
    fn rebalance_chunks(
        &mut self,
        chunked_mesh: &mut ChunkedMesh,
    ) -> Result<(usize, usize), SafetyError> {
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

            let before = Surface::read(chunked_mesh)?;
            let mut progressed = false;
            for chunk_id in oversized {
                if crate::chunking::partition::split_chunk(chunked_mesh, chunk_id).is_some() {
                    progressed = true;
                    let surface = Surface::read(chunked_mesh)?;
                    surface.validate()?;
                    surface.validate_orientation(&before)?;
                    chunks_split += 1;
                    // Invalidate octree for split chunks
                    self.chunk_octrees.remove(&chunk_id);
                }
            }
            if !progressed {
                break;
            }
        }

        // Phase 2: Merge undersized adjacent chunks
        loop {
            let merge_pair = self.find_mergeable_pair(chunked_mesh, config);
            if let Some((a, b)) = merge_pair {
                let before = Surface::read(chunked_mesh)?;
                if crate::chunking::merge::merge_two_chunks(chunked_mesh, a, b).is_some() {
                    let surface = Surface::read(chunked_mesh)?;
                    surface.validate()?;
                    surface.validate_orientation(&before)?;
                    chunks_merged += 1;
                    // Invalidate octrees for merged chunks
                    self.chunk_octrees.remove(&a);
                    self.chunk_octrees.remove(&b);
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        // Rebuild spatial grid if chunks changed
        if chunks_split > 0 || chunks_merged > 0 {
            chunked_mesh.rebuild_spatial_grid();
        }

        Ok((chunks_split, chunks_merged))
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
        self.last_admitted_surface = None;
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
        pipeline.cancel_stroke(&mut ChunkedMesh::new());
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

#[cfg(all(test, feature = "bevy"))]
mod native_history_tests {
    use super::*;
    use crate::{PartitionConfig, partition_mesh};
    use bevy::prelude::{Meshable, Sphere};
    use painting::half_edge::HalfEdgeMesh;

    fn fixture() -> (ChunkedMesh, SculptingPipeline) {
        let source = Sphere::new(1.).mesh().uv(16, 8);
        let imported = HalfEdgeMesh::from_bevy_mesh_welded(&source).unwrap();
        let mesh = partition_mesh(
            &imported,
            &PartitionConfig {
                target_faces: 1000,
                min_faces: 1,
                max_faces: 2000,
            },
        );
        let preset = BrushPreset {
            radius: 0.75,
            strength: 0.015,
            spacing: 0.,
            autosmooth: 0.,
            ..BrushPreset::push()
        };
        let config = PipelineConfig {
            tessellation_config: TessellationConfig {
                mode: TessellationMode::BudgetCurvature,
                max_splits_per_pass: 8,
                max_tessellation_iterations: 1,
                curvature_split_threshold: 0.01,
                ..Default::default()
            },
            rebalance_after_stroke: false,
            ..Default::default()
        };
        let mut pipeline = SculptingPipeline::with_config(preset, config);
        pipeline.update_budget_from_coverage(1000);
        (mesh, pipeline)
    }
    fn input(x: f32) -> BrushInput {
        BrushInput {
            position: Vec3::new(x, 0., 1.),
            normal: Vec3::Z,
            pressure: 1.,
            timestamp_ms: 1,
        }
    }
    fn stroke(p: &mut SculptingPipeline, mesh: &mut ChunkedMesh) {
        p.begin_stroke(7, input(0.));
        assert!(p.process_input(input(0.01), mesh).rejected.is_none());
        assert!(p.end_stroke(mesh).rejected.is_none());
    }
    #[test]
    fn native_cancel_restores_224_faces_and_complete_checkpoint() {
        let (mut mesh, mut pipeline) = fixture();
        let before = mesh.clone();
        assert_eq!(mesh.total_face_count(), 224);
        pipeline.begin_stroke(7, input(0.));
        let result = pipeline.process_input(input(0.01), &mut mesh);
        assert!(result.rejected.is_none(), "{:?}", result.rejected);
        assert_eq!(mesh.total_face_count(), 232);
        pipeline.cancel_stroke(&mut mesh);
        assert_eq!(mesh.total_face_count(), 224);
        assert!(mesh.same_authoritative_state(&before));
        assert_eq!(pipeline.history_status().undo_strokes, 0);
        assert!(pipeline.end_stroke(&mut mesh).packets.is_empty());
        assert!(mesh.chunks.values().all(|c| c.dirty && c.topology_changed));
    }
    #[test]
    fn native_history_roundtrip_preserves_exact_geometry_uvs_and_counters() {
        let (mut mesh, mut pipeline) = fixture();
        let before = mesh.clone();
        stroke(&mut pipeline, &mut mesh);
        let after = mesh.clone();
        assert!(!before.same_authoritative_state(&after));
        assert_eq!(pipeline.history_status().undo_strokes, 1);
        assert!(pipeline.restore_history(&mut mesh, false).unwrap());
        assert!(before.same_authoritative_state(&mesh));
        assert!(pipeline.restore_history(&mut mesh, true).unwrap());
        assert!(after.same_authoritative_state(&mesh));
        assert!(pipeline.end_stroke(&mut mesh).packets.is_empty());
    }
    #[test]
    fn native_history_refuses_active_transaction_and_external_counter_edit() {
        let (mut mesh, mut pipeline) = fixture();
        stroke(&mut pipeline, &mut mesh);
        pipeline.begin_stroke(7, input(0.));
        assert!(pipeline.restore_history(&mut mesh, false).is_err());
        pipeline.cancel_stroke(&mut mesh);
        mesh.next_original_vertex_id += 1;
        let edited = mesh.clone();
        let status = pipeline.history_status();
        assert!(pipeline.restore_history(&mut mesh, false).is_err());
        assert!(edited.same_authoritative_state(&mesh));
        assert_eq!(status, pipeline.history_status());
    }
    #[test]
    fn native_history_reset_admits_valid_external_baseline_without_replay() {
        let (mut mesh, mut pipeline) = fixture();
        stroke(&mut pipeline, &mut mesh);
        assert!(pipeline.restore_history(&mut mesh, false).unwrap());
        assert_eq!(pipeline.history_status().redo_strokes, 1);
        mesh.next_original_vertex_id += 1;
        let external = mesh.clone();
        pipeline
            .reset_history(
                7,
                &mut mesh,
                Some("External edit: local history cleared.".into()),
            )
            .unwrap();
        assert!(mesh.same_authoritative_state(&external));
        assert_eq!(pipeline.history_status().undo_strokes, 0);
        assert_eq!(pipeline.history_status().redo_strokes, 0);
        assert_eq!(pipeline.history_status().retained_snapshot_bytes, 0);
        assert_eq!(
            pipeline.history_notice(),
            Some("External edit: local history cleared.")
        );
        assert!(pipeline.end_stroke(&mut mesh).packets.is_empty());
        assert!(!pipeline.restore_history(&mut mesh, false).unwrap());
        stroke(&mut pipeline, &mut mesh);
        assert_eq!(pipeline.history_status().undo_strokes, 1);
        assert!(pipeline.restore_history(&mut mesh, false).unwrap());
        assert!(mesh.same_authoritative_state(&external));
    }
    #[test]
    fn native_history_reset_refuses_active_and_invalid_mesh_atomically() {
        let (mut mesh, mut pipeline) = fixture();
        stroke(&mut pipeline, &mut mesh);
        let status = pipeline.history_status();
        let before = mesh.clone();
        pipeline.begin_stroke(7, input(0.));
        pipeline.history_notice = Some("existing notice".into());
        assert!(pipeline.reset_history(7, &mut mesh, None).is_err());
        assert!(pipeline.stroke_active());
        assert_eq!(pipeline.history_status(), status);
        assert_eq!(pipeline.history_notice(), Some("existing notice"));
        assert!(mesh.same_authoritative_state(&before));
        pipeline.cancel_stroke(&mut mesh);
        let mut invalid = mesh.clone();
        invalid.next_original_vertex_id = 0;
        assert!(pipeline.reset_history(7, &mut invalid, None).is_err());
        assert_eq!(pipeline.history_status(), status);
        let mut invalid = mesh.clone();
        invalid
            .chunks
            .values_mut()
            .next()
            .unwrap()
            .mesh
            .vertex_mut(painting::half_edge::VertexId(0))
            .unwrap()
            .position
            .x = f32::NAN;
        assert!(pipeline.reset_history(7, &mut invalid, None).is_err());
        assert_eq!(pipeline.history_status(), status);
        assert_eq!(pipeline.history_notice(), Some("existing notice"));
        assert!(pipeline.restore_history(&mut mesh, false).unwrap());
    }
    #[test]
    fn native_history_reset_rebuilds_stale_bounds_after_external_translation() {
        let (mut mesh, mut pipeline) = fixture();
        stroke(&mut pipeline, &mut mesh);
        for chunk in mesh.chunks.values_mut() {
            for index in 0..chunk.mesh.vertices().len() {
                if let Some(vertex) = chunk
                    .mesh
                    .vertex_mut(painting::half_edge::VertexId(index as u32))
                {
                    vertex.position += Vec3::X * 10.;
                }
            }
        }
        let external = mesh.clone();
        pipeline.reset_history(7, &mut mesh, None).unwrap();
        assert!(mesh.same_authoritative_state(&external));
        let mut dab = input(10.);
        pipeline.begin_stroke(7, dab);
        dab.position.x += 0.01;
        let result = pipeline.process_input(dab, &mut mesh);
        assert!(result.rejected.is_none(), "{:?}", result.rejected);
        assert!(
            result.vertices_modified > 0,
            "translated mesh must be discoverable"
        );
        let end = pipeline.end_stroke(&mut mesh);
        assert!(end.rejected.is_none(), "{:?}", end.rejected);
        assert!(!end.packets.is_empty());
        assert_eq!(pipeline.history_status().undo_strokes, 1);
        assert!(pipeline.restore_history(&mut mesh, false).unwrap());
        assert!(mesh.same_authoritative_state(&external));
    }
    #[test]
    fn native_history_reset_does_not_admit_a_conflicting_stroke() {
        let (mut mesh, mut pipeline) = fixture();
        stroke(&mut pipeline, &mut mesh);
        mesh.next_original_vertex_id += 1;
        let external = mesh.clone();
        pipeline.begin_stroke(7, input(0.));
        assert!(
            pipeline
                .process_input(input(0.01), &mut mesh)
                .rejected
                .is_some()
        );
        assert!(pipeline.reset_history(7, &mut mesh, None).is_err());
        let end = pipeline.end_stroke(&mut mesh);
        assert!(end.rejected.is_some());
        assert!(end.packets.is_empty());
        assert!(mesh.same_authoritative_state(&external));
        assert_eq!(pipeline.history_status().undo_strokes, 1);
        pipeline.reset_history(7, &mut mesh, None).unwrap();
        stroke(&mut pipeline, &mut mesh);
        assert!(pipeline.restore_history(&mut mesh, false).unwrap());
        assert!(mesh.same_authoritative_state(&external));
    }
    #[test]
    fn native_history_revalidates_invalid_target_without_partial_swap() {
        let (mut mesh, mut pipeline) = fixture();
        let after = SculptSnapshot::capture(7, &mesh);
        let mut invalid = after.clone();
        invalid.mesh.next_original_vertex_id = 0;
        // Simulate old persisted provenance: current validation must reject it.
        pipeline
            .history
            .record_accepted(invalid, after, |_| Ok::<_, SafetyError>(()))
            .unwrap();
        pipeline.history_owner = Some(7);
        let before = mesh.clone();
        let status = pipeline.history_status();
        assert!(pipeline.restore_history(&mut mesh, false).is_err());
        assert!(before.same_authoritative_state(&mesh));
        assert_eq!(status, pipeline.history_status());
    }
    #[test]
    fn native_restore_rechecks_complete_geometric_overlap_policy() {
        use bevy::asset::RenderAssetUsages;
        use bevy::mesh::{Indices, Mesh, PrimitiveTopology};
        let mut source = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        source.insert_attribute(
            Mesh::ATTRIBUTE_POSITION,
            vec![
                [0., 0., 0.06],
                [0.5, 0., 0.06],
                [0., 0.5, 0.06],
                [-1., -1., 0.],
                [2., -1., 0.],
                [-1., 2., 0.],
            ],
        );
        source.insert_indices(Indices::U32(vec![0, 1, 2, 3, 4, 5]));
        let imported = HalfEdgeMesh::from_bevy_mesh(&source).unwrap();
        let mut mesh = partition_mesh(&imported, &PartitionConfig::default());
        let after = SculptSnapshot::capture(7, &mesh);
        after.validate().unwrap();
        let mut crossing = after.clone();
        let chunk = crossing.mesh.chunks.values_mut().next().unwrap();
        let vertex = chunk
            .mesh
            .vertices()
            .iter()
            .find(|v| v.position == Vec3::new(0., 0., 0.06))
            .unwrap()
            .id;
        chunk.mesh.vertex_mut(vertex).unwrap().position.z = -0.06;
        // Half-edge connectivity remains valid, while the detached geometry crosses.
        chunk.mesh.validate().unwrap();
        let mut pipeline = SculptingPipeline::new(BrushPreset::push());
        pipeline
            .history
            .record_accepted(crossing, after, |_| Ok::<_, SafetyError>(()))
            .unwrap();
        pipeline.history_owner = Some(7);
        let before = mesh.clone();
        assert!(matches!(
            pipeline.restore_history(&mut mesh, false),
            Err(HistoryError::Validation(SafetyError::Intersection))
        ));
        assert!(mesh.same_authoritative_state(&before));
        assert_eq!(pipeline.history_status().undo_strokes, 1);
    }

    #[test]
    fn native_grab_moves_once_and_stationary_frames_do_not_amplify_drag() {
        let (mut mesh, mut pipeline) = fixture();
        pipeline.config.tessellation_enabled = false;
        pipeline.set_brush_preset(BrushPreset {
            radius: 0.75,
            strength: 0.4,
            ..BrushPreset::grab()
        });
        let before = mesh.clone();
        pipeline.begin_stroke(7, input(0.));
        assert!(
            pipeline
                .process_input(input(0.02), &mut mesh)
                .rejected
                .is_none()
        );
        let moved = mesh.clone();
        assert!(!before.same_authoritative_state(&moved));
        for _ in 0..20 {
            assert!(
                pipeline
                    .process_input(input(0.02), &mut mesh)
                    .rejected
                    .is_none()
            );
        }
        assert!(
            moved.same_authoritative_state(&mesh),
            "holding Grab still must not reapply cumulative displacement"
        );
        assert!(pipeline.end_stroke(&mut mesh).rejected.is_none());
    }
    #[test]
    fn native_accepted_branch_clears_redo_after_real_undo() {
        let (mut mesh, mut pipeline) = fixture();
        let baseline = mesh.clone();
        stroke(&mut pipeline, &mut mesh);
        pipeline.restore_history(&mut mesh, false).unwrap();
        assert_eq!(pipeline.history_status().redo_strokes, 1);
        pipeline.brush_engine.preset.strength = 0.01;
        stroke(&mut pipeline, &mut mesh);
        assert_eq!(pipeline.history_status().undo_strokes, 1);
        assert_eq!(pipeline.history_status().redo_strokes, 0);
        assert!(!pipeline.restore_history(&mut mesh, true).unwrap());
        pipeline.restore_history(&mut mesh, false).unwrap();
        assert!(mesh.same_authoritative_state(&baseline));
    }

    #[test]
    fn native_over_budget_history_reports_unavailability_without_losing_geometry() {
        let (mut mesh, mut pipeline) = fixture();
        let baseline = mesh.clone();
        pipeline.history = SculptHistory::new(crate::history::HistoryLimits {
            max_strokes: 0,
            max_snapshot_bytes: 0,
        });
        stroke(&mut pipeline, &mut mesh);
        assert!(!mesh.same_authoritative_state(&baseline));
        assert_eq!(pipeline.history_status().undo_strokes, 0);
        assert!(
            pipeline
                .history_notice()
                .unwrap()
                .contains("cannot be undone")
        );
    }

    #[test]
    fn native_noop_keeps_redo_branch() {
        let (mut mesh, mut pipeline) = fixture();
        stroke(&mut pipeline, &mut mesh);
        pipeline.restore_history(&mut mesh, false).unwrap();
        let status = pipeline.history_status();
        pipeline.begin_stroke(7, input(0.));
        assert!(pipeline.end_stroke(&mut mesh).packets.is_empty());
        assert_eq!(status, pipeline.history_status());
    }
}
