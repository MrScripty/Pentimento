# Sculpt history closure and native integration boundary

This isolated candidate starts at PR #18 head
`b188a8271907bea15aa1ba56b3ab67bf9ebc1af8`. It changes only a new history module,
new tests, and this contract. Shared pipeline, chunking, scene, IPC and panel
files remain under root's ownership pending an explicit boundary agreement.
The feature is not yet connected to the running editor. No public push/merge
is authorized. No geometry guard or tolerance is implemented here.

## Reconciliation with root's final guard

The separate source-only guard reference is now available through normal public
Git access: `reference/layered-sculpt-guard-b4c3219`, base
`36f3c7d4d24b7b884700a3116c6d8d36e4a03bef`, head
`99019d94fd584c9253e0b9f2f684ae2e47af76af`. Its base tree exactly matches public
`b188a827`; the full-index binary diff is 130064 bytes with SHA-256
`44834691976e580e6a2f5190891c00cf9d12ace8f37dbbd48c58a6e2a1f38266`.
All seven modified preimage blob IDs match current public PR18. The ten guard
files do not overlap the Cloud keyboard/capture changes. The invalid earlier
text transfer was discarded; no guard patch is applied in this history branch.

Read-only inspection confirms complete-stroke rollback, suppression of rejected
stroke packets, exact private half-edge/global-map witnesses, chunk-map key/ID
checks, and admission of changed input on zero-dab processing and at stroke end.
The detached reference passed `cargo test --locked --offline -p sculpting
--features bevy`: 66 unit, four geometry and 16 layered-safety tests; two manual
performance measurements remain ignored. This qualifies that reference's CPU
correctness tests, not the combined PR18/editor/history integration.

The guard exports `validate_sculpt_surface`, but does not yet expose the complete
opaque safety snapshot/current-policy prepared replacement API requested below.
Its private pipeline checkpoint, budget, admitted witnesses and octrees remain
owned by the guard workstream. A whole owning `ChunkedMesh` clone preserves
private allocation counters; rebuilding from merged render geometry does not.
History must reuse that owner rather than create another guard or change its
tolerances. Production wiring remains blocked on the agreed snapshot/restore
boundary.

An additional actual reference-pipeline probe found that `cancel_stroke()` still
drops brush/active state without restoring its owning checkpoint. A dab changed
224 faces to 232; cancellation left 232, failing the rollback assertion. This
negative probe lives outside Git and is separate from the 86 passing reference
correctness tests and 28 passing history compatibility tests. Rejection rollback
is verified; genuine cancellation rollback is **not** supplied by this reference.
Never call its `cancel_stroke()` as authorization to undo an active transaction.
Refuse undo/redo while active, or require a native-owner cancellation operation
that atomically restores the complete checkpoint before history restoration.
No guard/pipeline source is modified to address this finding.

Earlier cancel/no-op-packet/Exit observations describe the **public b188
baseline**. The off-target fixture does not assert raw packet emission as desired
behavior. Exit/reentry lifecycle remains a native-owner hook decision.

## Minimal hooks: reuse one guard transaction

| Native event | History action | Native ownership |
| --- | --- | --- |
| Before guard begin | `check_current::<GuardError>(&baseline)` | Reuse guard's existing immutable full baseline, validate admission and pin target/session/external generation. |
| Final accepted changed commit | `record_accepted(before, after, validate_certificate)` | Supply endpoints from the one completed guard transaction after final post-processing/readmission. |
| Accepted exact no-op | `record_accepted` returns `NoChange` | Compare authoritative content; preserve redo. |
| Rejection | No record call | Guard rejection restores baseline and suppresses packets. History/redo remain unchanged. |
| Cancel | No record call only after complete rollback | Reference cancellation does not restore geometry. Native owner must supply genuine rollback or keep history commands blocked while active. |
| Undo/redo | `undo_replace(&current, replace_validated)` / `redo_replace(&current, replace_validated)` | Existing native operation rechecks, validates/prepares and atomically installs complete state. |
| Target/session replacement or resolved conflict | Explicit `clear()` | Native owner chooses/reports lifecycle; never attach old entries to a new imported target silently. |

