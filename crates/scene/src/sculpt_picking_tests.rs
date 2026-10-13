//! Regression fixtures and side-by-side CPU benchmark for native sculpt picking.
use super::*;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::PrimitiveTopology;
use std::hint::black_box;
use std::time::Instant;

fn mesh(positions: Vec<[f32; 3]>, indices: Indices, normals: Vec<[f32; 3]>) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_indices(indices);
    mesh
}

#[test]
fn closest_triangle_identity_prevents_nearby_shell_normal_mixup() {
    let positions = vec![
        [0., 0., 0.],
        [1., 0., 0.],
        [0., 1., 0.],
        [0., 0., 0.0005],
        [1., 0., 0.0005],
        [0., 1., 0.0005],
    ];
    let mut normals = vec![Vec3::X.to_array(); 3];
    normals.extend([Vec3::Z.to_array(); 3]);
    let mesh = mesh(positions, Indices::U32(vec![0, 1, 2, 3, 4, 5]), normals);
    let ray = Ray3d::new(Vec3::new(0.25, 0.25, 1.), Dir3::NEG_Z);
    let transform = GlobalTransform::IDENTITY;
    let old = legacy::pick(&ray, &mesh, &transform).unwrap();
    assert_eq!(
        old.1,
        Vec3::X,
        "reproduce the old proximity-based wrong triangle"
    );
    let actual = ray_mesh_intersection(&ray, &mesh, &transform).unwrap();
    assert!((actual.0.z - 0.0005).abs() < 1e-7);
    assert_eq!(actual.1, Vec3::Z);
}

#[test]
fn picking_transforms_normal_with_inverse_transpose_and_supports_u16() {
    let normal = Vec3::ONE.normalize();
    let mesh = mesh(
        vec![[0., 0., 0.], [1., -1., 0.], [1., 0., -1.]],
        Indices::U16(vec![0, 1, 2]),
        vec![normal.to_array(); 3],
    );
    let transform = GlobalTransform::from(Transform::from_scale(Vec3::new(2., 1., 0.5)));
    let point = transform.transform_point(Vec3::new(0.5, -0.25, -0.25));
    let expected = Vec3::new(0.5, 1., 2.).normalize();
    let ray = Ray3d::new(point + expected * 3., Dir3::new(-expected).unwrap());
    let old = legacy::pick(&ray, &mesh, &transform).unwrap();
    assert!(
        old.1.distance(expected) > 0.1,
        "reproduce rotation-only normal defect"
    );
    let (hit, actual) = ray_mesh_intersection(&ray, &mesh, &transform).unwrap();
    assert!(hit.distance(point) < 1e-5);
    assert!(actual.distance(expected) < 1e-6);
}

#[test]
fn malformed_triangles_and_singular_transforms_do_not_panic() {
    let mut mesh = mesh(
        vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]],
        Indices::U32(vec![100, 0, 1, 0, 1, 2, 1]),
        Vec::new(),
    );
    let ray = Ray3d::new(Vec3::new(0.25, 0.25, 1.), Dir3::NEG_Z);
    let (_, normal) = ray_mesh_intersection(&ray, &mesh, &GlobalTransform::IDENTITY).unwrap();
    assert_eq!(normal, Vec3::Z, "missing normals use geometric face normal");
    assert!(
        ray_mesh_intersection(
            &ray,
            &mesh,
            &GlobalTransform::from(Transform::from_scale(Vec3::ZERO))
        )
        .is_none()
    );
    mesh.insert_indices(Indices::U32(vec![0, 0, 0]));
    assert!(ray_mesh_intersection(&ray, &mesh, &GlobalTransform::IDENTITY).is_none());
}

fn grid(side: usize) -> Mesh {
    let mut positions = Vec::new();
    for y in 0..=side {
        for x in 0..=side {
            positions.push([x as f32, y as f32, 0.]);
        }
    }
    let mut indices = Vec::new();
    for y in 0..side {
        for x in 0..side {
            let a = (y * (side + 1) + x) as u32;
            let b = a + 1;
            let c = a + side as u32 + 1;
            indices.extend([a, b, c, b, c + 1, c]);
        }
    }
    let normals = vec![Vec3::Z.to_array(); positions.len()];
    mesh(positions, Indices::U32(indices), normals)
}

