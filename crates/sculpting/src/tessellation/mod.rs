//! Adaptive tessellation for sculpting.
//!
//! This module provides screen-space adaptive tessellation similar to Blender's
//! DynTopo mode. Edges are split when they appear too long on screen and
//! collapsed when they're too short.
//!
//! ## Screen-Space Detail
//!
//! Detail size is based on how edges appear on screen:
//! - **Zoom in** → edges appear larger → more subdivision
//! - **Zoom out** → edges appear smaller → coarser detail
//!
//! ## Determinism
//!
//! For undo/redo via dab replay to work correctly, tessellation must be
//! deterministic. This means:
//! - Process edges in consistent order (sorted by ID)
//! - Use exact floating-point comparisons where possible
//! - Tie-break consistently (e.g., lower edge ID wins)

mod edge_collapse;
mod edge_split;
pub mod curvature;
mod metrics;

pub use edge_collapse::{
    calculate_collapse_position, can_collapse_edge, can_collapse_edge_safe, collapse_edge,
    collapse_or_flip_edge, would_cause_flip, CollapseCheck, CollapseOrFlipResult,
    CollapseRejection, CollapseResult,
};
pub use edge_split::{
    calculate_curvature_aware_split_position, calculate_split_position, can_split_edge,
    interpolate_vertex_attributes, split_edge, split_edge_curvature_aware, SplitResult,
};
pub use metrics::{
    calculate_edge_screen_length, calculate_mesh_quality, calculate_triangle_aspect_ratio,
    calculate_world_edge_length, evaluate_edge, is_degenerate_triangle, is_valid_valence,
    EdgeEvaluation, MeshQuality, ScreenSpaceConfig,
};
pub use curvature::{dihedral_angle, evaluate_edge_curvature, CurvatureEvaluation};

use crate::budget::VertexBudget;
use crate::chunking::MeshChunk;
use crate::types::TessellationConfig;
use glam::Vec3;
use painting::half_edge::{HalfEdgeId, HalfEdgeMesh, VertexId};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use tracing::{debug, trace};

/// Tessellation decision for a single edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TessellationDecision {
    /// Edge should be split (subdivided)
    Split,
    /// Edge should be collapsed (merged)
    Collapse,
    /// No action needed
    None,
}

/// Apply tessellation within a brush radius.
///
/// Iterates split and collapse passes until convergence (no more changes)
/// or `max_tessellation_iterations` is reached. This matches SculptGL's
/// convergence loop pattern (Subdivision.js:458-461).
///
/// ## Key design decisions
///
/// - **Vertex-pair lookup**: Edges are stored as `(VertexId, VertexId)` pairs
///   rather than `HalfEdgeId`s, because sequential splits rewire half-edge
///   pointers. Vertex pairs are stable across topology changes.
/// - **Midpoint deduplication**: A `HashMap` keyed by `(min(v0,v1), max(v0,v1))`
///   prevents creating duplicate midpoints when both directions of the same
///   edge appear in the split list.
/// - **Twin rebuild**: After all splits, `rebuild_twins_from_edge_map()` ensures
///   100% twin consistency by deriving twins from the authoritative edge_map.
/// - **Curvature-aware positioning**: Uses normal-based offset (SculptGL pattern)
///   to preserve surface curvature during subdivision.
/// - **Tangent-plane smoothing**: After splits, new vertices and their 1-ring
///   neighbors are smoothed on the tangent plane to eliminate noise.
pub fn tessellate_at_brush(
    chunk: &mut MeshChunk,
    brush_center: Vec3,
    brush_radius: f32,
    config: &TessellationConfig,
    screen_config: &ScreenSpaceConfig,
    next_original_vertex_id: &mut u32,
) -> TessellationStats {
    trace!("tessellate_at_brush: START");
    let mut stats = TessellationStats::default();
    let influence_radius = brush_radius * 1.5;

    // Check if chunk is already at max face limit
    if chunk.mesh.face_count() >= config.max_faces_per_chunk {
        debug!(
            "tessellate: at max faces ({} >= {}), skipping",
            chunk.mesh.face_count(),
            config.max_faces_per_chunk
        );
        return stats;
    }

    // Convergence loop: iterate split+collapse until no changes or max iterations
    for iteration in 0..config.max_tessellation_iterations {
        let iter_start = std::time::Instant::now();
        let mut collapses_this_iter = 0usize;

        // ===== SPLIT PASS =====
        debug!("  tessellate iter {}: starting split_pass (faces={})", iteration, chunk.mesh.face_count());
        let splits_this_iter = split_pass(
            chunk,
            brush_center,
            influence_radius,
            config,
            screen_config,
            next_original_vertex_id,
        );

        stats.edges_split += splits_this_iter;
        if splits_this_iter > 0 {
            chunk.topology_changed = true;
        }

        // ===== COLLAPSE PASS =====
        debug!("  tessellate iter {}: starting collapse_pass (faces={}, splits={})", iteration, chunk.mesh.face_count(), splits_this_iter);
        if config.collapse_enabled && chunk.mesh.face_count() > config.min_faces {
            collapses_this_iter = collapse_pass(
                chunk,
                brush_center,
                influence_radius,
                config,
                screen_config,
                next_original_vertex_id,
            );

            stats.edges_collapsed += collapses_this_iter;
            if collapses_this_iter > 0 {
                chunk.topology_changed = true;
            }
        }

        debug!(
            "  tessellate iter {}: done in {:?} - split={}, collapsed={}, faces={}",
            iteration, iter_start.elapsed(), splits_this_iter, collapses_this_iter, chunk.mesh.face_count()
        );

        // Converged - no more changes needed
        if splits_this_iter == 0 && collapses_this_iter == 0 {
            break;
        }

        // Safety check: don't exceed max face count
        if chunk.mesh.face_count() >= config.max_faces_per_chunk {
            debug!(
                "tessellate: reached max faces ({}) during iteration, stopping",
                chunk.mesh.face_count()
            );
            break;
        }
    }

    trace!("tessellate_at_brush: END stats={:?}", stats);

    // Validate chunk after tessellation in debug builds
    #[cfg(debug_assertions)]
    if stats.edges_split > 0 || stats.edges_collapsed > 0 {
        if let Err(e) = validate_chunk_after_tessellation(chunk) {
            tracing::error!("CHUNK VALIDATION FAILED after tessellation: {}", e);
            tracing::error!(
                "Chunk state: {} vertices, {} faces, {} half-edges",
                chunk.mesh.vertex_count(),
                chunk.mesh.face_count(),
                chunk.mesh.half_edges().len()
            );
            tracing::error!(
                "local_to_original has {} entries",
                chunk.local_to_original.len()
            );
        }
    }

    stats
}

