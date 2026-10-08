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

Diagnostic commit `a16dd82` extends this to the production native forwarding and
paint mode plugins. Its separate-frame and UI-drag controls pass; five tests
fail because a later UI/focus event discards an earlier valid gesture or because
Escape acts on frame-final state rather than the stroke at that event position.
The native forwarder now publishes a chronological scene-owned event stream.
UI-origin button ownership stays latched, entering UI or losing focus closes a
scene gesture, and newly reported UI rectangles close a stationary held gesture.
The conservative frame-wide flags remain available to other scene systems.
Paint drains raw events as before but consumes the native stream when provided.
Escape cancels the transaction at its chronological position, restores the
captured pixels on the stroke's original layer, and emits no completion packet.
Undo refuses an active transaction until it finishes or cancels, preserving the
rollback baseline and existing undo entries.

Run the full production-path regressions with:

```sh
cargo test -p pentimento --features egui input::
cargo test -p painting
```

The paired Radius range/number controls now share an immediate local draft and
reconcile it with backend state. A real browser pointer regression reproduces
the former pending mismatch, checks backend float acknowledgement and mode
reentry, and verifies no scene stroke starts from the panel.

Hosted native continuity on `678e017` then exposed a second paint defect: all
sixteen pointer moves reached the forwarder in order, but the line stopped
after the initial dab. The brush resampler overwrote its accumulated distance
with only the current segment whenever no dab was emitted. Several moves below
half a dab spacing therefore never reached the next dab. Diagnostic commit
`9404636` reproduces this through both `PaintingPipeline` and the full production
native forwarding/paint plugins, checking actual float pixels and exact Undo.
The fix retains accumulated distance when no dab is emitted; brush spacing,
pressure interpolation and stroke resets are unchanged. The native drag and
continuous changed-pixel assertion remain unchanged for requalification.

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
The native follow-up retains the original toolset assertions and stroke timing,
adds changed magenta pixels along every four-pixel section of the actual paint
drag (endpoint dabs cannot pass), and captures the open native menu and a
nonempty source after projection controls. Sculpt Radius qualification requires
both paired DOM values and a diagnostic receipt from the actual pipeline preset.
These extra requirements need a successful hosted run on the published source;
local CPU and browser passes alone do not qualify native rendering.