#[test]
fn picking_matches_reference_for_repeated_hits_and_mesh_replacement() {
    let mut mesh = grid(12);
    let transform = GlobalTransform::IDENTITY;
    for step in 0..64 {
        let ray = Ray3d::new(
            Vec3::new(
                (step * 7 % 12) as f32 + 0.25,
                (step * 5 % 12) as f32 + 0.25,
                2.,
            ),
            Dir3::NEG_Z,
        );
        assert_eq!(
            ray_mesh_intersection(&ray, &mesh, &transform),
            legacy::pick(&ray, &mesh, &transform)
        );
    }
    let ray = Ray3d::new(Vec3::new(0.25, 0.25, 2.), Dir3::NEG_Z);
    let baseline = ray_mesh_intersection(&ray, &mesh, &transform).unwrap();
    let VertexAttributeValues::Float32x3(positions) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION).unwrap()
    else {
        panic!()
    };
    for position in positions {
        position[2] = 0.75;
    }
    assert_eq!(
        ray_mesh_intersection(&ray, &mesh, &transform).unwrap().0.z,
        0.75
    );
    mesh = grid(12); // Models cancellation/history replacing the asset.
    assert_eq!(
        ray_mesh_intersection(&ray, &mesh, &transform).unwrap(),
        baseline
    );
}

#[test]
#[ignore = "manual CPU benchmark; run with --release --ignored --nocapture"]
fn benchmark_sculpt_picking() {
    let side = 128;
    let mesh = grid(side);
    let rays: Vec<_> = (0..128)
        .map(|step| {
            Ray3d::new(
                Vec3::new(
                    (step * 71 % side) as f32 + 0.25,
                    (step * 53 % side) as f32 + 0.25,
                    2.,
                ),
                Dir3::NEG_Z,
            )
        })
        .collect();
    let transform = GlobalTransform::IDENTITY;
    type Picker = fn(&Ray3d, &Mesh, &GlobalTransform) -> Option<(Vec3, Vec3)>;
    let pickers: [(&str, Picker); 2] = [
        ("baseline_cd85372", legacy::pick),
        ("single_pass", ray_mesh_intersection),
    ];
    for (_, pick) in pickers {
        for ray in &rays {
            black_box(pick(ray, &mesh, &transform));
        }
    }
    let mut expected = None;
    for (name, pick) in pickers {
        let mut trials = Vec::new();
        let mut sum = 0.;
        for _ in 0..7 {
            let start = Instant::now();
            let mut checksum = 0.;
            for ray in &rays {
                let (point, normal) =
                    pick(black_box(ray), black_box(&mesh), black_box(&transform)).unwrap();
                checksum += point.element_sum() + normal.element_sum();
            }
            trials.push(start.elapsed().as_secs_f64() * 1000.);
            sum = black_box(checksum);
        }
        if let Some(checksum) = expected {
            assert_eq!(sum, checksum);
        } else {
            expected = Some(sum);
        }
        trials.sort_by(f64::total_cmp);
        println!(
            "sculpt_picking,{name},triangles={},rays={},median_ms={:.6},min_ms={:.6},max_ms={:.6},checksum={sum}",
            side * side * 2,
            rays.len(),
            trials[3],
            trials[0],
            trials[6]
        );
    }
}

// Frozen implementation from cd853724, compiled only for regression/benchmark
// comparison. Production uses the identity-preserving implementation above.
mod legacy {
    use bevy::mesh::{Indices, VertexAttributeValues};
    use bevy::prelude::*;
    /// Simple ray-mesh intersection returning world position and normal
    pub(super) fn pick(
        ray: &Ray3d,
        mesh: &Mesh,
        transform: &GlobalTransform,
    ) -> Option<(Vec3, Vec3)> {
        // Get vertex positions
        let positions = match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
            Some(VertexAttributeValues::Float32x3(v)) => v,
            _ => return None,
        };