/// Apply budget+curvature tessellation within a brush radius.
///
/// Like `tessellate_at_brush` but uses curvature-prioritized split/collapse
/// subject to a global vertex budget instead of per-edge screen-space evaluation.
///
/// Edges are split in order of highest curvature first and collapsed in order
/// of lowest curvature first. Splitting stops when the budget is exhausted or
/// no edges exceed the curvature threshold.
pub fn tessellate_at_brush_budget(
    chunk: &mut MeshChunk,
    brush_center: Vec3,
    brush_radius: f32,
    config: &TessellationConfig,
    budget: &mut VertexBudget,
    next_original_vertex_id: &mut u32,
) -> TessellationStats {
    trace!("tessellate_at_brush_budget: START");
    let mut stats = TessellationStats::default();
    let influence_radius = brush_radius * 1.5;

    // Check if chunk is already at max face limit
    if chunk.mesh.face_count() >= config.max_faces_per_chunk {
        debug!(
            "tessellate_budget: at max faces ({} >= {}), skipping",
            chunk.mesh.face_count(),
            config.max_faces_per_chunk
        );
        return stats;
    }

    // Convergence loop: iterate split+collapse until no changes or max iterations
    for iteration in 0..config.max_tessellation_iterations {
        let mut collapses_this_iter = 0usize;

        // ===== SPLIT PASS (curvature-prioritized, budget-limited) =====
        let splits_this_iter = split_pass_budget(
            chunk,
            brush_center,
            influence_radius,
            config,
            budget,
            next_original_vertex_id,
        );

        stats.edges_split += splits_this_iter;
        if splits_this_iter > 0 {
            chunk.topology_changed = true;
        }

        // ===== COLLAPSE PASS (lowest-curvature first, also triggered when over budget) =====
        if config.collapse_enabled && chunk.mesh.face_count() > config.min_faces {
            collapses_this_iter = collapse_pass_budget(
                chunk,
                brush_center,
                influence_radius,
                config,
                budget,
                next_original_vertex_id,
            );

            stats.edges_collapsed += collapses_this_iter;
            if collapses_this_iter > 0 {
                chunk.topology_changed = true;
            }
        }

        debug!(
            "tessellate_budget iteration {}: split={}, collapsed={}, faces={}, budget_remaining={}",
            iteration, splits_this_iter, collapses_this_iter, chunk.mesh.face_count(), budget.remaining
        );

        // Converged - no more changes needed
        if splits_this_iter == 0 && collapses_this_iter == 0 {
            break;
        }

        // Safety check: don't exceed max face count
        if chunk.mesh.face_count() >= config.max_faces_per_chunk {
            debug!(
                "tessellate_budget: reached max faces ({}) during iteration, stopping",
                chunk.mesh.face_count()
            );
            break;
        }
    }

    trace!("tessellate_at_brush_budget: END stats={:?}", stats);

    // Validate chunk after tessellation in debug builds
    #[cfg(debug_assertions)]
    if stats.edges_split > 0 || stats.edges_collapsed > 0 {
        if let Err(e) = validate_chunk_after_tessellation(chunk) {
            tracing::error!("CHUNK VALIDATION FAILED after budget tessellation: {}", e);
            tracing::error!(
                "Chunk state: {} vertices, {} faces, {} half-edges",
                chunk.mesh.vertex_count(),
                chunk.mesh.face_count(),
                chunk.mesh.half_edges().len()
            );
        }
    }

    stats
}

