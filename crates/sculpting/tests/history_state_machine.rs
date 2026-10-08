//! Dependency-free history contract tests, also runnable with rustc --test.
#[path = "../src/history.rs"]
mod history;

use history::*;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Snapshot {
    target: u64,
    external_revision: u64,
    topology: Vec<u32>,
    positions: Vec<[u32; 3]>,
    uv_corners: Vec<[u32; 2]>,
    global_ids: Vec<u32>,
    next_id: u32,
    safety_epoch: u64,
    charge: usize,
    derived_cache: u64,
    chunk_ids_by_map_key: HashMap<u32, u32>,
    local_to_global: HashMap<u32, u32>,
    global_to_local: HashMap<u32, u32>,
    boundary_refs: Vec<(u32, u32, u32)>,
    next_chunk_id: u32,
    half_edge_lookup: HashMap<(u32, u32), u32>,
    outgoing_edges: Vec<Option<u32>>,
    source_indices: Vec<u32>,
    tombstones: Vec<bool>,
}

impl HistorySnapshot for Snapshot {
    fn same_state(&self, other: &Self) -> bool {
        let mut a = self.clone();
        let mut b = other.clone();
        a.charge = 0;
        b.charge = 0;
        a.derived_cache = 0;
        b.derived_cache = 0;
        a == b
    }
    fn retained_bytes(&self) -> usize {
        self.charge
    }
}

fn state(n: u32) -> Snapshot {
    Snapshot {
        target: 17,
        external_revision: 0,
        topology: (0..3 + n).collect(),
        positions: vec![[n, 1, 2]; 3 + n as usize],
        uv_corners: vec![[n, n + 1]; 3 + n as usize],
        global_ids: (100..103 + n).collect(),
        next_id: 103 + n,
        safety_epoch: 1,
        charge: 100,
        derived_cache: 0,
        chunk_ids_by_map_key: HashMap::from([(7, 7), (9, 9)]),
        local_to_global: HashMap::from([(0, 100), (1, 101)]),
        global_to_local: HashMap::from([(100, 0), (101, 1)]),
        boundary_refs: vec![(9, 1, 101)],
        next_chunk_id: 10,
        half_edge_lookup: HashMap::from([((0, 1), 0), ((1, 2), 1)]),
        outgoing_edges: vec![Some(0), Some(1), Some(2)],
        source_indices: vec![0, 1, 2],
        tombstones: vec![false, true],
    }
}

fn valid(_: &Snapshot) -> Result<(), &'static str> {
    Ok(())
}

fn record(history: &mut SculptHistory<Snapshot>, a: u32, b: u32) -> RecordOutcome {
    history.record_accepted(state(a), state(b), valid).unwrap()
}

#[test]
fn restores_every_authoritative_field_in_one_step() {
    let mut history = SculptHistory::default();
    record(&mut history, 0, 1);
    record(&mut history, 1, 2);
    let mut live = state(2);
    assert!(history.undo(&mut live, valid).unwrap());
    assert_eq!(live, state(1));
    assert!(history.undo(&mut live, valid).unwrap());
    assert_eq!(live, state(0));
    assert!(!history.undo(&mut live, valid).unwrap());
    assert!(history.redo(&mut live, valid).unwrap());
    assert_eq!(live, state(1));
    assert!(history.redo(&mut live, valid).unwrap());
    assert_eq!(live, state(2));
    assert!(!history.redo(&mut live, valid).unwrap());
}

#[test]
fn no_op_preserves_redo_and_retained_bytes() {
    let mut history = SculptHistory::default();
    record(&mut history, 0, 1);
    let mut live = state(1);
    history.undo(&mut live, valid).unwrap();
    let status = history.status();
    assert_eq!(record(&mut history, 0, 0), RecordOutcome::NoChange);
    assert_eq!(history.status(), status);
    history.redo(&mut live, valid).unwrap();
    assert_eq!(live, state(1));
}

