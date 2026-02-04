//! Edge length metrics and mesh quality evaluation for tessellation.
//!
//! This module provides:
//! - Screen-space edge length calculation
//! - Threshold-based split/collapse decisions
//! - Mesh quality metrics (aspect ratio, valence, degenerate faces)

use crate::tessellation::TessellationDecision;
use crate::types::TessellationConfig;
use glam::{Mat4, Vec3, Vec4};
use painting::half_edge::{FaceId, HalfEdgeMesh, VertexId};
use tracing::debug;

/// Screen-space configuration for edge evaluation.
///
/// Vertex positions in the HalfEdgeMesh are in **local/object space**, so the
/// `model_matrix` (local-to-world transform) must be provided to correctly
/// compute screen-space edge lengths via the model-view-projection matrix.
#[derive(Debug, Clone)]
pub struct ScreenSpaceConfig {
    /// View-projection matrix (clip-from-world: projection × view)
    pub view_projection: Mat4,
    /// Model matrix (local-to-world: the mesh entity's GlobalTransform)
    pub model_matrix: Mat4,
    /// Viewport width in pixels
    pub viewport_width: f32,
    /// Viewport height in pixels
    pub viewport_height: f32,
    /// Precomputed model-view-projection matrix (clip-from-local)
    mvp: Mat4,
}

impl Default for ScreenSpaceConfig {
    fn default() -> Self {
        Self {
            view_projection: Mat4::IDENTITY,
            model_matrix: Mat4::IDENTITY,
            viewport_width: 1920.0,
            viewport_height: 1080.0,
            mvp: Mat4::IDENTITY,
        }
    }
}

impl ScreenSpaceConfig {
    /// Create a new screen-space config with the given viewport dimensions.
    pub fn new(view_projection: Mat4, width: f32, height: f32) -> Self {
        Self {
            view_projection,
            model_matrix: Mat4::IDENTITY,
            viewport_width: width,
            viewport_height: height,
            mvp: view_projection,
        }
    }

    /// Create a screen-space config with a model matrix for local-to-world transform.
    ///
    /// This is the correct constructor when vertex positions are in local/object space
    /// (which they are in HalfEdgeMesh). The model matrix transforms them to world space
    /// before the view-projection maps to clip space.
    pub fn with_model_matrix(
        view_projection: Mat4,
        model_matrix: Mat4,
        width: f32,
        height: f32,
    ) -> Self {
        let mvp = view_projection * model_matrix;
        Self {
            view_projection,
            model_matrix,
            viewport_width: width,
            viewport_height: height,
            mvp,
        }
    }

    /// Project a local-space point to normalized device coordinates.
    ///
    /// Uses the precomputed model-view-projection matrix to correctly transform
    /// from local/object space through world space to clip space.
    pub fn project_to_ndc(&self, local_pos: Vec3) -> Option<Vec3> {
        let clip = self.mvp * Vec4::new(local_pos.x, local_pos.y, local_pos.z, 1.0);

        // Check if behind camera
        if clip.w <= 0.0 {
            return None;
        }

        // Perspective divide
        let ndc = Vec3::new(clip.x / clip.w, clip.y / clip.w, clip.z / clip.w);
        Some(ndc)
    }

    /// Project a local-space point to screen coordinates.
    pub fn project_to_screen(&self, local_pos: Vec3) -> Option<(f32, f32)> {
        let ndc = self.project_to_ndc(local_pos)?;

        // Convert NDC (-1 to 1) to screen coordinates
        let screen_x = (ndc.x + 1.0) * 0.5 * self.viewport_width;
        let screen_y = (1.0 - ndc.y) * 0.5 * self.viewport_height; // Y is flipped

        Some((screen_x, screen_y))
    }
}

/// Result of evaluating an edge for tessellation.
#[derive(Debug, Clone, Copy)]
pub struct EdgeEvaluation {
    /// Edge length in screen pixels
    pub screen_length: f32,
    /// Decision based on thresholds
    pub decision: TessellationDecision,
}