/// Run one budget-aware split pass: evaluate edges by curvature, split highest-curvature
/// edges first, respecting the vertex budget.
///
/// Returns the number of edges split.
fn split_pass_budget(
    chunk: &mut MeshChunk,
    brush_center: Vec3,
    influence_radius: f32,
    config: &TessellationConfig,
    budget: &mut VertexBudget,
    next_original_vertex_id: &mut u32,
) -> usize {
    if !budget.can_split() {
        return 0;
    }

    let edges_in_range = collect_edges_in_range(chunk, brush_center, influence_radius);

    // Evaluate edges by curvature and collect split candidates with their scores
    let mut split_candidates: Vec<(VertexId, VertexId, f32)> = Vec::new(); // (v0, v1, dihedral_angle)
    for &edge_id in &edges_in_range {
        let eval = evaluate_edge_curvature(&chunk.mesh, edge_id, config);
        if eval.decision == TessellationDecision::Split {
            let Some(he) = chunk.mesh.half_edge(edge_id) else { continue };
            let v0 = he.origin;
            let Some(next_he) = chunk.mesh.half_edge(he.next) else { continue };
            let v1 = next_he.origin;

            // Check minimum edge length floor
            if config.min_edge_length > 0.0 {
                let v0_pos = chunk.mesh.vertex(v0).map(|v| v.position);
                let v1_pos = chunk.mesh.vertex(v1).map(|v| v.position);
                if let (Some(p0), Some(p1)) = (v0_pos, v1_pos) {
                    if p0.distance(p1) < config.min_edge_length {
                        continue;
                    }
                }
            }

            split_candidates.push((v0, v1, eval.dihedral_angle));
        }
    }

    // Sort by curvature DESCENDING (highest curvature = highest priority to split)
    // Tiebreak by vertex pair for determinism
    split_candidates.sort_by(|a, b| {
        b.2.partial_cmp(&a.2)
            .unwrap_or(Ordering::Equal)
            .then_with(|| {
                let a_key = (a.0 .0.min(a.1 .0), a.0 .0.max(a.1 .0));
                let b_key = (b.0 .0.min(b.1 .0), b.0 .0.max(b.1 .0));
                a_key.cmp(&b_key)
            })
    });

    // Midpoint deduplication: prevents splitting the same edge twice
    let mut midpoint_map: HashMap<(VertexId, VertexId), VertexId> = HashMap::new();
    let mut actual_splits = 0usize;

    for (v0, v1, _curvature) in split_candidates {
        // Budget check
        if !budget.can_split() {
            break;
        }

        // Safety: max faces check
        if chunk.mesh.face_count() >= config.max_faces_per_chunk {
            break;
        }

        // Cap splits per pass to prevent runaway mesh growth
        if actual_splits >= config.max_splits_per_pass {
            break;
        }

        // Deduplicate: skip if this edge was already split
        let key = if v0.0 < v1.0 { (v0, v1) } else { (v1, v0) };
        if midpoint_map.contains_key(&key) {
            continue;
        }

        // Skip true cross-chunk boundary edges (both endpoints are boundary vertices).
        // Edges where only ONE endpoint is boundary are interior to this chunk and
        // safe to split — the new midpoint will be an interior vertex.
        // Previously this used `||` which created a "dead zone" around all boundary
        // vertices where tessellation couldn't operate, causing density discontinuities.
        if chunk.boundary_vertices.contains_key(&v0)
            && chunk.boundary_vertices.contains_key(&v1)
        {
            continue;
        }

        // Look up CURRENT half-edge ID (may have changed from earlier splits)
        let Some(edge_id) = chunk.mesh.find_half_edge(v0, v1) else {
            continue;
        };

        // Use curvature-aware split to preserve surface shape
        if let Some(split_result) = split_edge_curvature_aware(&mut chunk.mesh, edge_id) {
            midpoint_map.insert(key, split_result.new_vertex);
            actual_splits += 1;
            budget.record_split();

            // Register new vertex with globally unique original ID
            let unique_original_id = VertexId(*next_original_vertex_id);
            *next_original_vertex_id += 1;
            chunk
                .local_to_original
                .insert(split_result.new_vertex, unique_original_id);
            chunk
                .original_to_local
                .insert(unique_original_id, split_result.new_vertex);
        }
    }

    if actual_splits > 0 {
        chunk.mesh.rebuild_twins_from_edge_map();
        let new_verts: Vec<VertexId> = midpoint_map.values().copied().collect();
        tangent_smooth_new_vertices(&mut chunk.mesh, &new_verts, 0.5);
    }

    actual_splits
}

