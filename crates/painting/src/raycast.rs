//! Ray-mesh intersection for projection painting.
//!
//! This module provides ray-triangle intersection using the Moller-Trumbore algorithm,
//! with support for interpolating vertex attributes (UVs, normals) at hit points.

use glam::{Vec2, Vec3};

use crate::types::MeshHit;

/// Epsilon for floating point comparisons in ray intersection
const EPSILON: f32 = 1e-6;

/// Result of a ray-triangle intersection test
#[derive(Debug, Clone, Copy)]
pub struct TriangleHit {
    /// Distance along the ray to the intersection point
    pub t: f32,
    /// Barycentric coordinate u (weight for vertex 1)
    pub u: f32,
    /// Barycentric coordinate v (weight for vertex 2)
    pub v: f32,
}

/// Moller-Trumbore ray-triangle intersection algorithm.
///
/// Returns the hit distance and barycentric coordinates if the ray intersects
/// the triangle.
///
/// # Arguments
/// * `ray_origin` - Origin point of the ray
/// * `ray_dir` - Direction of the ray (should be normalized for consistent t values)
/// * `v0`, `v1`, `v2` - Triangle vertices in counter-clockwise order
///
/// # Returns
/// `Some(TriangleHit)` if ray intersects, `None` otherwise
pub fn ray_triangle_intersection(
    ray_origin: Vec3,
    ray_dir: Vec3,
    v0: Vec3,
    v1: Vec3,
    v2: Vec3,
) -> Option<TriangleHit> {
    // Edge vectors
    let edge1 = v1 - v0;
    let edge2 = v2 - v0;

    // Begin calculating determinant - also used to calculate u parameter
    let pvec = ray_dir.cross(edge2);
    let det = edge1.dot(pvec);

    // If determinant is near zero, ray lies in plane of triangle or misses
    if det.abs() < EPSILON {
        return None;
    }

    let inv_det = 1.0 / det;

    // Calculate distance from v0 to ray origin
    let tvec = ray_origin - v0;

    // Calculate u parameter and test bounds
    let u = tvec.dot(pvec) * inv_det;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }

    // Prepare to test v parameter
    let qvec = tvec.cross(edge1);

    // Calculate v parameter and test bounds
    let v = ray_dir.dot(qvec) * inv_det;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }

    // Calculate t - ray intersection distance
    let t = edge2.dot(qvec) * inv_det;

    // Only accept hits in front of the ray
    if t < EPSILON {
        return None;
    }

    Some(TriangleHit { t, u, v })
}

/// Interpolate a Vec3 attribute using barycentric coordinates.
pub fn interpolate_vec3(v0: Vec3, v1: Vec3, v2: Vec3, u: f32, v: f32) -> Vec3 {
    let w = 1.0 - u - v;
    v0 * w + v1 * u + v2 * v
}

/// Interpolate a Vec2 attribute (like UVs) using barycentric coordinates.
pub fn interpolate_vec2(v0: Vec2, v1: Vec2, v2: Vec2, u: f32, v: f32) -> Vec2 {
    let w = 1.0 - u - v;
    v0 * w + v1 * u + v2 * v
}

/// Mesh data extracted for raycasting.
///
/// This struct holds references to the raw mesh data needed for ray intersection,
/// avoiding repeated asset lookups.
pub struct MeshRaycastData {
    /// Vertex positions
    pub positions: Vec<Vec3>,
    /// Triangle indices (3 per triangle)
    pub indices: Vec<u32>,
    /// Vertex normals (same length as positions)
    pub normals: Vec<Vec3>,
    /// Vertex UVs (same length as positions, or empty if no UVs)
    pub uvs: Vec<Vec2>,
    /// Vertex tangents (same length as positions, or empty if no tangents)
    pub tangents: Vec<Vec3>,
}

impl MeshRaycastData {
    /// Get the number of triangles in the mesh
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Get the vertex indices for a triangle
    pub fn triangle_indices(&self, tri_index: usize) -> (u32, u32, u32) {
        let base = tri_index * 3;
        (
            self.indices[base],
            self.indices[base + 1],
            self.indices[base + 2],
        )
    }

    /// Get the vertex positions for a triangle
    pub fn triangle_positions(&self, tri_index: usize) -> (Vec3, Vec3, Vec3) {
        let (i0, i1, i2) = self.triangle_indices(tri_index);
        (
            self.positions[i0 as usize],
            self.positions[i1 as usize],
            self.positions[i2 as usize],
        )
    }
}

