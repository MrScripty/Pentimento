#![cfg(feature = "bevy")]

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use painting::half_edge::HalfEdgeMesh;
use sculpting::{
    BrushInput, BrushPreset, FalloffCurve, PartitionConfig, PipelineConfig, SculptingPipeline,
    merge_chunks, partition_mesh,
};

fn crossing_fixture() -> HalfEdgeMesh {
    let mut source = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    source.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![
            [0., 0., 0.06],
            [0.5, 0., 0.06],
            [0., 0.5, 0.06],
            [-1., -1., 0.],
            [2., -1., 0.],
            [-1., 2., 0.],
        ],
    );
    source.insert_indices(Indices::U32(vec![0, 1, 2, 3, 4, 5]));
    HalfEdgeMesh::from_bevy_mesh(&source).unwrap()
}

#[test]
fn repeated_strokes_cannot_drive_one_sheet_through_another() {
    let imported = crossing_fixture();
    let mut chunks = partition_mesh(&imported, &PartitionConfig::default());
    let mut pipeline = SculptingPipeline::with_config(
        BrushPreset {
            radius: 0.15,
            strength: 0.2,
            spacing: 0.,
            autosmooth: 0.,
            falloff: FalloffCurve::Constant,
            ..BrushPreset::push()
        },
        PipelineConfig {
            tessellation_enabled: false,
            rebalance_after_stroke: false,
            ..Default::default()
        },
    );
    for stroke in 0..8 {
        let input = BrushInput {
            position: Vec3::new(0., 0., 0.06),
            normal: -Vec3::Z,
            pressure: 1.,
            timestamp_ms: stroke * 10,
        };
        pipeline.begin_stroke(0, input);
        pipeline.process_input(
            BrushInput {
                timestamp_ms: input.timestamp_ms + 1,
                ..input
            },
            &mut chunks,
        );
        pipeline.end_stroke(&mut chunks);
        let mesh = merge_chunks(&chunks).mesh;
        assert!(mesh.check_manifold().is_ok());
        // The unchanged upper two corners are above z=0. A non-positive tip
        // means the top face contacts/crosses the interior of the lower face.
        assert!(
            mesh.vertices()[0].position.z > 1e-7,
            "stroke {stroke}: topological manifoldness accepted intersecting sheets: {:?}",
            mesh.vertices()[0].position
        );
    }
}

use sculpting::{
    ChunkedMesh, DeformationType, SafetyError, TessellationConfig, TessellationMode,
    validate_sculpt_surface,
};

fn snapshot(mesh: &ChunkedMesh) -> String {
    // Includes geometry/UVs, topology, both global identity maps, boundary
    // references, spatial index, bounds and allocation counters. Dirty flags
    // intentionally differ after rollback because the GPU needs a full upload.
    let mut copy = mesh.clone();
    for chunk in copy.chunks.values_mut() {
        chunk.clear_dirty();
    }
    format!("{copy:?}")
}

#[test]
fn rejection_rolls_back_the_whole_stroke_and_suppresses_replay() {
    let mut chunks = partition_mesh(
        &crossing_fixture(),
        &PartitionConfig {
            target_faces: 1,
            min_faces: 1,
            max_faces: 1,
        },
    );
    assert_eq!(chunks.chunk_count(), 2);
    let original = snapshot(&chunks);
    let mut pipeline = SculptingPipeline::with_config(
        BrushPreset {
            radius: 0.15,
            strength: 0.2,
            spacing: 0.,
            autosmooth: 0.,
            falloff: FalloffCurve::Constant,
            ..BrushPreset::push()
        },
        PipelineConfig {
            tessellation_enabled: false,
            rebalance_after_stroke: false,
            ..Default::default()
        },
    );
    let input = BrushInput {
        position: Vec3::new(0., 0., 0.06),
        normal: -Vec3::Z,
        pressure: 1.,
        timestamp_ms: 0,
    };
    pipeline.begin_stroke(0, input);
    for dab in 1..3 {
        let result = pipeline.process_input(
            BrushInput {
                timestamp_ms: dab,
                ..input
            },
            &mut chunks,
        );
        assert!(result.rejected.is_none());
        validate_sculpt_surface(&chunks).unwrap();
    }
    assert_ne!(
        snapshot(&chunks),
        original,
        "the first two dabs must really deform"
    );
    let result = pipeline.process_input(
        BrushInput {
            timestamp_ms: 3,
            ..input
        },
        &mut chunks,
    );
    assert_eq!(result.rejected, Some(SafetyError::Intersection));
    assert_eq!(snapshot(&chunks), original);
    assert!(
        chunks
            .chunks
            .values()
            .all(|c| c.dirty && c.topology_changed)
    );
    for dab in 4..9 {
        assert!(
            pipeline
                .process_input(
                    BrushInput {
                        timestamp_ms: dab,
                        ..input
                    },
                    &mut chunks
                )
                .rejected
                .is_some()
        );
        assert_eq!(snapshot(&chunks), original);
    }
    let end = pipeline.end_stroke(&mut chunks);
    assert!(end.rejected.is_some());
    assert!(end.packets.is_empty());
    assert_eq!(snapshot(&chunks), original);
    // A fresh stroke in the opposite direction must remain usable.
    let input = BrushInput {
        normal: Vec3::Z,
        timestamp_ms: 10,
        ..input
    };
    pipeline.begin_stroke(0, input);
    assert!(
        pipeline
            .process_input(
                BrushInput {
                    timestamp_ms: 11,
                    ..input
                },
                &mut chunks
            )
            .rejected
            .is_none()
    );
    assert!(!pipeline.end_stroke(&mut chunks).packets.is_empty());
    assert_ne!(snapshot(&chunks), original);
}

fn assert_surface(mesh: &ChunkedMesh, label: &str) {
    validate_sculpt_surface(mesh).unwrap_or_else(|e| panic!("{label}: {e}"));
    let merged = merge_chunks(mesh).mesh;
    merged
        .check_manifold()
        .unwrap_or_else(|e| panic!("{label}: {e}"));
    // Every exported half-edge has a live owner. No orphan/tombstone edges may
    // escape compaction into the merged sculpt result.
    assert!(
        merged.half_edges().iter().all(|e| e.face.is_some()),
        "{label}"
    );
    assert_eq!(
        merged.half_edges().len(),
        merged.face_count() * 3,
        "{label}"
    );
}

fn global_positions(mesh: &ChunkedMesh) -> std::collections::BTreeMap<u32, Vec3> {
    let mut positions = std::collections::BTreeMap::new();
    for chunk in mesh.chunks.values() {
        for vertex in chunk.mesh.vertices() {
            let global = chunk.local_to_original[&vertex.id].0;
            if let Some(previous) = positions.insert(global, vertex.position) {
                assert_eq!(previous, vertex.position, "split global vertex {global}");
            }
        }
    }
    positions
}

