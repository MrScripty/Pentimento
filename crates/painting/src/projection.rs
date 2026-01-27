//! Brush projection math for 3D mesh painting.
//!
//! This module handles projecting brushes onto mesh surfaces using tangent-space
//! calculations to ensure brushes appear circular on the surface regardless of
//! viewing angle.
//!
//! It also provides utilities for projection painting, where a 2D canvas is
//! projected onto 3D geometry from a fixed camera position.

use glam::{Vec2, Vec3};

use crate::types::ProjectedDab;

/// Construct an orthonormal tangent-space basis from a surface normal.
///
/// Returns (tangent, bitangent, normal) forming a right-handed coordinate system
/// where normal points "up" from the surface.
///
/// # Arguments
/// * `normal` - The surface normal (will be normalized)
/// * `reference_tangent` - Optional tangent from mesh vertex data. If provided,
///   it will be orthogonalized against the normal using Gram-Schmidt.
///   If None, an arbitrary perpendicular vector is chosen.
///
/// # Returns
/// A tuple of (tangent, bitangent, normal) vectors, all normalized.
pub fn build_tangent_space(normal: Vec3, reference_tangent: Option<Vec3>) -> (Vec3, Vec3, Vec3) {
    let n = normal.normalize();

    let t = if let Some(ref_t) = reference_tangent {
        // Gram-Schmidt orthogonalization: remove component parallel to normal
        let projected = ref_t - n * n.dot(ref_t);
        if projected.length_squared() > 1e-6 {
            projected.normalize()
        } else {
            // Reference tangent was parallel to normal, fall back to arbitrary
            arbitrary_perpendicular(n)
        }
    } else {
        arbitrary_perpendicular(n)
    };

    // Bitangent completes the right-handed basis
    let b = n.cross(t).normalize();

    (t, b, n)
}

/// Find an arbitrary vector perpendicular to the given normal.
///
/// Uses the axis least aligned with the normal to ensure numerical stability.
fn arbitrary_perpendicular(normal: Vec3) -> Vec3 {
    // Choose the axis that is least aligned with the normal
    let axis = if normal.x.abs() < normal.y.abs() {
        if normal.x.abs() < normal.z.abs() {
            Vec3::X
        } else {
            Vec3::Z
        }
    } else if normal.y.abs() < normal.z.abs() {
        Vec3::Y
    } else {
        Vec3::Z
    };

    normal.cross(axis).normalize()
}

/// Project a brush dab onto a surface tangent plane.
///
/// This is the core function for ensuring brushes appear circular on the surface
/// regardless of viewing angle. The brush is always circular in tangent space,
/// which means it appears undistorted on the surface.
///
/// # Arguments
/// * `brush_world_radius` - Brush radius in world units
/// * `hit_pos` - World position where the brush hits the surface
/// * `tex_pos` - Position in texture space (UV or Ptex local coordinates)
/// * `surface_normal` - Surface normal at hit point
/// * `world_to_texel_scale` - Conversion factor from world units to texture pixels
///
/// # Returns
/// A `ProjectedDab` with texture-space position, size, and shape parameters.
///
/// # Note
/// Since we're projecting from the surface normal (not camera view), the brush
/// is always circular in tangent space. The `angle` and `aspect_ratio` in the
/// returned `ProjectedDab` are for future use with brush rotation/tilt.
pub fn project_brush_to_surface(
    brush_world_radius: f32,
    _hit_pos: Vec3,
    tex_pos: Vec2,
    _surface_normal: Vec3,
    world_to_texel_scale: f32,
) -> ProjectedDab {
    // For normal-based projection, the brush is always circular in tangent space.
    // The size is converted from world units to texture pixels.
    let texel_radius = brush_world_radius * world_to_texel_scale;

    ProjectedDab {
        tex_pos,
        size: texel_radius * 2.0, // Diameter
        angle: 0.0,               // No rotation (aligned with tangent)
        aspect_ratio: 1.0,        // Circular on surface
    }
}

/// Convert world-space brush size to texture-space pixels.
///
/// This accounts for the mapping between world units and UV/texture coordinates.
///
/// # Arguments
/// * `world_radius` - Brush radius in world units
/// * `world_to_uv_scale` - How many UV units per world unit (varies across the surface)
/// * `texture_resolution` - Texture dimensions in pixels (width, height)
///
/// # Returns
/// Brush radius in texture pixels.
pub fn world_to_texel_size(
    world_radius: f32,
    world_to_uv_scale: Vec2,
    texture_resolution: (u32, u32),
) -> f32 {
    // Average scale factor (handles non-uniform UV scaling)
    let avg_uv_per_world = (world_to_uv_scale.x + world_to_uv_scale.y) / 2.0;
    let uv_radius = world_radius * avg_uv_per_world;

    // UV is 0-1, convert to pixels using average resolution
    let avg_resolution = (texture_resolution.0 + texture_resolution.1) as f32 / 2.0;
    uv_radius * avg_resolution
}

