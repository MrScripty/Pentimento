//! Surface operations for the painting pipeline

use crate::tiles::TileCoord;

use super::PaintingPipeline;

impl PaintingPipeline {
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

    /// Clear the surface to a solid color
    pub fn clear(&mut self, color: [f32; 4]) {
        self.surface.surface_mut().clear(color);
        // Mark all tiles as dirty
        for ty in 0..self.surface.tiles_y() {
            for tx in 0..self.surface.tiles_x() {
                self.surface
                    .mark_dirty(tx * self.surface.tile_size(), ty * self.surface.tile_size());
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