fn smoothing_fixture(max_faces: usize) -> ChunkedMesh {
    let source = Sphere::new(1.).mesh().uv(16, 8);
    let mesh = HalfEdgeMesh::from_bevy_mesh_welded(&source).unwrap();
    partition_mesh(
        &mesh,
        &PartitionConfig {
            target_faces: max_faces / 2,
            min_faces: 1,
            max_faces,
        },
    )
}

fn smoothing_pipeline(deformation_type: DeformationType, autosmooth: f32) -> SculptingPipeline {
    SculptingPipeline::with_config(
        BrushPreset {
            radius: 0.9,
            strength: 0.15,
            spacing: 0.,
            autosmooth,
            deformation_type,
            ..BrushPreset::default()
        },
        PipelineConfig {
            tessellation_enabled: false,
            rebalance_after_stroke: false,
            ..Default::default()
        },
    )
}

fn without_positions_or_normals(mesh: &ChunkedMesh) -> ChunkedMesh {
    let mut copy = mesh.clone();
    for chunk in copy.chunks.values_mut() {
        for index in 0..chunk.mesh.vertex_count() {
            let vertex = chunk
                .mesh
                .vertex_mut(painting::half_edge::VertexId(index as u32))
                .unwrap();
            vertex.position = Vec3::ZERO;
            vertex.normal = Vec3::ZERO;
        }
        for index in 0..chunk.mesh.face_count() {
            chunk
                .mesh
                .face_mut(painting::half_edge::FaceId(index as u32))
                .unwrap()
                .normal = Vec3::ZERO;
        }
    }
    copy
}

#[test]
fn smoothing_uses_the_complete_one_ring_across_chunk_seams() {
    let mut errors = Vec::new();
    for (deformation, autosmooth) in [(DeformationType::Smooth, 0.), (DeformationType::Push, 0.35)]
    {
        let mut whole = smoothing_fixture(1000);
        let mut split = smoothing_fixture(32);
        assert_eq!(whole.chunk_count(), 1);
        assert!(split.chunk_count() > 1);
        let original = global_positions(&whole);
        assert_eq!(original, global_positions(&split));
        for mesh in [&mut whole, &mut split] {
            let mut pipeline = smoothing_pipeline(deformation, autosmooth);
            let input = BrushInput {
                position: Vec3::Z,
                normal: Vec3::Z,
                pressure: 1.,
                timestamp_ms: 0,
            };
            pipeline.begin_stroke(0, input);
            let result = pipeline.process_input(
                BrushInput {
                    timestamp_ms: 1,
                    ..input
                },
                mesh,
            );
            assert!(result.rejected.is_none(), "{deformation:?}: {result:?}");
            let end = pipeline.end_stroke(mesh);
            assert!(end.rejected.is_none());
            assert!(!end.packets.is_empty());
            assert_surface(mesh, "complete smoothing ring");
        }
        let whole_positions = global_positions(&whole);
        assert_ne!(whole_positions, original, "{deformation:?} must deform");
        let split_positions = global_positions(&split);
        let difference = whole_positions
            .iter()
            .map(|(global, position)| position.distance(split_positions[global]))
            .fold(0., f32::max);
        eprintln!("{deformation:?} autosmooth={autosmooth}: partition error {difference}");
        errors.push((deformation, difference));
    }
    assert!(
        errors.iter().all(|(_, error)| *error < 1e-6),
        "partition errors {errors:?}"
    );
}

#[test]
fn intersecting_smoothing_paths_preserve_topology_uvs_and_exact_history() {
    let mut meshes = [smoothing_fixture(1000), smoothing_fixture(32)];
    let structure = meshes.each_ref().map(without_positions_or_normals);
    assert!(
        meshes[0].chunks.values().any(|chunk| {
            chunk
                .mesh
                .half_edges()
                .iter()
                .any(|edge| chunk.mesh.is_uv_seam_edge(edge.id))
        }),
        "fixture must contain real face-corner UV seams"
    );
    let mut pipelines = [
        smoothing_pipeline(DeformationType::Smooth, 0.),
        smoothing_pipeline(DeformationType::Smooth, 0.),
    ];
    let mut endpoints = vec![meshes.clone()];
    for stroke in 0..12 {
        let direction = [
            Vec3::Z,
            Vec3::new(0.25, 0.12, 1.).normalize(),
            Vec3::new(-0.15, 0.25, 1.).normalize(),
            Vec3::new(0.2, -0.2, 1.).normalize(),
        ][stroke % 4];
        let (deformation_type, autosmooth) = [
            (DeformationType::Smooth, 0.),
            (DeformationType::Push, 0.25),
            (DeformationType::Grab, 0.2),
            (DeformationType::Smooth, 0.15),
        ][stroke % 4];
        for index in 0..2 {
            pipelines[index].set_brush_preset(BrushPreset {
                radius: 0.9,
                strength: 0.08,
                spacing: 0.,
                deformation_type,
                autosmooth,
                ..BrushPreset::default()
            });
            let input = BrushInput {
                position: direction,
                normal: direction,
                pressure: 1.,
                timestamp_ms: stroke as u64 * 10,
            };
            pipelines[index].begin_stroke(0, input);
            for dab in 1..4 {
                let result = pipelines[index].process_input(
                    BrushInput {
                        position: direction + Vec3::X * dab as f32 * 0.015,
                        timestamp_ms: input.timestamp_ms + dab,
                        ..input
                    },
                    &mut meshes[index],
                );
                assert!(
                    result.rejected.is_none(),
                    "stroke {stroke} dab {dab}: {result:?}"
                );
                assert_surface(&meshes[index], "intersecting smoothing dab");
                assert!(
                    structure[index]
                        .same_authoritative_state(&without_positions_or_normals(&meshes[index]))
                );
            }
            let end = pipelines[index].end_stroke(&mut meshes[index]);
            assert!(end.rejected.is_none());
            assert!(!end.packets.is_empty());
            assert!(!endpoints.last().unwrap()[index].same_authoritative_state(&meshes[index]));
            assert_surface(&meshes[index], "intersecting smoothing endpoint");
        }
        assert_eq!(
            global_positions(&meshes[0]),
            global_positions(&meshes[1]),
            "stroke {stroke}"
        );
        endpoints.push(meshes.clone());
    }
    for target in (0..12).rev() {
        for index in 0..2 {
            assert!(
                pipelines[index]
                    .restore_history(&mut meshes[index], false)
                    .unwrap()
            );
            assert!(endpoints[target][index].same_authoritative_state(&meshes[index]));
            assert_surface(&meshes[index], "smoothing undo");
        }
    }
    for index in 0..2 {
        assert!(
            !pipelines[index]
                .restore_history(&mut meshes[index], false)
                .unwrap()
        );
        // An empty stroke must preserve the pending redo branch and emit no replay.
        let input = BrushInput {
            position: Vec3::splat(10.),
            normal: Vec3::Z,
            pressure: 1.,
            timestamp_ms: 200,
        };
        pipelines[index].begin_stroke(0, input);
        pipelines[index].process_input(
            BrushInput {
                timestamp_ms: 201,
                ..input
            },
            &mut meshes[index],
        );
        assert!(
            pipelines[index]
                .end_stroke(&mut meshes[index])
                .packets
                .is_empty()
        );
        assert!(endpoints[0][index].same_authoritative_state(&meshes[index]));
    }
    for target in 1..=12 {
        for index in 0..2 {
            assert!(
                pipelines[index]
                    .restore_history(&mut meshes[index], true)
                    .unwrap()
            );
            assert!(endpoints[target][index].same_authoritative_state(&meshes[index]));
            assert_surface(&meshes[index], "smoothing redo");
        }
    }
    for index in 0..2 {
        assert!(
            !pipelines[index]
                .restore_history(&mut meshes[index], true)
                .unwrap()
        );
    }
}