/// Run one budget-aware collapse pass: collapse lowest-curvature edges first.
///
/// Also triggers forced collapse when over budget, even for edges above the
/// curvature threshold, to bring vertex count back within the budget.
///
/// Returns the number of edges collapsed.
fn collapse_pass_budget(
    chunk: &mut MeshChunk,
    brush_center: Vec3,
    influence_radius: f32,
    config: &TessellationConfig,
    budget: &mut VertexBudget,
    next_original_vertex_id: &mut u32,
) -> usize {
    let edges_in_range = collect_edges_in_range(chunk, brush_center, influence_radius);
    let over_budget = budget.is_over_budget();

    // Collect collapse candidates with curvature scores
    let mut collapse_candidates: Vec<(VertexId, VertexId, f32)> = Vec::new();
    for &edge_id in &edges_in_range {
        let eval = evaluate_edge_curvature(&chunk.mesh, edge_id, config);

        // Candidate if: below curvature threshold OR we're over budget
        let is_candidate = eval.decision == TessellationDecision::Collapse || over_budget;
        if !is_candidate {
            continue;
        }

        let check = can_collapse_edge_safe(&chunk.mesh, edge_id);
        match check {
            CollapseCheck::Safe(_) | CollapseCheck::UseEdgeFlip => {
                let Some(he) = chunk.mesh.half_edge(edge_id) else { continue };
                let v0 = he.origin;
                let Some(next_he) = chunk.mesh.half_edge(he.next) else { continue };
                let v1 = next_he.origin;
                collapse_candidates.push((v0, v1, eval.dihedral_angle));
            }
            CollapseCheck::Rejected(_) => {}
        }
    }

    // Sort by curvature ASCENDING (lowest curvature = collapse first)
    // Tiebreak by vertex pair for determinism
    collapse_candidates.sort_by(|a, b| {
        a.2.partial_cmp(&b.2)
            .unwrap_or(Ordering::Equal)
            .then_with(|| {
                let a_key = (a.0 .0.min(a.1 .0), a.0 .0.max(a.1 .0));
                let b_key = (b.0 .0.min(b.1 .0), b.0 .0.max(b.1 .0));
                a_key.cmp(&b_key)
            })
    });

    let mut actual_collapses = 0usize;
    let mut actual_flips = 0usize;
    // Track vertices affected by previous collapses this pass (see collapse_pass for details).
    let mut dirty_vertices: HashSet<VertexId> = HashSet::new();

    for (v0, v1, _curvature) in collapse_candidates {
        if chunk.mesh.face_count() <= config.min_faces {
            break;
        }

        // If we were collapsing just because we're over budget, stop once we're back within budget
        if over_budget && !budget.is_over_budget() {
            break;
        }

        // Skip boundary edges
        if chunk.boundary_vertices.contains_key(&v0)
            || chunk.boundary_vertices.contains_key(&v1)
        {
            continue;
        }

        // Skip if either vertex was affected by a prior collapse this pass
        if dirty_vertices.contains(&v0) || dirty_vertices.contains(&v1) {
            continue;
        }

        // Look up CURRENT half-edge ID
        let Some(edge_id) = chunk.mesh.find_half_edge(v0, v1) else {
            continue;
        };

        // Collect neighbors BEFORE topology changes (ring walks are still intact)
        let neighbors_v0 = chunk.mesh.get_adjacent_vertices(v0);
        let neighbors_v1 = chunk.mesh.get_adjacent_vertices(v1);

        match collapse_or_flip_edge(&mut chunk.mesh, edge_id) {
            Some(CollapseOrFlipResult::Collapsed(_)) => {
                actual_collapses += 1;
                budget.record_collapse();
                dirty_vertices.insert(v0);
                dirty_vertices.insert(v1);
                for v in neighbors_v0 { dirty_vertices.insert(v); }
                for v in neighbors_v1 { dirty_vertices.insert(v); }
            }
            Some(CollapseOrFlipResult::Flipped) => {
                actual_flips += 1;
                dirty_vertices.insert(v0);
                dirty_vertices.insert(v1);
                for v in neighbors_v0 { dirty_vertices.insert(v); }
                for v in neighbors_v1 { dirty_vertices.insert(v); }
            }
            None => {}
        }
    }

    // Compact after collapses or flips to remove dead elements and rebuild edge_map.
    // Flips can create edge_map inconsistencies that need cleanup even if no collapses occurred.
    if actual_collapses > 0 || actual_flips > 0 {
        // Save boundary vertex original IDs BEFORE compact can lose them.
        // Compact's liveness detection may falsely mark boundary vertices as dead,
        // causing their local_to_original mapping to be silently lost. We save the
        // mapping here so we can recover it after compact.
        let boundary_originals: HashMap<VertexId, VertexId> = chunk
            .boundary_vertices
            .keys()
            .filter_map(|&lid| chunk.local_to_original.get(&lid).map(|&oid| (lid, oid)))
            .collect();

        let compaction = chunk.mesh.compact();

        // Update local_to_original and original_to_local with remapped vertex IDs
        let old_l2o = std::mem::take(&mut chunk.local_to_original);
        for (old_local, original) in old_l2o {
            if let Some(&new_local) = compaction.vertex_map.get(&old_local) {
                chunk.local_to_original.insert(new_local, original);
                chunk.original_to_local.insert(original, new_local);
            }
        }

        // Rebuild original_to_local from local_to_original
        chunk.original_to_local.clear();
        for (&local, &original) in &chunk.local_to_original {
            chunk.original_to_local.insert(original, local);
        }

        // Update boundary_vertices with remapped vertex IDs
        let old_boundary = std::mem::take(&mut chunk.boundary_vertices);
        for (old_local, refs) in old_boundary {
            if let Some(&new_local) = compaction.vertex_map.get(&old_local) {
                chunk.boundary_vertices.insert(new_local, refs);
            }
        }

        // Recover any boundary vertex mappings that were lost during compact.
        // If compact removed a boundary vertex from vertex_map but it survived
        // in the mesh (e.g., due to liveness detection disagreement), restore
        // its original ID to maintain cross-chunk identity.
        let mut boundary_recovered = 0;
        for (&old_local, &original) in &boundary_originals {
            if let Some(&new_local) = compaction.vertex_map.get(&old_local) {
                if !chunk.local_to_original.contains_key(&new_local) {
                    chunk.local_to_original.insert(new_local, original);
                    chunk.original_to_local.insert(original, new_local);
                    boundary_recovered += 1;
                }
            }
        }
        if boundary_recovered > 0 {
            tracing::warn!(
                "collapse_pass_budget: recovered {} boundary vertex mappings that \
                 would have been lost during compact",
                boundary_recovered
            );
        }

        // Safety net: ensure ALL mesh vertices have local_to_original mappings.
        repair_vertex_mappings(chunk, next_original_vertex_id);
    }

    actual_collapses
}

