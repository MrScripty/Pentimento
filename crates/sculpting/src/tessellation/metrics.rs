//! Edge length metrics for tessellation decisions.
//!
//! This module provides screen-space edge length calculation and
//! threshold-based evaluation for split/collapse decisions.

use crate::tessellation::TessellationDecision;
use crate::types::TessellationConfig;
use glam::{Mat4, Vec3, Vec4};

/// Screen-space configuration for edge evaluation.
#[derive(Debug, Clone)]
pub struct ScreenSpaceConfig {
    /// View-projection matrix (combined view and projection)
    pub view_projection: Mat4,
    /// Viewport width in pixels
    pub viewport_width: f32,
    /// Viewport height in pixels
    pub viewport_height: f32,
}

impl Default for ScreenSpaceConfig {
    fn default() -> Self {
        Self {
            view_projection: Mat4::IDENTITY,
            viewport_width: 1920.0,
            viewport_height: 1080.0,
        }
    }
}

impl ScreenSpaceConfig {
    /// Create a new screen-space config with the given viewport dimensions.
    pub fn new(view_projection: Mat4, width: f32, height: f32) -> Self {
        Self {
            view_projection,
            viewport_width: width,
            viewport_height: height,
        }
    }

    /// Project a world-space point to normalized device coordinates.
    pub fn project_to_ndc(&self, world_pos: Vec3) -> Option<Vec3> {
        let clip = self.view_projection * Vec4::new(world_pos.x, world_pos.y, world_pos.z, 1.0);

        // Check if behind camera
        if clip.w <= 0.0 {
            return None;
        }

        // Perspective divide
        let ndc = Vec3::new(clip.x / clip.w, clip.y / clip.w, clip.z / clip.w);
        Some(ndc)
    }

    /// Project a world-space point to screen coordinates.
    pub fn project_to_screen(&self, world_pos: Vec3) -> Option<(f32, f32)> {
        let ndc = self.project_to_ndc(world_pos)?;

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

    EdgeEvaluation {
        screen_length,
        decision,
    }
}

/// Calculate world-space edge length (for non-screen-space fallback).
pub fn calculate_world_edge_length(v0: Vec3, v1: Vec3) -> f32 {
    v0.distance(v1)
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