#[test]
fn an_empty_or_zero_strength_dab_does_not_refresh_normals_into_a_history_entry() {
    for (position, strength) in [(Vec3::splat(10.), 0.15), (Vec3::Z, 0.)] {
        let mut mesh = smoothing_fixture(32);
        let original = mesh.clone();
        let mut pipeline = smoothing_pipeline(DeformationType::Push, 0.);
        let input = BrushInput {
            position: Vec3::Z,
            normal: Vec3::Z,
            pressure: 1.,
            timestamp_ms: 0,
        };
        pipeline.begin_stroke(0, input);
        pipeline.process_input(
            BrushInput {
                timestamp_ms: 1,
                ..input
            },
            &mut mesh,
        );
        assert!(!pipeline.end_stroke(&mut mesh).packets.is_empty());
        let accepted = mesh.clone();
        assert!(pipeline.restore_history(&mut mesh, false).unwrap());
        assert!(original.same_authoritative_state(&mesh));
        let status = pipeline.history_status();
        pipeline.set_brush_preset(BrushPreset {
            radius: 0.9,
            strength,
            spacing: 0.,
            autosmooth: 0.,
            ..BrushPreset::push()
        });
        let input = BrushInput {
            position,
            timestamp_ms: 10,
            ..input
        };
        pipeline.begin_stroke(0, input);
        let result = pipeline.process_input(
            BrushInput {
                timestamp_ms: 11,
                ..input
            },
            &mut mesh,
        );
        assert!(result.rejected.is_none());
        assert_eq!(result.vertices_modified, 0);
        assert!(pipeline.end_stroke(&mut mesh).packets.is_empty());
        assert!(original.same_authoritative_state(&mesh));
        assert_eq!(status, pipeline.history_status());
        assert!(pipeline.restore_history(&mut mesh, true).unwrap());
        assert!(accepted.same_authoritative_state(&mesh));
        assert_surface(&mesh, "redo after no-op");
    }
}

#[test]
fn deformation_refreshes_normals_beyond_the_moved_vertex_query() {
    let mut mesh = partition_mesh(&crossing_fixture(), &PartitionConfig::default());
    let mut pipeline = SculptingPipeline::with_config(
        BrushPreset {
            radius: 0.15,
            strength: 0.3,
            spacing: 0.,
            autosmooth: 0.,
            ..BrushPreset::push()
        },
        PipelineConfig {
            tessellation_enabled: false,
            rebalance_after_stroke: false,
            ..Default::default()
        },
    );
    let before = global_positions(&mesh);
    let input = BrushInput {
        position: Vec3::new(0., 0., 0.06),
        normal: Vec3::Z,
        pressure: 1.,
        timestamp_ms: 0,
    };
    pipeline.begin_stroke(0, input);
    let result = pipeline.process_input(
        BrushInput {
            timestamp_ms: 1,
            ..input
        },
        &mut mesh,
    );
    assert!(result.rejected.is_none());
    assert_eq!(result.vertices_modified, 1);
    let after = global_positions(&mesh);
    assert_ne!(before[&0], after[&0]);
    assert_eq!(before[&1], after[&1]);
    assert_eq!(before[&2], after[&2]);
    let chunk = mesh.chunks.values().next().unwrap();
    let normal = (after[&1] - after[&0])
        .cross(after[&2] - after[&0])
        .normalize();
    for global in 0..3 {
        let local = chunk.original_to_local[&painting::half_edge::VertexId(global)];
        assert!(
            chunk.mesh.vertex(local).unwrap().normal.distance(normal) < 1e-6,
            "normal at {global}"
        );
    }
    assert_surface(&mesh, "normals outside query");
}

#[test]
fn every_chunk_with_changed_shared_normals_is_dirty_for_render_sync() {
    let mut mesh = smoothing_fixture(32);
    for chunk in mesh.chunks.values_mut() {
        chunk.clear_dirty();
    }
    let before = mesh.clone();
    let mut pipeline = smoothing_pipeline(DeformationType::Push, 0.);
    let input = BrushInput {
        position: Vec3::Z,
        normal: Vec3::Z,
        pressure: 1.,
        timestamp_ms: 0,
    };
    pipeline.begin_stroke(0, input);
    let result = pipeline.process_input(
        BrushInput {
            timestamp_ms: 1,
            ..input
        },
        &mut mesh,
    );
    assert!(result.rejected.is_none());
    let mut changes_outside_position_edits = 0;
    for (id, chunk) in &mesh.chunks {
        let old = &before.chunks[id];
        let positions_changed = chunk
            .mesh
            .vertices()
            .iter()
            .any(|vertex| vertex.position != old.mesh.vertex(vertex.id).unwrap().position);
        let normals_changed = chunk
            .mesh
            .vertices()
            .iter()
            .any(|vertex| vertex.normal != old.mesh.vertex(vertex.id).unwrap().normal);
        if positions_changed || normals_changed {
            assert!(chunk.dirty, "changed chunk {id:?} missed render sync");
        }
        changes_outside_position_edits += usize::from(normals_changed && !positions_changed);
    }
    assert!(
        changes_outside_position_edits > 0,
        "fixture must exercise normal-only chunk changes"
    );
    assert_surface(&mesh, "shared normal sync");
}

