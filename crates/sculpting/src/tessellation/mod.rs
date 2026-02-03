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

use crate::chunking::MeshChunk;
use crate::types::TessellationConfig;
use glam::Vec3;
use painting::half_edge::{HalfEdgeId, VertexId};
use std::collections::HashSet;
use tracing::{info, trace};

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
/// This evaluates all edges near the brush and applies split/collapse
/// as needed to maintain target edge density.
///
/// The `next_original_vertex_id` counter is used to assign globally unique
/// "original" IDs to tessellation-created vertices. This prevents ID collisions
/// when chunks are later merged.
///
/// Returns the number of edges modified.
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

    // Check if chunk is already at max face limit - skip all tessellation
    let current_face_count = chunk.mesh.face_count();
    if current_face_count >= config.max_faces_per_chunk {
        info!(
            "tessellate: at max faces ({} >= {}), skipping splits",
            current_face_count, config.max_faces_per_chunk
        );
        return stats;
    }

    // Collect edges to evaluate (those with at least one vertex in range)
    trace!("tessellate_at_brush: collecting edges in range");
    let edges_to_evaluate = collect_edges_in_range(chunk, brush_center, influence_radius);
    trace!(
        "tessellate_at_brush: found {} edges to evaluate",
        edges_to_evaluate.len()
    );

    // Split pass first (splitting creates new edges that might need collapsing)
    trace!("tessellate_at_brush: evaluating for splits");
    let mut edges_to_split: Vec<HalfEdgeId> = Vec::new();

    for &edge_id in &edges_to_evaluate {
        let eval = evaluate_edge_in_chunk(chunk, edge_id, config, screen_config);
        if eval.decision == TessellationDecision::Split {
            edges_to_split.push(edge_id);
        }
    }
    trace!(
        "tessellate_at_brush: {} edges marked for split",
        edges_to_split.len()
    );

    // Sort for determinism
    edges_to_split.sort_by_key(|e| e.0);

    // Limit the number of splits per dab to prevent exponential growth
    let mut actual_splits = 0;
    for edge_id in edges_to_split.into_iter() {
        // Check max splits per dab limit
        if actual_splits >= config.max_splits_per_dab {
            info!(
                "tessellate: reached max splits per dab ({}), stopping",
                config.max_splits_per_dab
            );
            break;
        }

        // Check max faces per chunk limit before each split
        if chunk.mesh.face_count() >= config.max_faces_per_chunk {
            info!(
                "tessellate: reached max faces ({}), stopping splits",
                config.max_faces_per_chunk
            );
            break;
        }

        // CRITICAL: Skip boundary edges - splitting them would desync chunks!
        // When edge AB is shared between chunk A and chunk B, splitting it in
        // chunk A creates a midpoint vertex that chunk B doesn't know about.
        // This causes mesh tearing when chunks are merged/rendered.
        let he = match chunk.mesh.half_edge(edge_id) {
            Some(he) => he,
            None => continue,
        };
        let v0_id = he.origin;
        let v1_id = match chunk.mesh.half_edge(he.next) {
            Some(next_he) => next_he.origin,
            None => continue,
        };

        let v0_is_boundary = chunk.boundary_vertices.contains_key(&v0_id);
        let v1_is_boundary = chunk.boundary_vertices.contains_key(&v1_id);
        if v0_is_boundary || v1_is_boundary {
            trace!(
                "tessellate: skipping boundary edge {:?} (v0_boundary={}, v1_boundary={})",
                edge_id, v0_is_boundary, v1_is_boundary
            );
            continue;
        }

        trace!("tessellate_at_brush: splitting edge {:?}", edge_id);
        if let Some(split_result) = split_edge(&mut chunk.mesh, edge_id) {
            trace!(
                "tessellate_at_brush: split successful, new vertex {:?}",
                split_result.new_vertex
            );
            stats.edges_split += 1;
            actual_splits += 1;
            chunk.topology_changed = true;

            // CRITICAL: Register new vertex in chunk mappings with a GLOBALLY UNIQUE ID.
            // Each tessellation-created vertex gets a unique "original" ID from the counter.
            // This prevents ID collisions when chunks are merged - without this, vertices
            // in different chunks with the same local ID would incorrectly merge into one.
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

    // Collapse pass (after splits, some edges may be too short)
    // Skip collapse if disabled in config (collapse algorithm creates non-manifold geometry)
    if !config.collapse_enabled {
        trace!("tessellate_at_brush: collapse disabled in config");
        trace!("tessellate_at_brush: END stats={:?}", stats);
        return stats;
    }

    // Check if we're already at minimum face count
    let current_face_count = chunk.mesh.face_count();
    if current_face_count <= config.min_faces {
        trace!(
            "tessellate_at_brush: skipping collapse pass - at minimum face count ({} <= {})",
            current_face_count,
            config.min_faces
        );
        trace!("tessellate_at_brush: END stats={:?}", stats);
        return stats;
    }

    trace!("tessellate_at_brush: re-collecting edges after splits");
    let edges_to_evaluate = collect_edges_in_range(chunk, brush_center, influence_radius);
    trace!(
        "tessellate_at_brush: found {} edges for collapse evaluation",
        edges_to_evaluate.len()
    );
    let mut edges_to_collapse: Vec<HalfEdgeId> = Vec::new();

    for &edge_id in &edges_to_evaluate {
        let eval = evaluate_edge_in_chunk(chunk, edge_id, config, screen_config);
        if eval.decision == TessellationDecision::Collapse {
            // Use the new comprehensive safety check
            let check = can_collapse_edge_safe(&chunk.mesh, edge_id);
            match check {
                CollapseCheck::Safe(_) | CollapseCheck::UseEdgeFlip => {
                    edges_to_collapse.push(edge_id);
                }
                CollapseCheck::Rejected(_) => {
                    // Edge cannot be collapsed or flipped
                }
            }
        }
    }
    trace!(
        "tessellate_at_brush: {} edges marked for collapse",
        edges_to_collapse.len()
    );

    // Sort for determinism
    edges_to_collapse.sort_by_key(|e| e.0);

    for edge_id in edges_to_collapse {
        // Check minimum face count before EACH collapse
        if chunk.mesh.face_count() <= config.min_faces {
            trace!(
                "tessellate_at_brush: stopping collapse - reached minimum face count ({})",
                config.min_faces
            );
            break;
        }

        // CRITICAL: Skip boundary edges - collapsing them would desync chunks!
        // Same reasoning as for splits: boundary edges are shared between chunks.
        let he = match chunk.mesh.half_edge(edge_id) {
            Some(he) => he,
            None => continue,
        };
        let v0_id = he.origin;
        let v1_id = match chunk.mesh.half_edge(he.next) {
            Some(next_he) => next_he.origin,
            None => continue,
        };

        if chunk.boundary_vertices.contains_key(&v0_id)
            || chunk.boundary_vertices.contains_key(&v1_id)
        {
            trace!(
                "tessellate: skipping boundary edge collapse {:?}",
                edge_id
            );
            continue;
        }

        // Use collapse_or_flip_edge which handles edge flip fallback
        trace!("tessellate_at_brush: attempting collapse/flip on edge {:?}", edge_id);
        match collapse_or_flip_edge(&mut chunk.mesh, edge_id) {
            Some(CollapseOrFlipResult::Collapsed(_)) => {
                trace!("tessellate_at_brush: collapse successful");
                stats.edges_collapsed += 1;
                chunk.topology_changed = true;
            }
            Some(CollapseOrFlipResult::Flipped) => {
                trace!("tessellate_at_brush: edge flip applied instead of collapse");
                // Edge flip doesn't reduce face count, but it improves mesh quality
                chunk.topology_changed = true;
            }
            None => {
                trace!("tessellate_at_brush: edge {:?} rejected for collapse/flip", edge_id);
            }
        }
    }

    trace!("tessellate_at_brush: END stats={:?}", stats);

    // Validate chunk after tessellation in debug builds
    #[cfg(debug_assertions)]
    if stats.edges_split > 0 || stats.edges_collapsed > 0 {
        if let Err(e) = validate_chunk_after_tessellation(chunk) {
            tracing::error!("CHUNK VALIDATION FAILED after tessellation: {}", e);
            // Log additional diagnostic info
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
