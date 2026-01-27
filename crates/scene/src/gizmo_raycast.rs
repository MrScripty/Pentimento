//! Ray-geometry intersection for gizmo handle picking.
//!
//! This module provides mathematical ray intersection tests for the various
//! geometric primitives used in the transform gizmo (capsules, cones, spheres, tori).

use bevy::prelude::*;

/// Epsilon for floating point comparisons
const EPSILON: f32 = 1e-6;

/// Which gizmo handle is being interacted with
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GizmoHandle {
    #[default]
    None,
    // Translation arrows
    TranslateX,
    TranslateY,
    TranslateZ,
    // Rotation rings
    RotateX,
    RotateY,
    RotateZ,
    // Scale handles
    ScaleX,
    ScaleY,
    ScaleZ,
    ScaleUniform,
}

impl GizmoHandle {
    /// Returns true if this is a translation handle
    pub fn is_translate(&self) -> bool {
        matches!(
            self,
            GizmoHandle::TranslateX | GizmoHandle::TranslateY | GizmoHandle::TranslateZ
        )
    }

    /// Returns true if this is a rotation handle
    pub fn is_rotate(&self) -> bool {
        matches!(
            self,
            GizmoHandle::RotateX | GizmoHandle::RotateY | GizmoHandle::RotateZ
        )
    }

    /// Returns true if this is a scale handle
    pub fn is_scale(&self) -> bool {
        matches!(
            self,
            GizmoHandle::ScaleX
                | GizmoHandle::ScaleY
                | GizmoHandle::ScaleZ
                | GizmoHandle::ScaleUniform
        )
    }
}

/// Result of a gizmo handle raycast
#[derive(Debug, Clone, Copy)]
pub struct GizmoRaycastHit {
    pub handle: GizmoHandle,
    pub distance: f32,
    /// World-space hit position (for tangent calculation on rotation rings)
    pub hit_point: Vec3,
}

/// Geometry parameters for gizmo handles
#[derive(Resource)]
pub struct GizmoGeometry {
    /// Length of translation arrows
    pub arrow_length: f32,
    /// Radius of arrow shaft (for picking)
    pub arrow_shaft_radius: f32,
    /// Length of arrow head cone
    pub arrow_head_length: f32,
    /// Base radius of arrow head cone
    pub arrow_head_radius: f32,
    /// Radius of rotation rings
    pub ring_radius: f32,
    /// Thickness of rotation rings (tube radius for picking)
    pub ring_thickness: f32,
    /// Size of scale handle cubes
    pub scale_cube_size: f32,
    /// Radius of center sphere (for uniform scale)
    pub center_sphere_radius: f32,
}

impl Default for GizmoGeometry {
    fn default() -> Self {
        Self {
            arrow_length: 1.5,
            arrow_shaft_radius: 0.05,
            arrow_head_length: 0.25,
            arrow_head_radius: 0.12,
            ring_radius: 1.0,
            ring_thickness: 0.06,
            scale_cube_size: 0.12,
            center_sphere_radius: 0.15,
        }
    }
}

/// Ray-sphere intersection test.
/// Returns the distance to the closest intersection point, or None if no hit.
pub fn ray_sphere_intersection(
    ray_origin: Vec3,
    ray_dir: Vec3,
    sphere_center: Vec3,
    sphere_radius: f32,
) -> Option<f32> {
    let oc = ray_origin - sphere_center;
    let a = ray_dir.dot(ray_dir);
    let b = 2.0 * oc.dot(ray_dir);
    let c = oc.dot(oc) - sphere_radius * sphere_radius;
    let discriminant = b * b - 4.0 * a * c;

    if discriminant < 0.0 {
        return None;
    }

    let sqrt_d = discriminant.sqrt();
    let t1 = (-b - sqrt_d) / (2.0 * a);
    let t2 = (-b + sqrt_d) / (2.0 * a);

    // Return closest positive intersection
    if t1 > EPSILON {
        Some(t1)
    } else if t2 > EPSILON {
        Some(t2)
    } else {
        None
    }
}

