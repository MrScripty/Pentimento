//! Brush projection math for 3D mesh painting.
//!
//! This module handles projecting brushes onto mesh surfaces using tangent-space
//! calculations to ensure brushes appear circular on the surface regardless of
//! viewing angle.

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
}
