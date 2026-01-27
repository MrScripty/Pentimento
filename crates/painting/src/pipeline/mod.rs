//! Complete painting pipeline
//!
//! This module provides the main painting pipeline that connects:
//! - Input handling (from Bevy systems via PaintEvent)
//! - Brush engine (dab generation)
//! - CPU surface (dab application)
//! - Stroke recording (for storage and sync)
//!
//! The pipeline is designed to be used from Bevy systems but does not
//! depend on Bevy itself.

mod stroke;
mod surface_ops;
mod undo;

use std::collections::{HashMap, HashSet};

use crate::brush::{BrushEngine, BrushPreset};
use crate::log::{StrokeLog, StrokeRecorder};
use crate::tiles::{TileCoord, TiledSurface};
use crate::types::BlendMode;

pub use undo::UndoEntry;

/// Complete painting pipeline for a canvas
///
/// This struct manages the full painting workflow:
/// 1. Input comes in via `begin_stroke`, `stroke_to`, `end_stroke`
/// 2. The brush engine generates dabs from input
/// 3. Dabs are applied to the CPU surface
/// 4. Dabs are recorded for storage/sync
/// 5. Dirty tiles are tracked for GPU upload
pub struct PaintingPipeline {
    /// CPU surface for painting
    pub surface: TiledSurface,
    /// Brush engine for dab generation
    pub(crate) brush: BrushEngine,
    /// Current stroke recorder (None if not painting)
    pub(crate) recorder: Option<StrokeRecorder>,
    /// Stroke log for storage
    pub(crate) log: StrokeLog,
    /// Current brush color
    pub(crate) color: [f32; 4],
    /// Current blend mode (Normal or Erase)
    pub(crate) blend_mode: BlendMode,
    /// Current stroke ID (used during active stroke)
    pub(crate) current_stroke_id: Option<u64>,
    /// Current space ID (used during active stroke)
    pub(crate) current_space_id: Option<u32>,
    /// Tiles captured for undo during current stroke
    pub(crate) pending_undo_captures: HashMap<TileCoord, Vec<[f32; 4]>>,
    /// Set of tiles already captured this stroke (to avoid re-capturing)
    pub(crate) captured_tiles: HashSet<TileCoord>,
    /// Undo stack (most recent at end)
    pub(crate) undo_stack: Vec<UndoEntry>,
    /// Maximum undo levels
    pub(crate) max_undo_levels: usize,
}

impl PaintingPipeline {
    /// Create a new painting pipeline with the given surface dimensions
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            surface: TiledSurface::with_default_tile_size(width, height),
            brush: BrushEngine::with_default_preset(),
            recorder: None,
            log: StrokeLog::new(),
            color: [0.0, 0.0, 0.0, 1.0], // Default to black
            blend_mode: BlendMode::Normal,
            current_stroke_id: None,
            current_space_id: None,
            pending_undo_captures: HashMap::new(),
            captured_tiles: HashSet::new(),
            undo_stack: Vec::new(),
            max_undo_levels: 20,
        }
    }

    /// Get the surface width
    pub fn width(&self) -> u32 {
        self.surface.surface().width
    }

    /// Get the surface height
    pub fn height(&self) -> u32 {
        self.surface.surface().height
    }

    /// Set the brush preset
    pub fn set_brush(&mut self, preset: BrushPreset) {
        self.brush.set_preset(preset);
    }

    /// Get the current brush preset
    pub fn brush_preset(&self) -> &BrushPreset {
        self.brush.preset()
    }

    /// Set the brush color
    pub fn set_color(&mut self, color: [f32; 4]) {
        self.color = color;
    }

    /// Get the current brush color
    pub fn color(&self) -> [f32; 4] {
        self.color
    }

    /// Set the blend mode
    pub fn set_blend_mode(&mut self, mode: BlendMode) {
        self.blend_mode = mode;
    }

    /// Get the current blend mode
    pub fn blend_mode(&self) -> BlendMode {
        self.blend_mode
    }

    /// Get reference to stroke log
    pub fn log(&self) -> &StrokeLog {
        &self.log
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_creation() {
        let pipeline = PaintingPipeline::new(256, 256);
        assert_eq!(pipeline.width(), 256);
        assert_eq!(pipeline.height(), 256);
    }

    #[test]
    fn test_pipeline_stroke() {
        let mut pipeline = PaintingPipeline::new(256, 256);
        pipeline.set_color([1.0, 0.0, 0.0, 1.0]);

        pipeline.begin_stroke(0, 1, 0);
        assert!(pipeline.is_stroking());

        pipeline.stroke_to(100.0, 100.0, 1.0);
        pipeline.stroke_to(150.0, 100.0, 1.0);
        pipeline.end_stroke();

        assert!(!pipeline.is_stroking());
        assert!(pipeline.has_dirty_tiles());
    }

    #[test]
    fn test_pipeline_cancel_stroke() {
        let mut pipeline = PaintingPipeline::new(256, 256);

        pipeline.begin_stroke(0, 1, 0);
        pipeline.stroke_to(100.0, 100.0, 1.0);
        pipeline.cancel_stroke();

        assert!(!pipeline.is_stroking());
        // Log should be empty (stroke was cancelled)
        assert_eq!(pipeline.log().total_packet_count(), 0);
    }

    #[test]
    fn test_pipeline_stroke_log() {
        let mut pipeline = PaintingPipeline::new(256, 256);

        pipeline.begin_stroke(42, 1, 0);
        pipeline.stroke_to(100.0, 100.0, 1.0);
        pipeline.stroke_to(150.0, 100.0, 1.0);
        pipeline.end_stroke();

        // Should have recorded the stroke
        assert!(pipeline.log().total_packet_count() > 0);
        let packets = pipeline.log().query_by_space(42);
        assert!(!packets.is_empty());
    }

    #[test]
    fn test_pipeline_dirty_tiles() {
        let mut pipeline = PaintingPipeline::new(256, 256);

        pipeline.begin_stroke(0, 1, 0);
        pipeline.stroke_to(100.0, 100.0, 1.0);
        pipeline.end_stroke();

        let dirty = pipeline.take_dirty_tiles();
        assert!(!dirty.is_empty());

        // After taking, should be empty
        assert!(!pipeline.has_dirty_tiles());
    }

    #[test]
    fn test_pipeline_clear() {
        let mut pipeline = PaintingPipeline::new(256, 256);
        pipeline.clear([1.0, 1.0, 1.0, 1.0]);

        // All tiles should be dirty
        let dirty = pipeline.take_dirty_tiles();
        assert!(!dirty.is_empty());
    }
}
