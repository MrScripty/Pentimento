//! Tile management and dirty tracking for CPU surfaces

mod dab_application;
mod data_access;
mod dirty_tracking;

use crate::constants::DEFAULT_TILE_SIZE;
use crate::surface::CpuSurface;
use std::collections::HashSet;

// Re-export the calculate_hardness_falloff function for tests
pub use dab_application::calculate_hardness_falloff;

/// Tile coordinates
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileCoord {
    pub x: u32,
    pub y: u32,
}

/// Manages tiled access to a surface with dirty tracking
pub struct TiledSurface {
    pub(crate) surface: CpuSurface,
    pub(crate) tile_size: u32,
    tiles_x: u32,
    tiles_y: u32,
    pub(crate) dirty_tiles: HashSet<TileCoord>,
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::BlendMode;

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

        let result = surface.apply_dab(128.0, 128.0, 10.0, [1.0, 0.0, 0.0, 1.0], 1.0, 1.0, BlendMode::Normal);

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

        surface.apply_dab(128.0, 128.0, 10.0, [1.0, 0.0, 0.0, 1.0], 1.0, 1.0, BlendMode::Normal);

        assert!(surface.has_dirty_tiles());
    }

    #[test]
    fn test_apply_dab_erase() {
        let mut surface = TiledSurface::new(256, 256, 128);
        // Fill with red
        surface.surface_mut().clear([1.0, 0.0, 0.0, 1.0]);

        // Erase at center
        let result = surface.apply_dab(128.0, 128.0, 10.0, [0.0, 0.0, 0.0, 1.0], 1.0, 1.0, BlendMode::Erase);

        assert!(result.is_some());

        // Center pixel should be erased (alpha reduced)
        let center = surface.surface().get_pixel(128, 128).unwrap();
        assert!(center[3] < 0.5); // Alpha should be reduced
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

    #[test]
    fn test_apply_dab_ellipse_circular() {
        // Ellipse with aspect_ratio=1.0 should behave like a circle
        let mut surface = TiledSurface::new(256, 256, 128);
        surface.surface_mut().clear([1.0, 1.0, 1.0, 1.0]);

        let result = surface.apply_dab_ellipse(
            128.0, 128.0, 10.0,
            [1.0, 0.0, 0.0, 1.0],
            1.0, 1.0,
            BlendMode::Normal,
            0.0,  // angle
            1.0,  // aspect_ratio (circular)
        );

        assert!(result.is_some());
        let center = surface.surface().get_pixel(128, 128).unwrap();
        assert!((center[0] - 1.0).abs() < 0.01); // Red
    }

    #[test]
    fn test_apply_dab_ellipse_stretched() {
        // Ellipse with aspect_ratio=0.5 should be stretched
        let mut surface = TiledSurface::new(256, 256, 128);
        surface.surface_mut().clear([1.0, 1.0, 1.0, 1.0]);

        let result = surface.apply_dab_ellipse(
            128.0, 128.0, 20.0,
            [1.0, 0.0, 0.0, 1.0],
            1.0, 1.0,
            BlendMode::Normal,
            0.0,      // angle = 0 (horizontal major axis)
            0.5,      // aspect_ratio (half as tall as wide)
        );

        assert!(result.is_some());

        // Center should be painted
        let center = surface.surface().get_pixel(128, 128).unwrap();
        assert!((center[0] - 1.0).abs() < 0.01);

        // Point along major axis (x direction) should be painted
        let on_major = surface.surface().get_pixel(145, 128).unwrap();
        assert!((on_major[0] - 1.0).abs() < 0.1); // Should be red

        // Point beyond minor axis extent should NOT be painted
        // radius_minor = 20 * 0.5 = 10, so y=128+12 should be outside
        let outside_minor = surface.surface().get_pixel(128, 141).unwrap();
        assert!((outside_minor[0] - 1.0).abs() < 0.01); // Should still be white (original)
        assert!((outside_minor[1] - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_apply_dab_ellipse_rotated() {
        // Rotated ellipse should paint in a rotated pattern
        let mut surface = TiledSurface::new(256, 256, 128);
        surface.surface_mut().clear([1.0, 1.0, 1.0, 1.0]);

        let angle = std::f32::consts::FRAC_PI_4; // 45 degrees
        let result = surface.apply_dab_ellipse(
            128.0, 128.0, 20.0,
            [1.0, 0.0, 0.0, 1.0],
            1.0, 1.0,
            BlendMode::Normal,
            angle,
            0.3,  // Very elliptical
        );

        assert!(result.is_some());

        // Center should be painted
        let center = surface.surface().get_pixel(128, 128).unwrap();
        assert!((center[0] - 1.0).abs() < 0.01);

        // The bounding box should be computed correctly for rotated ellipse
        let (_x, _y, w, h) = result.unwrap();
        assert!(w > 0 && h > 0);
        // For a 45-degree rotated ellipse, width and height should be similar
        // (not as extreme as unrotated would be)
    }
}
