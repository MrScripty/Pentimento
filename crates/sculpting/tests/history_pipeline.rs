#![cfg(feature = "bevy")]
//! Actual sculpt pipeline + history contract tests. The immutable fixture owns
//! the entire ChunkedMesh, including private counters/indexes. Its Arc identity
//! is its exact snapshot provenance; it is NOT the production capture adapter.
//! Production must use root's safety snapshot and geometry validator.
#[path = "../src/history.rs"]
mod history;

use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use history::*;
use painting::half_edge::{HalfEdgeMesh, VertexId};
use sculpting::{
    BrushInput, BrushPreset, ChunkedMesh, PartitionConfig, PipelineConfig, SculptingPipeline,
    TessellationConfig, TessellationMode, merge_chunks, partition_mesh,
};
use std::sync::Arc;

const FIXTURE_SNAPSHOT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug)]
struct Frozen(Arc<ChunkedMesh>);

impl Frozen {
    fn capture(mesh: ChunkedMesh) -> Self {
        // Only these small, single-origin fixtures use the fixed upper charge.
        // Root's adapter must measure capacities of ALL owned allocations.
        assert!(mesh.chunk_count() < 32);
        assert!(mesh.total_vertex_count() < 4096);
        assert!(mesh.total_face_count() < 4096);
        Self(Arc::new(mesh))
    }
}

