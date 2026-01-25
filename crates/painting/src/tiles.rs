//! Tile management and dirty tracking for CPU surfaces

use crate::constants::DEFAULT_TILE_SIZE;
use crate::surface::CpuSurface;
use std::collections::HashSet;

/// Tile coordinates
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileCoord {
    pub x: u32,
    pub y: u32,
}

/// Manages tiled access to a surface with dirty tracking
pub struct TiledSurface {
    surface: CpuSurface,
    tile_size: u32,
    tiles_x: u32,
    tiles_y: u32,
    dirty_tiles: HashSet<TileCoord>,
}

impl TiledSurface {
    /// Create a new tiled surface with the given dimensions and tile size
    pub fn new(width: u32, height: u32, tile_size: u32) -> Self {
        let tiles_x = (width + tile_size - 1) / tile_size;
        let tiles_y = (height + tile_size - 1) / tile_size;

        Self {
            surface: CpuSurface::new(width, height),
            tile_size,
            tiles_x,
            tiles_y,
            dirty_tiles: HashSet::new(),
        }
    }

    /// Create a new tiled surface with the default tile size
    pub fn with_default_tile_size(width: u32, height: u32) -> Self {
        Self::new(width, height, DEFAULT_TILE_SIZE)
    }

    /// Get the tile size
    #[inline]
    pub fn tile_size(&self) -> u32 {
        self.tile_size
    }

    /// Get the number of tiles in x direction
    #[inline]
    pub fn tiles_x(&self) -> u32 {
        self.tiles_x
    }

    /// Get the number of tiles in y direction
    #[inline]
    pub fn tiles_y(&self) -> u32 {
        self.tiles_y
    }

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

        // Mark all tiles in the range
        for ty in tile_y_start..=tile_y_end {
            for tx in tile_x_start..=tile_x_end {
                self.dirty_tiles.insert(TileCoord { x: tx, y: ty });
            }
        }
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

    /// Get the underlying surface for direct pixel access
    #[inline]
    pub fn surface(&self) -> &CpuSurface {
        &self.surface
    }

    /// Get mutable access to the underlying surface
    #[inline]
    pub fn surface_mut(&mut self) -> &mut CpuSurface {
        &mut self.surface
    }

    /// Apply a dab to the surface (basic circle stamp)
    /// Returns bounding box of affected region (x, y, width, height)
    /// Returns None if the dab is completely outside the surface
    pub fn apply_dab(
        &mut self,
        center_x: f32,
        center_y: f32,
        radius: f32,
        color: [f32; 4],
        opacity: f32,
        hardness: f32,
    ) -> Option<(u32, u32, u32, u32)> {
        if radius <= 0.0 || opacity <= 0.0 {
            return None;
        }

        // Calculate bounding box
        let x_min_f = (center_x - radius).floor();
        let y_min_f = (center_y - radius).floor();
        let x_max_f = (center_x + radius).ceil();
        let y_max_f = (center_y + radius).ceil();

        // Clamp to surface bounds
        let x_min = (x_min_f.max(0.0) as u32).min(self.surface.width);
        let y_min = (y_min_f.max(0.0) as u32).min(self.surface.height);
        let x_max = (x_max_f.max(0.0) as u32).min(self.surface.width);
        let y_max = (y_max_f.max(0.0) as u32).min(self.surface.height);

        // Check if completely outside
        if x_min >= x_max || y_min >= y_max {
            return None;
        }

        let radius_sq = radius * radius;

        // Apply dab to each pixel in the bounding box
        for py in y_min..y_max {
            for px in x_min..x_max {
                // Calculate distance from center (use pixel center)
                let dx = (px as f32 + 0.5) - center_x;
                let dy = (py as f32 + 0.5) - center_y;
                let dist_sq = dx * dx + dy * dy;

                // Skip if outside the circle
                if dist_sq > radius_sq {
                    continue;
                }

                // Calculate normalized distance (0 at center, 1 at edge)
                let distance_normalized = (dist_sq.sqrt() / radius).min(1.0);

                // Calculate falloff based on hardness
                let falloff = calculate_hardness_falloff(distance_normalized, hardness);

                if falloff > 0.0 {
                    // Blend the color with the calculated falloff
                    let effective_opacity = opacity * falloff;
                    self.surface.blend_pixel(px, py, color, effective_opacity);
                }
            }
        }

        // Mark the affected region as dirty
        let width = x_max - x_min;
        let height = y_max - y_min;
        self.mark_region_dirty(x_min, y_min, width, height);

        Some((x_min, y_min, width, height))
    }
}

