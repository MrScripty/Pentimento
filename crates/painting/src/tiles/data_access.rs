//! Tile data access and region queries

use super::{TileCoord, TiledSurface};

impl TiledSurface {
    /// Get tile data for upload (returns pixel data for a tile)
    /// The returned Vec has tile_size * tile_size elements (or less for edge tiles)
    pub fn get_tile_data(&self, coord: TileCoord) -> Vec<[f32; 4]> {
        let tile_start_x = coord.x * self.tile_size;
        let tile_start_y = coord.y * self.tile_size;

        // Calculate actual tile dimensions (may be smaller at edges)
        let tile_width = self.tile_size.min(self.surface.width.saturating_sub(tile_start_x));
        let tile_height = self.tile_size.min(self.surface.height.saturating_sub(tile_start_y));

        let mut data = Vec::with_capacity((tile_width * tile_height) as usize);

        for dy in 0..tile_height {
            for dx in 0..tile_width {
                let x = tile_start_x + dx;
                let y = tile_start_y + dy;
                if let Some(pixel) = self.surface.get_pixel(x, y) {
                    data.push(pixel);
                }
            }
        }

        data
    }

    /// Get tile bounds (x, y, width, height) in pixel coordinates
    pub fn get_tile_bounds(&self, coord: TileCoord) -> (u32, u32, u32, u32) {
        let tile_start_x = coord.x * self.tile_size;
        let tile_start_y = coord.y * self.tile_size;

        let tile_width = self.tile_size.min(self.surface.width.saturating_sub(tile_start_x));
        let tile_height = self.tile_size.min(self.surface.height.saturating_sub(tile_start_y));

        (tile_start_x, tile_start_y, tile_width, tile_height)
    }

    /// Get pixel data for a rectangular region
    /// Returns Vec of [f32; 4] pixels in row-major order
    /// The region is clamped to surface bounds
    pub fn get_region_data(&self, x: u32, y: u32, width: u32, height: u32) -> Vec<[f32; 4]> {
        // Clamp to surface bounds
        let x_end = (x + width).min(self.surface.width);
        let y_end = (y + height).min(self.surface.height);
        let actual_width = x_end.saturating_sub(x);
        let actual_height = y_end.saturating_sub(y);

        if actual_width == 0 || actual_height == 0 {
            return Vec::new();
        }

        let mut data = Vec::with_capacity((actual_width * actual_height) as usize);

        for row in y..y_end {
            for col in x..x_end {
                if let Some(pixel) = self.surface.get_pixel(col, row) {
                    data.push(pixel);
                }
            }
        }

        data
    }

    /// Compute bounding box of given tile coordinates in pixel coordinates
    /// Returns (x, y, width, height) or None if no tiles provided
    pub fn compute_tiles_bounding_box(&self, tiles: &[TileCoord]) -> Option<(u32, u32, u32, u32)> {
        if tiles.is_empty() {
            return None;
        }

        let mut min_x = u32::MAX;
        let mut min_y = u32::MAX;
        let mut max_x = 0u32;
        let mut max_y = 0u32;

        for tile in tiles {
            let (tile_x, tile_y, tile_w, tile_h) = self.get_tile_bounds(*tile);
            min_x = min_x.min(tile_x);
            min_y = min_y.min(tile_y);
            max_x = max_x.max(tile_x + tile_w);
            max_y = max_y.max(tile_y + tile_h);
        }

        let width = max_x.saturating_sub(min_x);
        let height = max_y.saturating_sub(min_y);

        if width > 0 && height > 0 {
            Some((min_x, min_y, width, height))
        } else {
            None
        }
    }
}