/// Cast a ray against mesh data and return the closest hit.
///
/// # Arguments
/// * `ray_origin` - Origin of the ray in mesh local space
/// * `ray_dir` - Direction of the ray (should be normalized)
/// * `mesh_data` - Extracted mesh geometry data
///
/// # Returns
/// `Some(MeshHit)` with the closest intersection, `None` if no hit
pub fn raycast_mesh(
    ray_origin: Vec3,
    ray_dir: Vec3,
    mesh_data: &MeshRaycastData,
) -> Option<MeshHit> {
    let mut closest_hit: Option<(TriangleHit, u32)> = None;

    // Test all triangles (brute force - consider BVH for large meshes)
    for tri_idx in 0..mesh_data.triangle_count() {
        let (v0, v1, v2) = mesh_data.triangle_positions(tri_idx);

        if let Some(hit) = ray_triangle_intersection(ray_origin, ray_dir, v0, v1, v2) {
            let dominated = match &closest_hit {
                Some((prev, _)) => hit.t >= prev.t,
                None => false,
            };
            if !dominated {
                closest_hit = Some((hit, tri_idx as u32));
            }
        }
    }

    // Convert triangle hit to MeshHit with interpolated attributes
    closest_hit.map(|(hit, face_id)| {
        let (i0, i1, i2) = mesh_data.triangle_indices(face_id as usize);
        let (v0, v1, _v2) = mesh_data.triangle_positions(face_id as usize);

        // World position
        let world_pos = ray_origin + ray_dir * hit.t;

        // Barycentric coordinates (w, u, v format for Vec3)
        let w = 1.0 - hit.u - hit.v;
        let barycentric = Vec3::new(w, hit.u, hit.v);

        // Interpolate normal
        let n0 = mesh_data.normals[i0 as usize];
        let n1 = mesh_data.normals[i1 as usize];
        let n2 = mesh_data.normals[i2 as usize];
        let normal = interpolate_vec3(n0, n1, n2, hit.u, hit.v).normalize();

        // Interpolate UV if available
        let uv = if !mesh_data.uvs.is_empty() {
            let uv0 = mesh_data.uvs[i0 as usize];
            let uv1 = mesh_data.uvs[i1 as usize];
            let uv2 = mesh_data.uvs[i2 as usize];
            Some(interpolate_vec2(uv0, uv1, uv2, hit.u, hit.v))
        } else {
            None
        };

        // Interpolate tangent if available, otherwise compute from triangle
        let tangent = if !mesh_data.tangents.is_empty() {
            let t0 = mesh_data.tangents[i0 as usize];
            let t1 = mesh_data.tangents[i1 as usize];
            let t2 = mesh_data.tangents[i2 as usize];
            interpolate_vec3(t0, t1, t2, hit.u, hit.v).normalize()
        } else {
            // Compute tangent from triangle edge
            let edge = (v1 - v0).normalize();
            // Gram-Schmidt orthogonalize against normal
            (edge - normal * normal.dot(edge)).normalize()
        };

        // Bitangent from cross product
        let bitangent = normal.cross(tangent).normalize();

        MeshHit {
            world_pos,
            face_id,
            barycentric,
            normal,
            tangent,
            bitangent,
            uv,
        }
    })
}

/// Batch raycast multiple rays against a mesh.
///
/// More efficient than individual raycasts when projecting many canvas pixels.
///
/// # Arguments
/// * `rays` - Iterator of (origin, direction) pairs in mesh local space
/// * `mesh_data` - Extracted mesh geometry data
///
/// # Returns
/// Vector of optional hits, one per input ray
pub fn batch_raycast_mesh<'a>(
    rays: impl Iterator<Item = (Vec3, Vec3)>,
    mesh_data: &MeshRaycastData,
) -> Vec<Option<MeshHit>> {
    rays.map(|(origin, dir)| raycast_mesh(origin, dir, mesh_data))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ray_triangle_hit() {
        // Triangle in XY plane at z=0
        let v0 = Vec3::new(0.0, 0.0, 0.0);
        let v1 = Vec3::new(1.0, 0.0, 0.0);
        let v2 = Vec3::new(0.0, 1.0, 0.0);

        // Ray pointing down at center of triangle
        let origin = Vec3::new(0.25, 0.25, 1.0);
        let dir = Vec3::new(0.0, 0.0, -1.0);

        let hit = ray_triangle_intersection(origin, dir, v0, v1, v2);
        assert!(hit.is_some());

        let hit = hit.unwrap();
        assert!((hit.t - 1.0).abs() < EPSILON);
        assert!((hit.u - 0.25).abs() < EPSILON);
        assert!((hit.v - 0.25).abs() < EPSILON);
    }

    #[test]
    fn test_ray_triangle_miss() {
        // Triangle in XY plane at z=0
        let v0 = Vec3::new(0.0, 0.0, 0.0);
        let v1 = Vec3::new(1.0, 0.0, 0.0);
        let v2 = Vec3::new(0.0, 1.0, 0.0);

        // Ray pointing down but missing triangle
        let origin = Vec3::new(2.0, 2.0, 1.0);
        let dir = Vec3::new(0.0, 0.0, -1.0);

        let hit = ray_triangle_intersection(origin, dir, v0, v1, v2);
        assert!(hit.is_none());
    }

    #[test]
    fn test_ray_triangle_behind() {
        // Triangle in XY plane at z=0
        let v0 = Vec3::new(0.0, 0.0, 0.0);
        let v1 = Vec3::new(1.0, 0.0, 0.0);
        let v2 = Vec3::new(0.0, 1.0, 0.0);

        // Ray pointing away from triangle
        let origin = Vec3::new(0.25, 0.25, 1.0);
        let dir = Vec3::new(0.0, 0.0, 1.0);

        let hit = ray_triangle_intersection(origin, dir, v0, v1, v2);
        assert!(hit.is_none());
    }

    #[test]
    fn test_interpolate_vec2() {
        let v0 = Vec2::new(0.0, 0.0);
        let v1 = Vec2::new(1.0, 0.0);
        let v2 = Vec2::new(0.0, 1.0);

        // At vertex 0 (u=0, v=0)
        let result = interpolate_vec2(v0, v1, v2, 0.0, 0.0);
        assert!((result - v0).length() < EPSILON);

        // At vertex 1 (u=1, v=0)
        let result = interpolate_vec2(v0, v1, v2, 1.0, 0.0);
        assert!((result - v1).length() < EPSILON);

        // At vertex 2 (u=0, v=1)
        let result = interpolate_vec2(v0, v1, v2, 0.0, 1.0);
        assert!((result - v2).length() < EPSILON);

        // At center (u=1/3, v=1/3)
        let center = (v0 + v1 + v2) / 3.0;
        let result = interpolate_vec2(v0, v1, v2, 1.0 / 3.0, 1.0 / 3.0);
        assert!((result - center).length() < EPSILON);
    }
}