`validate_certificate` must verify root's actual accepted proof under the current
policy, not be an unconditional-success placeholder. It can verify the current
proof instead of rerunning an independent guard. Individually valid endpoints
are insufficient: the guard must attest that before/after belong to the same
completed stroke owner (target/session/external generation). Reject undo/redo
while that transaction is active. A cancellation alternative requires genuine
native rollback; the reference `cancel_stroke()` cannot satisfy it.

`replace_validated(expected, target) -> Result<(), GuardError>` is an adapter to
root's existing replacement operation, with these strict requirements:

1. Under native editor ownership, recompare **actual live** state with expected.
   History's supplied current capture may become stale before publication. A
   key-only/identity-only external edit must cause a conflict.
2. Validate the exact detached target under the current final policy, including
   overlaps, seams, global IDs and private topology. Rebuild indexes/bounds and
   finish every fallible operation before publication. Do not repair target
   geometry/identities to make it pass.
3. Atomically install owning mesh, identities/counters and prepared safety state;
   finish infallible cache invalidation/GPU-dirty marking before success and the
   next dab/draw. Emit no synthetic replay packets.
4. On error leave native geometry/safety state unchanged. History advances its
   cursor only after callback success; it cannot undo a callback's partial swap.

This avoids reorganizing pipeline ownership merely to satisfy generic `&mut S`.
Existing owning-snapshot `undo`/`redo` and prepared methods remain usable when
the owning snapshot itself is authoritative native state. No history pending
stroke/rollback engine is added.

## Safety snapshot API requested from root

Root should supply an opaque immutable `SafeSculptSnapshot` implementing
`HistorySnapshot` and `Clone`. Its authoritative payload must include:

- The complete chunked half-edge meshes: vertices, half-edges, faces, tombstones,
  outgoing references, edge lookup maps, positions, normals, per-corner UV0 and
  fallback vertex UV/source indices.
- Chunk-map keys **and** values' chunk IDs, local/global and reverse vertex maps,
  boundary relationships, and **both**
  allocation counters (`next_chunk_id` is private today;
  `next_original_vertex_id` is public). Cloning/replacing the whole owning state
  is preferable to reconstructing it through import/merge/partition.
- Configuration and any non-disposable safety state needed to resume editing.
  Bounds/spatial indexes can be retained or prepared anew before publication.
- Target/session identity, external-edit generation and policy provenance.
  Use the existing conflict owner; never overwrite an external edit or silently
  establish a new baseline after a mismatch.

Snapshots must remain immutable across all owners: Clone preserves exact
authoritative content, and neither callbacks nor later edits may mutate shared
live/retained authoritative data through interior mutability. Working copies or
copy-on-write must isolate mutation.

`same_state` must compare authoritative state exactly, independent of HashMap
iteration order. Derived GPU flags and disposable caches should not make a
no-op look changed. A revision must distinguish external edits, but should not
make an equal stroke geometry look changed solely because input was received.
An unchecked hash or approximate position comparison is insufficient. Policy
certificates and rebuilt safety indexes may be disposable derived state; do not
let a cache refresh create a false conflict, or restore a stale certificate as
though it were validated under the active policy.

`retained_bytes` must conservatively account for inline state and owned
allocation **capacities**, including private maps and vector backing storage.
Recursively retained/shared parents must also be charged; bounded entry count
alone does not bound such storage. Shared snapshots may charge the allocation
at each endpoint. The generic
history engine cannot measure allocations hidden by the snapshot type.

The final geometry owner must expose validation of a detached snapshot under
the **current final safety policy**. It must cover the same invariants as stroke
acceptance, including geometric overlaps and cross-chunk seam relationships.
Connectivity/manifold checks alone are explicitly insufficient. Validity
certificates must not remain trusted across policy/version changes unchecked.
Validation must not mutate live state or history. Preparation of bounds, safety
indexes, identity maps and other fallible work must finish before publication.

