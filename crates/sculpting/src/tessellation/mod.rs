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

pub use edge_collapse::{can_collapse_edge, collapse_edge, CollapseResult};
pub use edge_split::{split_edge, SplitResult};
pub use metrics::{
    calculate_edge_screen_length, evaluate_edge, EdgeEvaluation, ScreenSpaceConfig,
};

use crate::chunking::MeshChunk;
use crate::types::TessellationConfig;
use glam::Vec3;
use painting::half_edge::HalfEdgeId;
use std::collections::HashSet;

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
/// Returns the number of edges modified.
pub fn tessellate_at_brush(
    chunk: &mut MeshChunk,
    brush_center: Vec3,
    brush_radius: f32,
    config: &TessellationConfig,
    screen_config: &ScreenSpaceConfig,
) -> TessellationStats {
    let mut stats = TessellationStats::default();
    let influence_radius = brush_radius * 1.5;

    // Collect edges to evaluate (those with at least one vertex in range)
    let edges_to_evaluate = collect_edges_in_range(chunk, brush_center, influence_radius);

    // Split pass first (splitting creates new edges that might need collapsing)
    let mut edges_to_split: Vec<HalfEdgeId> = Vec::new();

    for &edge_id in &edges_to_evaluate {
        let eval = evaluate_edge_in_chunk(chunk, edge_id, config, screen_config);
        if eval.decision == TessellationDecision::Split {
            edges_to_split.push(edge_id);
        }
    }

    // Sort for determinism
    edges_to_split.sort_by_key(|e| e.0);

    for edge_id in edges_to_split {
        if split_edge(&mut chunk.mesh, edge_id).is_some() {
            stats.edges_split += 1;
            chunk.topology_changed = true;
        }
    }

    // Collapse pass (after splits, some edges may be too short)
    let edges_to_evaluate = collect_edges_in_range(chunk, brush_center, influence_radius);
    let mut edges_to_collapse: Vec<HalfEdgeId> = Vec::new();

    for &edge_id in &edges_to_evaluate {
        let eval = evaluate_edge_in_chunk(chunk, edge_id, config, screen_config);
        if eval.decision == TessellationDecision::Collapse {
            if can_collapse_edge(&chunk.mesh, edge_id) {
                edges_to_collapse.push(edge_id);
            }
        }
    }

    // Sort for determinism
    edges_to_collapse.sort_by_key(|e| e.0);

    for edge_id in edges_to_collapse {
        // Re-check validity (previous collapses may have invalidated this edge)
        if can_collapse_edge(&chunk.mesh, edge_id) {
            if collapse_edge(&mut chunk.mesh, edge_id).is_some() {
                stats.edges_collapsed += 1;
                chunk.topology_changed = true;
            }
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
            // Add all edges connected to this vertex
            if let Some(start_he) = vertex.outgoing_half_edge {
                let mut current = start_he;
                loop {
                    edges.insert(current);

                    // Move to next outgoing edge via twin->next
                    let he = match chunk.mesh.half_edge(current) {
                        Some(he) => he,
                        None => break,
                    };

                    let twin = match he.twin {
                        Some(t) => t,
                        None => break,
                    };

                    let twin_he = match chunk.mesh.half_edge(twin) {
                        Some(t) => t,
                        None => break,
                    };

                    current = twin_he.next;
                    if current == start_he {
                        break;
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
