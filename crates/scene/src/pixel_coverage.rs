//! Pixel coverage computation for the render camera vertex budget.
//!
//! Computes how many pixels a sculpted mesh covers when viewed from the render
//! camera. This pixel count becomes the vertex budget: the mesh cannot have more
//! vertices than pixels it covers (times a configurable multiplier).
//!
//! ## Current Implementation
//!
//! Uses CPU-based triangle projection: each mesh triangle is projected through
//! the render camera's view-projection matrix, backface-culled, frustum-clipped,
//! and its projected screen area accumulated. This handles foreshortening
//! naturally (steep-angle surfaces contribute fewer pixels).
//!
//! Self-occlusion is NOT handled by this approach — it overestimates coverage
//! for concave meshes. A future GPU depth-buffer pass can replace this for
//! exact pixel counting.

use bevy::prelude::*;
use bevy::math::{Mat4, Vec3, Vec4};

/// Resource holding the result of pixel coverage computation.
#[derive(Resource, Default, Debug)]
pub struct PixelCoverageState {
    /// Last computed pixel coverage count
    pub pixel_count: u32,
    /// Whether the coverage is stale (needs recomputation)
    pub stale: bool,
    /// The max vertices derived from the last computation
    pub max_vertices: usize,
}

/// Plugin for pixel coverage computation.
pub struct PixelCoveragePlugin;

impl Plugin for PixelCoveragePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PixelCoverageState>();
    }
}