/// Ray-cylinder intersection test (infinite cylinder along an axis).
/// Returns the distance to the closest intersection point within the height bounds.
fn ray_cylinder_intersection(
    ray_origin: Vec3,
    ray_dir: Vec3,
    cylinder_base: Vec3,
    cylinder_axis: Vec3,
    cylinder_radius: f32,
    cylinder_height: f32,
) -> Option<f32> {
    // Transform ray into cylinder's local space where axis is along Y
    let d = ray_dir;
    let o = ray_origin - cylinder_base;

    // Project out the axis component
    let d_perp = d - cylinder_axis * d.dot(cylinder_axis);
    let o_perp = o - cylinder_axis * o.dot(cylinder_axis);

    let a = d_perp.dot(d_perp);
    let b = 2.0 * o_perp.dot(d_perp);
    let c = o_perp.dot(o_perp) - cylinder_radius * cylinder_radius;

    if a.abs() < EPSILON {
        return None; // Ray parallel to cylinder axis
    }

    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return None;
    }

    let sqrt_d = discriminant.sqrt();
    let t1 = (-b - sqrt_d) / (2.0 * a);
    let t2 = (-b + sqrt_d) / (2.0 * a);

    // Check both intersection points
    for t in [t1, t2] {
        if t > EPSILON {
            let hit_point = o + d * t;
            let height = hit_point.dot(cylinder_axis);
            if height >= 0.0 && height <= cylinder_height {
                return Some(t);
            }
        }
    }

    None
}

/// Ray-cone intersection test.
/// The cone has its apex at cone_apex, extends along cone_axis, with half-angle determined by radius/height.
fn ray_cone_intersection(
    ray_origin: Vec3,
    ray_dir: Vec3,
    cone_apex: Vec3,
    cone_axis: Vec3,
    cone_height: f32,
    cone_base_radius: f32,
) -> Option<f32> {
    let o = ray_origin - cone_apex;
    let d = ray_dir;

    // cos^2(theta) where theta is the half-angle
    let cos_sq = cone_height * cone_height / (cone_height * cone_height + cone_base_radius * cone_base_radius);

    let d_dot_v = d.dot(cone_axis);
    let o_dot_v = o.dot(cone_axis);

    let a = d_dot_v * d_dot_v - cos_sq * d.dot(d);
    let b = 2.0 * (d_dot_v * o_dot_v - cos_sq * o.dot(d));
    let c = o_dot_v * o_dot_v - cos_sq * o.dot(o);

    if a.abs() < EPSILON {
        // Degenerate case
        return None;
    }

    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return None;
    }

    let sqrt_d = discriminant.sqrt();
    let t1 = (-b - sqrt_d) / (2.0 * a);
    let t2 = (-b + sqrt_d) / (2.0 * a);

    // Check both intersection points
    for t in [t1, t2] {
        if t > EPSILON {
            let hit_point = o + d * t;
            let height = hit_point.dot(cone_axis);
            // Cone extends from apex (0) to base (cone_height)
            if height >= 0.0 && height <= cone_height {
                return Some(t);
            }
        }
    }

    None
}

/// Ray-torus intersection test (approximated with arc segments for simplicity).
/// The torus lies in a plane perpendicular to torus_axis, centered at torus_center.
fn ray_torus_intersection(
    ray_origin: Vec3,
    ray_dir: Vec3,
    torus_center: Vec3,
    torus_axis: Vec3,
    major_radius: f32,
    minor_radius: f32,
) -> Option<f32> {
    // Approximate torus with capsule segments around the ring
    let num_segments = 32;
    let mut closest_t: Option<f32> = None;

    // Build orthonormal basis for the torus plane
    let (tangent, bitangent) = get_perpendicular_vectors(torus_axis);

    for i in 0..num_segments {
        let angle1 = (i as f32 / num_segments as f32) * std::f32::consts::TAU;
        let angle2 = ((i + 1) as f32 / num_segments as f32) * std::f32::consts::TAU;

        let p1 = torus_center + (tangent * angle1.cos() + bitangent * angle1.sin()) * major_radius;
        let p2 = torus_center + (tangent * angle2.cos() + bitangent * angle2.sin()) * major_radius;

        // Test ray against capsule segment
        if let Some(t) = ray_capsule_intersection(ray_origin, ray_dir, p1, p2, minor_radius) {
            if closest_t.is_none() || t < closest_t.unwrap() {
                closest_t = Some(t);
            }
        }
    }

    closest_t
}

