//! Local, bounded history of complete accepted sculpt states.
//!
//! This module deliberately knows neither geometry tolerances nor dab replay.
//! The safety owner supplies immutable snapshots, exact state comparison, and
//! validation against its current policy. Never admit a trial/rejected stroke.
//! Restore validates a detached snapshot and swaps the whole state only on
//! success; callers then invalidate pipeline/render caches before rendering.
//!
//! The byte limit bounds *accounted retained snapshot storage*, not process RSS.
//! Snapshot implementations must conservatively include their owned allocation
//! capacities. Shared storage may be double-counted. Collection metadata is
//! additionally bounded by `max_strokes`. Live geometry, capture/validation
//! scratch, and one temporary restore clone are outside the retained byte limit.

use std::collections::VecDeque;

/// A complete immutable editor state, including topology, UV corners, global
/// identities/counters and the safety state required for an atomic restore.
/// Clones must preserve authoritative state exactly. Neither a clone nor an
/// admission/preparation callback may mutate authoritative data shared with
/// retained snapshots or live state through interior mutability.
pub trait HistorySnapshot: Clone {
    /// Exact authoritative-state comparison, excluding disposable GPU flags and
    /// caches. Include target/session identity and external-edit revision. Do
    /// not use approximate position equality or an unchecked hash alone.
    fn same_state(&self, other: &Self) -> bool;