#[test]
fn a_collapsing_smooth_stroke_rolls_back_and_preserves_redo() {
    let mut mesh = partition_mesh(
        &crossing_fixture(),
        &PartitionConfig {
            target_faces: 1,
            min_faces: 1,
            max_faces: 1,
        },
    );
    let original = mesh.clone();
    let mut pipeline = smoothing_pipeline(DeformationType::Push, 0.);
    let input = BrushInput {
        position: Vec3::new(0., 0., 0.06),
        normal: Vec3::Z,
        pressure: 1.,
        timestamp_ms: 0,
    };
    pipeline.begin_stroke(0, input);
    pipeline.process_input(
        BrushInput {
            timestamp_ms: 1,
            ..input
        },
        &mut mesh,
    );
    assert!(!pipeline.end_stroke(&mut mesh).packets.is_empty());
    let accepted = mesh.clone();
    assert!(pipeline.restore_history(&mut mesh, false).unwrap());
    let status = pipeline.history_status();
    pipeline.set_brush_preset(BrushPreset {
        radius: 0.8,
        strength: 2. / 3.,
        spacing: 0.,
        autosmooth: 0.3,
        falloff: FalloffCurve::Constant,
        deformation_type: DeformationType::Smooth,
        ..BrushPreset::default()
    });
    let input = BrushInput {
        timestamp_ms: 10,
        ..input
    };
    pipeline.begin_stroke(0, input);
    let result = pipeline.process_input(
        BrushInput {
            timestamp_ms: 11,
            ..input
        },
        &mut mesh,
    );
    // The ideal target is one point; f32 roundoff can instead leave an
    // inverted sliver. Both must be rejected by the unchanged geometric guard.
    assert!(matches!(
        result.rejected,
        Some(SafetyError::DegenerateFace | SafetyError::InvertedFace)
    ));
    assert!(original.same_authoritative_state(&mesh));
    let end = pipeline.end_stroke(&mut mesh);
    assert_eq!(end.rejected, result.rejected);
    assert!(end.packets.is_empty());
    assert_eq!(status, pipeline.history_status());
    assert_surface(&mesh, "rejected smooth rollback");
    assert!(pipeline.restore_history(&mut mesh, true).unwrap());
    assert!(accepted.same_authoritative_state(&mesh));
    assert_surface(&mesh, "redo after rejected smooth");
}

#[test]
fn layered_brushes_at_uv_seams_poles_and_chunk_boundaries_stay_embedded() {
    let source = Sphere::new(1.).mesh().uv(16, 8);
    let mesh = HalfEdgeMesh::from_bevy_mesh_welded(&source).unwrap();
    let mut chunks = partition_mesh(
        &mesh,
        &PartitionConfig {
            target_faces: 64,
            min_faces: 8,
            max_faces: 80,
        },
    );
    assert!(chunks.chunk_count() > 1);
    let mut pipeline = SculptingPipeline::with_config(
        BrushPreset::default(),
        PipelineConfig {
            tessellation_config: TessellationConfig {
                mode: TessellationMode::BudgetCurvature,
                max_splits_per_pass: 3,
                max_tessellation_iterations: 1,
                curvature_split_threshold: 0.01,
                ..Default::default()
            },
            chunk_config: sculpting::ChunkConfig {
                target_faces: 64,
                min_faces: 8,
                max_faces: 96,
            },
            ..Default::default()
        },
    );
    pipeline.update_budget_from_coverage(900);
    let mut accepted = 0;
    let mut split = 0;
    for stroke in 0..32 {
        let direction = [
            Vec3::X,
            Vec3::new(0.98, 0.12, 0.08).normalize(),
            Vec3::Y,
            Vec3::new(0.12, 0.98, 0.08).normalize(),
        ][stroke % 4];
        let deformation_type = [
            DeformationType::Push,
            DeformationType::Pull,
            DeformationType::Smooth,
            DeformationType::Flatten,
            DeformationType::Inflate,
            DeformationType::Pinch,
            DeformationType::Crease,
            DeformationType::Grab,
        ][stroke % 8];
        pipeline.set_brush_preset(BrushPreset {
            radius: 0.8,
            strength: if stroke % 7 == 0 { 1. } else { 0.15 },
            spacing: 0.,
            autosmooth: 0.1,
            deformation_type,
            ..BrushPreset::default()
        });
        let before = snapshot(&chunks);
        let input = BrushInput {
            position: direction,
            normal: direction * if stroke % 2 == 0 { 1. } else { -1. },
            pressure: 1.,
            timestamp_ms: stroke as u64 * 20,
        };
        pipeline.begin_stroke(0, input);
        for dab in 1..4 {
            let result = pipeline.process_input(
                BrushInput {
                    position: direction + Vec3::Z * dab as f32 * 0.015,
                    timestamp_ms: input.timestamp_ms + dab,
                    ..input
                },
                &mut chunks,
            );
            split += result.tessellation.map_or(0, |t| t.edges_split);
            assert_surface(&chunks, &format!("stroke {stroke} dab {dab}"));
        }
        let end = pipeline.end_stroke(&mut chunks);
        assert_surface(&chunks, &format!("stroke {stroke} end"));
        if end.rejected.is_some() {
            assert_eq!(snapshot(&chunks), before);
        } else {
            accepted += 1;
        }
    }
    eprintln!("UV/seam sequence: {accepted}/32 accepted, {split} admitted splits");
    assert!(
        accepted >= 4,
        "even this deliberately aggressive sequence must admit useful strokes"
    );
    assert!(split > 0, "sequence must exercise adaptive topology");
}

#[test]
fn malformed_or_overlapping_initial_surface_is_rejected_without_repair() {
    let mut mesh = crossing_fixture();
    for vertex in [
        painting::half_edge::VertexId(0),
        painting::half_edge::VertexId(1),
        painting::half_edge::VertexId(2),
    ] {
        let mut p = mesh.vertex(vertex).unwrap().position;
        p.z = 0.;
        mesh.set_vertex_position(vertex, p);
    }
    let mut chunks = partition_mesh(&mesh, &PartitionConfig::default());
    let before = snapshot(&chunks);
    let mut pipeline = SculptingPipeline::new(BrushPreset::push());
    let input = BrushInput {
        position: Vec3::ZERO,
        normal: Vec3::Z,
        pressure: 1.,
        timestamp_ms: 0,
    };
    pipeline.begin_stroke(0, input);
    let result = pipeline.process_input(
        BrushInput {
            timestamp_ms: 1,
            ..input
        },
        &mut chunks,
    );
    assert_eq!(result.rejected, Some(SafetyError::Intersection));
    assert_eq!(snapshot(&chunks), before);
    assert!(pipeline.end_stroke(&mut chunks).packets.is_empty());
}

#[test]
fn large_displacement_cannot_tunnel_through_a_disconnected_sheet() {
    let mut mesh = crossing_fixture();
    // Make the moving face small enough that all of it receives the push,
    // while all lower-face vertices remain outside the brush.
    mesh.set_vertex_position(painting::half_edge::VertexId(1), Vec3::new(0.05, 0., 0.06));
    mesh.set_vertex_position(painting::half_edge::VertexId(2), Vec3::new(0., 0.05, 0.06));
    let mut chunks = partition_mesh(&mesh, &PartitionConfig::default());
    let before = snapshot(&chunks);
    let mut pipeline = SculptingPipeline::with_config(
        BrushPreset {
            radius: 0.15,
            strength: 1.,
            spacing: 0.,
            autosmooth: 0.,
            falloff: FalloffCurve::Constant,
            ..BrushPreset::push()
        },
        PipelineConfig {
            tessellation_enabled: false,
            rebalance_after_stroke: false,
            ..Default::default()
        },
    );
    let input = BrushInput {
        position: Vec3::new(0., 0., 0.06),
        normal: -Vec3::Z,
        pressure: 1.,
        timestamp_ms: 0,
    };
    pipeline.begin_stroke(0, input);
    let result = pipeline.process_input(
        BrushInput {
            timestamp_ms: 1,
            ..input
        },
        &mut chunks,
    );
    assert_eq!(result.rejected, Some(SafetyError::Intersection));
    assert_eq!(snapshot(&chunks), before);
    assert!(pipeline.end_stroke(&mut chunks).packets.is_empty());
}

