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
use crate::layer::LayerStack;
use crate::log::{StrokeLog, StrokeRecorder};
use crate::tiles::TileCoord;
use crate::types::BlendMode;

pub use undo::UndoEntry;

/// Complete painting pipeline for a canvas
///
/// This struct manages the full painting workflow:
/// 1. Input comes in via `begin_stroke`, `stroke_to`, `end_stroke`
/// 2. The brush engine generates dabs from input
/// 3. Dabs are applied to the active layer's CPU surface
/// 4. Dabs are recorded for storage/sync
/// 5. Layers are composited and dirty tiles tracked for GPU upload
pub struct PaintingPipeline {
    /// Layer stack (replaces single surface)
    pub layers: LayerStack,
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
    /// Layer owned by the active stroke, independent of later UI selection.
    pub(crate) current_layer_id: Option<u32>,
    /// Tiles captured for undo during current stroke
    pub(crate) pending_undo_captures: HashMap<TileCoord, Vec<[f32; 4]>>,
    /// Set of tiles already captured this stroke (to avoid re-capturing)
    pub(crate) captured_tiles: HashSet<TileCoord>,
    /// Undo stack (most recent at end)
    pub(crate) undo_stack: Vec<UndoEntry>,
    /// Undone strokes, with tile snapshots of their committed result.
    pub(crate) redo_stack: Vec<UndoEntry>,
    /// Maximum undo levels
    pub(crate) max_undo_levels: usize,
}