/// Calculate falloff based on hardness
/// distance_normalized is 0 at center, 1 at edge
/// hardness is 0.0 (soft) to 1.0 (hard)
#[inline]
fn calculate_hardness_falloff(distance_normalized: f32, hardness: f32) -> f32 {
    if hardness >= 1.0 {
        // Pure hard edge
        if distance_normalized <= 1.0 {
            1.0
        } else {
            0.0
        }
    } else {
        let t = distance_normalized.clamp(0.0, 1.0);
        let soft = 1.0 - t; // Linear falloff for soft brush
        let hard = if t <= 1.0 { 1.0 } else { 0.0 };
        // Interpolate between soft and hard based on hardness
        soft * (1.0 - hardness) + hard * hardness
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tiled_surface_creation() {
        let surface = TiledSurface::new(256, 256, 128);
        assert_eq!(surface.tiles_x(), 2);
        assert_eq!(surface.tiles_y(), 2);
        assert_eq!(surface.tile_size(), 128);
    }

    #[test]
    fn test_tiled_surface_non_aligned() {
        // 300x300 with 128 tile size should give 3x3 tiles
        let surface = TiledSurface::new(300, 300, 128);
        assert_eq!(surface.tiles_x(), 3);
        assert_eq!(surface.tiles_y(), 3);
    }

    #[test]
    fn test_mark_dirty() {
        let mut surface = TiledSurface::new(256, 256, 128);

        surface.mark_dirty(0, 0);
        assert!(surface.has_dirty_tiles());
        assert_eq!(surface.dirty_tile_count(), 1);

        surface.mark_dirty(130, 130);
        assert_eq!(surface.dirty_tile_count(), 2);

        let tiles = surface.take_dirty_tiles();
        assert_eq!(tiles.len(), 2);
        assert!(!surface.has_dirty_tiles());
    }

    #[test]
    fn test_mark_region_dirty() {
        let mut surface = TiledSurface::new(256, 256, 128);

        // Mark a region that spans all 4 tiles
        surface.mark_region_dirty(100, 100, 56, 56);
        assert_eq!(surface.dirty_tile_count(), 4);
    }

    #[test]
    fn test_get_tile_data() {
        let mut surface = TiledSurface::new(256, 256, 128);

        // Set a known pixel
        surface.surface_mut().set_pixel(0, 0, [1.0, 0.0, 0.0, 1.0]);

        let tile_data = surface.get_tile_data(TileCoord { x: 0, y: 0 });
        assert_eq!(tile_data.len(), 128 * 128);
        assert_eq!(tile_data[0], [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn test_apply_dab() {
        let mut surface = TiledSurface::new(256, 256, 128);
        surface.surface_mut().clear([1.0, 1.0, 1.0, 1.0]);

        let result = surface.apply_dab(128.0, 128.0, 10.0, [1.0, 0.0, 0.0, 1.0], 1.0, 1.0);

        assert!(result.is_some());
        let (_x, _y, w, h) = result.unwrap();
        assert!(w > 0 && h > 0);

        // Center pixel should be red
        let center = surface.surface().get_pixel(128, 128).unwrap();
        assert!((center[0] - 1.0).abs() < 0.01); // Red
        assert!(center[1] < 0.5); // Not white anymore
    }

    #[test]
    fn test_apply_dab_marks_dirty() {
        let mut surface = TiledSurface::new(256, 256, 128);

        surface.apply_dab(128.0, 128.0, 10.0, [1.0, 0.0, 0.0, 1.0], 1.0, 1.0);

        assert!(surface.has_dirty_tiles());
    }

    #[test]
    fn test_hardness_falloff() {
        // Hard brush (hardness = 1.0)
        assert_eq!(calculate_hardness_falloff(0.0, 1.0), 1.0);
        assert_eq!(calculate_hardness_falloff(0.5, 1.0), 1.0);
        assert_eq!(calculate_hardness_falloff(1.0, 1.0), 1.0);

        // Soft brush (hardness = 0.0)
        assert_eq!(calculate_hardness_falloff(0.0, 0.0), 1.0);
        assert_eq!(calculate_hardness_falloff(0.5, 0.0), 0.5);
        assert_eq!(calculate_hardness_falloff(1.0, 0.0), 0.0);

        // Medium brush (hardness = 0.5)
        let mid = calculate_hardness_falloff(0.5, 0.5);
        assert!(mid > 0.5 && mid < 1.0); // Between soft and hard
    }

    #[test]
    fn test_edge_tile_data() {
        // Create a surface where edge tiles are partial
        let surface = TiledSurface::new(150, 150, 128);

        // Get the edge tile (should be 22x22 pixels)
        let tile_data = surface.get_tile_data(TileCoord { x: 1, y: 1 });
        assert_eq!(tile_data.len(), 22 * 22);
    }

    #[test]
    fn test_get_tile_bounds() {
        let surface = TiledSurface::new(150, 150, 128);

        let (x, y, w, h) = surface.get_tile_bounds(TileCoord { x: 0, y: 0 });
        assert_eq!((x, y, w, h), (0, 0, 128, 128));

        let (x, y, w, h) = surface.get_tile_bounds(TileCoord { x: 1, y: 1 });
        assert_eq!((x, y, w, h), (128, 128, 22, 22));
    }
}
