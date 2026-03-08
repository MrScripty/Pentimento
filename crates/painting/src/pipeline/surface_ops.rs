//! Surface operations for the painting pipeline

use crate::tiles::TileCoord;

use super::PaintingPipeline;

impl PaintingPipeline {
    /// Take dirty tiles for GPU upload
    ///
    /// Composites all layers first, then returns the list of tile coordinates
    /// that have been modified. The dirty flags are cleared.
    pub fn take_dirty_tiles(&mut self) -> Vec<TileCoord> {
        self.layers.composite();
        self.layers.clear_layer_dirty_flags();
        self.layers.composited_surface_mut().take_dirty_tiles()
    }

    /// Check if there are any dirty tiles (on any layer)
    pub fn has_dirty_tiles(&self) -> bool {
        // Check if any layer has dirty tiles
        for info in self.layers.layer_info() {
            if let Some(layer) = self.layers.layer(info.id) {
                if layer.surface.has_dirty_tiles() {
                    return true;
                }
            }
        }
        false
    }

    /// Get tile data for upload from the composited surface
    ///
    /// Returns the pixel data for a tile as a Vec of [f32; 4] values.
    pub fn get_tile_data(&self, coord: TileCoord) -> Vec<[f32; 4]> {
        self.layers.composited_surface().get_tile_data(coord)
    }

    /// Get tile bounds in pixel coordinates
    ///
    /// Returns (x, y, width, height) for the given tile.
    pub fn get_tile_bounds(&self, coord: TileCoord) -> (u32, u32, u32, u32) {
        self.layers.composited_surface().get_tile_bounds(coord)
    }

    /// Get the tile size
    pub fn tile_size(&self) -> u32 {
        self.layers.composited_surface().tile_size()
    }

    /// Get pixel data for a rectangular region from the composited surface
    ///
    /// Returns Vec of [f32; 4] pixels in row-major order.
    /// The region is clamped to surface bounds.
    pub fn get_region_data(&self, x: u32, y: u32, width: u32, height: u32) -> Vec<[f32; 4]> {
        self.layers.composited_surface().get_region_data(x, y, width, height)
    }

    /// Compute bounding box of given tile coordinates in pixel coordinates
    ///
    /// Returns (x, y, width, height) or None if no tiles provided.
    pub fn compute_tiles_bounding_box(&self, tiles: &[TileCoord]) -> Option<(u32, u32, u32, u32)> {
        self.layers.composited_surface().compute_tiles_bounding_box(tiles)
    }

    /// Clear the active layer's surface to a solid color
    pub fn clear(&mut self, color: [f32; 4]) {
        if let Some(layer) = self.layers.active_layer_mut() {
            layer.surface.surface_mut().clear(color);
            // Mark all tiles as dirty on the layer
            let tile_size = layer.surface.tile_size();
            let tiles_x = (layer.surface.surface().width + tile_size - 1) / tile_size;
            let tiles_y = (layer.surface.surface().height + tile_size - 1) / tile_size;
            for ty in 0..tiles_y {
                for tx in 0..tiles_x {
                    layer.surface.mark_dirty(tx * tile_size, ty * tile_size);
                }
            }
        }
    }

    /// Get raw surface data as bytes from the composited surface (for full texture upload)
    pub fn surface_as_bytes(&self) -> &[u8] {
        self.layers.composited_surface().surface().as_bytes()
    }

    /// Get a single pixel's color from the composited surface
    ///
    /// Returns None if coordinates are out of bounds.
    pub fn get_pixel(&self, x: u32, y: u32) -> Option<[f32; 4]> {
        self.layers.composited_surface().surface().get_pixel(x, y)
    }
}