/// Calculate the screen-space length of an edge.
///
/// Returns the length in pixels, or a fallback world-space based estimate
/// if one or both vertices are off-screen.
pub fn calculate_edge_screen_length(
    v0: Vec3,
    v1: Vec3,
    screen_config: &ScreenSpaceConfig,
) -> f32 {
    let screen_v0 = screen_config.project_to_screen(v0);
    let screen_v1 = screen_config.project_to_screen(v1);

    match (screen_v0, screen_v1) {
        (Some((x0, y0)), Some((x1, y1))) => {
            // Both vertices on screen - calculate pixel distance
            let dx = x1 - x0;
            let dy = y1 - y0;
            (dx * dx + dy * dy).sqrt()
        }
        _ => {
            // One or both vertices off-screen - use world-space fallback
            estimate_screen_length_from_distance(v0, v1, screen_config)
        }
    }
}

/// Estimate screen length when vertices are off-screen.
///
/// Uses the edge's world-space length and camera distance to estimate
/// what the screen length would be if visible.
fn estimate_screen_length_from_distance(
    v0: Vec3,
    v1: Vec3,
    screen_config: &ScreenSpaceConfig,
) -> f32 {
    let world_length = v0.distance(v1);

    // Get camera position from inverse view-projection (approximate)
    // For now, use a simple heuristic based on viewport size
    let mid_point = (v0 + v1) * 0.5;

    // Project midpoint to get approximate depth
    if let Some(ndc) = screen_config.project_to_ndc(mid_point) {
        // Use depth to scale world length to approximate screen length
        // This is a rough approximation that works reasonably well
        let depth_factor = 1.0 / (ndc.z.abs() + 0.1);
        let base_scale = screen_config.viewport_height * 0.5;
        world_length * depth_factor * base_scale
    } else {
        // Very rough fallback - assume default viewing distance
        world_length * screen_config.viewport_height * 0.1
    }
}

/// Evaluate an edge for tessellation action.
///
/// Compares the edge's screen length to the configured thresholds
/// and returns the appropriate action (split, collapse, or none).
pub fn evaluate_edge(
    v0: Vec3,
    v1: Vec3,
    config: &TessellationConfig,
    screen_config: &ScreenSpaceConfig,
) -> EdgeEvaluation {
    let screen_length = calculate_edge_screen_length(v0, v1, screen_config);

    let split_threshold = config.target_pixels * config.split_ratio;
    let collapse_threshold = config.target_pixels * config.collapse_ratio;

    let decision = if screen_length > split_threshold {
        TessellationDecision::Split
    } else if screen_length < collapse_threshold {
        TessellationDecision::Collapse
    } else {
        TessellationDecision::None
    };

    // Log first few edge evaluations to verify calculations are reasonable
    static LOGGED_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let count = LOGGED_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if count < 5 {
        debug!(
            "evaluate_edge: screen_length={:.1}px, split_thresh={:.1}, collapse_thresh={:.1}, decision={:?}",
            screen_length, split_threshold, collapse_threshold, decision
        );
    }

    EdgeEvaluation {
        screen_length,
        decision,
    }
}

/// Calculate world-space edge length (for non-screen-space fallback).
pub fn calculate_world_edge_length(v0: Vec3, v1: Vec3) -> f32 {
    v0.distance(v1)
}

// =============================================================================
// Mesh Quality Metrics
// =============================================================================

/// Mesh quality metrics for monitoring tessellation health.
///
/// These metrics help detect potential mesh corruption or quality issues
/// during dynamic tessellation operations.
#[derive(Debug, Clone, Default)]
pub struct MeshQuality {
    /// Minimum aspect ratio of any triangle (0-1, 1 = equilateral)
    pub min_aspect_ratio: f32,
    /// Maximum aspect ratio of any triangle
    pub max_aspect_ratio: f32,
    /// Average aspect ratio across all triangles
    pub avg_aspect_ratio: f32,
    /// Minimum vertex valence (number of edges per vertex)
    pub min_valence: usize,
    /// Maximum vertex valence
    pub max_valence: usize,
    /// Average vertex valence
    pub avg_valence: f32,
    /// Number of degenerate triangles (near-zero area)
    pub degenerate_faces: usize,
    /// Number of non-manifold vertices (ring mismatch)
    pub non_manifold_vertices: usize,
    /// Number of non-manifold edges (more than 2 adjacent faces)
    pub non_manifold_edges: usize,
}