The engine's `undo`/`redo` clone and validate the target, then replace an entire
owning live snapshot and advance the cursor. For fallible index/guard rebuilds,
use `undo_prepared`/`redo_prepared`: root's callback takes a borrowed immutable
target and returns the complete validated/prepared owning snapshot. The engine
checks that preparation did not change authoritative target state, then swaps
the whole result. Failure leaves live state and cursor unchanged. Root must make
that owning snapshot the authoritative editor state (or use the strict native
replacement adapter described above).
Do not advance the history cursor and subsequently attempt a fallible, partial
mesh/guard restore. Complete authoritative state must swap together.

## Stroke transaction and packets

1. Capture the complete validated baseline immediately before any stroke dab.
   Keep a separate pending transaction; it is not an undo entry.
2. Apply dabs using the final safety owner. Rejection restores the baseline
   through the safety rollback path, clears the pending transaction and emits
   no stroke packet. Cancellation must do the same before being treated as an
   uncommitted stroke. Both public b188 and the verified reference
   `pipeline.cancel_stroke()` currently drop brush/active state without rollback;
   the native owner must provide a genuine cancellation path or refuse history
   commands while active.
3. End the stroke and finish rebalancing/compaction, then run the final acceptance
   gate. Only the committed, accepted endpoint may enter `record_accepted`.
4. Exact no-op detection preserves redo, records no entry and withholds packets.
   `vertices_modified` is not an adequate change detector: it counts affected
   vertices even when displacement is zero. Raw pipeline packets are also not
   proof of a committed change. Final native packet/admission policy is tested
   by the guard owner, not inferred from this public-baseline fixture.
5. Only a changed accepted transition clears redo. Record failures must not be
   advertised as a successful undoable commit. Root decides its existing
   conflict/rollback policy before publishing a new endpoint.
6. Undo/redo restores local state and emits **no replay stroke packets**. Until a
   state-replacement protocol exists, do not label this local restore as a
   synchronized/replayed brush stroke. Rejected/clamped dabs require special
   caution: their raw input packet may describe geometry that was never accepted.

The history engine intentionally has no active-stroke or packet API. The editor
must reject undo/redo while a transaction is active, or cancel that transaction
with a genuine rollback before restoring history. The verified reference's
`cancel_stroke()` does not implement that alternative. Brush settings/camera motion
do not become sculpt geometry entries.

## Restore and editor wiring boundaries

After a successful restore, before the next draw/dab, root's adapter must:

- Invalidate pipeline octrees and refresh the vertex budget's current usage.
- Publish the restored bounds/spatial and safety indexes together with geometry.
- Invalidate the scene's merged render-vertex mapping, remove obsolete chunk
  entities and mark every restored chunk for full topology/GPU synchronization.
- Finish stroke/adjustment ownership bookkeeping and notify the UI with backend
  `undo_strokes`, `redo_strokes`, retention status and any conflict/validation
  error. Do not derive availability from the existence of a mouse stroke alone.

Suggested eventual IPC additions are sculpt-local `Undo`/`Redo` commands plus a
backend-owned history status message. Buttons and Ctrl+Z/Ctrl+Shift+Z should use
the same native path, obey sculpt mode and frontend input ownership, and be
disabled while a stroke is active. Update the current "Sculpt undo is not
available yet" panel notice only after this path genuinely restores geometry.
These are proposals; no shared file is changed by this candidate.

Session behavior also needs agreement: public b188 Exit destroys the chunked mesh
and pipeline and reentry imports the rendered mesh anew. Preserving complete
global identities across Exit/reentry requires keeping the authoritative session
snapshot, or explicitly declaring session-scoped history and clearing it on
Exit/target change. Do not attach old entries to a newly imported target merely
because its visible positions happen to match. Pipeline and chunked mesh sizing
configs must stay aligned: current rebalance selects oversized chunks using
pipeline limits while `split_chunk` independently uses mesh limits; a mismatch
can cause the loop to keep re-adding the same chunk under fresh IDs. The
rebalance test aligns its configs without changing this shared implementation.

## Bounded retention behavior