#[test]
fn rejected_endpoints_preserve_history_and_redo() {
    let mut history = SculptHistory::default();
    record(&mut history, 0, 1);
    let mut live = state(1);
    history.undo(&mut live, valid).unwrap();
    let status = history.status();
    let result = history.record_accepted(state(0), state(2), |s| {
        if s == &state(2) {
            Err("unsafe final geometry")
        } else {
            Ok(())
        }
    });
    assert_eq!(
        result,
        Err(HistoryError::Validation("unsafe final geometry"))
    );
    assert_eq!(history.status(), status);
    assert_eq!(live, state(0));
    history.redo(&mut live, valid).unwrap();
    assert_eq!(live, state(1));
}

#[test]
fn rejected_baseline_is_not_admitted_even_into_empty_history() {
    let mut history = SculptHistory::default();
    let result = history.record_accepted(state(0), state(1), |_| Err("unsafe baseline"));
    assert_eq!(result, Err(HistoryError::Validation("unsafe baseline")));
    assert_eq!(history.status().undo_strokes, 0);
    assert_eq!(history.status().retained_snapshot_bytes, 0);
}

#[test]
fn new_accepted_branch_drops_redo() {
    let mut history = SculptHistory::default();
    record(&mut history, 0, 1);
    record(&mut history, 1, 2);
    let mut live = state(2);
    history.undo(&mut live, valid).unwrap();
    record(&mut history, 1, 3);
    live = state(3);
    assert_eq!(history.status().redo_strokes, 0);
    assert!(!history.redo(&mut live, valid).unwrap());
    history.undo(&mut live, valid).unwrap();
    assert_eq!(live, state(1));
    history.undo(&mut live, valid).unwrap();
    assert_eq!(live, state(0));
}

#[test]
fn validation_failure_is_atomic_in_both_directions() {
    let mut history = SculptHistory::default();
    record(&mut history, 0, 1);
    let mut live = state(1);
    let status = history.status();
    assert_eq!(
        history.undo(&mut live, |_| Err("policy changed")),
        Err(HistoryError::Validation("policy changed"))
    );
    assert_eq!(live, state(1));
    assert_eq!(history.status(), status);
    history.undo(&mut live, valid).unwrap();
    let status = history.status();
    assert_eq!(
        history.redo(&mut live, |_| Err("policy changed")),
        Err(HistoryError::Validation("policy changed"))
    );
    assert_eq!(live, state(0));
    assert_eq!(history.status(), status);
}

#[test]
fn external_edit_conflicts_preserve_live_state_and_history() {
    let mut history = SculptHistory::default();
    record(&mut history, 0, 1);
    let status = history.status();
    for field in 0..18 {
        let mut external = state(1);
        match field {
            0 => external.target += 1,
            1 => external.external_revision += 1,
            2 => external.positions[0][0] += 1,
            3 => external.uv_corners[0][0] += 1,
            4 => external.next_id += 1,
            5 => external.safety_epoch += 1,
            6 => external.topology[0] += 1,
            7 => external.global_ids[0] += 1,
            8 => {
                let id = external.chunk_ids_by_map_key.remove(&7).unwrap();
                external.chunk_ids_by_map_key.insert(8, id);
            }
            9 => *external.chunk_ids_by_map_key.get_mut(&7).unwrap() += 1,
            10 => *external.local_to_global.get_mut(&0).unwrap() += 1,
            11 => *external.global_to_local.get_mut(&100).unwrap() += 1,
            12 => external.boundary_refs[0].2 += 1,
            13 => external.next_chunk_id += 1,
            14 => *external.half_edge_lookup.get_mut(&(0, 1)).unwrap() += 1,
            15 => external.outgoing_edges[0] = None,
            16 => external.source_indices[0] += 1,
            _ => external.tombstones[0] = true,
        }
        let original = external.clone();
        assert_eq!(
            history.undo(&mut external, valid),
            Err(HistoryError::Conflict)
        );
        assert_eq!(external, original);
        assert_eq!(history.status(), status);
        assert_eq!(
            history.record_accepted(external, state(2), valid),
            Err(HistoryError::Conflict)
        );
        assert_eq!(history.status(), status);
    }
    let mut live = state(1);
    history.undo(&mut live, valid).unwrap();
    live.external_revision += 1;
    let status = history.status();
    assert_eq!(history.redo(&mut live, valid), Err(HistoryError::Conflict));
    assert_eq!(live.external_revision, 1);
    assert_eq!(history.status(), status);
}

