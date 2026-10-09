# Dot sculpt-history reconstruction

This candidate is a separate reconstruction. It is **not** the unrecovered
`cloud/sculpt-history-native` commit `1a4601a` and does not claim its prior test results.

Source lineage:

- Public PR18 source: `3a21b55455796965f5f132d630a704dd0645ecf4`.
- Verified ten-file safety reference: `99019d94fd584c9253e0b9f2f684ae2e47af76af`,
  matching private guard `b4c3219` over reference base `36f3c7d4`.
- Actually recovered generic history: `10494d5bb1fea7876b36ac1028dc702e5781d88d`.
  Bundle SHA-256: `0112dd117d97674c07872b8a3dfecc06f5e37d481da5d54fff9e2547f6157fff`.

The historical contract document records the limitations of the older reference.
This candidate replaces its cancellation gap with an owning checkpoint rollback:
`cancel_stroke(&mut ChunkedMesh)` restores the cloned full mesh and stroke budget,
invalidates admission and octree caches, marks every restored chunk for full GPU
upload, and discards brush packets. The exact 224→232→224 regression exercises it.

History is owned by the live pipeline and lasts for one sculpt session. Accepted
endpoints include full half-edge state, per-corner UVs, hidden edge lookup maps,
identity maps, boundary relationships, both allocation counters, and chunk config.
Comparison is structural, not hash-only or approximate. Retained byte accounting
includes capacities of all owned allocations with conservative hash table overhead.
The 64-stroke/128 MiB defaults bound retained snapshots, not process RSS or capture
scratch. An over-limit accepted stroke clears history and reports its unavailability.

Restores compare the complete expected live geometry, validate the detached target
with the current complete overlap/topology guard and snapshot identity checks, and
prepare bounds/spatial indexes before assigning the whole mesh. The cursor advances
only after successful replacement. Current camera, brush settings and budget policy
remain live; the restored vertex count is recomputed against that policy. No safety
certificate from the snapshot bypasses current validation. Active transactions block
undo/redo; Escape explicitly cancels the transaction before a later history command.
External geometry/counter edits cause a conflict and are not overwritten. There is no
persistent cross-session or cross-target history and no durable external-edit revision
protocol yet; returning to byte-for-byte identical geometry is treated as identical.

Grab now applies incremental emitted-dab displacement. Previously every continuous
dab reapplied the full origin-to-current drag, including stationary frames, amplifying
motion until the guard could roll back the stroke. No tolerance is weakened.

The IPC/UI includes authoritative undo/redo counts, active-transaction lockout, a
history limit notice, buttons, Ctrl+Z/Ctrl+Shift+Z/Ctrl+Y, and Escape cancellation.
The native CEF test retains all 16 original PR18 assertion lines and all nine original
recorded checks, and adds four history records: undo, redo, cancel rollback and redo
branch invalidation. This test must still pass with the actual CEF application before
native visual qualification is claimed. Browser-mock panel tests are not that proof.
