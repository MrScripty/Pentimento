use super::*;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;

fn mesh(positions: Vec<[f32; 3]>, indices: Vec<u32>) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn tetrahedra() -> Mesh {
    mesh(
        vec![
            [0., 0., 0.],
            [1., 0., 0.],
            [0., 1., 0.],
            [0., 0., 1.],
            [3., 0., 0.],
            [4., 0., 0.],
            [3., 1., 0.],
            [3., 0., 1.],
        ],
        vec![
            0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3, 4, 6, 5, 4, 5, 7, 4, 7, 6, 5, 6, 7,
        ],
    )
}

#[test]
fn manifold_rejects_two_closed_vertex_fans() {
    let mut he = HalfEdgeMesh::from_bevy_mesh(&tetrahedra()).unwrap();
    for edge in &mut he.half_edges {
        if edge.origin == VertexId(4) {
            edge.origin = VertexId(0);
        }
    }
    he.vertices[4].outgoing_half_edge = None;
    he.edge_map.clear();
    for edge in &he.half_edges {
        he.edge_map.insert(
            (edge.origin, he.half_edges[edge.next.0 as usize].origin),
            edge.id,
        );
    }
    assert!(
        he.check_manifold().is_err(),
        "two tetrahedral fans sharing one vertex are not manifold"
    );
    assert!(!he.is_likely_manifold());
}

#[test]
fn manifold_accepts_a_boundary_triangle() {
    let he = HalfEdgeMesh::from_bevy_mesh(&mesh(
        vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]],
        vec![0, 1, 2],
    ))
    .unwrap();
    assert!(he.check_manifold().is_ok());
    assert_eq!(he.get_adjacent_vertices(VertexId(0)).len(), 2);
}

#[test]
fn import_preserves_uv_corners_and_declared_seam_connectivity() {
    let mut source = mesh(
        vec![
            [0., 0., 0.],
            [1., 0., 0.],
            [0., 1., 0.],
            [1., 0., 0.],
            [0., 0., 0.],
            [1., -1., 0.],
        ],
        vec![0, 1, 2, 3, 4, 5],
    );
    let uv = vec![
        [0., 0.],
        [0.5, 0.],
        [0., 1.],
        [1., 0.],
        [0.75, 0.],
        [1., 1.],
    ];
    source.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv.clone());
    let imported = HalfEdgeMesh::from_bevy_mesh(&source).unwrap();
    let output = imported.to_bevy_mesh();
    assert_eq!(
        output.attribute(Mesh::ATTRIBUTE_UV_0),
        Some(&VertexAttributeValues::Float32x2(uv))
    );
    assert!(imported.half_edges().iter().all(|edge| edge.twin.is_none()));
}

#[test]
fn import_does_not_join_touching_closed_components() {
    let mut source = tetrahedra();
    let VertexAttributeValues::Float32x3(positions) =
        source.attribute_mut(Mesh::ATTRIBUTE_POSITION).unwrap()
    else {
        panic!()
    };
    positions[4] = positions[0];
    let he = HalfEdgeMesh::from_bevy_mesh(&source).unwrap();
    assert_eq!(he.get_face_vertices(FaceId(4))[0], VertexId(4));
    assert!(he.check_manifold().is_ok());
}

#[test]
fn import_rejects_duplicate_directed_edge() {
    let source = mesh(
        vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., -1., 0.]],
        vec![0, 1, 2, 0, 1, 3],
    );
    assert!(HalfEdgeMesh::from_bevy_mesh(&source).is_err());
}

#[test]
fn invalid_indices_return_an_error_without_panicking() {
    let source = mesh(
        vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]],
        vec![0, 1, 9],
    );
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        HalfEdgeMesh::from_bevy_mesh(&source)
    }));
    assert!(matches!(result, Ok(Err(_))));
}

#[test]
fn compaction_does_not_delete_invalid_live_faces() {
    // Start with separate valid triangles; introduce the duplicate directed edge
    // that previously selected an arbitrary surviving face during compaction.
    let source = mesh(
        vec![
            [0., 0., 0.],
            [1., 0., 0.],
            [0., 1., 0.],
            [2., 0., 0.],
            [3., 0., 0.],
            [2., 1., 0.],
        ],
        vec![0, 1, 2, 3, 4, 5],
    );
    let mut he = HalfEdgeMesh::from_bevy_mesh(&source).unwrap();
    he.half_edges[3].origin = VertexId(0);
    he.half_edges[4].origin = VertexId(1);
    let before = format!("{he:?}");
    he.compact();
    assert_eq!(format!("{he:?}"), before);
    assert_eq!(he.face_count(), 2);
}