#[test]
fn repeated_adaptive_refinement_and_coarsening_strokes_stay_closed() {
    let mut source = Sphere::new(1.).mesh().ico(2).unwrap();
    source.remove_attribute(Mesh::ATTRIBUTE_UV_0);
    let original = HalfEdgeMesh::from_bevy_mesh_welded(&source).unwrap();
    let mut chunks = partition_mesh(
        &original,
        &PartitionConfig {
            target_faces: 10000,
            min_faces: 1,
            max_faces: 20000,
        },
    );
    let mut pipeline = SculptingPipeline::with_config(
        BrushPreset {
            radius: 2.1,
            strength: 0.06,
            spacing: 0.,
            autosmooth: 0.02,
            ..BrushPreset::push()
        },
        PipelineConfig {
            tessellation_config: TessellationConfig {
                mode: TessellationMode::BudgetCurvature,
                max_splits_per_pass: 4,
                max_tessellation_iterations: 1,
                curvature_split_threshold: 0.01,
                curvature_collapse_threshold: 0.,
                ..Default::default()
            },
            rebalance_after_stroke: false,
            ..Default::default()
        },
    );
    let mut splits = 0;
    let mut collapses = 0;
    let mut accepted = 0;
    for stroke in 0..16 {
        pipeline.update_budget_from_coverage(if stroke % 4 < 2 { 500 } else { 100 });
        let input = BrushInput {
            position: Vec3::ZERO,
            normal: if stroke % 2 == 0 { Vec3::Y } else { -Vec3::Y },
            pressure: 1.,
            timestamp_ms: stroke * 10,
        };
        let before = snapshot(&chunks);
        pipeline.begin_stroke(0, input);
        let result = pipeline.process_input(
            BrushInput {
                timestamp_ms: input.timestamp_ms + 1,
                ..input
            },
            &mut chunks,
        );
        assert_surface(&chunks, &format!("adaptive stroke {stroke}"));
        let end = pipeline.end_stroke(&mut chunks);
        assert_surface(&chunks, &format!("adaptive stroke {stroke} end"));
        if end.rejected.is_some() {
            assert_eq!(snapshot(&chunks), before);
        } else {
            accepted += 1;
            if let Some(t) = result.tessellation {
                splits += t.edges_split;
                collapses += t.edges_collapsed;
            }
        }
        assert!(
            merge_chunks(&chunks)
                .mesh
                .half_edges()
                .iter()
                .all(|e| e.twin.is_some())
        );
    }
    eprintln!("adaptive sequence: {accepted}/16 accepted, splits={splits}, collapses={collapses}");
    assert!(accepted >= 8);
    assert!(
        splits > 0 && collapses > 0,
        "must exercise both split and collapse, not only rejection"
    );
}

#[test]
fn ordinary_opposing_strokes_across_chunk_boundaries_are_not_blocked() {
    let source = Sphere::new(1.).mesh().uv(16, 8);
    let mesh = HalfEdgeMesh::from_bevy_mesh_welded(&source).unwrap();
    let mut chunks = partition_mesh(
        &mesh,
        &PartitionConfig {
            target_faces: 32,
            min_faces: 8,
            max_faces: 40,
        },
    );
    let mut pipeline = SculptingPipeline::with_config(
        BrushPreset {
            radius: 0.6,
            strength: 0.05,
            spacing: 0.,
            autosmooth: 0.,
            ..BrushPreset::push()
        },
        PipelineConfig {
            tessellation_enabled: false,
            rebalance_after_stroke: false,
            ..Default::default()
        },
    );
    for stroke in 0..48 {
        let direction = if stroke % 4 < 2 { Vec3::X } else { Vec3::Y };
        let input = BrushInput {
            position: direction,
            normal: direction * if stroke % 2 == 0 { 1. } else { -1. },
            pressure: 1.,
            timestamp_ms: stroke * 10,
        };
        pipeline.begin_stroke(0, input);
        let result = pipeline.process_input(
            BrushInput {
                timestamp_ms: input.timestamp_ms + 1,
                ..input
            },
            &mut chunks,
        );
        assert!(
            result.rejected.is_none(),
            "stroke {stroke}: {:?}",
            result.rejected
        );
        assert!(result.vertices_modified > 0);
        assert_surface(&chunks, &format!("ordinary stroke {stroke}"));
        let end = pipeline.end_stroke(&mut chunks);
        assert!(end.rejected.is_none() && !end.packets.is_empty());
        assert_surface(&chunks, &format!("ordinary stroke {stroke} end"));
    }
}

#[test]
#[ignore = "manual optimized interactive cost qualification"]
fn measure_layered_safety_cost() {
    for (segments, rings) in [(32, 16), (64, 32), (128, 64), (256, 128)] {
        let source = Sphere::new(1.).mesh().uv(segments, rings);
        let mesh = HalfEdgeMesh::from_bevy_mesh_welded(&source).unwrap();
        let mut chunks = partition_mesh(&mesh, &PartitionConfig::default());
        let mut pipeline = SculptingPipeline::new(BrushPreset {
            radius: 0.3,
            strength: 0.1,
            spacing: 0.,
            autosmooth: 0.1,
            ..BrushPreset::push()
        });
        let mut timings = Vec::new();
        let mut rejected = 0;
        for stroke in 0..6 {
            let input = BrushInput {
                position: Vec3::new(1., (stroke % 2) as f32 * 0.04, 0.),
                normal: Vec3::X * if stroke % 2 == 0 { 1. } else { -1. },
                pressure: 1.,
                timestamp_ms: stroke * 10,
            };
            pipeline.begin_stroke(0, input);
            let start = std::time::Instant::now();
            let first = pipeline.process_input(
                BrushInput {
                    timestamp_ms: input.timestamp_ms + 1,
                    ..input
                },
                &mut chunks,
            );
            let first_ms = start.elapsed().as_secs_f64() * 1000.;
            let next = std::time::Instant::now();
            let second = pipeline.process_input(
                BrushInput {
                    timestamp_ms: input.timestamp_ms + 2,
                    ..input
                },
                &mut chunks,
            );
            let second_ms = next.elapsed().as_secs_f64() * 1000.;
            let finish = std::time::Instant::now();
            let end = pipeline.end_stroke(&mut chunks);
            let end_ms = finish.elapsed().as_secs_f64() * 1000.;
            timings.push(first_ms + second_ms + end_ms);
            if end.rejected.is_some() {
                rejected += 1;
            }
            eprintln!(
                "faces={} stroke={stroke} first_dab_ms={first_ms:.2} next_dab_ms={second_ms:.2} end_ms={end_ms:.2} tess={:?}/{:?} rejected={:?}",
                mesh.face_count(),
                first.tessellation,
                second.tessellation,
                end.rejected
            );
            assert_surface(&chunks, "performance fixture");
        }
        let mut sorted = timings.clone();
        sorted.sort_by(f64::total_cmp);
        eprintln!(
            "SUMMARY faces={} strokes=6 rejected={rejected} first_stroke_ms={:.2} median_stroke_ms={:.2} worst_stroke_ms={:.2}",
            mesh.face_count(),
            timings[0],
            sorted[3],
            sorted[5]
        );
    }
}

