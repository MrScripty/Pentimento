//! Undo functionality for the painting pipeline

use std::collections::HashMap;

use crate::tiles::{TileCoord, TiledSurface};

use super::PaintingPipeline;

/// An undo entry containing captured tile data before a stroke
#[derive(Clone)]
pub struct UndoEntry {
    /// Stroke ID this entry corresponds to
    pub stroke_id: u64,
    /// Layer ID this stroke was painted on
    pub layer_id: u32,
    /// Captured tile data (tile coord -> pixel data)
    pub tiles: HashMap<TileCoord, Vec<[f32; 4]>>,
}

impl PaintingPipeline {
    /// Capture tile data before a dab modifies them (for undo support)
    pub(crate) fn capture_tiles_for_dab(&mut self, center_x: f32, center_y: f32, radius: f32) {
        if radius <= 0.0 {
            return;
        }

        let Some(layer) = self.current_layer_id.and_then(|id| self.layers.layer(id)) else {
            return;
        };

        let tile_size = layer.surface.tile_size() as f32;
        let width = layer.surface.surface().width as f32;
        let height = layer.surface.surface().height as f32;

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
                    // Capture tile data before modification from the active layer
                    let tile_data = layer.surface.get_tile_data(coord);
                    self.pending_undo_captures.insert(coord, tile_data);
                    self.captured_tiles.insert(coord);
                }
            }
        }
    }

    /// Check if undo is available
    pub fn can_undo(&self) -> bool {
        !self.is_stroking()
            && self
                .undo_stack
                .last()
                .is_some_and(|e| self.layers.layer(e.layer_id).is_some())
    }

    /// Get the number of undo levels available
    pub fn undo_count(&self) -> usize {
        self.undo_stack.len()
    }

    /// Redo is unavailable while a live transaction owns its pixel baseline.
    pub fn can_redo(&self) -> bool {
        !self.is_stroking()
            && self
                .redo_stack
                .last()
                .is_some_and(|e| self.layers.layer(e.layer_id).is_some())
    }

    pub fn redo_count(&self) -> usize {
        self.redo_stack.len()
    }

    /// Restore a committed stroke's captured preimage and retain its result for Redo.
    pub fn undo(&mut self) -> bool {
        self.exchange_history(false)
    }

    /// Restore the exact committed pixels, without replaying input or emitting packets.
    pub fn redo(&mut self) -> bool {
        self.exchange_history(true)
    }

    fn exchange_history(&mut self, redo: bool) -> bool {
        if self.is_stroking() {
            return false;
        }
        let stack = if redo {
            &mut self.redo_stack
        } else {
            &mut self.undo_stack
        };
        let Some(entry) = stack.last() else {
            return false;
        };
        // Refusal leaves the cursor intact if an external layer edit removed the owner.
        let Some(layer) = self.layers.layer_mut(entry.layer_id) else {
            return false;
        };
        let inverse = UndoEntry {
            stroke_id: entry.stroke_id,
            layer_id: entry.layer_id,
            tiles: entry
                .tiles
                .keys()
                .map(|coord| (*coord, layer.surface.get_tile_data(*coord)))
                .collect(),
        };
        for (coord, pixels) in &entry.tiles {
            restore_tile(&mut layer.surface, *coord, pixels);
        }
        stack.pop();
        if redo {
            self.undo_stack.push(inverse);
        } else {
            self.redo_stack.push(inverse);
        }
        true
    }
}

/// Restore a tile's pixel data from an undo entry on a specific surface
pub(super) fn restore_tile(surface: &mut TiledSurface, coord: TileCoord, tile_data: &[[f32; 4]]) {
    let tile_size = surface.tile_size();
    let tile_start_x = coord.x * tile_size;
    let tile_start_y = coord.y * tile_size;

    let surface_width = surface.surface().width;
    let surface_height = surface.surface().height;

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
                surface.surface_mut().set_pixel(x, y, tile_data[idx]);
                idx += 1;
            }
        }
    }

    // Mark the tile as dirty for GPU upload
    surface.mark_dirty(tile_start_x, tile_start_y);
}