impl PaintingPipeline {
    /// Create a new painting pipeline with the given surface dimensions
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            layers: LayerStack::new(width, height),
            brush: BrushEngine::with_default_preset(),
            recorder: None,
            log: StrokeLog::new(),
            color: [0.0, 0.0, 0.0, 1.0], // Default to black
            blend_mode: BlendMode::Normal,
            current_stroke_id: None,
            current_space_id: None,
            current_layer_id: None,
            pending_undo_captures: HashMap::new(),
            captured_tiles: HashSet::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_undo_levels: 20,
        }
    }

    /// Get the surface width
    pub fn width(&self) -> u32 {
        self.layers.composited_surface().surface().width
    }

    /// Get the surface height
    pub fn height(&self) -> u32 {
        self.layers.composited_surface().surface().height
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
    fn redo_restores_layered_strokes_and_cancel_preserves_cursor() {
        let mut p = PaintingPipeline::new(160, 130);
        p.set_color([1., 0., 1., 1.]);
        let original = {
            p.take_dirty_tiles();
            p.surface_as_bytes().to_vec()
        };
        p.begin_stroke(7, 1, 0);
        p.stroke_to(64., 64., 1.);
        p.end_stroke();
        p.take_dirty_tiles();
        let first = p.surface_as_bytes().to_vec();
        p.begin_stroke(7, 2, 0);
        p.stroke_to(69., 64., 1.);
        p.end_stroke();
        p.take_dirty_tiles();
        let second = p.surface_as_bytes().to_vec();
        let packets = p.log().total_packet_count();
        assert!(p.undo());
        p.take_dirty_tiles();
        assert_eq!(p.surface_as_bytes(), first);
        p.begin_stroke(7, 3, 0);
        p.stroke_to(100., 90., 1.);
        assert!(!p.redo());
        assert!(!p.undo());
        assert!(!p.can_redo());
        p.cancel_stroke();
        p.take_dirty_tiles();
        assert_eq!(p.surface_as_bytes(), first);
        assert!(p.redo());
        p.take_dirty_tiles();
        assert_eq!(p.surface_as_bytes(), second);
        assert_eq!(
            p.log().total_packet_count(),
            packets,
            "history must not emit replay packets"
        );
        assert!(p.undo());
        assert!(p.undo());
        p.take_dirty_tiles();
        assert_eq!(p.surface_as_bytes(), original);
        assert!(p.redo());
        assert!(p.redo());
        p.take_dirty_tiles();
        assert_eq!(p.surface_as_bytes(), second);
    }

    #[test]
    fn no_op_preserves_redo_and_new_changed_stroke_clears_it() {
        let mut p = PaintingPipeline::new(128, 128);
        p.begin_stroke(7, 1, 0);
        p.stroke_to(50., 50., 1.);
        p.end_stroke();
        assert!(p.undo());
        let mut zero = p.brush_preset().clone();
        zero.opacity = 0.;
        p.set_brush(zero);
        p.begin_stroke(7, 2, 0);
        p.stroke_to(60., 60., 1.);
        p.end_stroke();
        assert!(p.can_redo());
        assert_eq!(p.undo_count(), 0);
        let mut preset = p.brush_preset().clone();
        preset.opacity = 1.;
        p.set_brush(preset);
        p.begin_stroke(7, 3, 0);
        p.stroke_to(70., 70., 1.);
        p.end_stroke();
        assert!(!p.can_redo());
        assert_eq!(p.undo_count(), 1);
    }

    #[test]
    fn history_retains_owner_and_refuses_removed_layer_without_popping_cursor() {
        let mut p = PaintingPipeline::new(130, 130);
        let owner = p.layers.add_layer("owner".into());
        p.layers.set_active(owner);
        p.begin_stroke(7, 1, 0);
        p.stroke_to(129., 129., 1.);
        p.end_stroke();
        let other = p.layers.add_layer("other".into());
        p.layers.set_active(other);
        assert!(p.undo());
        assert!(p.redo());
        assert!(p.layers.remove_layer(owner));
        assert!(!p.undo());
        assert_eq!(p.undo_count(), 1);
    }

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
    fn small_pointer_moves_paint_the_same_continuous_stroke_as_one_long_move() {
        fn paint(step: usize) -> PaintingPipeline {
            let mut pipeline = PaintingPipeline::new(192, 128);
            pipeline.clear([0., 0., 0., 0.]);
            pipeline.set_color([1., 0., 1., 1.]);
            pipeline.set_brush(BrushPreset {
                base_size: 48.,
                min_size: 48.,
                max_size: 48.,
                spacing: 0.25,
                hardness: 1.,
                opacity: 1.,
                ..Default::default()
            });
            pipeline.begin_stroke(7, 1, 0);
            pipeline.stroke_to(40., 64., 1.);
            for x in (40 + step..=136).step_by(step) {
                pipeline.stroke_to(x as f32, 64., 1.);
            }
            pipeline.end_stroke();
            pipeline.take_dirty_tiles();
            pipeline
        }
        // Three-pixel events are below half of the twelve-pixel dab spacing.
        let mut fine = paint(3);
        let coarse = paint(96);
        assert!(
            fine.surface_as_bytes() == coarse.surface_as_bytes(),
            "input cadence must not discard travelled distance"
        );
        for x in 40..=136 {
            assert_eq!(
                fine.get_pixel(x, 64).unwrap(),
                [1., 0., 1., 1.],
                "gap at x={x}"
            );
        }
        assert_eq!(fine.undo_count(), 1);
        assert_eq!(fine.log().total_packet_count(), 1);
        assert!(fine.undo());
        fine.take_dirty_tiles();
        assert!(fine.surface_as_bytes().iter().all(|&v| v == 0));
    }

    #[test]
    fn cancelled_stroke_restores_owned_layer_and_preserves_prior_undo() {
        let mut pipeline = PaintingPipeline::new(128, 128);
        pipeline.clear([0., 0., 0., 0.]);
        pipeline.set_color([1., 0., 0., 1.]);
        pipeline.begin_stroke(0, 1, 0);
        pipeline.stroke_to(30., 30., 1.);
        pipeline.end_stroke();
        pipeline.take_dirty_tiles();
        let before = pipeline.surface_as_bytes().to_vec();
        pipeline.begin_stroke(0, 2, 0);
        pipeline.stroke_to(80., 80., 1.);
        pipeline.take_dirty_tiles();
        assert!(pipeline.surface_as_bytes() != before.as_slice());
        assert!(!pipeline.can_undo(), "live transaction owns its baseline");
        assert!(
            !pipeline.undo(),
            "undo must wait for finish or cancellation"
        );
        assert_eq!(pipeline.undo_count(), 1);
        let other = pipeline.layers.add_layer("Other".into());
        pipeline.layers.set_active(other);
        pipeline.cancel_stroke();
        pipeline.take_dirty_tiles();
        assert!(pipeline.surface_as_bytes() == before.as_slice());
        assert_eq!(pipeline.layers.active_layer_id(), other);
        assert_eq!(pipeline.undo_count(), 1);
        assert_eq!(pipeline.log().total_packet_count(), 1);
        assert!(!pipeline.is_stroking());
        assert!(pipeline.can_undo());
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
