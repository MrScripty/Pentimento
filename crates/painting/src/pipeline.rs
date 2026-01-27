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

use tracing::debug;

use std::collections::{HashMap, HashSet};

use crate::brush::{BrushEngine, BrushPreset, DabOutput};
use crate::log::{DabParams, StrokeConfig, StrokeLog, StrokeRecorder};
use crate::tiles::{TileCoord, TiledSurface};
use crate::types::{BlendMode, SpaceKind};
use crate::validation::to_size_field;

/// An undo entry containing captured tile data before a stroke
#[derive(Clone)]
pub struct UndoEntry {
    /// Stroke ID this entry corresponds to
    pub stroke_id: u64,
    /// Captured tile data (tile coord -> pixel data)
    pub tiles: HashMap<TileCoord, Vec<[f32; 4]>>,
}

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
    /// Current blend mode (Normal or Erase)
    blend_mode: BlendMode,
    /// Current stroke ID (used during active stroke)
    current_stroke_id: Option<u64>,
    /// Current space ID (used during active stroke)
    current_space_id: Option<u32>,
    /// Tiles captured for undo during current stroke
    pending_undo_captures: HashMap<TileCoord, Vec<[f32; 4]>>,
    /// Set of tiles already captured this stroke (to avoid re-capturing)
    captured_tiles: HashSet<TileCoord>,
    /// Undo stack (most recent at end)
    undo_stack: Vec<UndoEntry>,
    /// Maximum undo levels
    max_undo_levels: usize,
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

        // Clear pending undo captures for new stroke
        self.pending_undo_captures.clear();
        self.captured_tiles.clear();
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
                blend_mode: self.blend_mode,
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
            "  apply_dab: pos=({:.1}, {:.1}), radius={:.1}, opacity={:.2}, hardness={:.2}, mode={:?}",
            dab.x, dab.y, radius, dab.opacity, dab.hardness, self.blend_mode
        );

        // Capture tiles before modification for undo
        self.capture_tiles_for_dab(dab.x, dab.y, radius);

        let result = self.surface.apply_dab(
            dab.x,
            dab.y,
            radius,
            self.color,
            dab.opacity,
            dab.hardness,
            self.blend_mode,
        );
        if let Some((x, y, w, h)) = result {
            debug!("    -> affected region: ({}, {}) {}x{}", x, y, w, h);
        } else {
            debug!("    -> dab outside surface bounds");
        }
    }

    /// Capture tile data before a dab modifies them (for undo support)
    fn capture_tiles_for_dab(&mut self, center_x: f32, center_y: f32, radius: f32) {
        if radius <= 0.0 {
            return;
        }

        let tile_size = self.surface.tile_size() as f32;
        let width = self.surface.surface().width as f32;
        let height = self.surface.surface().height as f32;

        // Calculate bounding box of the dab
        let x_min = (center_x - radius).max(0.0);
        let y_min = (center_y - radius).max(0.0);
        let x_max = (center_x + radius).min(width);
        let y_max = (center_y + radius).min(height);

        if x_min >= x_max || y_min >= y_max {
            return;
        }

        // Calculate affected tile range
        let tile_x_start = (x_min / tile_size) as u32;
        let tile_y_start = (y_min / tile_size) as u32;
        let tile_x_end = (x_max / tile_size) as u32;
        let tile_y_end = (y_max / tile_size) as u32;

        // Capture any tiles not yet captured this stroke
        for ty in tile_y_start..=tile_y_end {
            for tx in tile_x_start..=tile_x_end {
                let coord = TileCoord { x: tx, y: ty };
                if !self.captured_tiles.contains(&coord) {
                    // Capture tile data before modification
                    let tile_data = self.surface.get_tile_data(coord);
                    self.pending_undo_captures.insert(coord, tile_data);
                    self.captured_tiles.insert(coord);
                }
            }
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

        // Finalize undo entry if we captured any tiles
        if !self.pending_undo_captures.is_empty() {
            let stroke_id = self.current_stroke_id.unwrap_or(0);
            let entry = UndoEntry {
                stroke_id,
                tiles: std::mem::take(&mut self.pending_undo_captures),
            };
            self.undo_stack.push(entry);

            // Limit undo stack size
            while self.undo_stack.len() > self.max_undo_levels {
                self.undo_stack.remove(0);
            }

            debug!("Saved undo entry for stroke {} ({} tiles)", stroke_id, self.undo_stack.last().map(|e| e.tiles.len()).unwrap_or(0));
        }

        self.captured_tiles.clear();
        self.brush.end_stroke();
        self.current_stroke_id = None;
        self.current_space_id = None;
    }

    /// Cancel the current stroke
    ///
    /// This aborts the stroke without saving it to the log.
    /// Note: The visual changes on the surface are NOT reverted.
    /// For proper undo, use the undo() method after canceling.
    pub fn cancel_stroke(&mut self) {
        if let Some(mut recorder) = self.recorder.take() {
            let _ = recorder.abort("Cancelled".to_string());
        }

        // Clear pending undo captures (don't save to undo stack)
        self.pending_undo_captures.clear();
        self.captured_tiles.clear();

        self.brush.end_stroke();
        self.current_stroke_id = None;
        self.current_space_id = None;
    }

    /// Check if a stroke is currently in progress
    pub fn is_stroking(&self) -> bool {
        self.current_stroke_id.is_some()
    }

    /// Check if undo is available
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Get the number of undo levels available
    pub fn undo_count(&self) -> usize {
        self.undo_stack.len()
    }

    /// Undo the last stroke
    ///
    /// Returns true if an undo was performed, false if no undo available
    pub fn undo(&mut self) -> bool {
        let Some(entry) = self.undo_stack.pop() else {
            debug!("Undo: no entries available");
            return false;
        };

        debug!("Undoing stroke {} ({} tiles)", entry.stroke_id, entry.tiles.len());

        // Restore each tile's data
        for (coord, tile_data) in entry.tiles {
            self.restore_tile(coord, &tile_data);
        }

        true
    }

    /// Restore a tile's pixel data from an undo entry
    fn restore_tile(&mut self, coord: TileCoord, tile_data: &[[f32; 4]]) {
        let tile_size = self.surface.tile_size();
        let tile_start_x = coord.x * tile_size;
        let tile_start_y = coord.y * tile_size;

        let surface_width = self.surface.surface().width;
        let surface_height = self.surface.surface().height;

        // Calculate actual tile dimensions (may be smaller at edges)
        let tile_width = tile_size.min(surface_width.saturating_sub(tile_start_x));
        let tile_height = tile_size.min(surface_height.saturating_sub(tile_start_y));

        // Write pixels back
        let mut idx = 0;
        for dy in 0..tile_height {
            for dx in 0..tile_width {
                if idx < tile_data.len() {
                    let x = tile_start_x + dx;
                    let y = tile_start_y + dy;
                    self.surface.surface_mut().set_pixel(x, y, tile_data[idx]);
                    idx += 1;
                }
            }
        }

        // Mark the tile as dirty for GPU upload
        self.surface.mark_dirty(tile_start_x, tile_start_y);
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

    /// Get pixel data for a rectangular region
    ///
    /// Returns Vec of [f32; 4] pixels in row-major order.
    /// The region is clamped to surface bounds.
    pub fn get_region_data(&self, x: u32, y: u32, width: u32, height: u32) -> Vec<[f32; 4]> {
        self.surface.get_region_data(x, y, width, height)
    }

    /// Compute bounding box of given tile coordinates in pixel coordinates
    ///
    /// Returns (x, y, width, height) or None if no tiles provided.
    pub fn compute_tiles_bounding_box(&self, tiles: &[TileCoord]) -> Option<(u32, u32, u32, u32)> {
        self.surface.compute_tiles_bounding_box(tiles)
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

    /// Get a single pixel's color
    ///
    /// Returns None if coordinates are out of bounds.
    pub fn get_pixel(&self, x: u32, y: u32) -> Option<[f32; 4]> {
        self.surface.surface().get_pixel(x, y)
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