/// Estimate the world-to-UV scale at a point on a triangle.
///
/// This computes how UV coordinates change relative to world-space positions,
/// which is needed to properly size brushes in texture space.
///
/// # Arguments
/// * `p0`, `p1`, `p2` - Triangle vertex positions in world space
/// * `uv0`, `uv1`, `uv2` - Triangle vertex UV coordinates
///
/// # Returns
/// Approximate UV units per world unit as a Vec2 (u_scale, v_scale).
pub fn estimate_uv_scale(
    p0: Vec3,
    p1: Vec3,
    p2: Vec3,
    uv0: Vec2,
    uv1: Vec2,
    uv2: Vec2,
) -> Vec2 {
    // Edge vectors in world space
    let e1_world = p1 - p0;
    let e2_world = p2 - p0;

    // Edge vectors in UV space
    let e1_uv = uv1 - uv0;
    let e2_uv = uv2 - uv0;

    // Compute world-space edge lengths
    let len1_world = e1_world.length();
    let len2_world = e2_world.length();

    // Compute UV-space edge lengths
    let len1_uv = e1_uv.length();
    let len2_uv = e2_uv.length();

    // Avoid division by zero
    let scale1 = if len1_world > 1e-6 {
        len1_uv / len1_world
    } else {
        1.0
    };
    let scale2 = if len2_world > 1e-6 {
        len2_uv / len2_world
    } else {
        1.0
    };

    // Return average scale (could be more sophisticated for anisotropic UVs)
    Vec2::new(scale1, scale2)
}

/// Convert barycentric coordinates to face-local texture coordinates for Ptex.
///
/// For Ptex-style per-face textures, we need to map barycentric coordinates
/// to a square texture tile. This uses a simple mapping where the triangle
/// is mapped to the lower-left half of the square.
///
/// # Arguments
/// * `barycentric` - Barycentric coordinates (u, v, w) where w = 1 - u - v
/// * `face_resolution` - Resolution of the face's texture tile
///
/// # Returns
/// Texture coordinates in pixels within the face tile.
pub fn barycentric_to_ptex_coords(barycentric: Vec3, face_resolution: u32) -> Vec2 {
    // Simple mapping: use first two barycentric coordinates directly
    // This maps the triangle to roughly half the texture space
    // A more sophisticated approach would use a triangle-to-square mapping
    let u = barycentric.x;
    let v = barycentric.y;

    Vec2::new(
        u * face_resolution as f32,
        v * face_resolution as f32,
    )
}

// ============================================================================
// Canvas Projection Utilities
// ============================================================================

/// Parameters describing a canvas plane for projection.
#[derive(Debug, Clone, Copy)]
pub struct CanvasPlaneParams {
    /// Canvas resolution in pixels (width, height)
    pub resolution: (u32, u32),
    /// World-space dimensions of the canvas plane
    pub world_size: (f32, f32),
}

/// Convert a canvas UV coordinate to a world-space position on the canvas plane.
///
/// # Arguments
/// * `canvas_uv` - UV coordinate on the canvas (0-1 range)
/// * `canvas_center` - World-space center position of the canvas plane
/// * `canvas_right` - Right direction vector of the canvas (local +X, normalized)
/// * `canvas_up` - Up direction vector of the canvas (local +Y, normalized)
/// * `canvas_params` - Canvas dimensions and resolution
///
/// # Returns
/// World-space position of the UV point on the canvas plane.
pub fn canvas_uv_to_world(
    canvas_uv: Vec2,
    canvas_center: Vec3,
    canvas_right: Vec3,
    canvas_up: Vec3,
    canvas_params: &CanvasPlaneParams,
) -> Vec3 {
    // UV is 0-1, convert to local position relative to center
    // UV (0,0) is top-left, (1,1) is bottom-right
    // Local coords: X goes right, Y goes up (but UV Y is inverted)
    let local_x = (canvas_uv.x - 0.5) * canvas_params.world_size.0;
    let local_y = (0.5 - canvas_uv.y) * canvas_params.world_size.1; // Invert Y for texture coords

    canvas_center + canvas_right * local_x + canvas_up * local_y
}