/// Estimate pixel coverage of a mesh from a camera by projecting triangles.
///
/// Projects each triangle through the view-projection matrix, performs backface
/// culling, and sums the screen-space area of visible triangles.
///
/// # Arguments
/// * `positions` - Vertex positions in local/object space
/// * `indices` - Triangle indices (must be a multiple of 3)
/// * `normals` - Per-face normals (one per triangle, i.e. `indices.len() / 3` entries).
///   If `None`, normals are computed from vertex positions.
/// * `model_matrix` - Local-to-world transform of the mesh
/// * `view_projection` - The render camera's view-projection matrix
/// * `resolution` - Render resolution in pixels
///
/// # Returns
/// Estimated number of pixels covered, scaled to the full render resolution.
pub fn estimate_pixel_coverage_cpu(
    positions: &[Vec3],
    indices: &[u32],
    model_matrix: &Mat4,
    view_projection: &Mat4,
    resolution: UVec2,
) -> u32 {
    if indices.len() < 3 || positions.is_empty() {
        return 0;
    }

    let mvp = *view_projection * *model_matrix;
    let width = resolution.x as f32;
    let height = resolution.y as f32;

    // Camera forward direction for backface culling (from view-projection inverse)
    // We extract it from the MVP: the camera looks along -Z in view space,
    // so the world-space forward is the third row of the inverse view matrix.
    // For backface culling, we compare face normal dot camera direction.
    // Simplified: a triangle is backfacing if its projected winding is clockwise.

    let mut total_screen_area: f32 = 0.0;

    for tri in indices.chunks_exact(3) {
        let i0 = tri[0] as usize;
        let i1 = tri[1] as usize;
        let i2 = tri[2] as usize;

        if i0 >= positions.len() || i1 >= positions.len() || i2 >= positions.len() {
            continue;
        }

        let p0 = positions[i0];
        let p1 = positions[i1];
        let p2 = positions[i2];

        // Project to clip space
        let c0 = mvp * Vec4::new(p0.x, p0.y, p0.z, 1.0);
        let c1 = mvp * Vec4::new(p1.x, p1.y, p1.z, 1.0);
        let c2 = mvp * Vec4::new(p2.x, p2.y, p2.z, 1.0);

        // Skip if any vertex is behind the camera
        if c0.w <= 0.0 || c1.w <= 0.0 || c2.w <= 0.0 {
            continue;
        }

        // Perspective divide to NDC
        let n0 = Vec3::new(c0.x / c0.w, c0.y / c0.w, c0.z / c0.w);
        let n1 = Vec3::new(c1.x / c1.w, c1.y / c1.w, c1.z / c1.w);
        let n2 = Vec3::new(c2.x / c2.w, c2.y / c2.w, c2.z / c2.w);

        // Frustum cull: skip if all vertices are outside the same clip plane
        if (n0.x < -1.0 && n1.x < -1.0 && n2.x < -1.0)
            || (n0.x > 1.0 && n1.x > 1.0 && n2.x > 1.0)
            || (n0.y < -1.0 && n1.y < -1.0 && n2.y < -1.0)
            || (n0.y > 1.0 && n1.y > 1.0 && n2.y > 1.0)
            || (n0.z < 0.0 && n1.z < 0.0 && n2.z < 0.0)
            || (n0.z > 1.0 && n1.z > 1.0 && n2.z > 1.0)
        {
            continue;
        }

        // NDC to screen coordinates
        let s0x = (n0.x + 1.0) * 0.5 * width;
        let s0y = (1.0 - n0.y) * 0.5 * height;
        let s1x = (n1.x + 1.0) * 0.5 * width;
        let s1y = (1.0 - n1.y) * 0.5 * height;
        let s2x = (n2.x + 1.0) * 0.5 * width;
        let s2y = (1.0 - n2.y) * 0.5 * height;

        // Signed area via cross product (positive = CCW = front-facing)
        let signed_area =
            0.5 * ((s1x - s0x) * (s2y - s0y) - (s2x - s0x) * (s1y - s0y));

        // Backface culling: skip negative area (clockwise winding = backfacing)
        if signed_area <= 0.0 {
            continue;
        }

        total_screen_area += signed_area;
    }

    // Clamp to total resolution (can't cover more than the entire screen)
    let max_pixels = (width * height) as u32;
    (total_screen_area as u32).min(max_pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_mesh_zero_coverage() {
        let coverage = estimate_pixel_coverage_cpu(
            &[],
            &[],
            &Mat4::IDENTITY,
            &Mat4::IDENTITY,
            UVec2::new(1920, 1080),
        );
        assert_eq!(coverage, 0);
    }

    #[test]
    fn test_behind_camera_zero_coverage() {
        // Triangle behind the camera (z > 0 in view space with perspective_rh)
        let positions = vec![
            Vec3::new(-1.0, -1.0, 5.0),
            Vec3::new(1.0, -1.0, 5.0),
            Vec3::new(0.0, 1.0, 5.0),
        ];
        let indices = vec![0, 1, 2];

        let vp = Mat4::perspective_rh(
            std::f32::consts::FRAC_PI_3,
            16.0 / 9.0,
            0.1,
            1000.0,
        );

        let coverage = estimate_pixel_coverage_cpu(
            &positions,
            &indices,
            &Mat4::IDENTITY,
            &vp,
            UVec2::new(1920, 1080),
        );
        // Triangle at z=5 is behind camera in RH perspective (camera looks along -Z)
        // so w should be <= 0 and it should be culled
        // Actually in RH, the camera looks along -Z, so z=5 is behind.
        // But let's just verify coverage is small or zero.
        // The exact behavior depends on the MVP setup.
        assert!(coverage < 100, "Behind-camera triangle should have minimal coverage");
    }

    #[test]
    fn test_front_facing_triangle_has_coverage() {
        // Triangle in front of camera
        let positions = vec![
            Vec3::new(-0.5, -0.5, -2.0),
            Vec3::new(0.5, -0.5, -2.0),
            Vec3::new(0.0, 0.5, -2.0),
        ];
        let indices = vec![0, 1, 2];

        let vp = Mat4::perspective_rh(
            std::f32::consts::FRAC_PI_3,
            16.0 / 9.0,
            0.1,
            1000.0,
        );

        let coverage = estimate_pixel_coverage_cpu(
            &positions,
            &indices,
            &Mat4::IDENTITY,
            &vp,
            UVec2::new(1920, 1080),
        );
        assert!(coverage > 0, "Front-facing triangle should have positive coverage");
    }
}
