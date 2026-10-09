# crates/egui-ui/src

## Purpose
This directory contains the native egui presentation layer for Pentimento's
experimental Bevy-integrated frontend path.

## Contents
| File/Folder | Description |
|-------------|-------------|
| `lib.rs` | Public facade for the egui UI crate and its exported state types. |
| `state.rs` | Framework-neutral egui-facing snapshot/runtime state and inbound message application. |
| `app.rs` | Immediate-mode egui layout for the current frontend spike. |

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
let snapshot = EguiUiSnapshot::default();
let _commands = show_root_ui(&egui::Context::default(), &snapshot, &mut runtime);
```

## API Consumer Contract
- Consumers provide the current backend snapshot and a mutable runtime state.
- The root UI returns typed `UiToBevy` commands for the host to dispatch.
- The host owns command timing, retry behavior, and any scene/resource side
  effects.

## Structured Producer Contract
- Produces in-process `UiToBevy` command values only.
- Command shapes and enum variants must remain aligned with `crates/ipc`.
- No persisted artifact or stable external serialization is produced here.
