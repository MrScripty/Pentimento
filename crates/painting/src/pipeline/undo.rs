//! Undo functionality for the painting pipeline

use std::collections::HashMap;
use tracing::debug;

use crate::tiles::TileCoord;

use super::PaintingPipeline;

/// An undo entry containing captured tile data before a stroke
#[derive(Clone)]
pub struct UndoEntry {
    /// Stroke ID this entry corresponds to
    pub stroke_id: u64,
    /// Captured tile data (tile coord -> pixel data)
    pub tiles: HashMap<TileCoord, Vec<[f32; 4]>>,
}

impl PaintingPipeline {
    /// Capture tile data before a dab modifies them (for undo support)
    pub(crate) fn capture_tiles_for_dab(&mut self, center_x: f32, center_y: f32, radius: f32) {
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

        debug!(
            "Undoing stroke {} ({} tiles)",
            entry.stroke_id,
            entry.tiles.len()
        );

        // Restore each tile's data
        for (coord, tile_data) in entry.tiles {
            self.restore_tile(coord, &tile_data);
        }

        true
    }

    /// Restore a tile's pixel data from an undo entry
    pub(crate) fn restore_tile(&mut self, coord: TileCoord, tile_data: &[[f32; 4]]) {
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
}
