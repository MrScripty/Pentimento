# crates/egui-ui/src

## Purpose
This directory contains the native egui presentation layer for Pentimento's
experimental Bevy-integrated frontend path.

## Contents
| File/Folder | Description |
|-------------|-------------|
| `lib.rs` | Public facade for the egui UI crate and its exported state types. |
| `state.rs` | Framework-neutral egui-facing snapshot/runtime state and inbound message application. |
| `app.rs` | Toolbar, panels, popup/modal capture geometry and startup state requests. |
| `paint_panel.rs` | Acknowledged Canvas/DirectUV brushes, sampling, projection and Canvas layers. |
| `sculpt_panel.rs` | Supported sculpt tools, controls and actual stroke Undo/Redo commands. |
| `uv_layer_panel.rs` | Receiver stacks, linear blend modes, Color/Mask targeting and shared UV history. |
| `presets.rs` | Backend-owned saved paint/sculpt catalogs. |
| `project_dialog.rs` | Local Save/Open/New workflows, frozen owner confirmation and pending receipts. |
| `controls.rs` | Shared typed command and widget helpers. |
| `tests.rs` | Actual headless egui frames and pointer/key interaction regressions. |

## Problem
Pentimento needs an egui implementation path that can mirror the existing native
frontend surface without pushing egui-specific layout code into the Bevy app
composition root.

## Constraints
- Must consume the shared `crates/ipc` contract instead of inventing a separate
  native-only vocabulary.
- Must remain presentation-focused; Bevy resource ownership and startup/resize
  lifecycles stay in `crates/app`.
- Must stay small enough that parity work can decompose panels instead of
  growing a second large monolith beside Dioxus.

## Decision
Keep the egui widget tree and UI-local runtime state in a dedicated crate that
exports a small public facade to the Bevy adapter layer, while shared
backend-derived native frontend state lives in `crates/frontend-core`.

## Alternatives Rejected
- Embedding egui directly in `crates/app`: rejected because it would mix
  immediate-mode widget code into application orchestration.
- Reusing the Dioxus crate for egui rendering: rejected because the framework
  runtime and rendering model are different enough to require separate
  presentation code.

## Invariants
- `crates/ipc` remains the only typed contract used for backend communication.
- This crate owns presentation and UI-local state only, not Bevy startup or
  scene mutation lifecycles.

## Revisit Triggers
- The egui path grows enough panel complexity to justify subdirectories.
- A framework-neutral native UI core is extracted and reduces the responsibility
  currently held by `state.rs`.

## Dependencies
**Internal:** `crates/ipc`, `crates/frontend-core`  
**External:** `egui` (the same version as `bevy_egui` uses in the Bevy adapter)

## Related ADRs
- `ADR-001` active frontends and contract ownership.

## Usage Examples
```rust
use pentimento_egui_ui::egui;
use pentimento_egui_ui::{EguiUiRuntime, EguiUiSnapshot, show_root_ui};

let mut runtime = EguiUiRuntime::default();
let mut snapshot = EguiUiSnapshot::default();
let _commands = show_root_ui(&egui::Context::default(), &mut snapshot, &mut runtime);
```

## API Consumer Contract
- Consumers provide the current backend snapshot and a mutable runtime state.
- The root UI returns typed `UiToBevy` commands for the host to dispatch.
- The host owns command timing and scene/resource side effects. It applies backend
  receipts before drawing and dispatches returned commands through the shared owner.
- Brush settings, selection, history availability, projection status and document
  ownership come from acknowledged snapshots. Editing controls send commands;
  they do not optimistically change accepted settings.
- `runtime.ui_regions()` supplies real panel/window/menu/picker rectangles in egui
  points. The native adapter converts by `pixels_per_point / window_scale_factor`
  and publishes `LayoutUpdate`; an open document modal includes the whole viewport.
- New freezes the exact decimal generation on confirmation creation. Pending
  operations suppress duplicate submission and cancellation; only a newer matching
  operation receipt closes the modal. The shared native reducer retains exactly
  three latest operation receipts so a different operation cannot hide completion.
- Rename drafts reconcile with changed accepted names and reset on submission.
  Draft maps retain live layer identities and clear on document/receiver replacement.
- UV history counters describe retained/pending payload, not total process memory.
  Backend notices expose expiration, admission refusal and external-edit conflicts.
  Sculpt Undo/Redo restores committed strokes through the existing validated owner;
  these controls do not add nondestructive sculpt deformation layers.

## Validation
`cargo test -p pentimento-egui-ui --lib` exercises real egui widgets without the
Bevy renderer. Production native-controller tests in
`crates/app/src/input/direct_uv_native_tests.rs` also drive egui widgets through
shared commands into CPU paint assets and input arbitration. Those headless checks
are separate from native GUI, GPU and stylus qualification. Dependency migration
and the egui version coupling remain a separate workstream.

## Structured Producer Contract
- Produces in-process `UiToBevy` command values only.
- Command shapes and enum variants must remain aligned with `crates/ipc`.
- No persisted artifact or stable external serialization is produced here.

## Remaining Input Qualification
Canvas and DirectUV history shortcuts follow native event ownership and modifier
order. Production CPU tests cover fresh egui text-field focus by mouse and touch,
held modifiers, accepted shortcuts before UI focus, startup layout and focus return;
blocked shortcuts do not replay on later frames. The qualified run passed 108
controller tests and 14 real egui widget tests, using freshly compiled App/Core/UI
source with compatible preserved official CPU libraries after full Cargo builds
exhausted disk. This does not qualify native GUI, GPU, stylus or Bevy 0.20 rendering.
Sculpt's raw keyboard history consumer still needs the scene owner's ordered-input
integration. Its existing validated history buttons remain available.
