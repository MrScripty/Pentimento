# Ordered paint input and validated sculpt history reset

This follow-up starts at PR19 `bb299e5edb9ed697436c7d3a532c28dded28d222`,
tree `0d96256d15b6cc7536156fc3babf1fdcd095ae94`. The recovered private Cloud
candidate `1a4601a` is preserved separately; its results are not attributed here.

## Reproduced paint defect

Diagnostic commit `4e2fa33` drives typed mouse messages through Bevy's actual
`InputPlugin`, calls the production paint input system and runs
`PaintingSystemPlugin`. It never synthesizes `PaintEvent`, manipulates
`ButtonInput`, or directly calls the painting pipeline to create a stroke.
The old handler passes the separate-frame positive control (painted pixels,
completed packet, undo entry, released ownership and pixel restoration by undo)
and UI-owned no-replay control. Whole gestures in one frame, repeated complete
batches and two strokes in one batch each fail with zero completion packets.

The repair consumes ordered `WindowEvent` messages each frame, uses the press
origin, preserves all movement before release, and completes each stroke before
the next press. Blocked and inactive batches are consumed without replay. Focus
loss closes ownership at its chronological position; post-regain hover supplies
the next press origin. Painting event processing follows the input system.
Eight real input tests compare complete batched pixels with separate-frame
controls and check packet IDs/dabs, actual undo entries and released ownership.

Reproduce with:

```sh
cargo test -p pentimento-scene --features sculpting paint_input_batch_tests
```

## External sculpt edit reset contract

`SculptingPipeline::reset_history(mesh_id, &mut mesh, notice)` is an explicit owner
operation. Finish or cancel an active transaction first; active and rejected
transactions refuse reset. The call validates full snapshot identities and the
complete surface with the current guard before changing any history, admission
cache or notice. On success it clears both history stacks, drops derived
octrees, rebuilds mesh bounds/spatial lookup in a detached copy, establishes the
validated admission witness, updates the vertex count
and stores the optional notice. It does not mutate mesh geometry or emit replay.
On failure history and caches remain unchanged. The guard tolerances are
unchanged.

Do not call reset automatically when a stroke reports a conflict. A conflicting
stroke must still be refused and its replay suppressed. After explicit external
edit admission, the next accepted stroke can undo back to that exact external
baseline. Exit/reentry already recreates the editor pipeline; this API provides
the same explicit recovery boundary to other callers without leaving the mode.
Retained snapshot accounting remains bounded by the existing history limits;
it is not a bound on process RSS or temporary validation allocations.

Four real production-pipeline regressions cover successful external admission
and next-stroke undo (including translated geometry with stale bounds), atomic refusal of active/invalid edits, and continued
refusal of a conflicting stroke before explicit reset.

## Qualification boundaries

Native CEF run `37771468705` passed all 15 checks on the unchanged starting tree,
including strict sculpt Undo/Redo/cancellation pixels, exit/reentry, paint undo
and projection controls. That successful ordinary stroke does not prove complete
same-frame paint batching. The new input regressions establish CPU input and
painting pipeline behavior; final native screenshot review belongs to root.
Keep failed diagnostic evidence and source mappings alongside the green results.