impl HistorySnapshot for Frozen {
    fn same_state(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    fn retained_bytes(&self) -> usize {
        FIXTURE_SNAPSHOT_BYTES
    }
}

fn fixture() -> (Frozen, SculptingPipeline) {
    let source = Sphere::new(1.).mesh().uv(16, 8);
    let imported = HalfEdgeMesh::from_bevy_mesh_welded(&source).unwrap();
    let chunks = partition_mesh(
        &imported,
        &PartitionConfig {
            target_faces: 1000,
            min_faces: 1,
            max_faces: 2000,
        },
    );
    let preset = BrushPreset {
        radius: 0.75,
        strength: 0.015,
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
    (Frozen::capture(chunks), pipeline)
}

/// Test-only detached working copy for fixtures with contiguous chunk IDs and
/// no removals/rebalances yet. This never stands in for a restore: restoration
/// swaps the original fully owned opaque snapshot, including private state.
fn trial_copy(snapshot: &Frozen) -> ChunkedMesh {
    let source = &snapshot.0;
    let mut trial = ChunkedMesh::with_config(source.config.clone());
    let mut ids: Vec<_> = source.chunks.keys().copied().collect();
    ids.sort_by_key(|id| id.0);
    for (i, id) in ids.iter().enumerate() {
        assert_eq!(id.0 as usize, i, "fixture must have contiguous chunk IDs");
        assert_eq!(trial.add_chunk(source.chunks[id].clone()), *id);
    }
    trial.next_original_vertex_id = source.next_original_vertex_id;
    trial.rebuild_spatial_grid();
    trial
}

// This checks fixture topology/finite attributes, not root's private overlap
// guard. A separately supplied validator can reject any target before swapping.
fn fixture_valid(snapshot: &Frozen) -> Result<(), &'static str> {
    for chunk in snapshot.0.chunks.values() {
        chunk
            .mesh
            .check_manifold()
            .map_err(|_| "invalid topology")?;
        if chunk
            .mesh
            .vertices()
            .iter()
            .any(|v| !v.position.is_finite() || !v.normal.is_finite())
            || chunk
                .mesh
                .half_edges()
                .iter()
                .any(|e| e.corner_uv.is_some_and(|uv| !uv.is_finite()))
        {
            return Err("non-finite attributes");
        }
    }
    Ok(())
}

fn sculpt(
    pipeline: &mut SculptingPipeline,
    mesh: &mut ChunkedMesh,
    center: Vec3,
) -> sculpting::StrokeEndResult {
    // This fixture's independent Frozen history supplies its own endpoints.
    // A production pipeline now owns a separate history and rightly refuses an
    // externally replaced baseline. Give each independently supplied fixture a
    // fresh native owner, preserving the actual brush/safety/budget configuration.
    // Production same-owner undo/redo/cancel is covered in native_history_tests.
    let mut owner =
        SculptingPipeline::with_config(pipeline.brush_preset().clone(), pipeline.config.clone());
    owner.budget = pipeline.budget.clone();
    *pipeline = owner;
    let input = BrushInput {
        position: center,
        normal: center.normalize_or_zero(),
        pressure: 1.,
        timestamp_ms: 1,
    };
    pipeline.begin_stroke(0, input);
    let result = pipeline.process_input(
        BrushInput {
            timestamp_ms: 2,
            ..input
        },
        mesh,
    );
    assert!(
        result.rejected.is_none(),
        "fixture stroke rejected: {:?}",
        result.rejected
    );
    if center == Vec3::X {
        assert!(result.vertices_modified > 0);
        assert!(result.tessellation.unwrap().edges_split > 0);
    }
    let end = pipeline.end_stroke(mesh);
    assert!(
        end.rejected.is_none(),
        "fixture completion rejected: {:?}",
        end.rejected
    );
    end
}

fn exported(snapshot: &Frozen) -> Mesh {
    merge_chunks(&snapshot.0).mesh.to_bevy_mesh()
}

fn assert_same_render(actual: &Frozen, expected: &Frozen) {
    let actual = exported(actual);
    let expected = exported(expected);
    for attribute in [
        Mesh::ATTRIBUTE_POSITION,
        Mesh::ATTRIBUTE_NORMAL,
        Mesh::ATTRIBUTE_UV_0,
    ] {
        assert_eq!(actual.attribute(attribute), expected.attribute(attribute));
    }
    assert_eq!(
        actual.indices().unwrap().iter().collect::<Vec<_>>(),
        expected.indices().unwrap().iter().collect::<Vec<_>>()
    );
}

#[test]
fn real_strokes_restore_complete_topology_uv_positions_and_global_ids() {
    let (baseline, mut pipeline) = fixture();
    let mut history = SculptHistory::default();
    let mut live = baseline.clone();
    let mut accepted = vec![baseline.clone()];
    for _ in 0..2 {
        let mut trial = trial_copy(&live);
        let end = sculpt(&mut pipeline, &mut trial, Vec3::X);
        assert!(!end.packets.is_empty());
        let after = Frozen::capture(trial);
        fixture_valid(&after).unwrap();
        history
            .record_accepted(live.clone(), after.clone(), fixture_valid)
            .unwrap();
        live = after;
        accepted.push(live.clone());
    }
    assert!(live.0.total_face_count() > baseline.0.total_face_count());
    assert!(live.0.next_original_vertex_id > baseline.0.next_original_vertex_id);
    let VertexAttributeValues::Float32x2(uvs) = exported(&live)
        .attribute(Mesh::ATTRIBUTE_UV_0)
        .unwrap()
        .clone()
    else {
        panic!()
    };
    assert!(uvs.iter().any(|uv| uv[0] == 0.));
    assert!(uvs.iter().any(|uv| uv[0] == 1.));
    for expected in accepted[..2].iter().rev() {
        let current = live.clone();
        assert!(
            history
                .undo_replace(&current, |expected, target| {
                    if !live.same_state(expected) {
                        return Err("native external-edit conflict");
                    }
                    fixture_valid(target)?;
                    live = target.clone();
                    pipeline.invalidate_caches();
                    Ok(())
                })
                .unwrap()
        );
        pipeline.invalidate_caches();
        assert!(live.same_state(expected));
        assert_same_render(&live, expected);
        assert_eq!(
            live.0.next_original_vertex_id,
            expected.0.next_original_vertex_id
        );
        for (id, chunk) in &live.0.chunks {
            assert_eq!(
                chunk.local_to_original,
                expected.0.chunks[id].local_to_original
            );
            assert_eq!(
                chunk.original_to_local,
                expected.0.chunks[id].original_to_local
            );
        }
    }
    for expected in &accepted[1..] {
        let current = live.clone();
        assert!(
            history
                .redo_replace(&current, |expected, target| {
                    if !live.same_state(expected) {
                        return Err("native external-edit conflict");
                    }
                    fixture_valid(target)?;
                    live = target.clone();
                    pipeline.invalidate_caches();
                    Ok(())
                })
                .unwrap()
        );
        pipeline.invalidate_caches();
        assert!(live.same_state(expected));
        assert_same_render(&live, expected);
    }
}

#[test]
fn off_target_stroke_and_rejected_trial_preserve_real_redo() {
    let (baseline, mut pipeline) = fixture();
    let mut history = SculptHistory::default();
    let mut trial = trial_copy(&baseline);
    sculpt(&mut pipeline, &mut trial, Vec3::X);
    let accepted = Frozen::capture(trial);
    history
        .record_accepted(baseline.clone(), accepted.clone(), fixture_valid)
        .unwrap();
    let mut live = accepted.clone();
    history.undo(&mut live, fixture_valid).unwrap();
    let status = history.status();

    let mut miss = trial_copy(&live);
    let _baseline_packets = sculpt(&mut pipeline, &mut miss, Vec3::splat(50.));
    let miss = Frozen::capture(miss);
    assert_same_render(&miss, &live);
    assert_eq!(
        miss.0.next_original_vertex_id,
        live.0.next_original_vertex_id
    );
    // The acceptance adapter detects the no-op, keeps its baseline provenance,
    // and withholds the raw pipeline packets; history never produces packets.
    assert_eq!(
        history
            .record_accepted(live.clone(), live.clone(), fixture_valid)
            .unwrap(),
        RecordOutcome::NoChange
    );
    assert_eq!(history.status(), status);
    // Packet suppression belongs to the final native guard. This b188 fixture
    // does not assert its old packet behavior as the desired integration policy.

    let mut rejected = trial_copy(&live);
    sculpt(&mut pipeline, &mut rejected, Vec3::X);
    let rejected = Frozen::capture(rejected);
    let result = history.record_accepted(live.clone(), rejected.clone(), |s| {
        if s.same_state(&rejected) {
            Err("safety owner rejected trial")
        } else {
            fixture_valid(s)
        }
    });
    assert_eq!(
        result,
        Err(HistoryError::Validation("safety owner rejected trial"))
    );
    assert!(live.same_state(&baseline));
    assert_eq!(history.status(), status);
    history.redo(&mut live, fixture_valid).unwrap();
    assert_same_render(&live, &accepted);
}

#[test]
fn invalid_restore_target_is_rejected_without_geometry_or_cursor_changes() {
    let (baseline, mut pipeline) = fixture();
    let mut trial = trial_copy(&baseline);
    sculpt(&mut pipeline, &mut trial, Vec3::X);
    let accepted = Frozen::capture(trial);
    let mut history = SculptHistory::default();
    history
        .record_accepted(baseline.clone(), accepted.clone(), fixture_valid)
        .unwrap();
    let mut live = accepted.clone();
    let status = history.status();
    assert_eq!(
        history.undo(&mut live, |_| Err("new safety policy rejects baseline")),
        Err(HistoryError::Validation(
            "new safety policy rejects baseline"
        ))
    );
    assert!(live.same_state(&accepted));
    assert_same_render(&live, &accepted);
    assert_eq!(history.status(), status);
    history.undo(&mut live, fixture_valid).unwrap();
    let status = history.status();
    assert_eq!(
        history.redo(&mut live, |_| Err("new safety policy rejects redo")),
        Err(HistoryError::Validation("new safety policy rejects redo"))
    );
    assert!(live.same_state(&baseline));
    assert_eq!(history.status(), status);
}

#[test]
fn continued_stroke_after_undo_clears_redo_and_preserves_identity_allocation() {
    let (baseline, mut pipeline) = fixture();
    let mut trial = trial_copy(&baseline);
    sculpt(&mut pipeline, &mut trial, Vec3::X);
    let accepted = Frozen::capture(trial);
    let mut history = SculptHistory::default();
    history
        .record_accepted(baseline.clone(), accepted.clone(), fixture_valid)
        .unwrap();
    let mut live = accepted;
    history.undo(&mut live, fixture_valid).unwrap();
    pipeline.invalidate_caches();
    let mut branch = trial_copy(&live);
    let next_id = branch.next_original_vertex_id;
    assert_eq!(branch.allocate_original_vertex_id(), VertexId(next_id));
    sculpt(&mut pipeline, &mut branch, Vec3::NEG_X);
    let branch = Frozen::capture(branch);
    history
        .record_accepted(live.clone(), branch.clone(), fixture_valid)
        .unwrap();
    live = branch.clone();
    assert_eq!(history.status().redo_strokes, 0);
    assert!(!history.redo(&mut live, fixture_valid).unwrap());
    history.undo(&mut live, fixture_valid).unwrap();
    assert!(live.same_state(&baseline));
    assert_eq!(live.0.next_original_vertex_id, next_id);
    history.redo(&mut live, fixture_valid).unwrap();
    assert_same_render(&live, &branch);
}

#[test]
fn external_geometry_edit_is_not_overwritten_by_history() {
    let (baseline, mut pipeline) = fixture();
    let mut trial = trial_copy(&baseline);
    sculpt(&mut pipeline, &mut trial, Vec3::X);
    let accepted = Frozen::capture(trial);
    let mut history = SculptHistory::default();
    history
        .record_accepted(baseline, accepted.clone(), fixture_valid)
        .unwrap();
    let mut external = trial_copy(&accepted);
    let chunk = external.chunks.values_mut().next().unwrap();
    chunk.mesh.vertex_mut(VertexId(0)).unwrap().position += Vec3::splat(0.1);
    let mut live = Frozen::capture(external);
    let external = live.clone();
    let status = history.status();
    assert_eq!(
        history.undo(&mut live, fixture_valid),
        Err(HistoryError::Conflict)
    );
    assert!(live.same_state(&external));
    assert_same_render(&live, &external);
    assert_eq!(history.status(), status);
    // Also no allocations are performed by clear; it releases snapshot owners.
    history.clear();
    assert_eq!(history.status().retained_snapshot_bytes, 0);
}

#[test]
fn post_stroke_rebalance_and_private_chunk_counter_restore_together() {
    let (baseline, mut pipeline) = fixture();
    let baseline_next_vertex = baseline.0.next_original_vertex_id;
    assert_eq!(baseline.0.chunk_count(), 1);
    pipeline.config.rebalance_after_stroke = true;
    pipeline.config.chunk_config.max_faces = 48;
    pipeline.config.chunk_config.min_faces = 1;
    pipeline.config.chunk_config.target_faces = 32;
    let mut trial = trial_copy(&baseline);
    // The current pipeline and chunk splitter each consult their own sizing
    // config. Keep them aligned; mismatched limits can re-add chunks forever.
    trial.config = pipeline.config.chunk_config.clone();
    let end = sculpt(&mut pipeline, &mut trial, Vec3::X);
    assert!(end.chunks_split > 0);
    let accepted = Frozen::capture(trial);
    assert!(accepted.0.chunk_count() > 1);
    fixture_valid(&accepted).unwrap();
    let mut history = SculptHistory::default();
    history
        .record_accepted(baseline.clone(), accepted.clone(), fixture_valid)
        .unwrap();
    let mut live = accepted.clone();
    history.undo(&mut live, fixture_valid).unwrap();
    assert_same_render(&live, &baseline);
    assert_eq!(live.0.chunk_count(), 1);
    history.redo(&mut live, fixture_valid).unwrap();
    assert_same_render(&live, &accepted);
    for (id, chunk) in &live.0.chunks {
        assert_eq!(
            chunk.boundary_vertices.len(),
            accepted.0.chunks[id].boundary_vertices.len()
        );
        for (vertex, refs) in &chunk.boundary_vertices {
            let expected = &accepted.0.chunks[id].boundary_vertices[vertex];
            assert_eq!(refs.len(), expected.len());
            for (actual, expected) in refs.iter().zip(expected) {
                assert_eq!(actual.chunk_id, expected.chunk_id);
                assert_eq!(actual.vertex_id, expected.vertex_id);
                assert_eq!(actual.original_vertex_id, expected.original_vertex_id);
            }
        }
    }
    history.undo(&mut live, fixture_valid).unwrap();
    history.clear();
    drop(baseline);
    drop(accepted);
    // Recover the whole restored owning mesh after releasing history owners.
    // This checks its PRIVATE allocation counter without adding a new API.
    let mut restored = Arc::try_unwrap(live.0).unwrap();
    let template = restored.chunks.values().next().unwrap().clone();
    assert_eq!(restored.add_chunk(template), sculpting::ChunkId(1));
    assert_eq!(
        restored.allocate_original_vertex_id(),
        VertexId(baseline_next_vertex)
    );
}

#[test]
fn malformed_actual_pipeline_endpoint_is_never_admitted() {
    let (baseline, mut pipeline) = fixture();
    let mut trial = trial_copy(&baseline);
    sculpt(&mut pipeline, &mut trial, Vec3::X);
    let chunk = trial.chunks.values_mut().next().unwrap();
    chunk.mesh.vertex_mut(VertexId(0)).unwrap().position.x = f32::NAN;
    let invalid = Frozen::capture(trial);
    let mut history = SculptHistory::default();
    assert_eq!(
        history.record_accepted(baseline.clone(), invalid, fixture_valid),
        Err(HistoryError::Validation("non-finite attributes"))
    );
    assert_eq!(history.status().undo_strokes, 0);
    assert_eq!(history.status().retained_snapshot_bytes, 0);
    let mut live = baseline.clone();
    assert!(!history.undo(&mut live, fixture_valid).unwrap());
    assert_same_render(&live, &baseline);
}
