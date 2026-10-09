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
