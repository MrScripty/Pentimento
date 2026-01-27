//! Projection target storage abstraction for projection painting.
//!
//! This module provides:
//! - [`ProjectionTargetStorage`] trait for different storage backends
//! - [`UvAtlasTarget`] for meshes with UV coordinates
//! - Placeholder for PTex targets (implemented by another agent)

use glam::Vec2;

use crate::tiles::TiledSurface;
use crate::types::{BlendMode, MeshHit, MeshStorageMode};

/// Dirty region for GPU upload
#[derive(Clone)]
pub struct DirtyRegion {
    /// Pixel offset in texture (x, y)
    pub offset: (u32, u32),
    /// Region dimensions (width, height)
    pub size: (u32, u32),
    /// RGBA8 pixel data (row-major)
    pub data: Vec<u8>,
}

/// Trait for projection target storage backends.
///
/// Implementations handle storing and retrieving projected paint data,
/// whether using traditional UV atlases or per-face PTex tiles.
pub trait ProjectionTargetStorage {
    /// Get the storage mode for this target
    fn storage_mode(&self) -> MeshStorageMode;

    /// Convert a mesh hit to texture coordinates.
    ///
    /// For UV atlas targets, this returns the interpolated UV from the mesh.
    /// For PTex targets, this returns face-local coordinates.
    fn hit_to_tex_coord(&self, hit: &MeshHit) -> Option<Vec2>;

    /// Apply a projected pixel to the target storage.
    ///
    /// # Arguments
    /// * `tex_coord` - Texture coordinate (UV or face-local)
    /// * `color` - RGBA color to apply
    /// * `opacity` - Opacity for blending (0.0-1.0)
    /// * `blend_mode` - How to combine with existing pixels
    fn apply_projected_pixel(
        &mut self,
        tex_coord: Vec2,
        color: [f32; 4],
        opacity: f32,
        blend_mode: BlendMode,
    );

    /// Apply a projected dab (brush stamp) to the target storage.
    ///
    /// # Arguments
    /// * `tex_coord` - Center position in texture coordinates
    /// * `radius` - Radius in texture pixels
    /// * `color` - RGBA color to apply
    /// * `opacity` - Overall opacity (0.0-1.0)
    /// * `hardness` - Edge hardness (0.0=soft, 1.0=hard)
    /// * `blend_mode` - How to combine with existing pixels
    fn apply_projected_dab(
        &mut self,
        tex_coord: Vec2,
        radius: f32,
        color: [f32; 4],
        opacity: f32,
        hardness: f32,
        blend_mode: BlendMode,
    );

    /// Take dirty regions for GPU upload, clearing the dirty state.
    fn take_dirty_regions(&mut self) -> Vec<DirtyRegion>;

    /// Check if there are any dirty regions pending upload.
    fn has_dirty_regions(&self) -> bool;

    /// Get the texture resolution.
    fn resolution(&self) -> (u32, u32);

    /// Clear the entire surface to a given color.
    fn clear(&mut self, color: [f32; 4]);
}

/// UV atlas projection target for meshes with UV coordinates.
///
/// Stores projected paint in a traditional 2D texture that maps
/// to mesh UVs.
pub struct UvAtlasTarget {
    /// Tiled CPU surface for paint storage
    surface: TiledSurface,
    /// Texture resolution (width, height)
    resolution: (u32, u32),
}

impl UvAtlasTarget {
    /// Create a new UV atlas target with the given resolution.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            surface: TiledSurface::with_default_tile_size(width, height),
            resolution: (width, height),
        }
    }

    /// Get the underlying surface for direct access.
    pub fn surface(&self) -> &TiledSurface {
        &self.surface
    }

    /// Get mutable access to the underlying surface.
    pub fn surface_mut(&mut self) -> &mut TiledSurface {
        &mut self.surface
    }
}

impl ProjectionTargetStorage for UvAtlasTarget {
    fn storage_mode(&self) -> MeshStorageMode {
        MeshStorageMode::UvAtlas {
            resolution: self.resolution,
        }
    }

    fn hit_to_tex_coord(&self, hit: &MeshHit) -> Option<Vec2> {
        // For UV atlas, use the interpolated UV from the mesh
        hit.uv
    }

