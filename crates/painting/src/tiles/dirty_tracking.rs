//! Dirty tile tracking for incremental updates

use tracing::debug;

use super::{TileCoord, TiledSurface};

impl TiledSurface {
    /// Mark a pixel as modified (marks containing tile dirty)
    #[inline]
    pub fn mark_dirty(&mut self, x: u32, y: u32) {
        if x >= self.surface.width || y >= self.surface.height {
            return;
        }
        let tile_x = x / self.tile_size;
        let tile_y = y / self.tile_size;
        self.dirty_tiles.insert(TileCoord { x: tile_x, y: tile_y });
    }

    /// Mark a rectangular region as dirty
    pub fn mark_region_dirty(&mut self, x: u32, y: u32, w: u32, h: u32) {
        if w == 0 || h == 0 {
            return;
        }

        // Clamp to surface bounds
        let x_end = (x + w).min(self.surface.width);
        let y_end = (y + h).min(self.surface.height);

        if x >= self.surface.width || y >= self.surface.height {
            return;
        }

        // Calculate tile range
        let tile_x_start = x / self.tile_size;
        let tile_y_start = y / self.tile_size;
        let tile_x_end = (x_end.saturating_sub(1)) / self.tile_size;
        let tile_y_end = (y_end.saturating_sub(1)) / self.tile_size;

        let tiles_before = self.dirty_tiles.len();

        // Mark all tiles in the range
        for ty in tile_y_start..=tile_y_end {
            for tx in tile_x_start..=tile_x_end {
                self.dirty_tiles.insert(TileCoord { x: tx, y: ty });
            }
        }

        let tiles_after = self.dirty_tiles.len();
        debug!(
            "mark_region_dirty: ({}, {}) {}x{} -> {} new tiles (total {})",
            x, y, w, h, tiles_after - tiles_before, tiles_after
        );
    }

    /// Get all dirty tiles and clear the dirty set
    pub fn take_dirty_tiles(&mut self) -> Vec<TileCoord> {
        self.dirty_tiles.drain().collect()
    }

    /// Check if any tiles are dirty
    #[inline]
    pub fn has_dirty_tiles(&self) -> bool {
        !self.dirty_tiles.is_empty()
    }

    /// Get the number of dirty tiles
    #[inline]
    pub fn dirty_tile_count(&self) -> usize {
        self.dirty_tiles.len()
    }
}