/// Ray-capsule intersection (cylinder with hemispherical caps).
fn ray_capsule_intersection(
    ray_origin: Vec3,
    ray_dir: Vec3,
    cap1: Vec3,
    cap2: Vec3,
    radius: f32,
) -> Option<f32> {
    let axis = cap2 - cap1;
    let height = axis.length();
    if height < EPSILON {
        // Degenerate capsule = sphere
        return ray_sphere_intersection(ray_origin, ray_dir, cap1, radius);
    }
    let axis_norm = axis / height;

    // Test cylinder body
    let mut closest_t = ray_cylinder_intersection(ray_origin, ray_dir, cap1, axis_norm, radius, height);

    // Test hemispherical caps
    if let Some(t) = ray_sphere_intersection(ray_origin, ray_dir, cap1, radius) {
        // Check if hit is on the cap side (not inside cylinder)
        let hit = ray_origin + ray_dir * t;
        let h = (hit - cap1).dot(axis_norm);
        if h <= 0.0 {
            if closest_t.is_none() || t < closest_t.unwrap() {
                closest_t = Some(t);
            }
        }
    }

    if let Some(t) = ray_sphere_intersection(ray_origin, ray_dir, cap2, radius) {
        let hit = ray_origin + ray_dir * t;
        let h = (hit - cap1).dot(axis_norm);
        if h >= height {
            if closest_t.is_none() || t < closest_t.unwrap() {
                closest_t = Some(t);
            }
        }
    }

    closest_t
}

/// Ray-box intersection (axis-aligned box).
fn ray_box_intersection(
    ray_origin: Vec3,
    ray_dir: Vec3,
    box_center: Vec3,
    box_half_size: Vec3,
) -> Option<f32> {
    let min = box_center - box_half_size;
    let max = box_center + box_half_size;

    let inv_dir = Vec3::new(
        if ray_dir.x.abs() > EPSILON { 1.0 / ray_dir.x } else { f32::INFINITY },
        if ray_dir.y.abs() > EPSILON { 1.0 / ray_dir.y } else { f32::INFINITY },
        if ray_dir.z.abs() > EPSILON { 1.0 / ray_dir.z } else { f32::INFINITY },
    );

    let t1 = (min.x - ray_origin.x) * inv_dir.x;
    let t2 = (max.x - ray_origin.x) * inv_dir.x;
    let t3 = (min.y - ray_origin.y) * inv_dir.y;
    let t4 = (max.y - ray_origin.y) * inv_dir.y;
    let t5 = (min.z - ray_origin.z) * inv_dir.z;
    let t6 = (max.z - ray_origin.z) * inv_dir.z;

    let tmin = t1.min(t2).max(t3.min(t4)).max(t5.min(t6));
    let tmax = t1.max(t2).min(t3.max(t4)).min(t5.max(t6));

    if tmax < 0.0 || tmin > tmax {
        return None;
    }

    if tmin > EPSILON {
        Some(tmin)
    } else if tmax > EPSILON {
        Some(tmax)
    } else {
        None
    }
}

/// Get two perpendicular vectors to a given vector.
fn get_perpendicular_vectors(v: Vec3) -> (Vec3, Vec3) {
    let arbitrary = if v.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
    let tangent = v.cross(arbitrary).normalize();
    let bitangent = v.cross(tangent).normalize();
    (tangent, bitangent)
}