#[test]
fn explicit_conflict_reset_allows_a_new_baseline() {
    let mut history = SculptHistory::default();
    record(&mut history, 0, 1);
    history.clear();
    record(&mut history, 5, 6);
    let mut live = state(6);
    history.undo(&mut live, valid).unwrap();
    assert_eq!(live, state(5));
    assert_eq!(history.status().retained_snapshot_bytes, 200);
}

#[test]
fn count_limit_evicts_only_oldest_complete_transitions() {
    let mut history = SculptHistory::new(HistoryLimits {
        max_strokes: 2,
        max_snapshot_bytes: 1000,
    });
    record(&mut history, 0, 1);
    record(&mut history, 1, 2);
    assert_eq!(
        record(&mut history, 2, 3),
        RecordOutcome::Recorded { evicted_strokes: 1 }
    );
    let mut live = state(3);
    history.undo(&mut live, valid).unwrap();
    history.undo(&mut live, valid).unwrap();
    assert_eq!(live, state(1));
    assert!(!history.undo(&mut live, valid).unwrap());
}

#[test]
fn byte_limit_is_shared_between_undo_and_redo() {
    let mut history = SculptHistory::new(HistoryLimits {
        max_strokes: 100,
        max_snapshot_bytes: 400,
    });
    for n in 0..100 {
        record(&mut history, n, n + 1);
        assert!(history.status().retained_snapshot_bytes <= 400);
        assert!(history.status().undo_strokes <= 2);
    }
    let mut live = state(100);
    let bytes = history.status().retained_snapshot_bytes;
    history.undo(&mut live, valid).unwrap();
    assert_eq!(history.status().retained_snapshot_bytes, bytes);
    assert_eq!(
        history.status().undo_strokes + history.status().redo_strokes,
        2
    );
}

#[test]
fn oversized_accepted_stroke_is_an_explicit_history_barrier() {
    let mut history = SculptHistory::new(HistoryLimits {
        max_strokes: 10,
        max_snapshot_bytes: 400,
    });
    record(&mut history, 0, 1);
    let mut live = state(1);
    history.undo(&mut live, valid).unwrap();
    let mut oversized = state(2);
    oversized.charge = 500;
    assert_eq!(
        history
            .record_accepted(state(0), oversized.clone(), valid)
            .unwrap(),
        RecordOutcome::AcceptedWithoutUndo {
            required_snapshot_bytes: Some(600)
        }
    );
    live = oversized;
    assert_eq!(history.status().retained_snapshot_bytes, 0);
    assert!(!history.undo(&mut live, valid).unwrap());
    assert!(!history.redo(&mut live, valid).unwrap());
    assert_eq!(live, {
        let mut s = state(2);
        s.charge = 500;
        s
    });
    // A later affordable transition establishes a new bounded baseline.
    history.clear();
    record(&mut history, 2, 3);
    live = state(3);
    history.undo(&mut live, valid).unwrap();
    assert_eq!(live, state(2));
}

#[test]
fn size_overflow_and_disabled_history_are_explicit_barriers() {
    for limits in [
        HistoryLimits {
            max_strokes: 10,
            max_snapshot_bytes: usize::MAX,
        },
        HistoryLimits {
            max_strokes: 0,
            max_snapshot_bytes: usize::MAX,
        },
        HistoryLimits {
            max_strokes: 10,
            max_snapshot_bytes: 0,
        },
    ] {
        let mut history = SculptHistory::new(limits);
        let mut huge = state(1);
        huge.charge = usize::MAX;
        assert_eq!(
            history.record_accepted(state(0), huge, valid).unwrap(),
            RecordOutcome::AcceptedWithoutUndo {
                required_snapshot_bytes: None
            }
        );
        assert_eq!(history.status().undo_strokes, 0);
        assert_eq!(history.status().redo_strokes, 0);
        assert_eq!(history.status().retained_snapshot_bytes, 0);
    }
}

#[test]
fn branching_at_the_oldest_retained_baseline_keeps_correct_target() {
    let mut history = SculptHistory::new(HistoryLimits {
        max_strokes: 1,
        max_snapshot_bytes: 200,
    });
    record(&mut history, 0, 1);
    record(&mut history, 1, 2);
    let mut live = state(2);
    history.undo(&mut live, valid).unwrap();
    record(&mut history, 1, 9);
    live = state(9);
    history.undo(&mut live, valid).unwrap();
    assert_eq!(live, state(1));
    assert_eq!(history.status().redo_strokes, 1);
}