Default retention is 64 accepted strokes and 128 MiB of accounted snapshot
storage across undo **and** redo together. Both endpoints are charged for each
entry, conservatively allowing shared endpoint storage to be double-counted.
Oldest complete transitions are evicted first. The oldest retained baseline is
the furthest undo target; there is no replay across missing entries.

A changed accepted stroke exceeding the byte limit, zero retention, or size
overflow produces `AcceptedWithoutUndo`, clears both stacks and leaves accepted
live geometry unchanged. Older entries cannot jump across this missing stroke.
Surface this event in the editor rather than implying that every accepted
stroke can be undone. A later affordable transition can start a new baseline.
No-op/rejected strokes do not cause this retention barrier.

This is a **retained snapshot** bound, not a peak-memory/RSS promise. Collection
metadata is additionally bounded by the stroke-count limit. Live mesh data,
equality work, the guard-owned pending baseline, capture/validation scratch,
GPU copies and a detached restore candidate are outside the retained byte
budget. Snapshot admission happens
after the caller captures it. If root requires a peak allocation limit, its
capture API must preflight/reserve storage before cloning; history cannot
retroactively prevent the capture allocation. Do not ship a UI claiming a hard
process-memory cap.

## Candidate tests

`history_state_machine.rs` runs directly with `rustc --test`, or through Cargo.
It checks complete state replacement, atomic validation failures, conflict
preservation, no-op/rejection/redo semantics, count/byte eviction and oversized
or overflowing entries. Prepared restores additionally verify that disposable
cache rebuilds can succeed before the swap, while geometry/global-ID changes
during preparation are rejected atomically. Value-based metadata regressions
cover chunk-map keys versus IDs, local/global and reverse maps, boundary IDs,
counters, half-edge lookup/outgoing/source metadata and tombstones. HashMap
insertion order and disposable capacity changes preserve no-op redo. Native
replacement regressions cover target denial, stale captures after comparison
but before publication, and callback/cursor atomicity in both directions.

`history_pipeline.rs` uses the actual `SculptingPipeline`, UV-sphere import,
adaptive split/deform path and render export. It freezes the entire owned
`ChunkedMesh` behind Arc and checks restore of topology, positions, normals,
UV0 corners and global IDs, continued sculpting after undo, branch invalidation,
off-target no-ops, validation denial, malformed endpoints and external geometry
conflicts. Post-stroke rebalancing additionally verifies boundary references and
private chunk-ID allocation after restoration.
Its fixture-only detached working-copy helper is restricted to contiguous
chunk IDs; restore itself swaps the original complete snapshot, never that
reconstruction. Fixture snapshot provenance uses Arc identity and a fixed
conservative size charge. Neither is a production capture/memory adapter.
Fixture validation checks topology and finite attributes only; these results
do not qualify the private layered guard or claim self-intersection freedom.

Run:

```sh
cargo test -p sculpting --features bevy --test history_state_machine --test history_pipeline --test geometry_regressions
```

No UI/CEF or performance acceptance is claimed by this isolated candidate.
Build outputs, logs and generated lockfile evidence are retained outside Git.

## Independent closure and concrete blocker

An independent read-only reviewer examined the original candidate and closure
changes. No core cursor/eviction/accounting defect was found. The reviewer
identified the Arc fixture's missing value-comparison coverage, immutable-clone
and native atomic callback requirements, and stale baseline wording; the closure
adds the regressions/contracts above. Its fresh closure compilation/execution
passed 21/21 dependency-free tests. Full owning-crate verification passed 93
tests (61 sculpt unit, 4 existing geometry, 7 actual-pipeline history and 21
state-machine). These are public-baseline plus isolated-history results.

Production integration remains blocked on final native guard source at an exact
commit, complete immutable snapshot/equality/accounting APIs and agreed begin/
accepted/rejected/validated-replacement/lifecycle hooks. The unavailable owner
prevents honest qualification of production private-snapshot completeness,
final geometry safety and native replacement atomicity. No pipeline/UI/guard
file is changed while those hooks are prepared.