/// Run one split pass: evaluate edges, split those that are too long, rebuild twins, smooth.
///
/// Returns the number of edges split.
fn split_pass(
    chunk: &mut MeshChunk,
    brush_center: Vec3,
    influence_radius: f32,
    config: &TessellationConfig,
    screen_config: &ScreenSpaceConfig,
    next_original_vertex_id: &mut u32,
) -> usize {
    // Collect edges as vertex pairs (stable across topology changes)
    let edges_in_range = collect_edges_in_range(chunk, brush_center, influence_radius);

    let mut edges_to_split: Vec<(VertexId, VertexId)> = Vec::new();
    for &edge_id in &edges_in_range {
        let eval = evaluate_edge_in_chunk(chunk, edge_id, config, screen_config);
        if eval.decision == TessellationDecision::Split {
            // Convert to vertex pair
            let Some(he) = chunk.mesh.half_edge(edge_id) else { continue };
            let v0 = he.origin;
            let Some(next_he) = chunk.mesh.half_edge(he.next) else { continue };
            let v1 = next_he.origin;
            edges_to_split.push((v0, v1));
        }
    }

    // Sort vertex pairs for determinism
    edges_to_split.sort_by_key(|(v0, v1)| {
        let a = v0.0.min(v1.0);
        let b = v0.0.max(v1.0);
        (a, b)
    });

    // Midpoint deduplication: prevents splitting the same edge twice
    let mut midpoint_map: HashMap<(VertexId, VertexId), VertexId> = HashMap::new();
    let mut actual_splits = 0usize;

    for (v0, v1) in edges_to_split {
        // Safety: max faces check
        if chunk.mesh.face_count() >= config.max_faces_per_chunk {
            break;
        }

        // Cap splits per pass to prevent runaway mesh growth
        if actual_splits >= config.max_splits_per_pass {
            break;
        }

        // Deduplicate: skip if this edge was already split
        let key = if v0.0 < v1.0 { (v0, v1) } else { (v1, v0) };
        if midpoint_map.contains_key(&key) {
            continue;
        }

        // Skip true cross-chunk boundary edges (both endpoints are boundary vertices).
        // Edges where only ONE endpoint is boundary are interior to this chunk and
        // safe to split — the new midpoint will be an interior vertex.
        // Previously this used `||` which created a "dead zone" around all boundary
        // vertices where tessellation couldn't operate, causing density discontinuities.
        if chunk.boundary_vertices.contains_key(&v0)
            && chunk.boundary_vertices.contains_key(&v1)
        {
            continue;
        }

        // Look up CURRENT half-edge ID (may have changed from earlier splits)
        let Some(edge_id) = chunk.mesh.find_half_edge(v0, v1) else {
            continue; // Edge no longer exists (modified by prior split)
        };

        // Use curvature-aware split to preserve surface shape
        if let Some(split_result) = split_edge_curvature_aware(&mut chunk.mesh, edge_id) {
            midpoint_map.insert(key, split_result.new_vertex);
            actual_splits += 1;

            // Register new vertex with globally unique original ID
            let unique_original_id = VertexId(*next_original_vertex_id);
            *next_original_vertex_id += 1;
            chunk
                .local_to_original
                .insert(split_result.new_vertex, unique_original_id);
            chunk
                .original_to_local
                .insert(unique_original_id, split_result.new_vertex);
        }
    }

    if actual_splits > 0 {
        // Rebuild twin pointers from the authoritative edge_map.
        // Sequential splits can corrupt twin pointers because each split rewires
        // next/prev/twin on surrounding half-edges that later splits reference.
        // The edge_map is always correct (maintained by each split), so we use it
        // to derive consistent twins.
        chunk.mesh.rebuild_twins_from_edge_map();

        // Tangent-plane smooth new vertices to prevent spiky/noisy mesh
        let new_verts: Vec<VertexId> = midpoint_map.values().copied().collect();
        tangent_smooth_new_vertices(&mut chunk.mesh, &new_verts, 0.5);
    }

    actual_splits
}