#[test]
fn empty_history_never_validates_or_mutates_live_state() {
    let mut history = SculptHistory::default();
    let mut live = state(0);
    let unexpected = |_: &Snapshot| -> Result<(), ()> { panic!("no target to validate") };
    assert!(!history.undo(&mut live, unexpected).unwrap());
    assert!(!history.redo(&mut live, unexpected).unwrap());
    assert_eq!(live, state(0));
}

#[test]
fn safety_preparation_rebuilds_derived_state_before_atomic_swap() {
    let mut history = SculptHistory::default();
    record(&mut history, 0, 1);
    let mut live = state(1);
    history
        .undo_prepared(&mut live, |target| -> Result<Snapshot, &'static str> {
            let mut prepared = target.clone();
            valid(&prepared)?;
            prepared.derived_cache = 27;
            Ok(prepared)
        })
        .unwrap();
    assert!(live.same_state(&state(0)));
    assert_eq!(live.derived_cache, 27);
    // The rebuilt cache is disposable, so it must not trigger a false conflict.
    history
        .redo_prepared(&mut live, |target| -> Result<Snapshot, &'static str> {
            let mut prepared = target.clone();
            valid(&prepared)?;
            prepared.derived_cache = 28;
            Ok(prepared)
        })
        .unwrap();
    assert!(live.same_state(&state(1)));
    assert_eq!(live.derived_cache, 28);
}

#[test]
fn safety_preparation_cannot_silently_modify_restored_geometry() {
    let mut history = SculptHistory::default();
    record(&mut history, 0, 1);
    let mut live = state(1);
    let status = history.status();
    let result: Result<bool, HistoryError<()>> = history.undo_prepared(&mut live, |target| {
        let mut prepared = target.clone();
        prepared.positions[0][0] += 1;
        Ok(prepared)
    });
    assert_eq!(result, Err(HistoryError::PreparedStateChanged));
    assert_eq!(live, state(1));
    assert_eq!(history.status(), status);
    history.undo(&mut live, valid).unwrap();
    let status = history.status();
    let result: Result<bool, HistoryError<()>> = history.redo_prepared(&mut live, |target| {
        let mut prepared = target.clone();
        prepared.global_ids[0] += 1;
        Ok(prepared)
    });
    assert_eq!(result, Err(HistoryError::PreparedStateChanged));
    assert_eq!(live, state(0));
    assert_eq!(history.status(), status);
}

#[test]
fn chunk_key_and_identity_metadata_conflicts_block_redo_and_begin() {
    let mut history = SculptHistory::default();
    record(&mut history, 0, 1);
    let mut live = state(1);
    history.undo(&mut live, valid).unwrap();
    let status = history.status();
    for field in 0..5 {
        let mut edited = live.clone();
        match field {
            0 => {
                let id = edited.chunk_ids_by_map_key.remove(&7).unwrap();
                edited.chunk_ids_by_map_key.insert(8, id);
            }
            1 => *edited.chunk_ids_by_map_key.get_mut(&7).unwrap() = 8,
            2 => *edited.local_to_global.get_mut(&0).unwrap() = 999,
            3 => edited.boundary_refs[0].0 = 8,
            _ => edited.next_chunk_id += 1,
        }
        assert_eq!(
            history.check_current::<()>(&edited),
            Err(HistoryError::Conflict)
        );
        let before = edited.clone();
        assert_eq!(
            history.redo(&mut edited, valid),
            Err(HistoryError::Conflict)
        );
        assert_eq!(edited, before);
        assert_eq!(history.status(), status);
    }
}

