//! CPU-only real sculpt pipeline benchmark, including admission and history.
//! Run `cargo run -p sculpting --features bevy --release --example core_profile
//! -- 96 5` (sphere segments, independent trials). No GPU/display is used.
#[cfg(not(feature = "bevy"))]
fn main() {
    eprintln!("Run with --features bevy");
}
#[cfg(feature = "bevy")]
fn main() {
    use bevy::prelude::*;
    use painting::half_edge::HalfEdgeMesh;
    use sculpting::{
        BrushInput, BrushPreset, DeformationType, PartitionConfig, PipelineConfig,
        SculptingPipeline, partition_mesh,
    };
    use std::{hint::black_box, time::Instant};
    let segments = std::env::args()
        .nth(1)
        .map(|s| s.parse::<u32>().unwrap())
        .unwrap_or(96)
        .max(8);
    let trials = std::env::args()
        .nth(2)
        .map(|s| s.parse::<usize>().unwrap())
        .unwrap_or(5)
        .max(1);
    let source = Sphere::new(1.).mesh().uv(segments, segments / 2);
    let imported = HalfEdgeMesh::from_bevy_mesh_welded(&source).unwrap();
    let fixture = partition_mesh(
        &imported,
        &PartitionConfig {
            target_faces: 2000,
            min_faces: 500,
            max_faces: 3000,
        },
    );
    println!(
        "# faces={} vertices={} chunks={}",
        fixture.total_face_count(),
        fixture.total_vertex_count(),
        fixture.chunk_count()
    );
    println!("workload,operations,median_ms,p95_ms,changed_vertices,position_checksum");
    for tool in [DeformationType::Push, DeformationType::Smooth] {
        let mut first = Vec::new();
        let mut warm = Vec::new();
        let mut finish = Vec::new();
        let mut noop = Vec::new();
        let mut batched = Vec::new();
        let mut changed = 0;
        let mut endpoint_hash = 0;
        for _ in 0..trials {
            let mut mesh = fixture.clone();
            let preset = BrushPreset {
                radius: 0.2,
                strength: 0.015,
                spacing: 0.,
                autosmooth: 0.,
                deformation_type: tool,
                ..BrushPreset::push()
            };
            let mut pipeline = SculptingPipeline::with_config(
                preset,
                PipelineConfig {
                    tessellation_enabled: false,
                    rebalance_after_stroke: false,
                    ..Default::default()
                },
            );
            let input = BrushInput {
                position: Vec3::X,
                normal: Vec3::X,
                pressure: 1.,
                timestamp_ms: 1,
            };
            pipeline.begin_stroke(0, input);
            let start = Instant::now();
            let result = pipeline.process_input(input, &mut mesh);
            first.push(start.elapsed().as_secs_f64() * 1000.);
            assert!(result.rejected.is_none(), "{:?}", result.rejected);
            changed = result.vertices_modified;
            let start = Instant::now();
            for i in 0..8 {
                let result = black_box(pipeline.process_input(
                    BrushInput {
                        timestamp_ms: i + 2,
                        ..input
                    },
                    &mut mesh,
                ));
                assert!(result.rejected.is_none(), "{:?}", result.rejected);
            }
            warm.push(start.elapsed().as_secs_f64() * 1000.);
            pipeline.brush_engine.preset.spacing = 0.25;
            let start = Instant::now();
            for i in 0..32 {
                let result = black_box(pipeline.process_input(
                    BrushInput {
                        timestamp_ms: i + 20,
                        ..input
                    },
                    &mut mesh,
                ));
                assert!(result.rejected.is_none(), "{:?}", result.rejected);
                assert_eq!(result.vertices_modified, 0);
            }
            noop.push(start.elapsed().as_secs_f64() * 1000.);
            let start = Instant::now();
            let end = pipeline.end_stroke(&mut mesh);
            finish.push(start.elapsed().as_secs_f64() * 1000.);
            assert!(end.rejected.is_none(), "{:?}", end.rejected);
            assert!(!end.packets.is_empty());
            let mut positions = std::collections::BTreeMap::new();
            for chunk in mesh.chunks.values() {
                for (&global, &local) in &chunk.original_to_local {
                    positions.insert(
                        global.0,
                        chunk.mesh.vertex(local).unwrap().position.to_array(),
                    );
                }
            }
            endpoint_hash = positions
                .iter()
                .fold(0xcbf29ce484222325u64, |hash, (id, p)| {
                    p.iter().fold(
                        (hash ^ u64::from(*id)).wrapping_mul(0x100000001b3),
                        |hash, v| (hash ^ u64::from(v.to_bits())).wrapping_mul(0x100000001b3),
                    )
                });
            assert!(pipeline.restore_history(&mut mesh, false).unwrap());
            assert!(mesh.same_authoritative_state(&fixture));
            assert!(pipeline.restore_history(&mut mesh, true).unwrap());
            let committed = mesh.clone();
            pipeline.brush_engine.preset.spacing = 0.05;
            pipeline.begin_stroke(0, input);
            assert!(pipeline.process_input(input, &mut mesh).rejected.is_none());
            let start = Instant::now();
            let batch = pipeline.process_input(
                BrushInput {
                    position: Vec3::new(1., 0.08, 0.),
                    timestamp_ms: 100,
                    ..input
                },
                &mut mesh,
            );
            batched.push(start.elapsed().as_secs_f64() * 1000.);
            assert!(batch.rejected.is_none(), "{:?}", batch.rejected);
            assert!(batch.vertices_modified > 0);
            pipeline.cancel_stroke(&mut mesh);
            assert!(mesh.same_authoritative_state(&committed));
        }
        for (name, mut times, count) in [
            ("first_dab", first, 1),
            ("warm_dabs", warm, 8),
            ("stationary_no_dab", noop, 32),
            ("finish_history", finish, 1),
            ("spaced_batch", batched, 8),
        ] {
            times.sort_by(f64::total_cmp);
            println!(
                "{tool:?}_{name},{count},{:.3},{:.3},{changed},{endpoint_hash:016x}",
                times[times.len() / 2],
                times[(times.len() * 95 / 100).min(times.len() - 1)]
            );
        }
    }
}