/// Raycast against all gizmo handles and return the closest hit.
///
/// # Arguments
/// * `ray_origin` - World space ray origin
/// * `ray_dir` - World space ray direction (should be normalized)
/// * `gizmo_center` - World space center of the gizmo
/// * `gizmo_orientation` - Orientation quaternion (for local space mode)
/// * `geometry` - Gizmo geometry parameters
///
/// # Returns
/// The closest hit handle and distance, or None if no hit.
pub fn raycast_gizmo(
    ray_origin: Vec3,
    ray_dir: Vec3,
    gizmo_center: Vec3,
    gizmo_orientation: Quat,
    geometry: &GizmoGeometry,
) -> Option<GizmoRaycastHit> {
    let mut closest_hit: Option<GizmoRaycastHit> = None;

    // Calculate axis directions based on orientation
    let x_axis = gizmo_orientation * Vec3::X;
    let y_axis = gizmo_orientation * Vec3::Y;
    let z_axis = gizmo_orientation * Vec3::Z;

    // Helper to update closest hit
    let mut check_hit = |handle: GizmoHandle, t: Option<f32>| {
        if let Some(dist) = t {
            if closest_hit.is_none() || dist < closest_hit.as_ref().unwrap().distance {
                closest_hit = Some(GizmoRaycastHit {
                    handle,
                    distance: dist,
                    hit_point: ray_origin + ray_dir * dist,
                });
            }
        }
    };

    // Test center sphere (uniform scale)
    check_hit(
        GizmoHandle::ScaleUniform,
        ray_sphere_intersection(ray_origin, ray_dir, gizmo_center, geometry.center_sphere_radius),
    );

    // Test translation arrows (capsule for shaft + cone for head)
    for (axis, handle) in [
        (x_axis, GizmoHandle::TranslateX),
        (y_axis, GizmoHandle::TranslateY),
        (z_axis, GizmoHandle::TranslateZ),
    ] {
        let shaft_start = gizmo_center;
        let shaft_end = gizmo_center + axis * (geometry.arrow_length - geometry.arrow_head_length);
        let head_base = shaft_end;
        let head_tip = gizmo_center + axis * geometry.arrow_length;

        // Arrow shaft (capsule)
        check_hit(
            handle,
            ray_capsule_intersection(
                ray_origin,
                ray_dir,
                shaft_start,
                shaft_end,
                geometry.arrow_shaft_radius,
            ),
        );

        // Arrow head (cone)
        check_hit(
            handle,
            ray_cone_intersection(
                ray_origin,
                ray_dir,
                head_tip,
                -axis, // Cone apex at tip, pointing back toward base
                geometry.arrow_head_length,
                geometry.arrow_head_radius,
            ),
        );
    }

    // Test rotation rings (tori)
    for (axis, handle) in [
        (x_axis, GizmoHandle::RotateX),
        (y_axis, GizmoHandle::RotateY),
        (z_axis, GizmoHandle::RotateZ),
    ] {
        check_hit(
            handle,
            ray_torus_intersection(
                ray_origin,
                ray_dir,
                gizmo_center,
                axis,
                geometry.ring_radius,
                geometry.ring_thickness,
            ),
        );
    }

    // Test scale handles (boxes at end of axes)
    for (axis, handle) in [
        (x_axis, GizmoHandle::ScaleX),
        (y_axis, GizmoHandle::ScaleY),
        (z_axis, GizmoHandle::ScaleZ),
    ] {
        let cube_center = gizmo_center + axis * geometry.arrow_length;
        let half_size = Vec3::splat(geometry.scale_cube_size * 0.5);
        check_hit(
            handle,
            ray_box_intersection(ray_origin, ray_dir, cube_center, half_size),
        );
    }

    closest_hit
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ray_sphere_hit() {
        let origin = Vec3::new(0.0, 0.0, 5.0);
        let dir = Vec3::new(0.0, 0.0, -1.0);
        let center = Vec3::ZERO;
        let radius = 1.0;

        let hit = ray_sphere_intersection(origin, dir, center, radius);
        assert!(hit.is_some());
        assert!((hit.unwrap() - 4.0).abs() < EPSILON);
    }

    #[test]
    fn test_ray_sphere_miss() {
        let origin = Vec3::new(0.0, 5.0, 5.0);
        let dir = Vec3::new(0.0, 0.0, -1.0);
        let center = Vec3::ZERO;
        let radius = 1.0;

        let hit = ray_sphere_intersection(origin, dir, center, radius);
        assert!(hit.is_none());
    }

    #[test]
    fn test_gizmo_handle_is_translate() {
        assert!(GizmoHandle::TranslateX.is_translate());
        assert!(GizmoHandle::TranslateY.is_translate());
        assert!(GizmoHandle::TranslateZ.is_translate());
        assert!(!GizmoHandle::RotateX.is_translate());
        assert!(!GizmoHandle::ScaleX.is_translate());
    }
}
