//! Stroke handling for the painting pipeline

use tracing::debug;

use crate::brush::DabOutput;
use crate::log::{DabParams, StrokeConfig, StrokeRecorder};
use crate::types::{Quantization, SpaceKind};
use crate::validation::to_size_field;

use super::PaintingPipeline;

impl PaintingPipeline {
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
                pressure_quant: Quantization::U8,
                speed_quant: Quantization::U8,
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
    pub(crate) fn apply_dab(&mut self, dab: &DabOutput) {
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

    /// Record a dab to the stroke recorder
    pub(crate) fn record_dab(&mut self, dab: &DabOutput, pressure: f32) {
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
            let entry = super::undo::UndoEntry {
                stroke_id,
                tiles: std::mem::take(&mut self.pending_undo_captures),
            };
            self.undo_stack.push(entry);

            // Limit undo stack size
            while self.undo_stack.len() > self.max_undo_levels {
                self.undo_stack.remove(0);
            }

            debug!(
                "Saved undo entry for stroke {} ({} tiles)",
                stroke_id,
                self.undo_stack.last().map(|e| e.tiles.len()).unwrap_or(0)
            );
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
}
