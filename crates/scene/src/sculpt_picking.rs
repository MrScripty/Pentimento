//! Allocation-free sculpt picking against the currently published mesh asset.
//!
//! Keep the winning triangle identity: searching by approximate position can
//! select the wrong normal on nearby shells. No cache can outlive mesh edits,
//! stroke cancellation or history restoration in this path.

use bevy::mesh::{Indices, VertexAttributeValues};
use bevy::prelude::*;

pub(crate) fn ray_mesh_intersection(
    ray: &Ray3d,
    mesh: &Mesh,
    transform: &GlobalTransform,
) -> Option<(Vec3, Vec3)> {
    let VertexAttributeValues::Float32x3(positions) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)?
    else {
        return None;
    };
    let inverse = transform.affine().inverse();
    let origin = inverse.transform_point3(ray.origin);
    let direction = inverse.transform_vector3(*ray.direction).try_normalize()?;
    if !origin.is_finite() || !direction.is_finite() {
        return None;
    }
    let hit = match mesh.indices()? {
        Indices::U16(indices) => nearest_triangle(
            indices.chunks_exact(3).map(|t| t.map_indices()),
            positions,
            origin,
            direction,
        ),
        Indices::U32(indices) => nearest_triangle(
            indices.chunks_exact(3).map(|t| t.map_indices()),
            positions,
            origin,
            direction,
        ),
    }?;
    let [a, b, c] = hit.indices;
    let v0 = Vec3::from(positions[a]);
    let v1 = Vec3::from(positions[b]);
    let v2 = Vec3::from(positions[c]);
    let weights = hit.weights;
    let face_normal = (v1 - v0).cross(v2 - v0);
    let normal = match mesh.attribute(Mesh::ATTRIBUTE_NORMAL) {
        Some(VertexAttributeValues::Float32x3(normals)) => normals
            .get(a)
            .zip(normals.get(b))
            .zip(normals.get(c))
            .map(|((n0, n1), n2)| {
                Vec3::from(*n0) * weights.x
                    + Vec3::from(*n1) * weights.y
                    + Vec3::from(*n2) * weights.z
            }),
        _ => None,
    }
    .and_then(Vec3::try_normalize)
    .or_else(|| face_normal.try_normalize())?;
    // Inverse transpose, including nonuniform scale; rotation alone is wrong.
    let world_normal = (inverse.matrix3.transpose() * normal).try_normalize()?;
    let world_position =
        transform.transform_point(v0 * weights.x + v1 * weights.y + v2 * weights.z);
    (world_position.is_finite() && world_normal.is_finite())
        .then_some((world_position, world_normal))
}

trait TriangleIndices {
    fn map_indices(&self) -> [usize; 3];
}
impl TriangleIndices for [u16] {
    fn map_indices(&self) -> [usize; 3] {
        [self[0] as usize, self[1] as usize, self[2] as usize]
    }
}
impl TriangleIndices for [u32] {
    fn map_indices(&self) -> [usize; 3] {
        [self[0] as usize, self[1] as usize, self[2] as usize]
    }
}

struct Hit {
    distance: f32,
    indices: [usize; 3],
    weights: Vec3,
}

#[inline]
fn nearest_triangle(
    triangles: impl Iterator<Item = [usize; 3]>,
    positions: &[[f32; 3]],
    origin: Vec3,
    direction: Vec3,
) -> Option<Hit> {
    let mut closest: Option<Hit> = None;
    for indices in triangles {
        let [a, b, c] = indices;
        let (Some(a), Some(b), Some(c)) = (positions.get(a), positions.get(b), positions.get(c))
        else {
            continue;
        };
        let (v0, v1, v2) = (Vec3::from(*a), Vec3::from(*b), Vec3::from(*c));
        let Some((distance, u, v)) = ray_triangle_intersection(origin, direction, v0, v1, v2)
        else {
            continue;
        };
        if closest.as_ref().is_none_or(|hit| distance < hit.distance) {
            closest = Some(Hit {
                distance,
                indices,
                weights: Vec3::new(1.0 - u - v, u, v),
            });
        }
    }
    closest
}

#[inline(always)]
fn ray_triangle_intersection(
    origin: Vec3,
    direction: Vec3,
    v0: Vec3,
    v1: Vec3,
    v2: Vec3,
) -> Option<(f32, f32, f32)> {
    const EPSILON: f32 = 1e-8;
    let edge1 = v1 - v0;
    let edge2 = v2 - v0;
    let h = direction.cross(edge2);
    let a = edge1.dot(h);
    if a.abs() < EPSILON {
        return None;
    }
    let f = 1.0 / a;
    let s = origin - v0;
    let u = f * s.dot(h);
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(edge1);
    let v = f * direction.dot(q);
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = f * edge2.dot(q);
    (t > EPSILON && t.is_finite()).then_some((t, u, v))
}

#[cfg(test)]
#[path = "sculpt_picking_tests.rs"]
mod tests;