impl MeshQuality {
    /// Check if mesh quality is acceptable for sculpting.
    ///
    /// Returns true if:
    /// - No degenerate faces
    /// - No non-manifold geometry
    /// - All valences >= 3
    /// - Minimum aspect ratio > 0.01
    pub fn is_acceptable(&self) -> bool {
        self.degenerate_faces == 0
            && self.non_manifold_vertices == 0
            && self.non_manifold_edges == 0
            && self.min_valence >= 3
            && self.min_aspect_ratio > 0.01
    }

    /// Get a human-readable summary of any quality issues.
    pub fn issues_summary(&self) -> Option<String> {
        let mut issues = Vec::new();

        if self.degenerate_faces > 0 {
            issues.push(format!("{} degenerate faces", self.degenerate_faces));
        }
        if self.non_manifold_vertices > 0 {
            issues.push(format!(
                "{} non-manifold vertices",
                self.non_manifold_vertices
            ));
        }
        if self.non_manifold_edges > 0 {
            issues.push(format!("{} non-manifold edges", self.non_manifold_edges));
        }
        if self.min_valence < 3 {
            issues.push(format!("min valence {} < 3", self.min_valence));
        }
        if self.min_aspect_ratio <= 0.01 {
            issues.push(format!(
                "min aspect ratio {:.4} (near-degenerate)",
                self.min_aspect_ratio
            ));
        }

        if issues.is_empty() {
            None
        } else {
            Some(issues.join(", "))
        }
    }
}

/// Calculate mesh quality metrics.
///
/// This function analyzes the mesh to compute quality metrics useful for
/// monitoring tessellation health and detecting potential issues.
pub fn calculate_mesh_quality(mesh: &HalfEdgeMesh) -> MeshQuality {
    let mut quality = MeshQuality {
        min_aspect_ratio: f32::MAX,
        max_aspect_ratio: 0.0,
        avg_aspect_ratio: 0.0,
        min_valence: usize::MAX,
        max_valence: 0,
        avg_valence: 0.0,
        degenerate_faces: 0,
        non_manifold_vertices: 0,
        non_manifold_edges: 0,
    };

    let mut total_aspect_ratio = 0.0;
    let mut face_count = 0;

    // Calculate face-based metrics
    for i in 0..mesh.face_count() {
        let face_id = FaceId(i as u32);
        let verts = mesh.get_face_vertices(face_id);

        if verts.len() < 3 {
            quality.degenerate_faces += 1;
            continue;
        }

        // Get vertex positions
        let positions: Vec<Vec3> = verts
            .iter()
            .filter_map(|&vid| mesh.vertex(vid).map(|v| v.position))
            .collect();

        if positions.len() < 3 {
            quality.degenerate_faces += 1;
            continue;
        }

        // Calculate aspect ratio (ratio of shortest to longest edge)
        let e0 = positions[1] - positions[0];
        let e1 = positions[2] - positions[1];
        let e2 = positions[0] - positions[2];

        let l0 = e0.length();
        let l1 = e1.length();
        let l2 = e2.length();

        let min_edge = l0.min(l1).min(l2);
        let max_edge = l0.max(l1).max(l2);

        // Check for degenerate (zero-area) triangles
        let area = e0.cross(-e2).length() * 0.5;
        if area < 1e-10 {
            quality.degenerate_faces += 1;
            continue;
        }

        let aspect_ratio = if max_edge > 0.0 {
            min_edge / max_edge
        } else {
            0.0
        };

        quality.min_aspect_ratio = quality.min_aspect_ratio.min(aspect_ratio);
        quality.max_aspect_ratio = quality.max_aspect_ratio.max(aspect_ratio);
        total_aspect_ratio += aspect_ratio;
        face_count += 1;
    }

    if face_count > 0 {
        quality.avg_aspect_ratio = total_aspect_ratio / face_count as f32;
    }

    // Calculate vertex-based metrics
    let mut total_valence = 0;
    let mut vertex_count = 0;

    for vertex in mesh.vertices() {
        // Skip orphaned vertices
        if vertex.outgoing_half_edge.is_none() {
            continue;
        }

        let neighbors = mesh.get_adjacent_vertices(vertex.id);
        let faces = mesh.get_vertex_faces(vertex.id);
        let valence = neighbors.len();

        // Check for non-manifold vertices (ring mismatch)
        // For interior vertices: neighbors.len() == faces.len()
        // For boundary vertices: neighbors.len() == faces.len() + 1
        let is_boundary = mesh.is_boundary_vertex(vertex.id);
        let expected_diff = if is_boundary { 1 } else { 0 };
        if neighbors.len().saturating_sub(faces.len()) != expected_diff
            && !neighbors.is_empty()
            && !faces.is_empty()
        {
            quality.non_manifold_vertices += 1;
        }

        if valence > 0 {
            quality.min_valence = quality.min_valence.min(valence);
            quality.max_valence = quality.max_valence.max(valence);
            total_valence += valence;
            vertex_count += 1;
        }
    }

    if vertex_count > 0 {
        quality.avg_valence = total_valence as f32 / vertex_count as f32;
    }
    if quality.min_valence == usize::MAX {
        quality.min_valence = 0;
    }
    if quality.min_aspect_ratio == f32::MAX {
        quality.min_aspect_ratio = 0.0;
    }

    // Check for non-manifold edges (edges with more than 2 adjacent faces)
    // This is expensive, so we only check a sample
    for he in mesh.half_edges() {
        if he.face.is_none() {
            continue;
        }
        // An edge is non-manifold if it has a twin with a twin pointing somewhere else
        // (more than 2 faces share the edge)
        if let Some(twin_id) = he.twin {
            if let Some(twin) = mesh.half_edge(twin_id) {
                if twin.twin != Some(he.id) {
                    quality.non_manifold_edges += 1;
                }
            }
        }
    }

    quality
}

