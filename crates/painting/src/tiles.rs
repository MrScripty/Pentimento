//! Tile management and dirty tracking for CPU surfaces

use tracing::debug;

use crate::constants::DEFAULT_TILE_SIZE;
use crate::surface::CpuSurface;
use crate::types::BlendMode;
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

    /// Apply a dab to the surface (basic circle stamp)
    /// Returns bounding box of affected region (x, y, width, height)
    /// Returns None if the dab is completely outside the surface
    ///
    /// This is a convenience wrapper around `apply_dab_ellipse` for circular dabs.
    pub fn apply_dab(
        &mut self,
        center_x: f32,
        center_y: f32,
        radius: f32,
        color: [f32; 4],
        opacity: f32,
        hardness: f32,
        blend_mode: BlendMode,
    ) -> Option<(u32, u32, u32, u32)> {
        // Delegate to ellipse implementation with circular parameters
        self.apply_dab_ellipse(
            center_x,
            center_y,
            radius,
            color,
            opacity,
            hardness,
            blend_mode,
            0.0, // angle
            1.0, // aspect_ratio (1.0 = circle)
        )
    }

    /// Apply an elliptical dab to the surface with rotation support.
    ///
    /// This is the core dab application function that supports both circular and
    /// elliptical brushes with arbitrary rotation. Used for 3D mesh painting where
    /// brushes must be projected onto surfaces at oblique angles.
    ///
    /// # Arguments
    /// * `center_x`, `center_y` - Dab center in pixel coordinates
    /// * `radius` - Radius along the major axis (in pixels)
    /// * `color` - RGBA color to apply
    /// * `opacity` - Overall opacity (0.0 to 1.0)
    /// * `hardness` - Edge hardness (0.0 = soft, 1.0 = hard)
    /// * `blend_mode` - How to combine with existing pixels
    /// * `angle` - Rotation angle in radians (counter-clockwise)
    /// * `aspect_ratio` - Ratio of minor to major axis (1.0 = circle, 0.5 = 2:1 ellipse)
    ///
    /// # Returns
    /// Bounding box of affected region (x, y, width, height), or None if outside surface.
    pub fn apply_dab_ellipse(
        &mut self,
        center_x: f32,
        center_y: f32,
        radius: f32,
        color: [f32; 4],
        opacity: f32,
        hardness: f32,
        blend_mode: BlendMode,
        angle: f32,
        aspect_ratio: f32,
    ) -> Option<(u32, u32, u32, u32)> {
        debug!(
            "TiledSurface::apply_dab_ellipse: center=({:.1}, {:.1}), radius={:.1}, aspect={:.2}, angle={:.2}rad, opacity={:.2}, hardness={:.2}, mode={:?}",
            center_x, center_y, radius, aspect_ratio, angle, opacity, hardness, blend_mode
        );

        if radius <= 0.0 || opacity <= 0.0 || aspect_ratio <= 0.0 {
            debug!("  -> skipped: invalid radius, opacity, or aspect_ratio");
            return None;
        }

        // Clamp aspect ratio to prevent extreme ellipses
        let aspect_ratio = aspect_ratio.clamp(0.01, 1.0);

        // For ellipse bounding box, we need to account for rotation.
        // The bounding box of a rotated ellipse with semi-axes a (major) and b (minor)
        // rotated by angle θ has half-widths:
        //   half_w = sqrt(a² cos²θ + b² sin²θ)
        //   half_h = sqrt(a² sin²θ + b² cos²θ)
        let cos_a = angle.cos();
        let sin_a = angle.sin();
        let cos_sq = cos_a * cos_a;
        let sin_sq = sin_a * sin_a;

        let radius_major = radius;
        let radius_minor = radius * aspect_ratio;
        let r_major_sq = radius_major * radius_major;
        let r_minor_sq = radius_minor * radius_minor;

        let half_w = (r_major_sq * cos_sq + r_minor_sq * sin_sq).sqrt();
        let half_h = (r_major_sq * sin_sq + r_minor_sq * cos_sq).sqrt();

        // Calculate bounding box
        let x_min_f = (center_x - half_w).floor();
        let y_min_f = (center_y - half_h).floor();
        let x_max_f = (center_x + half_w).ceil();
        let y_max_f = (center_y + half_h).ceil();

        // Clamp to surface bounds
        let x_min = (x_min_f.max(0.0) as u32).min(self.surface.width);
        let y_min = (y_min_f.max(0.0) as u32).min(self.surface.height);
        let x_max = (x_max_f.max(0.0) as u32).min(self.surface.width);
        let y_max = (y_max_f.max(0.0) as u32).min(self.surface.height);

        // Check if completely outside
        if x_min >= x_max || y_min >= y_max {
            return None;
        }

        // Apply dab to each pixel in the bounding box
        for py in y_min..y_max {
            for px in x_min..x_max {
                // Calculate distance from center (use pixel center)
                let dx = (px as f32 + 0.5) - center_x;
                let dy = (py as f32 + 0.5) - center_y;

                // Rotate point by -angle to align with ellipse axes
                let rotated_x = dx * cos_a + dy * sin_a;
                let rotated_y = -dx * sin_a + dy * cos_a;

                // Normalize to unit circle space (ellipse becomes circle)
                let normalized_x = rotated_x / radius_major;
                let normalized_y = rotated_y / radius_minor;
                let normalized_dist_sq = normalized_x * normalized_x + normalized_y * normalized_y;

                // Skip if outside the ellipse (distance > 1 in normalized space)
                if normalized_dist_sq > 1.0 {
                    continue;
                }

                // Calculate normalized distance (0 at center, 1 at edge)
                let distance_normalized = normalized_dist_sq.sqrt();

                // Calculate falloff based on hardness
                let falloff = calculate_hardness_falloff(distance_normalized, hardness);

                if falloff > 0.0 {
                    let effective_opacity = opacity * falloff;
                    match blend_mode {
                        BlendMode::Normal => {
                            // Blend the color with the calculated falloff
                            self.surface.blend_pixel(px, py, color, effective_opacity);
                        }
                        BlendMode::Erase => {
                            // Erase by reducing alpha
                            self.surface.erase_pixel(px, py, effective_opacity);
                        }
                    }
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