#[test]
fn vertex_queries_cover_both_sides_of_a_boundary_fan() {
    let source = mesh(
        vec![
            [0., 0., 0.],
            [1., 0., 0.],
            [1., 1., 0.],
            [0., 1., 0.],
            [-1., 1., 0.],
        ],
        vec![0, 1, 2, 0, 2, 3, 0, 3, 4],
    );
    let mut he = HalfEdgeMesh::from_bevy_mesh(&source).unwrap();
    he.vertices[0].outgoing_half_edge = Some(HalfEdgeId(3));
    assert_eq!(he.get_vertex_faces(VertexId(0)).len(), 3);
    assert_eq!(he.get_adjacent_vertices(VertexId(0)).len(), 4);
}

#[test]
fn high_valence_fan_is_not_truncated_at_one_hundred() {
    let count = 130;
    let mut positions = vec![[0., 0., 0.]];
    for i in 0..count {
        let angle = i as f32 * std::f32::consts::TAU / count as f32;
        positions.push([angle.cos(), angle.sin(), 0.]);
    }
    let indices = (0..count)
        .flat_map(|i| [0, i + 1, (i + 1) % count + 1])
        .collect();
    let he = HalfEdgeMesh::from_bevy_mesh(&mesh(positions, indices)).unwrap();
    assert_eq!(he.get_vertex_faces(VertexId(0)).len(), count as usize);
    assert_eq!(he.get_adjacent_vertices(VertexId(0)).len(), count as usize);
}

#[test]
fn explicit_weld_preserves_both_uv_charts_through_split_and_compaction() {
    let mut source = mesh(
        vec![
            [0., 0., 0.],
            [1., 0., 0.],
            [0., 1., 0.],
            [1., 0., 0.],
            [0., 0., 0.],
            [1., -1., 0.],
        ],
        vec![0, 1, 2, 3, 4, 5],
    );
    let uv = vec![
        [0., 0.],
        [0.5, 0.],
        [0., 1.],
        [1., 0.],
        [0.75, 0.],
        [1., 1.],
    ];
    source.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv.clone());
    let mut imported = HalfEdgeMesh::from_bevy_mesh_welded(&source).unwrap();
    assert_eq!(
        imported.to_bevy_mesh().attribute(Mesh::ATTRIBUTE_UV_0),
        Some(&VertexAttributeValues::Float32x2(uv))
    );
    let shared = imported.find_half_edge(VertexId(0), VertexId(1)).unwrap();
    assert!(imported.is_uv_seam_edge(shared));
    let before = format!("{imported:?}");
    assert!(!imported.flip_edge_topology(shared));
    assert!(imported.collapse_edge_topology(shared).is_none());
    assert_eq!(
        format!("{imported:?}"),
        before,
        "rejected seam edits must be atomic"
    );
    let (mid, _) = imported.split_edge_topology(shared).unwrap();
    let mids: Vec<_> = imported
        .half_edges()
        .iter()
        .filter(|edge| edge.origin == mid)
        .map(|edge| imported.corner_uv(edge.id).unwrap())
        .collect();
    assert_eq!(
        mids.iter().filter(|&&uv| uv == Vec2::new(0.25, 0.)).count(),
        2
    );
    assert_eq!(
        mids.iter()
            .filter(|&&uv| uv == Vec2::new(0.875, 0.))
            .count(),
        2
    );
    assert!(imported.check_manifold().is_ok());
    imported.try_compact().unwrap();
    assert_eq!(imported.face_count(), 4);
    assert!(imported.check_manifold().is_ok());
}

#[test]
fn explicit_weld_rejects_disconnected_vertex_fans_without_dropping_faces() {
    let mut source = tetrahedra();
    let VertexAttributeValues::Float32x3(positions) =
        source.attribute_mut(Mesh::ATTRIBUTE_POSITION).unwrap()
    else {
        panic!()
    };
    positions[4] = positions[0];
    assert!(HalfEdgeMesh::from_bevy_mesh_welded(&source).is_err());
    assert_eq!(
        HalfEdgeMesh::from_bevy_mesh(&source).unwrap().face_count(),
        8
    );
}

#[test]
fn tetrahedron_collapse_is_rejected_by_full_link_condition() {
    let source = mesh(
        vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
        vec![0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3],
    );
    let mut imported = HalfEdgeMesh::from_bevy_mesh(&source).unwrap();
    for edge in imported.half_edges.clone() {
        assert!(!imported.satisfies_collapse_link(edge.id));
        let before = format!("{imported:?}");
        assert!(imported.collapse_edge_topology(edge.id).is_none());
        assert_eq!(format!("{imported:?}"), before);
    }
}