/// Run one collapse pass: evaluate edges, collapse those that are too short.
///
/// Returns the number of edges collapsed.
fn collapse_pass(
    chunk: &mut MeshChunk,
    brush_center: Vec3,
    influence_radius: f32,
    config: &TessellationConfig,
    screen_config: &ScreenSpaceConfig,
    next_original_vertex_id: &mut u32,
) -> usize {
    let edges_in_range = collect_edges_in_range(chunk, brush_center, influence_radius);

    // Collect collapse candidates as vertex pairs
    let mut edges_to_collapse: Vec<(VertexId, VertexId)> = Vec::new();
    for &edge_id in &edges_in_range {
        let eval = evaluate_edge_in_chunk(chunk, edge_id, config, screen_config);
        if eval.decision == TessellationDecision::Collapse {
            let check = can_collapse_edge_safe(&chunk.mesh, edge_id);
            match check {
                CollapseCheck::Safe(_) | CollapseCheck::UseEdgeFlip => {
                    let Some(he) = chunk.mesh.half_edge(edge_id) else { continue };
                    let v0 = he.origin;
                    let Some(next_he) = chunk.mesh.half_edge(he.next) else { continue };
                    let v1 = next_he.origin;
                    edges_to_collapse.push((v0, v1));
                }
                CollapseCheck::Rejected(_) => {}
            }
        }
    }

    // Sort for determinism
    edges_to_collapse.sort_by_key(|(v0, v1)| {
        let a = v0.0.min(v1.0);
        let b = v0.0.max(v1.0);
        (a, b)
    });

    let mut actual_collapses = 0usize;
    let mut actual_flips = 0usize;
    // Track vertices affected by previous collapses this pass.
    // Sequential collapses can break ring walks (orphaned edges clear twin pointers
    // on neighboring edges), causing incomplete vertex redirects and edge_map corruption.
    // Skip any collapse involving a vertex whose ring may be broken.
    let mut dirty_vertices: HashSet<VertexId> = HashSet::new();

    for (v0, v1) in edges_to_collapse {
        if chunk.mesh.face_count() <= config.min_faces {
            break;
        }

        // Skip boundary edges
        if chunk.boundary_vertices.contains_key(&v0)
            || chunk.boundary_vertices.contains_key(&v1)
        {
            continue;
        }

        // Skip if either vertex was affected by a prior collapse this pass
        if dirty_vertices.contains(&v0) || dirty_vertices.contains(&v1) {
            continue;
        }

        // Look up CURRENT half-edge ID
        let Some(edge_id) = chunk.mesh.find_half_edge(v0, v1) else {
            continue;
        };

        // Collect neighbors BEFORE topology changes (ring walks are still intact)
        let neighbors_v0 = chunk.mesh.get_adjacent_vertices(v0);
        let neighbors_v1 = chunk.mesh.get_adjacent_vertices(v1);

        match collapse_or_flip_edge(&mut chunk.mesh, edge_id) {
            Some(CollapseOrFlipResult::Collapsed(_)) => {
                actual_collapses += 1;
                // Mark all affected vertices as dirty to prevent cascading corruption
                dirty_vertices.insert(v0);
                dirty_vertices.insert(v1);
                for v in neighbors_v0 { dirty_vertices.insert(v); }
                for v in neighbors_v1 { dirty_vertices.insert(v); }
            }
            Some(CollapseOrFlipResult::Flipped) => {
                actual_flips += 1;
                // Flip also modifies topology around these vertices
                dirty_vertices.insert(v0);
                dirty_vertices.insert(v1);
                for v in neighbors_v0 { dirty_vertices.insert(v); }
                for v in neighbors_v1 { dirty_vertices.insert(v); }
            }
            None => {}
        }
    }

    // Compact after collapses or flips to remove dead elements and rebuild edge_map.
    // Flips can create edge_map inconsistencies that need cleanup even if no collapses occurred.
    if actual_collapses > 0 || actual_flips > 0 {
        // Save boundary vertex original IDs BEFORE compact can lose them.
        // Compact's liveness detection may falsely mark boundary vertices as dead,
        // causing their local_to_original mapping to be silently lost. We save the
        // mapping here so we can recover it after compact.
        let boundary_originals: HashMap<VertexId, VertexId> = chunk
            .boundary_vertices
            .keys()
            .filter_map(|&lid| chunk.local_to_original.get(&lid).map(|&oid| (lid, oid)))
            .collect();

        let compaction = chunk.mesh.compact();

        // Update local_to_original and original_to_local with remapped vertex IDs
        let old_l2o = std::mem::take(&mut chunk.local_to_original);
        for (old_local, original) in old_l2o {
            if let Some(&new_local) = compaction.vertex_map.get(&old_local) {
                chunk.local_to_original.insert(new_local, original);
                chunk.original_to_local.insert(original, new_local);
            }
        }

        // Rebuild original_to_local from local_to_original
        chunk.original_to_local.clear();
        for (&local, &original) in &chunk.local_to_original {
            chunk.original_to_local.insert(original, local);
        }

        // Update boundary_vertices with remapped vertex IDs
        let old_boundary = std::mem::take(&mut chunk.boundary_vertices);
        for (old_local, refs) in old_boundary {
            if let Some(&new_local) = compaction.vertex_map.get(&old_local) {
                chunk.boundary_vertices.insert(new_local, refs);
            }
        }

        // Recover any boundary vertex mappings that were lost during compact.
        // If compact removed a boundary vertex from vertex_map but it survived
        // in the mesh (e.g., due to liveness detection disagreement), restore
        // its original ID to maintain cross-chunk identity.
        let mut boundary_recovered = 0;
        for (&old_local, &original) in &boundary_originals {
            if let Some(&new_local) = compaction.vertex_map.get(&old_local) {
                if !chunk.local_to_original.contains_key(&new_local) {
                    chunk.local_to_original.insert(new_local, original);
                    chunk.original_to_local.insert(original, new_local);
                    boundary_recovered += 1;
                }
            }
        }
        if boundary_recovered > 0 {
            tracing::warn!(
                "collapse_pass: recovered {} boundary vertex mappings that \
                 would have been lost during compact",
                boundary_recovered
            );
        }

        // Safety net: ensure ALL mesh vertices have local_to_original mappings.
        // After compact(), some vertices may lose their mapping if compact's liveness
        // detection differs from the mapping update (e.g., degenerate face removal
        // or defensive half-edge skipping). Missing mappings cause merge_chunks to
        // skip faces, creating visible holes in the mesh.
        repair_vertex_mappings(chunk, next_original_vertex_id);
    }

    actual_collapses
}