/// Construct a ray from the camera through a canvas UV point.
///
/// This is the core function for projection painting - it creates a ray
/// that can be intersected with scene geometry to project paint.
///
/// # Arguments
/// * `camera_pos` - World-space camera position
/// * `canvas_uv` - UV coordinate on the canvas (0-1 range)
/// * `canvas_center` - World-space center of the canvas plane
/// * `canvas_right` - Right direction of the canvas (normalized)
/// * `canvas_up` - Up direction of the canvas (normalized)
/// * `canvas_params` - Canvas dimensions
///
/// # Returns
/// Tuple of (ray_origin, ray_direction) where direction is normalized.
pub fn canvas_uv_to_ray(
    camera_pos: Vec3,
    canvas_uv: Vec2,
    canvas_center: Vec3,
    canvas_right: Vec3,
    canvas_up: Vec3,
    canvas_params: &CanvasPlaneParams,
) -> (Vec3, Vec3) {
    let canvas_world_pos = canvas_uv_to_world(
        canvas_uv,
        canvas_center,
        canvas_right,
        canvas_up,
        canvas_params,
    );

    let ray_origin = camera_pos;
    let ray_direction = (canvas_world_pos - camera_pos).normalize();

    (ray_origin, ray_direction)
}

/// Convert a pixel coordinate on the canvas to UV.
///
/// # Arguments
/// * `pixel` - Pixel coordinate (x, y)
/// * `resolution` - Canvas resolution (width, height)
///
/// # Returns
/// UV coordinate in 0-1 range (center of pixel).
pub fn pixel_to_canvas_uv(pixel: (u32, u32), resolution: (u32, u32)) -> Vec2 {
    Vec2::new(
        (pixel.0 as f32 + 0.5) / resolution.0 as f32,
        (pixel.1 as f32 + 0.5) / resolution.1 as f32,
    )
}

/// Extract canvas plane vectors from a 4x4 transform matrix.
///
/// # Arguments
/// * `transform` - 4x4 transformation matrix (column-major, as used by Bevy)
///
/// # Returns
/// Tuple of (center, right, up, forward) vectors.
pub fn extract_canvas_vectors_from_transform(transform: &[f32; 16]) -> (Vec3, Vec3, Vec3, Vec3) {
    // Column-major layout: columns are at indices 0-3, 4-7, 8-11, 12-15
    // Column 0 = X (right), Column 1 = Y (up), Column 2 = Z (forward), Column 3 = translation

    let right = Vec3::new(transform[0], transform[1], transform[2]).normalize();
    let up = Vec3::new(transform[4], transform[5], transform[6]).normalize();
    let forward = Vec3::new(transform[8], transform[9], transform[10]).normalize();
    let center = Vec3::new(transform[12], transform[13], transform[14]);

    (center, right, up, forward)
}