#[test]
fn map_insertion_order_and_disposable_capacity_do_not_create_a_stroke() {
    let mut history = SculptHistory::default();
    record(&mut history, 0, 1);
    let mut live = state(1);
    history.undo(&mut live, valid).unwrap();
    let status = history.status();
    let mut identical = state(0);
    identical.chunk_ids_by_map_key = HashMap::new();
    identical.chunk_ids_by_map_key.insert(9, 9);
    identical.chunk_ids_by_map_key.insert(7, 7);
    identical.local_to_global.reserve(512);
    identical.derived_cache = 99;
    identical.charge = usize::MAX;
    assert_eq!(history.check_current::<()>(&identical), Ok(()));
    assert_eq!(
        history
            .record_accepted(live.clone(), identical, valid)
            .unwrap(),
        RecordOutcome::NoChange
    );
    assert_eq!(history.status(), status);
    assert!(history.redo(&mut live, valid).unwrap());
}

fn replace_native(
    live: &mut Snapshot,
    expected: &Snapshot,
    target: &Snapshot,
    reject_target: bool,
) -> Result<(), &'static str> {
    if !live.same_state(expected) {
        return Err("native external-edit conflict");
    }
    if reject_target {
        return Err("native policy rejects target");
    }
    valid(target)?;
    let mut prepared = target.clone();
    prepared.derived_cache = 77;
    // Native adapter's single publication includes geometry and derived safety
    // state. No fallible work follows it; callback success permits cursor move.
    *live = prepared;
    Ok(())
}

#[test]
fn native_validated_replacement_is_atomic_in_both_directions() {
    let mut history = SculptHistory::default();
    record(&mut history, 0, 1);
    let mut live = state(1);
    let current = live.clone();
    let status = history.status();
    assert_eq!(
        history.undo_replace(&current, |expected, target| replace_native(
            &mut live, expected, target, true
        )),
        Err(HistoryError::Validation("native policy rejects target"))
    );
    assert_eq!(live, current);
    assert_eq!(history.status(), status);
    assert!(
        history
            .undo_replace(&current, |expected, target| replace_native(
                &mut live, expected, target, false
            ))
            .unwrap()
    );
    assert!(live.same_state(&state(0)));
    assert_eq!(live.derived_cache, 77);
    let current = live.clone();
    let status = history.status();
    assert_eq!(
        history.redo_replace(&current, |expected, target| replace_native(
            &mut live, expected, target, true
        )),
        Err(HistoryError::Validation("native policy rejects target"))
    );
    assert_eq!(live, current);
    assert_eq!(history.status(), status);
    assert!(
        history
            .redo_replace(&current, |expected, target| replace_native(
                &mut live, expected, target, false
            ))
            .unwrap()
    );
    assert!(live.same_state(&state(1)));
    assert_eq!(live.derived_cache, 77);
}

#[test]
fn stale_capture_is_rechecked_by_native_replacement_before_publication() {
    let mut history = SculptHistory::default();
    record(&mut history, 0, 1);
    let mut live = state(1);
    for redo in [false, true] {
        if redo {
            live = state(1);
            history.undo(&mut live, valid).unwrap();
        }
        let stale_capture = live.clone();
        let status = history.status();
        // Edit after history's capture but before guard publication.
        *live.chunk_ids_by_map_key.get_mut(&7).unwrap() += 1;
        let external = live.clone();
        let result = if redo {
            history.redo_replace(&stale_capture, |expected, target| {
                replace_native(&mut live, expected, target, false)
            })
        } else {
            history.undo_replace(&stale_capture, |expected, target| {
                replace_native(&mut live, expected, target, false)
            })
        };
        assert_eq!(
            result,
            Err(HistoryError::Validation("native external-edit conflict"))
        );
        assert_eq!(live, external);
        assert_eq!(history.status(), status);
    }
}

#[test]
fn native_replacement_is_never_called_for_empty_or_conflicting_history() {
    let mut history = SculptHistory::default();
    let unexpected =
        |_: &Snapshot, _: &Snapshot| -> Result<(), ()> { panic!("no replacement allowed") };
    assert!(!history.undo_replace(&state(0), unexpected).unwrap());
    assert!(!history.redo_replace(&state(0), unexpected).unwrap());
    record(&mut history, 0, 1);
    assert_eq!(
        history.undo_replace(&state(9), unexpected),
        Err(HistoryError::Conflict)
    );
    let mut live = state(1);
    history.undo(&mut live, valid).unwrap();
    assert_eq!(
        history.redo_replace(&state(9), unexpected),
        Err(HistoryError::Conflict)
    );
}