/// Apply tangent-plane smoothing to newly created split vertices and their 1-ring neighbors.
///
/// For each vertex, computes the Laplacian (average of neighbor positions), projects
/// the displacement onto the tangent plane (defined by the vertex normal), and applies
/// it with the given strength. This prevents spiky/noisy mesh after subdivision while
/// preserving surface curvature.
///
/// Adapted from SculptGL Subdivision.js tangent smooth pass (lines 419-424).
fn tangent_smooth_new_vertices(mesh: &mut HalfEdgeMesh, new_vertices: &[VertexId], strength: f32) {
    if new_vertices.is_empty() {
        return;
    }

    // Expand to include 1-ring neighbors of new vertices
    let mut vertices_to_smooth: HashSet<VertexId> = HashSet::new();
    for &vid in new_vertices {
        vertices_to_smooth.insert(vid);
        for neighbor in mesh.get_adjacent_vertices(vid) {
            vertices_to_smooth.insert(neighbor);
        }
    }

    // Read pass: calculate target positions
    let mut new_positions: Vec<(VertexId, Vec3)> = Vec::new();

    for &vid in &vertices_to_smooth {
        let Some(vertex) = mesh.vertex(vid) else { continue };
        let pos = vertex.position;
        let normal = vertex.normal.normalize_or_zero();

        // Skip if normal is zero (can't define tangent plane)
        if normal.length_squared() < 0.001 {
            continue;
        }

        let neighbors = mesh.get_adjacent_vertices(vid);
        if neighbors.is_empty() {
            continue;
        }

        // Laplacian: average of neighbor positions
        let mut avg = Vec3::ZERO;
        let mut count = 0;
        for nid in &neighbors {
            if let Some(n) = mesh.vertex(*nid) {
                avg += n.position;
                count += 1;
            }
        }
        if count == 0 {
            continue;
        }
        avg /= count as f32;

        // Project displacement onto tangent plane
        let displacement = avg - pos;
        let tangent_displacement = displacement - normal * displacement.dot(normal);

        let smoothed = pos + tangent_displacement * strength;
        new_positions.push((vid, smoothed));
    }

    // Write pass: apply positions
    for (vid, new_pos) in new_positions {
        mesh.set_vertex_position(vid, new_pos);
    }
}

/// Statistics from a tessellation pass.
#[derive(Debug, Default, Clone, Copy)]
pub struct TessellationStats {
    /// Number of edges that were split
    pub edges_split: usize,
    /// Number of edges that were collapsed
    pub edges_collapsed: usize,
}

/// Collect all edge IDs that have at least one vertex within the given radius.
///
/// Uses a face-based approach: for each vertex in range, find all faces
/// containing that vertex and collect their edges. This is more robust
/// than twin→next traversal for meshes with boundary edges or complex topology.
fn collect_edges_in_range(
    chunk: &MeshChunk,
    center: Vec3,
    radius: f32,
) -> HashSet<HalfEdgeId> {
    let radius_sq = radius * radius;
    let mut edges = HashSet::new();

    for vertex in chunk.mesh.vertices() {
        let dist_sq = vertex.position.distance_squared(center);
        if dist_sq <= radius_sq {
            // Get all faces adjacent to this vertex
            let faces = chunk.mesh.get_vertex_faces(vertex.id);

            // Collect all half-edges from these faces that originate from this vertex
            for face_id in faces {
                for he_id in chunk.mesh.get_face_half_edges(face_id) {
                    if let Some(he) = chunk.mesh.half_edge(he_id) {
                        if he.origin == vertex.id {
                            edges.insert(he_id);
                        }
                    }
                }
            }
        }
    }

    edges
}

/// Evaluate a single edge for tessellation decision.
fn evaluate_edge_in_chunk(
    chunk: &MeshChunk,
    edge_id: HalfEdgeId,
    config: &TessellationConfig,
    screen_config: &ScreenSpaceConfig,
) -> EdgeEvaluation {
    let Some(he) = chunk.mesh.half_edge(edge_id) else {
        return EdgeEvaluation {
            screen_length: 0.0,
            decision: TessellationDecision::None,
        };
    };

    let v0_id = he.origin;
    let v1_id = match chunk.mesh.half_edge(he.next) {
        Some(next_he) => next_he.origin,
        None => return EdgeEvaluation {
            screen_length: 0.0,
            decision: TessellationDecision::None,
        },
    };

    let v0_pos = match chunk.mesh.vertex(v0_id) {
        Some(v) => v.position,
        None => return EdgeEvaluation {
            screen_length: 0.0,
            decision: TessellationDecision::None,
        },
    };

    let v1_pos = match chunk.mesh.vertex(v1_id) {
        Some(v) => v.position,
        None => return EdgeEvaluation {
            screen_length: 0.0,
            decision: TessellationDecision::None,
        },
    };

    evaluate_edge(v0_pos, v1_pos, config, screen_config)
}