#[test]
fn a_strong_pull_cannot_invert_an_open_face() {
    let mut source = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    source.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![[0.01, 0., 0.], [0., 1., 0.], [0., -1., 0.]],
    );
    source.insert_indices(Indices::U32(vec![0, 1, 2]));
    let mesh = HalfEdgeMesh::from_bevy_mesh(&source).unwrap();
    let mut chunks = partition_mesh(&mesh, &PartitionConfig::default());
    let before = snapshot(&chunks);
    let mut pipeline = SculptingPipeline::with_config(
        BrushPreset {
            radius: 0.2,
            strength: 1.,
            spacing: 0.,
            autosmooth: 0.,
            falloff: FalloffCurve::Constant,
            ..BrushPreset::pull()
        },
        PipelineConfig {
            tessellation_enabled: false,
            rebalance_after_stroke: false,
            ..Default::default()
        },
    );
    let input = BrushInput {
        position: Vec3::new(-0.05, 0., 0.),
        normal: Vec3::Z,
        pressure: 1.,
        timestamp_ms: 0,
    };
    pipeline.begin_stroke(0, input);
    let result = pipeline.process_input(
        BrushInput {
            timestamp_ms: 1,
            ..input
        },
        &mut chunks,
    );
    assert_eq!(result.rejected, Some(SafetyError::InvertedFace));
    assert_eq!(snapshot(&chunks), before);
    assert!(pipeline.end_stroke(&mut chunks).packets.is_empty());
}

#[test]
fn high_valence_open_boundary_survives_neighboring_layered_strokes() {
    let mut source = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    let mut positions = vec![[0., 0., 0.]];
    for i in 0..128 {
        let angle = i as f32 * std::f32::consts::TAU / 128.;
        positions.push([angle.cos(), angle.sin(), 0.]);
    }
    let mut indices = Vec::new();
    for i in 0..128 {
        indices.extend([0, i + 1, (i + 1) % 128 + 1]);
    }
    source.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    source.insert_indices(Indices::U32(indices));
    let mesh = HalfEdgeMesh::from_bevy_mesh(&source).unwrap();
    assert_eq!(
        mesh.get_vertex_faces(painting::half_edge::VertexId(0))
            .len(),
        128
    );
    let mut chunks = partition_mesh(
        &mesh,
        &PartitionConfig {
            target_faces: 32,
            min_faces: 8,
            max_faces: 40,
        },
    );
    let mut pipeline = SculptingPipeline::with_config(
        BrushPreset::push(),
        PipelineConfig {
            tessellation_config: TessellationConfig {
                max_splits_per_pass: 2,
                max_tessellation_iterations: 1,
                ..Default::default()
            },
            rebalance_after_stroke: false,
            ..Default::default()
        },
    );
    let mut accepted = 0;
    let mut splits = 0;
    for stroke in 0..24 {
        pipeline.set_brush_preset(BrushPreset {
            radius: 0.8,
            strength: if stroke % 5 == 0 { 1. } else { 0.2 },
            spacing: 0.,
            autosmooth: 0.,
            deformation_type: if stroke % 3 == 0 {
                DeformationType::Smooth
            } else {
                DeformationType::Push
            },
            ..BrushPreset::push()
        });
        let before = snapshot(&chunks);
        let input = BrushInput {
            position: Vec3::new(if stroke % 4 < 2 { 0. } else { 0.35 }, 0., 0.),
            normal: Vec3::Z * if stroke % 2 == 0 { 1. } else { -1. },
            pressure: 1.,
            timestamp_ms: stroke * 10,
        };
        pipeline.begin_stroke(0, input);
        for dab in 1..3 {
            let result = pipeline.process_input(
                BrushInput {
                    timestamp_ms: input.timestamp_ms + dab,
                    ..input
                },
                &mut chunks,
            );
            splits += result.tessellation.map_or(0, |t| t.edges_split);
            if let Some(error) = &result.rejected {
                eprintln!("high-valence stroke {stroke} dab {dab}: {error}");
            }
            assert_surface(&chunks, &format!("boundary stroke {stroke} dab {dab}"));
        }
        let end = pipeline.end_stroke(&mut chunks);
        assert_surface(&chunks, &format!("boundary stroke {stroke} end"));
        if end.rejected.is_some() {
            assert_eq!(snapshot(&chunks), before);
        } else {
            accepted += 1;
        }
    }
    eprintln!("high-valence open sequence: {accepted}/24 accepted, {splits} admitted splits");
    assert!(accepted >= 4);
    assert!(splits > 0);
}

fn raw_fixture(positions: Vec<[f32; 3]>, indices: Vec<u32>) -> HalfEdgeMesh {
    let mut m = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    m.insert_indices(Indices::U32(indices));
    HalfEdgeMesh::from_bevy_mesh(&m).unwrap()
}
#[test]
fn folded_shared_edge_sliver_is_rejected() {
    let mesh = raw_fixture(
        vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0.5, 1e-8, 0.]],
        vec![0, 1, 2, 1, 0, 3],
    );
    mesh.check_manifold().unwrap();
    let chunks = partition_mesh(&mesh, &PartitionConfig::default());
    let validity = validate_sculpt_surface(&chunks);
    eprintln!("folded sliver validity: {validity:?}");
    assert_eq!(validity, Err(SafetyError::Intersection));
}
#[test]
fn flip_only_tessellation_marks_topology_dirty() {
    let mesh = raw_fixture(
        vec![
            [-0.1, 0., 0.],
            [0.1, 0., 0.],
            [0., -1., 0.],
            [0., -0.3, 0.1],
            [0., -0.3, -0.1],
        ],
        vec![0, 1, 3, 1, 2, 3, 2, 0, 3, 1, 0, 4, 2, 1, 4, 0, 2, 4],
    );
    mesh.check_manifold().unwrap();
    let mut chunks = partition_mesh(&mesh, &PartitionConfig::default());
    validate_sculpt_surface(&chunks).unwrap();
    let mut next = chunks.next_original_vertex_id;
    let chunk = chunks.chunks.values_mut().next().unwrap();
    chunk.clear_dirty();
    let before = format!("{:?}", chunk.mesh);
    let stats = sculpting::tessellate_at_brush_budget(
        chunk,
        Vec3::ZERO,
        0.1,
        &TessellationConfig {
            mode: TessellationMode::BudgetCurvature,
            curvature_split_threshold: 1000.,
            curvature_collapse_threshold: 100.,
            max_splits_per_pass: 0,
            max_tessellation_iterations: 1,
            min_faces: 4,
            ..Default::default()
        },
        &mut sculpting::VertexBudget::from_pixel_coverage(100, 1.),
        &mut next,
    );
    eprintln!(
        "flip fixture stats: {stats:?}; dirty={} topology={}",
        chunk.dirty, chunk.topology_changed
    );
    assert_eq!(stats.edges_split, 0);
    assert_eq!(stats.edges_collapsed, 0);
    assert_ne!(
        before,
        format!("{:?}", chunk.mesh),
        "fixture must perform a flip"
    );
    assert!(
        chunk.topology_changed && chunk.dirty,
        "flip-only tessellation must trigger full GPU rebuild"
    );
}