/// Calculate the world-space size of a brush at a given depth.
///
/// When projecting from camera through canvas, the brush size in world units
/// changes based on the distance to the hit point.
///
/// # Arguments
/// * `brush_pixel_radius` - Brush radius in canvas pixels
/// * `canvas_params` - Canvas dimensions
/// * `camera_to_canvas_dist` - Distance from camera to canvas plane
/// * `camera_to_hit_dist` - Distance from camera to hit point
///
/// # Returns
/// Brush radius in world units at the hit point depth.
pub fn project_brush_size_to_depth(
    brush_pixel_radius: f32,
    canvas_params: &CanvasPlaneParams,
    camera_to_canvas_dist: f32,
    camera_to_hit_dist: f32,
) -> f32 {
    // Convert pixel radius to world units on canvas
    let pixels_per_world_unit = canvas_params.resolution.0 as f32 / canvas_params.world_size.0;
    let canvas_world_radius = brush_pixel_radius / pixels_per_world_unit;

    // Scale by depth ratio
    canvas_world_radius * (camera_to_hit_dist / camera_to_canvas_dist)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_tangent_space_up_normal() {
        let (t, b, n) = build_tangent_space(Vec3::Y, None);

        // Normal should be preserved
        assert!((n - Vec3::Y).length() < 1e-6);

        // All vectors should be unit length
        assert!((t.length() - 1.0).abs() < 1e-6);
        assert!((b.length() - 1.0).abs() < 1e-6);
        assert!((n.length() - 1.0).abs() < 1e-6);

        // All vectors should be orthogonal
        assert!(t.dot(b).abs() < 1e-6);
        assert!(t.dot(n).abs() < 1e-6);
        assert!(b.dot(n).abs() < 1e-6);
    }

    #[test]
    fn test_build_tangent_space_with_reference() {
        let normal = Vec3::Y;
        let reference = Vec3::new(1.0, 0.1, 0.0); // Slightly off-axis

        let (t, _b, n) = build_tangent_space(normal, Some(reference));

        // Tangent should be close to X axis (orthogonalized reference)
        assert!(t.dot(Vec3::X).abs() > 0.9);

        // All vectors should be orthogonal
        assert!(t.dot(n).abs() < 1e-6);
    }

    #[test]
    fn test_project_brush_circular() {
        let dab = project_brush_to_surface(
            1.0,           // 1 world unit radius
            Vec3::ZERO,    // hit position
            Vec2::new(0.5, 0.5), // center of texture
            Vec3::Y,       // up normal
            100.0,         // 100 pixels per world unit
        );

        // Brush should be circular (aspect ratio 1.0)
        assert!((dab.aspect_ratio - 1.0).abs() < 1e-6);

        // Size should be diameter in pixels (2 * radius * scale)
        assert!((dab.size - 200.0).abs() < 1e-6);
    }

    #[test]
    fn test_uv_scale_estimation() {
        // Unit triangle in world space
        let p0 = Vec3::ZERO;
        let p1 = Vec3::X;
        let p2 = Vec3::Y;

        // UV maps to half the texture (0-0.5 range)
        let uv0 = Vec2::ZERO;
        let uv1 = Vec2::new(0.5, 0.0);
        let uv2 = Vec2::new(0.0, 0.5);

        let scale = estimate_uv_scale(p0, p1, p2, uv0, uv1, uv2);

        // Scale should be 0.5 UV units per world unit
        assert!((scale.x - 0.5).abs() < 1e-6);
        assert!((scale.y - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_canvas_uv_to_world_center() {
        let center = Vec3::new(0.0, 0.0, 5.0);
        let right = Vec3::X;
        let up = Vec3::Y;
        let params = CanvasPlaneParams {
            resolution: (256, 256),
            world_size: (2.0, 2.0),
        };

        // UV (0.5, 0.5) should be at center
        let world = canvas_uv_to_world(Vec2::new(0.5, 0.5), center, right, up, &params);
        assert!((world - center).length() < 1e-6);
    }

    #[test]
    fn test_canvas_uv_to_world_corners() {
        let center = Vec3::ZERO;
        let right = Vec3::X;
        let up = Vec3::Y;
        let params = CanvasPlaneParams {
            resolution: (256, 256),
            world_size: (2.0, 2.0),
        };

        // UV (0, 0) = top-left corner = (-1, 1, 0) in world (Y inverted)
        let top_left = canvas_uv_to_world(Vec2::new(0.0, 0.0), center, right, up, &params);
        assert!((top_left.x - (-1.0)).abs() < 1e-6);
        assert!((top_left.y - 1.0).abs() < 1e-6);

        // UV (1, 1) = bottom-right corner = (1, -1, 0) in world
        let bottom_right = canvas_uv_to_world(Vec2::new(1.0, 1.0), center, right, up, &params);
        assert!((bottom_right.x - 1.0).abs() < 1e-6);
        assert!((bottom_right.y - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn test_canvas_uv_to_ray() {
        let camera_pos = Vec3::new(0.0, 0.0, 0.0);
        let canvas_center = Vec3::new(0.0, 0.0, 2.0);
        let right = Vec3::X;
        let up = Vec3::Y;
        let params = CanvasPlaneParams {
            resolution: (256, 256),
            world_size: (2.0, 2.0),
        };

        // Ray through center should point straight at canvas
        let (origin, dir) = canvas_uv_to_ray(
            camera_pos,
            Vec2::new(0.5, 0.5),
            canvas_center,
            right,
            up,
            &params,
        );

        assert!((origin - camera_pos).length() < 1e-6);
        assert!((dir - Vec3::Z).length() < 1e-6); // Points toward +Z
    }

    #[test]
    fn test_pixel_to_canvas_uv() {
        let resolution = (256, 256);

        // First pixel
        let uv = pixel_to_canvas_uv((0, 0), resolution);
        assert!((uv.x - 0.5 / 256.0).abs() < 1e-6);
        assert!((uv.y - 0.5 / 256.0).abs() < 1e-6);

        // Center pixel
        let uv = pixel_to_canvas_uv((128, 128), resolution);
        assert!((uv.x - (128.5 / 256.0)).abs() < 1e-6);
        assert!((uv.y - (128.5 / 256.0)).abs() < 1e-6);
    }

    #[test]
    fn test_project_brush_size_to_depth() {
        let params = CanvasPlaneParams {
            resolution: (256, 256),
            world_size: (2.0, 2.0),
        };

        // At same depth as canvas, brush should be same size
        let size = project_brush_size_to_depth(10.0, &params, 2.0, 2.0);
        let expected = 10.0 * (2.0 / 256.0); // 10 pixels * world_size/resolution
        assert!((size - expected).abs() < 1e-6);

        // At twice the depth, brush should be twice as large
        let size_2x = project_brush_size_to_depth(10.0, &params, 2.0, 4.0);
        assert!((size_2x - expected * 2.0).abs() < 1e-6);
    }
}