    /// Conservative retained size, including inline state and owned capacities.
    /// Shared allocations may be counted in full at each endpoint.
    fn retained_bytes(&self) -> usize;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryLimits {
    pub max_strokes: usize,
    pub max_snapshot_bytes: usize,
}

impl Default for HistoryLimits {
    fn default() -> Self {
        Self {
            max_strokes: 64,
            max_snapshot_bytes: 128 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryStatus {
    pub undo_strokes: usize,
    pub redo_strokes: usize,
    pub retained_snapshot_bytes: usize,
    pub limits: HistoryLimits,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordOutcome {
    /// No authoritative change. Preserve the redo branch.
    NoChange,
    Recorded {
        evicted_strokes: usize,
    },
    /// The accepted stroke remains live, but cannot be undone. Clear both
    /// stacks: older entries cannot safely jump across this missing transition.
    AcceptedWithoutUndo {
        required_snapshot_bytes: Option<usize>,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub enum HistoryError<E> {
    /// The live state or new stroke baseline no longer matches history.
    /// Do not overwrite the external edit or silently discard the conflict.
    Conflict,
    Validation(E),
    /// Restore preparation may rebuild disposable state, but must never change
    /// the authoritative target geometry/identities/revision it is restoring.
    PreparedStateChanged,
}

#[derive(Debug)]
struct Entry<S> {
    before: S,
    after: S,
    bytes: usize,
}

/// Endpoint history, with no replay packets and no knowledge of active strokes.
/// The editor must finish/cancel its active transaction before undo/redo.
#[derive(Debug)]
pub struct SculptHistory<S> {
    entries: VecDeque<Entry<S>>,
    cursor: usize,
    bytes: usize,
    limits: HistoryLimits,
}

impl<S: HistorySnapshot> SculptHistory<S> {
    pub fn new(limits: HistoryLimits) -> Self {
        Self {
            entries: VecDeque::new(),
            cursor: 0,
            bytes: 0,
            limits,
        }
    }

    pub fn status(&self) -> HistoryStatus {
        HistoryStatus {
            undo_strokes: self.cursor,
            redo_strokes: self.entries.len() - self.cursor,
            retained_snapshot_bytes: self.bytes,
            limits: self.limits,
        }
    }

    /// Explicitly establish a new baseline after the editor resolves an
    /// external-edit conflict, switches targets, or abandons a sculpt session.
    pub fn clear(&mut self) {
        self.entries = VecDeque::new();
        self.cursor = 0;
        self.bytes = 0;
    }

    fn expected_state(&self) -> Option<&S> {
        if self.cursor > 0 {
            Some(&self.entries[self.cursor - 1].after)
        } else {
            self.entries.front().map(|entry| &entry.before)
        }
    }

    /// Check the guard-owned stroke baseline before beginning input. This does
    /// not capture a snapshot or create a second stroke transaction. An empty
    /// history accepts any baseline; the native guard still validates admission.
    pub fn check_current<E>(&self, current: &S) -> Result<(), HistoryError<E>> {
        if self
            .expected_state()
            .is_some_and(|expected| !current.same_state(expected))
        {
            return Err(HistoryError::Conflict);
        }
        Ok(())
    }

    /// Call only after final post-stroke processing and the safety owner's
    /// acceptance gate. Validation failures leave history unchanged; the editor
    /// owns trial rollback and must withhold packets for rejected/no-op strokes.
    pub fn record_accepted<E>(
        &mut self,
        before: S,
        after: S,
        mut validate: impl FnMut(&S) -> Result<(), E>,
    ) -> Result<RecordOutcome, HistoryError<E>> {
        self.check_current(&before)?;
        validate(&before).map_err(HistoryError::Validation)?;
        validate(&after).map_err(HistoryError::Validation)?;
        if before.same_state(&after) {
            return Ok(RecordOutcome::NoChange);
        }

        let bytes = before.retained_bytes().checked_add(after.retained_bytes());
        if self.limits.max_strokes == 0 || bytes.is_none_or(|n| n > self.limits.max_snapshot_bytes)
        {
            self.clear();
            return Ok(RecordOutcome::AcceptedWithoutUndo {
                required_snapshot_bytes: bytes,
            });
        }
        let bytes = bytes.unwrap();

        // Only a changed, accepted stroke destroys the redo branch.
        while self.entries.len() > self.cursor {
            self.bytes -= self.entries.pop_back().unwrap().bytes;
        }
        let mut evicted_strokes = 0;
        // Subtraction avoids overflow even for a usize::MAX configured budget.
        while self.entries.len() >= self.limits.max_strokes
            || self.bytes > self.limits.max_snapshot_bytes - bytes
        {
            self.bytes -= self.entries.pop_front().unwrap().bytes;
            self.cursor -= 1;
            evicted_strokes += 1;
        }
        self.entries.push_back(Entry {
            before,
            after,
            bytes,
        });
        self.bytes += bytes;
        self.cursor += 1;
        Ok(RecordOutcome::Recorded { evicted_strokes })
    }

    /// Replace the complete live snapshot; `false` means there is no undo step.
    /// Failure changes neither the live state nor history's cursor. The safety
    /// owner must validate with its final geometry policy, including overlaps.
    pub fn undo<E>(
        &mut self,
        live: &mut S,
        validate: impl FnOnce(&S) -> Result<(), E>,
    ) -> Result<bool, HistoryError<E>> {
        self.undo_prepared(live, |target| {
            let candidate = target.clone();
            validate(&candidate)?;
            Ok(candidate)
        })
    }

    /// Prepare a complete detached restore through the safety owner, then swap
    /// it atomically. Use this when fallible safety/index rebuilds must finish
    /// before publication. Preparation may change only disposable derived state.
    pub fn undo_prepared<E>(
        &mut self,
        live: &mut S,
        prepare: impl FnOnce(&S) -> Result<S, E>,
    ) -> Result<bool, HistoryError<E>> {
        if self.cursor == 0 {
            return Ok(false);
        }
        let entry = &self.entries[self.cursor - 1];
        Self::restore(live, &entry.after, &entry.before, prepare)?;
        self.cursor -= 1;
        Ok(true)
    }

    /// Replace the complete live snapshot; emits no stroke/replay data.
    pub fn redo<E>(
        &mut self,
        live: &mut S,
        validate: impl FnOnce(&S) -> Result<(), E>,
    ) -> Result<bool, HistoryError<E>> {
        self.redo_prepared(live, |target| {
            let candidate = target.clone();
            validate(&candidate)?;
            Ok(candidate)
        })
    }

    /// Redo with all fallible safety/index preparation completed before the swap.
    pub fn redo_prepared<E>(
        &mut self,
        live: &mut S,
        prepare: impl FnOnce(&S) -> Result<S, E>,
    ) -> Result<bool, HistoryError<E>> {
        if self.cursor == self.entries.len() {
            return Ok(false);
        }
        let entry = &self.entries[self.cursor];
        Self::restore(live, &entry.before, &entry.after, prepare)?;
        self.cursor += 1;
        Ok(true)
    }

    /// Restore using the native guard's existing validated mesh-replacement
    /// operation. `current` is an immutable capture of the authoritative native
    /// state, NOT an owning wrapper that this method will assign to.
    ///
    /// `replace(expected, target)` must recheck the actual live native state,
    /// validate/prepare the exact target under the current safety policy, and
    /// atomically install geometry, identities and safety state. On Err it must
    /// leave native state unchanged. It must not emit replay stroke packets or
    /// mutate retained snapshots. The cursor advances only after Ok. This is a
    /// strict callback contract: history cannot roll back a partial native swap.
    pub fn undo_replace<E>(
        &mut self,
        current: &S,
        replace: impl FnOnce(&S, &S) -> Result<(), E>,
    ) -> Result<bool, HistoryError<E>> {
        if self.cursor == 0 {
            return Ok(false);
        }
        let entry = &self.entries[self.cursor - 1];
        if !current.same_state(&entry.after) {
            return Err(HistoryError::Conflict);
        }
        replace(&entry.after, &entry.before).map_err(HistoryError::Validation)?;
        self.cursor -= 1;
        Ok(true)
    }

    /// Redo via the same strict atomic native replacement contract as undo.
    pub fn redo_replace<E>(
        &mut self,
        current: &S,
        replace: impl FnOnce(&S, &S) -> Result<(), E>,
    ) -> Result<bool, HistoryError<E>> {
        if self.cursor == self.entries.len() {
            return Ok(false);
        }
        let entry = &self.entries[self.cursor];
        if !current.same_state(&entry.before) {
            return Err(HistoryError::Conflict);
        }
        replace(&entry.before, &entry.after).map_err(HistoryError::Validation)?;
        self.cursor += 1;
        Ok(true)
    }

    fn restore<E>(
        live: &mut S,
        expected: &S,
        target: &S,
        prepare: impl FnOnce(&S) -> Result<S, E>,
    ) -> Result<(), HistoryError<E>> {
        if !live.same_state(expected) {
            return Err(HistoryError::Conflict);
        }
        let candidate = prepare(target).map_err(HistoryError::Validation)?;
        if !candidate.same_state(target) {
            return Err(HistoryError::PreparedStateChanged);
        }
        *live = candidate;
        Ok(())
    }
}

impl<S: HistorySnapshot> Default for SculptHistory<S> {
    fn default() -> Self {
        Self::new(HistoryLimits::default())
    }
}