#[test]
fn an_external_edit_cannot_bypass_the_last_admitted_surface() {
    let mut chunks = partition_mesh(&crossing_fixture(), &PartitionConfig::default());
    let original = snapshot(&chunks);
    let mut pipeline = SculptingPipeline::with_config(
        BrushPreset {
            radius: 0.15,
            strength: 0.2,
            spacing: 0.,
            autosmooth: 0.,
            ..BrushPreset::push()
        },
        PipelineConfig {
            tessellation_enabled: false,
            rebalance_after_stroke: false,
            ..Default::default()
        },
    );
    let input = BrushInput {
        position: Vec3::new(0., 0., 0.06),
        normal: Vec3::Z,
        pressure: 1.,
        timestamp_ms: 0,
    };
    pipeline.begin_stroke(0, input);
    assert!(
        pipeline
            .process_input(
                BrushInput {
                    timestamp_ms: 1,
                    ..input
                },
                &mut chunks
            )
            .rejected
            .is_none()
    );
    let chunk = chunks.chunks.values_mut().next().unwrap();
    let local = chunk.original_to_local[&painting::half_edge::VertexId(0)];
    chunk
        .mesh
        .set_vertex_position(local, Vec3::new(0., 0., -0.2));
    let result = pipeline.process_input(
        BrushInput {
            timestamp_ms: 2,
            ..input
        },
        &mut chunks,
    );
    assert_eq!(result.rejected, Some(SafetyError::Intersection));
    assert_eq!(snapshot(&chunks), original);
    assert!(pipeline.end_stroke(&mut chunks).packets.is_empty());
    // The same cached mesh owner must not blindly admit a newly invalid mesh
    // between strokes either. Initial-admission failure preserves that input.
    let chunk = chunks.chunks.values_mut().next().unwrap();
    let local = chunk.original_to_local[&painting::half_edge::VertexId(0)];
    chunk
        .mesh
        .set_vertex_position(local, Vec3::new(0., 0., -0.2));
    let invalid_input = snapshot(&chunks);
    pipeline.begin_stroke(
        0,
        BrushInput {
            timestamp_ms: 10,
            ..input
        },
    );
    assert_eq!(
        pipeline
            .process_input(
                BrushInput {
                    timestamp_ms: 11,
                    ..input
                },
                &mut chunks
            )
            .rejected,
        Some(SafetyError::Intersection)
    );
    assert_eq!(snapshot(&chunks), invalid_input);
    assert!(pipeline.end_stroke(&mut chunks).packets.is_empty());
}

fn cache_probe_mesh() -> ChunkedMesh {
    let mut m = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    m.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![
            [0., 0., 0.06],
            [0.5, 0., 0.06],
            [0., 0.5, 0.06],
            [-1., -1., 0.],
            [2., -1., 0.],
            [-1., 2., 0.],
        ],
    );
    m.insert_indices(Indices::U32(vec![0, 1, 2, 3, 4, 5]));
    partition_mesh(
        &HalfEdgeMesh::from_bevy_mesh(&m).unwrap(),
        &PartitionConfig {
            target_faces: 1,
            min_faces: 1,
            max_faces: 1,
        },
    )
}
fn cache_probe_pipeline() -> SculptingPipeline {
    SculptingPipeline::with_config(
        BrushPreset {
            radius: 0.15,
            strength: 0.1,
            spacing: 0.,
            autosmooth: 0.,
            ..BrushPreset::push()
        },
        PipelineConfig {
            tessellation_enabled: false,
            rebalance_after_stroke: false,
            ..Default::default()
        },
    )
}
fn cache_probe_input(t: u64) -> BrushInput {
    BrushInput {
        position: Vec3::new(0., 0., 0.06),
        normal: Vec3::Z,
        pressure: 1.,
        timestamp_ms: t,
    }
}
fn cache_probe_corrupt(m: &mut ChunkedMesh) {
    let c = m
        .chunks
        .values_mut()
        .find(|c| {
            c.original_to_local
                .contains_key(&painting::half_edge::VertexId(0))
        })
        .unwrap();
    let v = c.original_to_local[&painting::half_edge::VertexId(0)];
    c.mesh.set_vertex_position(v, Vec3::new(0., 0., -0.2));
}
#[test]
fn external_edit_before_end_must_roll_back() {
    let mut m = cache_probe_mesh();
    let mut p = cache_probe_pipeline();
    p.begin_stroke(0, cache_probe_input(0));
    assert!(
        p.process_input(cache_probe_input(1), &mut m)
            .rejected
            .is_none()
    );
    cache_probe_corrupt(&mut m);
    assert!(validate_sculpt_surface(&m).is_err());
    let end = p.end_stroke(&mut m);
    eprintln!(
        "end rejected={:?} packets={} surface={:?}",
        end.rejected,
        end.packets.len(),
        validate_sculpt_surface(&m)
    );
    assert!(end.rejected.is_some() && end.packets.is_empty());
    validate_sculpt_surface(&m).unwrap();
}
#[test]
fn duplicate_chunk_identity_must_not_poison_cross_stroke_cache() {
    let mut m = cache_probe_mesh();
    let mut p = cache_probe_pipeline();
    p.begin_stroke(0, cache_probe_input(0));
    assert!(
        p.process_input(cache_probe_input(1), &mut m)
            .rejected
            .is_none()
    );
    p.end_stroke(&mut m);
    let mut keys: Vec<_> = m.chunks.keys().copied().collect();
    keys.sort_by_key(|c| c.0);
    let duplicate = m.chunks[&keys[0]].clone();
    m.chunks.insert(keys[1], duplicate);
    assert!(validate_sculpt_surface(&m).is_err());
    let far = BrushInput {
        position: Vec3::splat(100.),
        ..cache_probe_input(10)
    };
    p.begin_stroke(0, far);
    let result = p.process_input(
        BrushInput {
            timestamp_ms: 11,
            ..far
        },
        &mut m,
    );
    eprintln!(
        "duplicate chunk rejected={:?} surface={:?}",
        result.rejected,
        validate_sculpt_surface(&m)
    );
    assert!(result.rejected.is_some());
}
#[test]
fn deleting_opposite_fan_faces_must_recheck_complete_surviving_links() {
    let mut m = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    m.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![
            [0., 0., 0.],
            [1., 0., 0.],
            [0., 1., 0.],
            [-1., 0., 0.],
            [0., -1., 0.],
        ],
    );
    m.insert_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3, 0, 3, 4, 0, 4, 1]));
    let mut chunks = partition_mesh(
        &HalfEdgeMesh::from_bevy_mesh(&m).unwrap(),
        &PartitionConfig {
            target_faces: 1,
            min_faces: 1,
            max_faces: 1,
        },
    );
    let mut p = cache_probe_pipeline();
    let far = BrushInput {
        position: Vec3::splat(100.),
        ..cache_probe_input(0)
    };
    p.begin_stroke(0, far);
    assert!(
        p.process_input(
            BrushInput {
                timestamp_ms: 1,
                ..far
            },
            &mut chunks
        )
        .rejected
        .is_none()
    );
    p.end_stroke(&mut chunks);
    let remove: Vec<_> = chunks
        .chunks
        .iter()
        .filter(|(_, c)| {
            let mut ids: Vec<_> = c.original_to_local.keys().map(|id| id.0).collect();
            ids.sort();
            ids == [0, 2, 3] || ids == [0, 1, 4]
        })
        .map(|(id, _)| *id)
        .collect();
    assert_eq!(remove.len(), 2);
    for id in remove {
        chunks.chunks.remove(&id);
    }
    assert!(matches!(
        validate_sculpt_surface(&chunks),
        Err(SafetyError::Topology(_))
    ));
    p.begin_stroke(0, far);
    let r = p.process_input(
        BrushInput {
            timestamp_ms: 11,
            ..far
        },
        &mut chunks,
    );
    eprintln!("partial surviving fan result: {:?}", r.rejected);
    assert!(matches!(r.rejected, Some(SafetyError::Topology(_))));
}