/// Ensure all mesh vertices have `local_to_original` mappings.
///
/// After `compact()`, some vertices may lose their mapping if the compaction's
/// liveness detection disagrees with the mapping update. For example:
/// - Degenerate face removal can orphan vertices that are later re-linked
/// - Defensive half-edge skipping in compact can desync the vertex map
/// - Edge flip fallback can create new topology not tracked by the mapping
///
/// Missing mappings cause `merge_chunks` to skip faces referencing those vertices,
/// creating visible holes in the rendered mesh.
///
/// This function scans all mesh vertices and assigns new globally unique original
/// IDs to any that lack a mapping.
fn repair_vertex_mappings(chunk: &mut MeshChunk, next_original_vertex_id: &mut u32) {
    let mut repaired_interior = 0;
    let mut recovered_boundary = 0;
    for i in 0..chunk.mesh.vertex_count() {
        let vid = VertexId(i as u32);
        if !chunk.local_to_original.contains_key(&vid) {
            // First, try to recover the original ID from boundary vertex info.
            // Boundary vertices store the original_vertex_id in their references,
            // so we can recover the cross-chunk identity even if the mapping was lost.
            if let Some(boundary_refs) = chunk.boundary_vertices.get(&vid) {
                if let Some(first_ref) = boundary_refs.first() {
                    chunk
                        .local_to_original
                        .insert(vid, first_ref.original_vertex_id);
                    chunk
                        .original_to_local
                        .insert(first_ref.original_vertex_id, vid);
                    recovered_boundary += 1;
                    continue;
                }
            }
            // Interior vertex: assign a new globally unique ID
            let unique_original_id = VertexId(*next_original_vertex_id);
            *next_original_vertex_id += 1;
            chunk.local_to_original.insert(vid, unique_original_id);
            chunk.original_to_local.insert(unique_original_id, vid);
            repaired_interior += 1;
        }
    }
    if recovered_boundary > 0 {
        tracing::error!(
            "repair_vertex_mappings: recovered {} BOUNDARY vertex mappings from \
             boundary_vertices info. This indicates compact incorrectly removed \
             boundary vertices. Without recovery, these would cause mesh tearing.",
            recovered_boundary
        );
    }
    if repaired_interior > 0 {
        tracing::warn!(
            "repair_vertex_mappings: assigned {} missing interior mappings \
             ({} total vertices). This indicates a mapping bug in collapse/compact.",
            repaired_interior,
            chunk.mesh.vertex_count()
        );
    }
}

/// Validate that a chunk mesh is fully traversable after tessellation.
/// This checks things that validate_connectivity() doesn't:
/// 1. All faces can be fully traversed and return 3 vertices
/// 2. All face vertex IDs exist in the vertex array
/// 3. All face vertices have local_to_original mappings
/// 4. Twin edges point in opposite directions
#[allow(dead_code)]
pub fn validate_chunk_after_tessellation(chunk: &MeshChunk) -> Result<(), String> {
    use tracing::warn;

    // Check 1: All faces are traversable with correct vertex count
    for i in 0..chunk.mesh.face_count() {
        let face_id = painting::half_edge::FaceId(i as u32);

        // Skip faces orphaned by edge collapse (should not exist after compact(),
        // but kept as a safety net)
        if !chunk.mesh.is_face_valid(face_id) {
            continue;
        }

        let verts = chunk.mesh.get_face_vertices(face_id);

        if verts.len() < 3 {
            return Err(format!(
                "Face {:?} has {} vertices (expected 3). Face half_edge = {:?}",
                face_id,
                verts.len(),
                chunk.mesh.face(face_id).map(|f| f.half_edge)
            ));
        }

        // Check 2: All vertex IDs are valid
        for vid in &verts {
            if chunk.mesh.vertex(*vid).is_none() {
                return Err(format!(
                    "Face {:?} references non-existent vertex {:?}",
                    face_id, vid
                ));
            }
        }

        // Check 3: All vertices have local_to_original mappings
        for vid in &verts {
            if !chunk.local_to_original.contains_key(vid) {
                warn!(
                    "Face {:?} vertex {:?} has no local_to_original mapping. \
                     This will cause the vertex to be missing after merge.",
                    face_id, vid
                );
                return Err(format!(
                    "Face {:?} vertex {:?} missing from local_to_original",
                    face_id, vid
                ));
            }
        }
    }

    // Check 4: Twin edges point in opposite directions
    for he in chunk.mesh.half_edges() {
        if he.face.is_none() {
            continue; // Skip orphaned
        }

        if let Some(twin_id) = he.twin {
            if let Some(twin) = chunk.mesh.half_edge(twin_id) {
                // Get destinations
                let he_dest = chunk.mesh.half_edge(he.next).map(|n| n.origin);
                let twin_dest = chunk.mesh.half_edge(twin.next).map(|n| n.origin);

                // he goes A→B, twin should go B→A
                // So he.origin = A, he_dest = B, twin.origin = B, twin_dest = A
                if Some(he.origin) != twin_dest || he_dest != Some(twin.origin) {
                    return Err(format!(
                        "Half-edge {:?} ({:?}→{:?}) has invalid twin {:?} ({:?}→{:?}). \
                         Twins should go in opposite directions.",
                        he.id, he.origin, he_dest,
                        twin_id, twin.origin, twin_dest
                    ));
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tessellation_decision() {
        assert_eq!(TessellationDecision::Split, TessellationDecision::Split);
        assert_ne!(TessellationDecision::Split, TessellationDecision::Collapse);
    }

    #[test]
    fn test_tessellation_stats_default() {
        let stats = TessellationStats::default();
        assert_eq!(stats.edges_split, 0);
        assert_eq!(stats.edges_collapsed, 0);
    }
}
