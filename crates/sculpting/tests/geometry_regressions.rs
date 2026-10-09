#![cfg(feature = "bevy")]

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use painting::half_edge::{HalfEdgeId, HalfEdgeMesh};
use sculpting::{
    BrushInput, BrushPreset, PartitionConfig, PipelineConfig, SculptingPipeline,
    TessellationConfig, TessellationMode, merge_chunks, partition_mesh,
};

fn corner_records(mesh: &Mesh) -> Vec<[u32; 5]> {
    let positions = mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .unwrap()
        .as_float3()
        .unwrap();
    let VertexAttributeValues::Float32x2(uvs) = mesh.attribute(Mesh::ATTRIBUTE_UV_0).unwrap()
    else {
        panic!()
    };
    let mut records: Vec<_> = mesh
        .indices()
        .unwrap()
        .iter()
        .map(|i| {
            [
                positions[i][0].to_bits(),
                positions[i][1].to_bits(),
                positions[i][2].to_bits(),
                uvs[i][0].to_bits(),
                uvs[i][1].to_bits(),
            ]
        })
        .collect();
    records.sort();
    records
}

#[test]
fn partition_and_merge_preserve_uv_sphere_corners() {
    let source = Sphere::new(1.).mesh().uv(16, 8);
    let imported = HalfEdgeMesh::from_bevy_mesh_welded(&source).unwrap();
    let chunks = partition_mesh(
        &imported,
        &PartitionConfig {
            target_faces: 32,
            min_faces: 16,
            max_faces: 40,
        },
    );
    assert!(chunks.chunk_count() > 1);
    let merged = merge_chunks(&chunks);
    assert_eq!(merged.mesh.face_count(), imported.face_count());
    assert!(merged.mesh.check_manifold().is_ok());
    // Welding intentionally uses canonical geometric positions. Compare after
    // import, where the geometric position contract has already been applied.
    assert_eq!(
        corner_records(&merged.mesh.to_bevy_mesh()),
        corner_records(&imported.to_bevy_mesh())
    );
}

#[test]
fn collapse_prediction_checks_destination_only_faces_and_zero_area() {
    let mut source = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    source.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![
            [0., 0., 0.],
            [1., 0., 0.],
            [0., 1., 0.],
            [1., 1., 0.],
            [2., 0., 0.],
        ],
    );
    source.insert_indices(Indices::U32(vec![0, 1, 2, 1, 3, 2, 1, 4, 3]));
    let imported = HalfEdgeMesh::from_bevy_mesh(&source).unwrap();
    assert!(sculpting::would_cause_flip(
        &imported,
        HalfEdgeId(0),
        Vec3::new(-2., 2., 0.)
    ));
    assert!(sculpting::would_cause_flip(
        &imported,
        HalfEdgeId(0),
        Vec3::new(-1., 1., 0.)
    ));
    assert!(!sculpting::would_cause_flip(
        &imported,
        HalfEdgeId(0),
        Vec3::new(0.5, 0., 0.)
    ));
    assert!(sculpting::would_cause_flip(
        &imported,
        HalfEdgeId(0),
        Vec3::splat(f32::NAN)
    ));
}

#[test]
fn repeated_split_collapse_compaction_keeps_a_closed_surface() {
    let source = Sphere::new(1.).mesh().ico(1).unwrap();
    let mut imported = HalfEdgeMesh::from_bevy_mesh_welded(&source).unwrap();
    // No UV chart changes in this topology stress fixture.
    let mut source = imported.to_bevy_mesh();
    source.remove_attribute(Mesh::ATTRIBUTE_UV_0);
    imported = HalfEdgeMesh::from_bevy_mesh_welded(&source).unwrap();
    for iteration in 0..12 {
        let edge = imported.half_edges()[iteration % imported.half_edges().len()].id;
        imported.split_edge_topology(edge).unwrap();
        assert!(imported.check_manifold().is_ok(), "split {iteration}");
        let candidate = imported
            .half_edges()
            .iter()
            .find(|edge| sculpting::can_collapse_edge(&imported, edge.id))
            .map(|edge| edge.id)
            .expect("fixture must exercise successful collapse");
        sculpting::collapse_edge(&mut imported, candidate).unwrap();
        assert!(imported.check_manifold().is_ok(), "collapse {iteration}");
        imported.try_compact().unwrap();
        assert!(imported.check_manifold().is_ok(), "compact {iteration}");
        assert!(imported.half_edges().iter().all(|edge| edge.twin.is_some()));
    }
}

#[test]
fn real_sculpt_pipeline_deforms_tessellates_and_exports_without_seam_loss() {
    let source = Sphere::new(1.).mesh().uv(16, 8);
    let imported = HalfEdgeMesh::from_bevy_mesh_welded(&source).unwrap();
    let mut chunks = partition_mesh(
        &imported,
        &PartitionConfig {
            target_faces: 1000,
            min_faces: 1,
            max_faces: 2000,
        },
    );
    let preset = BrushPreset {
        radius: 0.75,
        strength: 0.15,
        spacing: 0.,
        autosmooth: 0.,
        ..BrushPreset::push()
    };
    let config = PipelineConfig {
        tessellation_config: TessellationConfig {
            mode: TessellationMode::BudgetCurvature,
            max_splits_per_pass: 8,
            max_tessellation_iterations: 1,
            curvature_split_threshold: 0.01,
            ..Default::default()
        },
        rebalance_after_stroke: false,
        ..Default::default()
    };
    let mut pipeline = SculptingPipeline::with_config(preset, config);
    pipeline.update_budget_from_coverage(1000);
    let input = BrushInput {
        position: Vec3::X,
        normal: Vec3::X,
        pressure: 1.,
        timestamp_ms: 1,
    };
    pipeline.begin_stroke(0, input);
    let result = pipeline.process_input(
        BrushInput {
            timestamp_ms: 2,
            ..input
        },
        &mut chunks,
    );
    assert!(result.vertices_modified > 0);
    assert!(result.tessellation.as_ref().unwrap().edges_split > 0);
    let end = pipeline.end_stroke(&mut chunks);
    assert!(!end.packets.is_empty());
    let merged = merge_chunks(&chunks);
    assert!(merged.mesh.check_manifold().is_ok());
    assert!(
        merged
            .mesh
            .half_edges()
            .iter()
            .all(|edge| edge.twin.is_some())
    );
    assert!(merged.mesh.face_count() > imported.face_count());
    let exported = merged.mesh.to_bevy_mesh();
    let VertexAttributeValues::Float32x2(uvs) = exported.attribute(Mesh::ATTRIBUTE_UV_0).unwrap()
    else {
        panic!()
    };
    assert!(uvs.iter().any(|uv| uv[0] == 0.));
    assert!(uvs.iter().any(|uv| uv[0] == 1.));
    assert!(
        exported
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap()
            .as_float3()
            .unwrap()
            .iter()
            .any(|p| p[0] > 1.)
    );
}
