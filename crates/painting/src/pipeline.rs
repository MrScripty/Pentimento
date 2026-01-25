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

use tracing::{debug, info};

use crate::brush::{BrushEngine, BrushPreset, DabOutput};
use crate::log::{DabParams, StrokeConfig, StrokeLog, StrokeRecorder};
use crate::tiles::{TileCoord, TiledSurface};
use crate::types::{BlendMode, SpaceKind};
use crate::validation::to_size_field;

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
    brush: BrushEngine,
    /// Current stroke recorder (None if not painting)
    recorder: Option<StrokeRecorder>,
    /// Stroke log for storage
    log: StrokeLog,
    /// Current brush color
    color: [f32; 4],
    /// Current stroke ID (used during active stroke)
    current_stroke_id: Option<u64>,
    /// Current space ID (used during active stroke)
    current_space_id: Option<u32>,
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
            current_stroke_id: None,
            current_space_id: None,
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

    /// Begin a stroke
    ///
    /// - `space_id`: The canvas plane ID this stroke targets
    /// - `stroke_id`: Unique stroke identifier
    /// - `tool_id`: Brush preset ID (for libmypaint compatibility)
    pub fn begin_stroke(&mut self, space_id: u32, stroke_id: u64, _tool_id: u32) {
        // Reset brush engine state
        self.brush.begin_stroke();

        // Store current stroke info
        self.current_stroke_id = Some(stroke_id);
        self.current_space_id = Some(space_id);

        // Recorder will be initialized on first dab (we need initial position)
        self.recorder = None;
    }

    /// Continue a stroke with new input
    ///
    /// x, y are in surface pixel coordinates (0 to width/height)
    pub fn stroke_to(&mut self, x: f32, y: f32, pressure: f32) {
        let stroke_id = match self.current_stroke_id {
            Some(id) => id,
            None => {
                debug!("stroke_to: no active stroke, ignoring");
                return;
            }
        };
        let space_id = match self.current_space_id {
            Some(id) => id,
            None => return,
        };

        // Generate dabs from brush engine
        let dabs = self.brush.stroke_to(x, y, pressure);
        info!(
            "PaintingPipeline::stroke_to({:.1}, {:.1}, {:.2}) generated {} dabs",
            x, y, pressure, dabs.len()
        );

        // Initialize recorder on first dab if needed
        if self.recorder.is_none() && !dabs.is_empty() {
            let first_dab = &dabs[0];
            let mut recorder = StrokeRecorder::new();

            let config = StrokeConfig {
                space_kind: SpaceKind::CanvasPlane,
                space_id,
                stroke_id,
                timestamp_ms: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0),
                tool_id: self.brush.preset().id,
                blend_mode: BlendMode::Normal,
                color: self.color,
                flags: 0,
                face_id: 0,
                ptex_tile: 0,
                pressure_quant: crate::types::Quantization::U8,
                speed_quant: crate::types::Quantization::U8,
            };

            if recorder.start(config, first_dab.x, first_dab.y).is_ok() {
                self.recorder = Some(recorder);
            }
        }

        // Apply dabs to surface and record them
        for dab in dabs {
            self.apply_dab(&dab);
            self.record_dab(&dab, pressure);
        }
    }

    /// Apply a dab to the surface
    fn apply_dab(&mut self, dab: &DabOutput) {
        let radius = dab.size / 2.0;
        debug!(
            "  apply_dab: pos=({:.1}, {:.1}), radius={:.1}, opacity={:.2}, hardness={:.2}",
            dab.x, dab.y, radius, dab.opacity, dab.hardness
        );
        let result = self.surface.apply_dab(
            dab.x,
            dab.y,
            radius,
            self.color,
            dab.opacity,
            dab.hardness,
        );
        if let Some((x, y, w, h)) = result {
            debug!("    -> affected region: ({}, {}) {}x{}", x, y, w, h);
        } else {
            debug!("    -> dab outside surface bounds");
        }
    }

    /// Record a dab to the stroke recorder
    fn record_dab(&mut self, dab: &DabOutput, pressure: f32) {
        if let Some(ref mut recorder) = self.recorder {
            let params = DabParams {
                size: to_size_field(dab.size),
                pressure: (pressure.clamp(0.0, 1.0) * 65535.0) as u16,
                speed: 0, // TODO: Calculate from input
                hardness: (dab.hardness * 255.0) as u8,
                opacity: (dab.opacity * 255.0) as u8,
                angle: 0,
                aspect_ratio: 255, // Circular
            };

            let _ = recorder.add_dab(dab.x, dab.y, params);
        }
    }

    /// End the current stroke
    pub fn end_stroke(&mut self) {
        if let Some(mut recorder) = self.recorder.take() {
            if let Ok(packets) = recorder.finish() {
                for packet in packets {
                    self.log.append(packet);
                }
            }
        }

        self.brush.end_stroke();
        self.current_stroke_id = None;
        self.current_space_id = None;
    }

    /// Cancel the current stroke
    ///
    /// This aborts the stroke without saving it to the log.
    /// Note: The visual changes on the surface are NOT reverted.
    /// For proper undo, a separate undo system would be needed.
    pub fn cancel_stroke(&mut self) {
        if let Some(mut recorder) = self.recorder.take() {
            let _ = recorder.abort("Cancelled".to_string());
        }

        self.brush.end_stroke();
        self.current_stroke_id = None;
        self.current_space_id = None;
    }

    /// Check if a stroke is currently in progress
    pub fn is_stroking(&self) -> bool {
        self.current_stroke_id.is_some()
    }

    /// Take dirty tiles for GPU upload
    ///
    /// Returns the list of tile coordinates that have been modified
    /// since the last call. The dirty flags are cleared.
    pub fn take_dirty_tiles(&mut self) -> Vec<TileCoord> {
        self.surface.take_dirty_tiles()
    }

    /// Check if there are any dirty tiles
    pub fn has_dirty_tiles(&self) -> bool {
        self.surface.has_dirty_tiles()
    }

    /// Get tile data for upload
    ///
    /// Returns the pixel data for a tile as a Vec of [f32; 4] values.
    pub fn get_tile_data(&self, coord: TileCoord) -> Vec<[f32; 4]> {
        self.surface.get_tile_data(coord)
    }

    /// Get tile bounds in pixel coordinates
    ///
    /// Returns (x, y, width, height) for the given tile.
    pub fn get_tile_bounds(&self, coord: TileCoord) -> (u32, u32, u32, u32) {
        self.surface.get_tile_bounds(coord)
    }

    /// Get the tile size
    pub fn tile_size(&self) -> u32 {
        self.surface.tile_size()
    }

    /// Get reference to stroke log
    pub fn log(&self) -> &StrokeLog {
        &self.log
    }

    /// Clear the surface to a solid color
    pub fn clear(&mut self, color: [f32; 4]) {
        self.surface.surface_mut().clear(color);
        // Mark all tiles as dirty
        for ty in 0..self.surface.tiles_y() {
            for tx in 0..self.surface.tiles_x() {
                self.surface.mark_dirty(tx * self.surface.tile_size(), ty * self.surface.tile_size());
            }
        }
    }

    /// Get raw surface data as bytes (for full texture upload)
    pub fn surface_as_bytes(&self) -> &[u8] {
        self.surface.surface().as_bytes()
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
