# Bevy 20 development candidate

This branch uses official Bevy 0.20.0, official bevy_egui 0.43.0-rc.1
(prerelease), and egui 0.36.2. Cargo.lock records the tested dependency graph.
Qualification uses Rust 1.97.1. Frontend selection routes remain available.

The candidate carries the native painting/sculpting panels and the chronological
Canvas, Direct UV, and Sculpt input/history integration from cloud/egui-parity
commit c825e0a2e49f704611c5ff261898561971b85b53. The migration updates render
scheduling, WESL imports, and required public API bindings. Geometry algorithms,
safety tolerances, and history restoration/replay bodies are preserved.

## Sculpt history

Accepted, changed strokes record complete endpoint snapshots after the native
safety acceptance gate. Undo/redo uses the safety owner's validated atomic
replacement operation, restoring topology, positions, UV corners, global identities
and counters together. Rejected/no-op strokes preserve redo; a new accepted stroke
clears redo. External edits produce a conflict rather than overwriting geometry.
Undo/redo emits no synthetic stroke replay packets.

The default retention limits are 64 strokes and 128 MiB of accounted retained
snapshot allocation capacities. This is not a process RSS bound: live geometry,
validation/capture scratch, a temporary restore clone, and collection overhead are
outside the snapshot byte limit. Old entries expire within the limits. An accepted
stroke too large to retain remains live and clears both stacks; the editor reports
that it cannot be undone, preventing a jump across an unrecorded transition.

## Native migration repairs

Bevy 20 maintains separate mesh and wireframe view-specialization invalidation.
The Scene render bridge forwards changed mesh views to wireframe specialization
after view-key updates and before official wireframe specialization. This prevents
stale bind-group layouts when Depth View changes the depth prepass. It preserves
on-demand prepasses and the official renderer implementation.

The egui adapter mirrors authoritative Scene depth settings before drawing the
next toolbar frame. The toolbar can consequently enable and disable Depth View,
including after another controller changes the setting. A real-widget test covers
the widget, shared dispatcher, Scene resource, and acknowledged snapshot roundtrip.

## Qualification

These results describe migration milestone
136d1ff12a329e48709bd9f7bb8d01bc20c80f12. The later native brush UI followup
has separate qualification in NATIVE_BRUSH_PARITY.md.

- Production native executable: `cargo build -p pentimento --features egui --locked --offline` linked successfully.
- Production App, Scene, and Sculpt unit/integration suites: **429 passed, zero failed, four manual tests ignored** with `cargo test -p pentimento -p pentimento-scene -p sculpting --features pentimento/egui,sculpting/bevy --lib --tests --locked --offline -- --test-threads=2`.
- Production egui presentation widgets: **14 passed** separately with egui 0.36.2.
- The seven actual sculpt history pipeline tests and 21 history state-machine tests cover complete geometry/UV/identity restoration, admission, conflicts, invalid restore rejection, redo branching, and retention limits.
- Actual native GUI on Xvfb/Mesa 25.0.7 llvmpipe Vulkan: accepted Grab stroke changed the sphere from 1088 to 1096 faces; the Undo button restored 1088, Ctrl+Shift+Z restored 1096, Ctrl+Z plus an off-target gesture preserved redo, and a new accepted stroke cleared redo. Displayed history counts agreed with each operation.
- Actual native Depth View Off → On → Off → On → Off rendered the corresponding scene and toolbar labels without a GPU validation error.
- Combined selectable adapters: `cargo check -p pentimento --features egui,dioxus --locked --offline` passed. The missing official packages were acquired normally and checked against Cargo.lock checksums. Direct Taffy defaults now match official Blitz's layout feature selection. Dioxus Atom typing, required mouse event fields, and Bevy non-Send/AssetMut API bindings were updated without changing existing dispatch or input thresholds.
- Independent source reviews approved the migration, depth invalidation/snapshot repairs, and optional Dioxus API followup.

The production suite and GUI smoke used the frozen native source before the final
Dioxus-only API followup. The migration milestone native executable was rebuilt
from its complete delivery source and is byte-identical to the GUI-qualified executable
(SHA-256 1f4a38d0b54a3144db680e089cdc524680e60598688bb232123277e552de2fbd).
The lockfile and active native egui Rust/WESL paths are unchanged by that optional
Dioxus followup.

Physical GPU/stylus behavior, device recovery, lighting/atmosphere pixel parity,
and Dioxus/CEF executable runtime parity remain unqualified. The normal scene is
dark with visible geometry/wireframes on this software adapter; the native smoke
establishes editor/history and depth behavior, not lighting parity. The combined
Dioxus result is a compile check, not a claim that its executable was linked or run.

Generated build outputs, logs, source snapshots and JPEG85 runtime screenshots
remain outside Git. Unique qualification evidence was preserved before removing
obsolete reproducible outputs to make build space. Main and Pantograph are
unchanged. This is a development candidate for review; there is no main merge.