    fn apply_projected_pixel(
        &mut self,
        tex_coord: Vec2,
        color: [f32; 4],
        opacity: f32,
        blend_mode: BlendMode,
    ) {
        // Convert UV (0-1) to pixel coordinates
        let px = (tex_coord.x * self.resolution.0 as f32) as u32;
        let py = (tex_coord.y * self.resolution.1 as f32) as u32;

        if px >= self.resolution.0 || py >= self.resolution.1 {
            return;
        }

        match blend_mode {
            BlendMode::Normal => {
                self.surface.surface_mut().blend_pixel(px, py, color, opacity);
            }
            BlendMode::Erase => {
                self.surface.surface_mut().erase_pixel(px, py, opacity);
            }
        }

        self.surface.mark_dirty(px, py);
    }

    fn apply_projected_dab(
        &mut self,
        tex_coord: Vec2,
        radius: f32,
        color: [f32; 4],
        opacity: f32,
        hardness: f32,
        blend_mode: BlendMode,
    ) {
        // Convert UV (0-1) to pixel coordinates
        let center_x = tex_coord.x * self.resolution.0 as f32;
        let center_y = tex_coord.y * self.resolution.1 as f32;

        self.surface.apply_dab(
            center_x, center_y, radius, color, opacity, hardness, blend_mode,
        );
    }

    fn take_dirty_regions(&mut self) -> Vec<DirtyRegion> {
        let dirty_tiles = self.surface.take_dirty_tiles();
        let mut regions = Vec::with_capacity(dirty_tiles.len());

        for tile_coord in dirty_tiles {
            let (x, y, w, h) = self.surface.get_tile_bounds(tile_coord);
            let tile_data = self.surface.get_tile_data(tile_coord);

            // Convert f32 RGBA to u8 RGBA
            let rgba8 = tile_data_to_rgba8(&tile_data);

            regions.push(DirtyRegion {
                offset: (x, y),
                size: (w, h),
                data: rgba8,
            });
        }

        regions
    }

    fn has_dirty_regions(&self) -> bool {
        self.surface.has_dirty_tiles()
    }

    fn resolution(&self) -> (u32, u32) {
        self.resolution
    }

    fn clear(&mut self, color: [f32; 4]) {
        self.surface.surface_mut().clear(color);
        // Mark all tiles dirty after clear
        for ty in 0..self.surface.tiles_y() {
            for tx in 0..self.surface.tiles_x() {
                self.surface.mark_dirty(tx * self.surface.tile_size(), ty * self.surface.tile_size());
            }
        }
    }
}

/// Convert f32 RGBA tile data to u8 RGBA for GPU upload.
fn tile_data_to_rgba8(tile_data: &[[f32; 4]]) -> Vec<u8> {
    let mut output = Vec::with_capacity(tile_data.len() * 4);

    for pixel in tile_data {
        output.push(linear_to_srgb_u8(pixel[0]));
        output.push(linear_to_srgb_u8(pixel[1]));
        output.push(linear_to_srgb_u8(pixel[2]));
        output.push((pixel[3].clamp(0.0, 1.0) * 255.0) as u8);
    }

    output
}

/// Convert linear float to sRGB u8.
#[inline]
fn linear_to_srgb_u8(linear: f32) -> u8 {
    let linear = linear.clamp(0.0, 1.0);
    let srgb = if linear <= 0.0031308 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    };
    (srgb * 255.0) as u8
}

/// Placeholder PTex target stub.
///
/// This is a placeholder for the PTex implementation being done by another agent.
/// It provides the interface but panics if used, as the actual implementation
/// should come from the PTex system.
pub struct PtexTargetStub {
    face_resolution: u32,
}

impl PtexTargetStub {
    /// Create a new PTex target stub.
    pub fn new(face_resolution: u32) -> Self {
        Self { face_resolution }
    }
}

impl ProjectionTargetStorage for PtexTargetStub {
    fn storage_mode(&self) -> MeshStorageMode {
        MeshStorageMode::Ptex {
            face_resolution: self.face_resolution,
        }
    }

    fn hit_to_tex_coord(&self, hit: &MeshHit) -> Option<Vec2> {
        // For PTex, convert barycentric to face-local coordinates
        // The actual mapping depends on the PTex implementation
        Some(crate::projection::barycentric_to_ptex_coords(
            hit.barycentric,
            self.face_resolution,
        ))
    }