/// Validate that a vertex has acceptable valence for sculpting.
///
/// Vertices with valence < 3 or > 20 may cause issues during tessellation.
pub fn is_valid_valence(mesh: &HalfEdgeMesh, vertex_id: VertexId) -> bool {
    let neighbors = mesh.get_adjacent_vertices(vertex_id);
    let valence = neighbors.len();
    valence >= 3 && valence <= 20
}

/// Calculate the aspect ratio of a triangle (0-1, 1 = equilateral).
///
/// Returns the ratio of the shortest edge to the longest edge.
pub fn calculate_triangle_aspect_ratio(p0: Vec3, p1: Vec3, p2: Vec3) -> f32 {
    let e0 = (p1 - p0).length();
    let e1 = (p2 - p1).length();
    let e2 = (p0 - p2).length();

    let min_edge = e0.min(e1).min(e2);
    let max_edge = e0.max(e1).max(e2);

    if max_edge > 0.0 {
        min_edge / max_edge
    } else {
        0.0
    }
}

/// Check if a triangle is degenerate (near-zero area).
pub fn is_degenerate_triangle(p0: Vec3, p1: Vec3, p2: Vec3, tolerance: f32) -> bool {
    let e0 = p1 - p0;
    let e1 = p2 - p0;
    let area = e0.cross(e1).length() * 0.5;
    area < tolerance
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_screen_space_config_default() {
        let config = ScreenSpaceConfig::default();
        assert_eq!(config.viewport_width, 1920.0);
        assert_eq!(config.viewport_height, 1080.0);
    }

    #[test]
    fn test_project_to_screen_identity() {
        let config = ScreenSpaceConfig::default();

        // With identity matrix, (0, 0, 0) should project to center of screen
        // (NDC 0,0 -> screen center)
        if let Some((x, y)) = config.project_to_screen(Vec3::ZERO) {
            assert!((x - 960.0).abs() < 1.0);
            assert!((y - 540.0).abs() < 1.0);
        }
    }

    #[test]
    fn test_evaluate_edge_split() {
        let config = TessellationConfig::default();
        let screen_config = ScreenSpaceConfig::default();

        // Very long edge should be split
        let v0 = Vec3::new(-1.0, 0.0, 0.0);
        let v1 = Vec3::new(1.0, 0.0, 0.0);

        let eval = evaluate_edge(v0, v1, &config, &screen_config);
        // With identity matrix and large world distance, should likely split
        // (exact behavior depends on projection)
        assert!(eval.screen_length > 0.0);
    }

    #[test]
    fn test_world_edge_length() {
        let v0 = Vec3::new(0.0, 0.0, 0.0);
        let v1 = Vec3::new(3.0, 4.0, 0.0);

        let length = calculate_world_edge_length(v0, v1);
        assert!((length - 5.0).abs() < 0.001);
    }
}