        // Get indices
        let indices = match mesh.indices() {
            Some(Indices::U32(i)) => i.iter().map(|&x| x as usize).collect::<Vec<_>>(),
            Some(Indices::U16(i)) => i.iter().map(|&x| x as usize).collect::<Vec<_>>(),
            None => return None,
        };

        // Get optional normals
        let normals = match mesh.attribute(Mesh::ATTRIBUTE_NORMAL) {
            Some(VertexAttributeValues::Float32x3(v)) => Some(v),
            _ => None,
        };

        // Transform ray to local space
        let inv_transform = transform.affine().inverse();
        let local_ray_origin = inv_transform.transform_point3(ray.origin);
        let local_ray_dir = inv_transform.transform_vector3(*ray.direction).normalize();

        let mut closest_hit: Option<(f32, Vec3, Vec3)> = None; // (t, local_pos, barycentric)

        // Iterate through triangles
        for triangle in indices.chunks(3) {
            if triangle.len() != 3 {
                continue;
            }

            let i0 = triangle[0];
            let i1 = triangle[1];
            let i2 = triangle[2];

            let v0 = Vec3::from(positions[i0]);
            let v1 = Vec3::from(positions[i1]);
            let v2 = Vec3::from(positions[i2]);

            // Möller–Trumbore intersection
            if let Some((t, u, v)) =
                ray_triangle_intersection(local_ray_origin, local_ray_dir, v0, v1, v2)
            {
                if t > 0.0 && (closest_hit.is_none() || t < closest_hit.as_ref().unwrap().0) {
                    let w = 1.0 - u - v;
                    let local_pos = v0 * w + v1 * u + v2 * v;
                    closest_hit = Some((t, local_pos, Vec3::new(w, u, v)));
                }
            }
        }

        let (_t, local_pos, barycentric) = closest_hit?;

        // Transform position to world space
        let world_pos = transform.transform_point(local_pos);

        // Get normal - find the triangle again to interpolate normal
        let mut normal = Vec3::Y;
        for triangle in indices.chunks(3) {
            if triangle.len() != 3 {
                continue;
            }

            let i0 = triangle[0];
            let i1 = triangle[1];
            let i2 = triangle[2];

            let v0 = Vec3::from(positions[i0]);
            let v1 = Vec3::from(positions[i1]);
            let v2 = Vec3::from(positions[i2]);

            // Check if this is the triangle we hit
            let test_pos = v0 * barycentric.x + v1 * barycentric.y + v2 * barycentric.z;
            if test_pos.distance(local_pos) < 0.001 {
                if let Some(normals) = normals {
                    let n0 = Vec3::from(normals[i0]);
                    let n1 = Vec3::from(normals[i1]);
                    let n2 = Vec3::from(normals[i2]);
                    let local_normal =
                        (n0 * barycentric.x + n1 * barycentric.y + n2 * barycentric.z).normalize();
                    normal = (transform.rotation() * local_normal).normalize();
                } else {
                    let edge1 = v1 - v0;
                    let edge2 = v2 - v0;
                    let local_normal = edge1.cross(edge2).normalize();
                    normal = (transform.rotation() * local_normal).normalize();
                }
                break;
            }
        }

        Some((world_pos, normal))
    }

    /// Möller–Trumbore ray-triangle intersection
    fn ray_triangle_intersection(
        ray_origin: Vec3,
        ray_dir: Vec3,
        v0: Vec3,
        v1: Vec3,
        v2: Vec3,
    ) -> Option<(f32, f32, f32)> {
        const EPSILON: f32 = 1e-8;

        let edge1 = v1 - v0;
        let edge2 = v2 - v0;

        let h = ray_dir.cross(edge2);
        let a = edge1.dot(h);

        if a.abs() < EPSILON {
            return None;
        }

        let f = 1.0 / a;
        let s = ray_origin - v0;
        let u = f * s.dot(h);

        if !(0.0..=1.0).contains(&u) {
            return None;
        }

        let q = s.cross(edge1);
        let v = f * ray_dir.dot(q);

        if v < 0.0 || u + v > 1.0 {
            return None;
        }

        let t = f * edge2.dot(q);

        if t > EPSILON { Some((t, u, v)) } else { None }
    }
}
