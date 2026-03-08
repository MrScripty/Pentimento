# crates/app/src/render/ui_dioxus

## Purpose
This directory adapts the native Dioxus UI crate into the Bevy render and input lifecycle.

## Contents
| File/Folder | Description |
|-------------|-------------|
| `setup.rs` | Startup path for the Dioxus renderer resources. |
| `render.rs` | Render-loop integration and texture update path. |
| `event_bridge.rs` | Bevy-to-Dioxus event plumbing. |
| `ipc_handler.rs` | UI-originated command handling back into Bevy state. |
| `scene_builder.rs` | Native UI scene construction helpers. |

## Problem
The application crate needs a thin orchestration layer that can run Dioxus as an active frontend without absorbing the entire UI implementation.

## Constraints
- Must bridge Bevy resources to `crates/dioxus-ui`.
- Must keep Dioxus-specific lifecycles out of the generic render module.
- Must remain compatible with the same IPC contract surface as the browser UI.

## Decision
Keep all Dioxus-specific app wiring in this subdirectory and let `crates/dioxus-ui` own the actual UI implementation.

## Alternatives Rejected
- Merging Dioxus setup into `render/mod.rs`: rejected because the generic render module already multiplexes multiple frontends.

## Invariants
- Dioxus resource creation and teardown stay isolated here.
- Backend commands from the native UI are translated through shared IPC types.

## Revisit Triggers
- Another native frontend path needs the same render lifecycle hooks.
- The Dioxus renderer stops needing Bevy-side orchestration glue.

## Dependencies
**Internal:** `crates/app/src/render`, `crates/dioxus-ui`, `crates/ipc`  
**External:** Bevy render graph, pollster

## Related ADRs
- `ADR-001` active frontends and contract ownership.

## Usage Examples
```rust
ui_dioxus::setup::setup_dioxus_renderer(world)?;
```

## API Consumer Contract
- This code is consumed by `crates/app/src/render/mod.rs`.
- Callers must create resources before the render loop begins.
- Inbound UI commands are handled synchronously on the Bevy side.

## Structured Producer Contract
- Produces in-process event envelopes and render-target updates, not saved artifacts.
- Contract changes must remain aligned with `crates/dioxus-ui` and `crates/ipc`.
- No compatibility guarantee exists outside the active application process.
