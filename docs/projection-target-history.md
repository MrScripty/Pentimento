# Projection receiver inspection and stroke Redo

This slice starts from draft PR20 `ad92c9a9ac79d1c5a33c7dfabd72b018342790af`.
PR20 stays unchanged. The painting roadmap still defers libmypaint and tablet
input; the existing engine supports six round presets. Inspection of the actual
panels and engine found a more immediate acceptance gap: Apply/Live dispatch
and source-canvas screenshots did not prove that a receiver rendered paint,
and the canvas had no stroke Redo implementation.

## Editor behavior

- Undo and Redo restore exact captured layer pixels on the active canvas.
  Ctrl+Z and Ctrl+Shift+Z use the same history operations as the panel.
  Modifiers are read at each native key event, including a complete chord in
  one frame. Key repeats, UI-owned input and focus-loss batches do not consume
  history or replay when input ownership returns.
- A new changed stroke clears Redo; a no-op or cancelled stroke preserves it.
  Active transactions refuse history movement. Removing a stroke's owning layer
  refuses restoration without popping the cursor or touching another layer.
- Both stacks together retain at most the existing 20 captured-tile entries.
  Undo/Redo swap one tile snapshot per entry instead of retaining two copies.
  Storage depends on canvas dimensions and covered tiles; this is not a process
  RSS limit and does not bound the existing stroke log or temporary allocations.
- **Show source canvas** controls the actual canvas entity's visibility. Hiding
  it keeps the same canvas, brush settings, pixels, projection view and history.
  The hidden canvas remains the brush target; this is projection painting, not
  a switch to direct mesh painting. Turn on Live projection to paint while
  inspecting the UV receiver without the source covering it.
- In Live projection, source stroke Undo, Redo and cancel refresh the existing
  per-canvas projection layer. Repeated Apply replaces that layer and does not
  accumulate the same paint. With Live off, Apply is a snapshot: source edits
  require another Apply to refresh the target.

Projection does not edit mesh topology, vertex positions, UVs or identities.
The sculpt layered-stroke admission guard and its tolerances are unchanged.

## Qualification

CPU pipeline tests check exact layered source restoration, cancellation with a
Redo cursor, no-op/changed branching, layer ownership and removed-layer refusal.
Actual Bevy projection tests check target Image bytes and material bindings over
repeated Undo/Redo/cancel cycles and preserve receiver geometry and UV data.
Shared command tests verify Redo availability and actual source Visibility.

Native CEF qualification retains the original toolset assertions. Extra cases
paint over the real sphere with the actual source canvas hidden, using native
mouse/keyboard and production IPC. Read-only diagnostics report actual atlas
pixels, bound Image bytes, geometry/UV/index fingerprints and viewport bounds.
The receiver pixel oracle excludes every visible source-paint bound, so a source
line or CPU-only projection cannot pass. Layered history regions use unchanged
endpoint frames to calibrate noise and retain the existing restoration thresholds.
Active target pixels are captured before Escape; Undo and Redo must visibly
restore their respective committed target endpoints, as well as exact atlas and
Image fingerprints. A total 900-second window accommodates the added native
cases; each condition keeps its existing 30-second limit.

Generated logs, source bundles and JPEG85 captures stay outside Git. Hosted
native results and manual image review must be reported separately; passing
command dispatch or browser mock interactions alone does not qualify projection.

The published opaque fixture `cb38953` passed actual receiver pixels (110 and
905 magenta pixels for the first and layered strokes), exact cancel restoration
(538 pixels), and exact Undo/button Redo restoration (797 pixels), with matching
Image/atlas endpoints and unchanged geometry. Its final Ctrl+Shift+Z case timed
out. The subsequent local shortcut repair has a failing-before production input
regression and 35 passing input tests, including exact pixels for batched history
chords. It remains unqualified in the native renderer; no new hosted runs were
requested after the owner's CI spending constraint.

The reviewed shortcut fix is published as commit
`b5ce1e720f7f97905af6204e4c5664553bd739f5`, followed by this documentation-only
publication note. Local checks pass 35 production input tests and 50 Node tests;
independent review passes all 35 input tests and 15 native readiness tests.
The publication commit skips hosted CI under the owner's spending constraint.
Skipped checks are pending; this does not qualify the native shortcut or make
the successor's hosted checks green.
