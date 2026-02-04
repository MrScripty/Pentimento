//! Global vertex budget for budget-based tessellation.
//!
//! The vertex budget is derived from the render camera's pixel coverage of the
//! model. The maximum number of vertices allowed equals the number of pixels
//! the model covers on screen (times a configurable multiplier). This ensures
//! mesh density never exceeds what's visible at the render resolution.

/// Global vertex budget for a sculpted mesh.
///
/// Tracks the maximum allowed vertices (from pixel coverage), current count,
/// and remaining capacity. The budget is updated per-dab from the pixel
/// coverage system.
#[derive(Debug, Clone)]
pub struct VertexBudget {
    /// Maximum vertices allowed (derived from pixel_coverage * vertices_per_pixel)
    pub max_vertices: usize,
    /// Current total vertex count across all chunks
    pub current_vertices: usize,
    /// Remaining vertices available for splits (negative = over budget)
    pub remaining: isize,
    /// Pixel coverage from last computation
    pub pixel_coverage: u32,
    /// Whether the budget needs recalculation (e.g. render camera moved)
    pub stale: bool,
}

impl Default for VertexBudget {
    fn default() -> Self {
        Self {
            max_vertices: usize::MAX, // No limit until pixel coverage is computed
            current_vertices: 0,
            remaining: isize::MAX,
            pixel_coverage: 0,
            stale: true,
        }
    }
}

impl VertexBudget {
    /// Create a budget from a known pixel coverage and multiplier.
    pub fn from_pixel_coverage(pixel_coverage: u32, vertices_per_pixel: f32) -> Self {
        let max_vertices = (pixel_coverage as f32 * vertices_per_pixel) as usize;
        Self {
            max_vertices: max_vertices.max(100), // Floor to prevent degenerate meshes
            current_vertices: 0,
            remaining: max_vertices.max(100) as isize,
            pixel_coverage,
            stale: false,
        }
    }

    /// Update the budget's max from new pixel coverage data.
    pub fn update_max(&mut self, pixel_coverage: u32, vertices_per_pixel: f32) {
        self.pixel_coverage = pixel_coverage;
        self.max_vertices = ((pixel_coverage as f32 * vertices_per_pixel) as usize).max(100);
        self.recalculate_remaining();
        self.stale = false;
    }

    /// Update the current vertex count and recalculate remaining.
    pub fn update_current(&mut self, current_vertices: usize) {
        self.current_vertices = current_vertices;
        self.recalculate_remaining();
    }

    /// Record that a split occurred (one new vertex created).
    pub fn record_split(&mut self) {
        self.current_vertices += 1;
        self.remaining -= 1;
    }

    /// Record that a collapse occurred (one vertex removed).
    pub fn record_collapse(&mut self) {
        self.current_vertices = self.current_vertices.saturating_sub(1);
        self.remaining += 1;
    }

    /// Whether splits are allowed (budget has capacity).
    pub fn can_split(&self) -> bool {
        self.remaining > 0
    }

    /// Whether the mesh is over budget and needs collapsing.
    pub fn is_over_budget(&self) -> bool {
        self.remaining < 0
    }

    fn recalculate_remaining(&mut self) {
        self.remaining = self.max_vertices as isize - self.current_vertices as isize;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_budget_allows_splits() {
        let budget = VertexBudget::default();
        assert!(budget.can_split());
        assert!(!budget.is_over_budget());
    }

    #[test]
    fn test_budget_from_pixel_coverage() {
        let budget = VertexBudget::from_pixel_coverage(1000, 1.0);
        assert_eq!(budget.max_vertices, 1000);
        assert_eq!(budget.remaining, 1000);
        assert!(budget.can_split());
    }

    #[test]
    fn test_budget_tracks_splits() {
        let budget = VertexBudget::from_pixel_coverage(3, 1.0);
        // min floor is 100
        assert_eq!(budget.max_vertices, 100);

        let mut budget = VertexBudget::from_pixel_coverage(200, 1.0);
        assert_eq!(budget.max_vertices, 200);

        budget.update_current(198);
        assert_eq!(budget.remaining, 2);

        budget.record_split();
        assert_eq!(budget.remaining, 1);
        assert!(budget.can_split());

        budget.record_split();
        assert_eq!(budget.remaining, 0);
        assert!(!budget.can_split());

        budget.record_split();
        assert_eq!(budget.remaining, -1);
        assert!(budget.is_over_budget());
    }

    #[test]
    fn test_budget_tracks_collapses() {
        let mut budget = VertexBudget::from_pixel_coverage(100, 1.0);
        budget.update_current(120); // Over budget
        assert!(budget.is_over_budget());

        budget.record_collapse();
        assert_eq!(budget.current_vertices, 119);
        assert_eq!(budget.remaining, -19);
    }

    #[test]
    fn test_budget_update_max() {
        let mut budget = VertexBudget::from_pixel_coverage(1000, 1.0);
        budget.update_current(500);

        // Camera moved closer — more pixels
        budget.update_max(2000, 1.0);
        assert_eq!(budget.max_vertices, 2000);
        assert_eq!(budget.remaining, 1500);

        // Camera moved farther — fewer pixels
        budget.update_max(300, 1.0);
        assert_eq!(budget.max_vertices, 300);
        assert_eq!(budget.remaining, -200);
        assert!(budget.is_over_budget());
    }

    #[test]
    fn test_vertices_per_pixel_multiplier() {
        let budget = VertexBudget::from_pixel_coverage(1000, 2.0);
        assert_eq!(budget.max_vertices, 2000);

        let budget = VertexBudget::from_pixel_coverage(1000, 0.5);
        assert_eq!(budget.max_vertices, 500);
    }
}