    fn apply_projected_pixel(
        &mut self,
        _tex_coord: Vec2,
        _color: [f32; 4],
        _opacity: f32,
        _blend_mode: BlendMode,
    ) {
        // TODO: Delegate to PTex storage when available
        tracing::warn!("PTex projection not yet implemented");
    }

    fn apply_projected_dab(
        &mut self,
        _tex_coord: Vec2,
        _radius: f32,
        _color: [f32; 4],
        _opacity: f32,
        _hardness: f32,
        _blend_mode: BlendMode,
    ) {
        // TODO: Delegate to PTex storage when available
        tracing::warn!("PTex projection not yet implemented");
    }

    fn take_dirty_regions(&mut self) -> Vec<DirtyRegion> {
        // PTex uses a different upload mechanism
        Vec::new()
    }

    fn has_dirty_regions(&self) -> bool {
        false
    }

    fn resolution(&self) -> (u32, u32) {
        // PTex doesn't have a single resolution
        (self.face_resolution, self.face_resolution)
    }

    fn clear(&mut self, _color: [f32; 4]) {
        tracing::warn!("PTex clear not yet implemented");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uv_atlas_target_creation() {
        let target = UvAtlasTarget::new(512, 512);
        assert_eq!(target.resolution(), (512, 512));
        assert!(!target.has_dirty_regions());
    }

    #[test]
    fn test_uv_atlas_apply_pixel() {
        let mut target = UvAtlasTarget::new(256, 256);

        // Apply a red pixel at UV (0.5, 0.5) = pixel (128, 128)
        target.apply_projected_pixel(
            Vec2::new(0.5, 0.5),
            [1.0, 0.0, 0.0, 1.0],
            1.0,
            BlendMode::Normal,
        );

        assert!(target.has_dirty_regions());

        // Check the pixel was written
        let pixel = target.surface().surface().get_pixel(128, 128).unwrap();
        assert!((pixel[0] - 1.0).abs() < 0.01); // Red
    }

    #[test]
    fn test_uv_atlas_apply_dab() {
        let mut target = UvAtlasTarget::new(256, 256);
        target.clear([1.0, 1.0, 1.0, 1.0]); // White background

        // Clear dirty regions from clear
        let _ = target.take_dirty_regions();

        // Apply a blue dab
        target.apply_projected_dab(
            Vec2::new(0.5, 0.5),
            10.0,
            [0.0, 0.0, 1.0, 1.0],
            1.0,
            1.0,
            BlendMode::Normal,
        );

        assert!(target.has_dirty_regions());

        // Center should be blue
        let center = target.surface().surface().get_pixel(128, 128).unwrap();
        assert!((center[2] - 1.0).abs() < 0.01); // Blue
    }

    #[test]
    fn test_uv_atlas_dirty_regions() {
        let mut target = UvAtlasTarget::new(256, 256);

        // Apply pixels in different tiles
        target.apply_projected_pixel(Vec2::new(0.1, 0.1), [1.0, 0.0, 0.0, 1.0], 1.0, BlendMode::Normal);
        target.apply_projected_pixel(Vec2::new(0.9, 0.9), [0.0, 1.0, 0.0, 1.0], 1.0, BlendMode::Normal);

        let regions = target.take_dirty_regions();
        assert_eq!(regions.len(), 2);

        // After taking, should have no dirty regions
        assert!(!target.has_dirty_regions());
    }

    #[test]
    fn test_hit_to_tex_coord_uv() {
        let target = UvAtlasTarget::new(256, 256);

        let hit = MeshHit {
            world_pos: Vec3::ZERO,
            face_id: 0,
            barycentric: Vec3::new(0.33, 0.33, 0.34),
            normal: Vec3::Y,
            tangent: Vec3::X,
            bitangent: Vec3::Z,
            uv: Some(Vec2::new(0.5, 0.5)),
        };

        let tex_coord = target.hit_to_tex_coord(&hit);
        assert!(tex_coord.is_some());
        let coord = tex_coord.unwrap();
        assert!((coord.x - 0.5).abs() < 0.01);
        assert!((coord.y - 0.5).abs() < 0.01);
    }

    use glam::Vec3;
}