#[test]
fn an_external_edit_is_rejected_even_when_spacing_emits_no_dab() {
    let mut mesh = cache_probe_mesh();
    let mut pipeline = cache_probe_pipeline();
    pipeline.begin_stroke(0, cache_probe_input(0));
    assert!(
        pipeline
            .process_input(cache_probe_input(1), &mut mesh)
            .rejected
            .is_none()
    );
    pipeline.brush_engine.preset.spacing = 1.;
    // Same-position input demonstrably produces no dab in the valid control.
    let no_dab = pipeline.process_input(cache_probe_input(2), &mut mesh);
    assert!(no_dab.rejected.is_none());
    assert_eq!(no_dab.vertices_modified, 0);
    cache_probe_corrupt(&mut mesh);
    let result = pipeline.process_input(cache_probe_input(3), &mut mesh);
    assert_eq!(result.rejected, Some(SafetyError::Intersection));
    assert_surface(&mesh, "zero-dab rollback");
    assert!(pipeline.end_stroke(&mut mesh).packets.is_empty());
}

#[test]
#[ignore = "manual optimized rejected-stroke cost qualification"]
fn measure_rejected_stroke_cost() {
    for (segments, rings) in [(32, 16), (128, 64), (256, 128)] {
        let sphere = Sphere::new(1.).mesh().uv(segments, rings);
        let mut positions = sphere
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap()
            .as_float3()
            .unwrap()
            .to_vec();
        let offset = positions.len() as u32;
        let mut indices: Vec<u32> = sphere.indices().unwrap().iter().map(|i| i as u32).collect();
        positions.extend([
            [3., 0., 0.06],
            [3.5, 0., 0.06],
            [3., 0.5, 0.06],
            [2., -1., 0.],
            [5., -1., 0.],
            [2., 2., 0.],
        ]);
        indices.extend([
            offset,
            offset + 1,
            offset + 2,
            offset + 3,
            offset + 4,
            offset + 5,
        ]);
        let mut source = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        source.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        source.insert_indices(Indices::U32(indices));
        let imported = HalfEdgeMesh::from_bevy_mesh_welded(&source).unwrap();
        let mut mesh = partition_mesh(&imported, &PartitionConfig::default());
        let original = mesh.clone();
        let mut pipeline = SculptingPipeline::with_config(
            BrushPreset {
                radius: 0.15,
                strength: 0.2,
                spacing: 0.,
                autosmooth: 0.,
                falloff: FalloffCurve::Constant,
                ..BrushPreset::push()
            },
            PipelineConfig {
                tessellation_enabled: false,
                rebalance_after_stroke: false,
                ..Default::default()
            },
        );
        for stroke in 0..3 {
            let input = BrushInput {
                position: Vec3::new(3., 0., 0.06),
                normal: -Vec3::Z,
                pressure: 1.,
                timestamp_ms: stroke * 10,
            };
            pipeline.begin_stroke(0, input);
            let start = std::time::Instant::now();
            for dab in 1..3 {
                assert!(
                    pipeline
                        .process_input(
                            BrushInput {
                                timestamp_ms: input.timestamp_ms + dab,
                                ..input
                            },
                            &mut mesh
                        )
                        .rejected
                        .is_none()
                );
            }
            let accepted_ms = start.elapsed().as_secs_f64() * 1000.;
            let rejected_start = std::time::Instant::now();
            let rejected = pipeline.process_input(
                BrushInput {
                    timestamp_ms: input.timestamp_ms + 3,
                    ..input
                },
                &mut mesh,
            );
            let rejected_ms = rejected_start.elapsed().as_secs_f64() * 1000.;
            assert_eq!(rejected.rejected, Some(SafetyError::Intersection));
            let ignored_start = std::time::Instant::now();
            assert!(
                pipeline
                    .process_input(
                        BrushInput {
                            timestamp_ms: input.timestamp_ms + 4,
                            ..input
                        },
                        &mut mesh
                    )
                    .rejected
                    .is_some()
            );
            let ignored_ms = ignored_start.elapsed().as_secs_f64() * 1000.;
            let end = pipeline.end_stroke(&mut mesh);
            assert!(end.rejected.is_some() && end.packets.is_empty());
            assert_eq!(
                mesh.next_original_vertex_id,
                original.next_original_vertex_id
            );
            for (&id, chunk) in &original.chunks {
                let restored = &mesh.chunks[&id];
                assert_eq!(restored.mesh, chunk.mesh);
                assert_eq!(restored.local_to_original, chunk.local_to_original);
                assert_eq!(restored.original_to_local, chunk.original_to_local);
                assert!(restored.dirty && restored.topology_changed);
            }
            assert_surface(&mesh, "large rejection rollback");
            eprintln!(
                "REJECTION faces={} stroke={stroke} two_accepted_dabs_ms={accepted_ms:.2} rejected_and_rollback_ms={rejected_ms:.2} ignored_after_rejection_ms={ignored_ms:.3} packets={}",
                imported.face_count(),
                end.packets.len()
            );
        }
    }
}