#[test]
fn uv_sphere_weld_is_closed_and_preserves_every_source_corner() {
    let source = Sphere::new(1.).mesh().uv(16, 8);
    let imported = HalfEdgeMesh::from_bevy_mesh_welded(&source).unwrap();
    assert!(imported.half_edges().iter().all(|edge| edge.twin.is_some()));
    assert!(imported.check_manifold().is_ok());
    let VertexAttributeValues::Float32x2(source_uv) =
        source.attribute(Mesh::ATTRIBUTE_UV_0).unwrap()
    else {
        panic!()
    };
    let expected: Vec<_> = source
        .indices()
        .unwrap()
        .iter()
        .map(|index| source_uv[index])
        .collect();
    assert_eq!(
        imported.to_bevy_mesh().attribute(Mesh::ATTRIBUTE_UV_0),
        Some(&VertexAttributeValues::Float32x2(expected))
    );
}

#[test]
fn malformed_attributes_and_non_triangle_topology_return_errors() {
    let mut source = mesh(
        vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]],
        vec![0, 1, 2],
    );
    source.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0., 0., 1.]]);
    assert!(HalfEdgeMesh::from_bevy_mesh(&source).is_err());
    let mut source = mesh(
        vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]],
        vec![0, 1, 2],
    );
    source.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0., 0.]]);
    assert!(HalfEdgeMesh::from_bevy_mesh(&source).is_err());
    let source = mesh(
        vec![[f32::NAN, 0., 0.], [1., 0., 0.], [0., 1., 0.]],
        vec![0, 1, 2],
    );
    assert!(HalfEdgeMesh::from_bevy_mesh(&source).is_err());
    let mut lines = Mesh::new(PrimitiveTopology::LineList, RenderAssetUsages::default());
    lines.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]],
    );
    lines.insert_indices(Indices::U32(vec![0, 1, 2]));
    assert!(HalfEdgeMesh::from_bevy_mesh(&lines).is_err());
}

#[test]
fn checked_compaction_rejects_invalid_cycle_atomically() {
    let mut imported = HalfEdgeMesh::from_bevy_mesh(&tetrahedra()).unwrap();
    imported.half_edges[0].next = HalfEdgeId(99);
    let before = format!("{imported:?}");
    assert!(imported.try_compact().is_err());
    assert_eq!(format!("{imported:?}"), before);
}

#[test]
fn import_rejects_zero_area_triangles_without_removing_them() {
    let source = mesh(
        vec![[0., 0., 0.], [1., 0., 0.], [2., 0., 0.]],
        vec![0, 1, 2],
    );
    assert!(HalfEdgeMesh::from_bevy_mesh(&source).is_err());
    assert!(HalfEdgeMesh::from_bevy_mesh_welded(&source).is_err());
}

#[test]
fn reverse_winding_duplicate_facets_are_rejected() {
    let source = mesh(
        vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]],
        vec![0, 1, 2, 1, 0, 2],
    );
    assert!(HalfEdgeMesh::from_bevy_mesh(&source).is_err());
    assert!(HalfEdgeMesh::from_bevy_mesh_welded(&source).is_err());
}

#[test]
fn raw_split_rejects_identical_opposite_vertices_atomically() {
    let source = mesh(
        vec![
            [0., 0., 0.],
            [1., 0., 0.],
            [0., 1., 0.],
            [2., 0., 0.],
            [3., 0., 0.],
            [2., 1., 0.],
        ],
        vec![0, 1, 2, 3, 4, 5],
    );
    let mut imported = HalfEdgeMesh::from_bevy_mesh(&source).unwrap();
    for (edge, origin) in [(3, 1), (4, 0), (5, 2)] {
        imported.half_edges[edge].origin = VertexId(origin);
    }
    for vertex in &mut imported.vertices[3..] {
        vertex.outgoing_half_edge = None;
    }
    imported.edge_map.clear();
    for edge in &imported.half_edges {
        imported.edge_map.insert(
            (
                edge.origin,
                imported.half_edges[edge.next.0 as usize].origin,
            ),
            edge.id,
        );
    }
    imported.rebuild_twins_from_edge_map();
    assert!(imported.check_manifold().is_err());
    let before = format!("{imported:?}");
    assert!(imported.split_edge_topology(HalfEdgeId(0)).is_none());
    assert_eq!(format!("{imported:?}"), before);
}

#[test]
fn rejected_duplicate_facet_weld_preserves_separately_indexed_sheets() {
    let source = mesh(
        vec![
            [0., 0., 0.],
            [1., 0., 0.],
            [0., 1., 0.],
            [1., 0., 0.],
            [0., 0., 0.],
            [0., 1., 0.],
        ],
        vec![0, 1, 2, 3, 4, 5],
    );
    assert!(HalfEdgeMesh::from_bevy_mesh_welded(&source).is_err());
    let preserved = HalfEdgeMesh::from_bevy_mesh(&source).unwrap();
    assert_eq!(preserved.face_count(), 2);
    assert_eq!(preserved.vertex_count(), 6);
    assert!(preserved.check_manifold().is_ok());
    assert_eq!(preserved.to_bevy_mesh().count_vertices(), 6);
}
