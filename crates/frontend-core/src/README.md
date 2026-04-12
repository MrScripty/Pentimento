# crates/frontend-core/src

## Purpose
This directory contains shared frontend abstractions that multiple Pentimento
frontend implementations depend on at runtime.

## Contents
| File/Folder | Description |
|-------------|-------------|
| `lib.rs` | Public facade for shared frontend abstractions and contracts. |
| `native_state.rs` | Shared native frontend snapshot state and backend message application helpers. |

## Problem
Pentimento now has multiple native frontend implementations. They need a shared
place for framework-agnostic frontend state so Dioxus and egui do not each
maintain their own drifting copy of the same backend snapshot logic.

## Constraints
- Must remain lightweight enough to be depended on by multiple frontend crates.
- Must not absorb framework runtime code from Dioxus, egui, or webview hosts.
- Must continue to expose the `CompositeBackend` abstraction used by the
  capture-based frontend stack.

## Decision
Keep shared frontend contracts and native frontend state in `frontend-core`,
leaving framework-specific rendering and widget code in their own crates.

## Alternatives Rejected
- Duplicating the native snapshot state in each frontend crate: rejected because
  it would create immediate drift between Dioxus and egui.
- Moving native snapshot state into `crates/ipc`: rejected because it is a
  frontend-owned runtime convenience type, not a process-boundary contract.

## Invariants
- `frontend-core` stays free of frontend framework runtime dependencies.
- Shared native snapshot state remains derived solely from `crates/ipc`
  messages.

## Revisit Triggers
- The shared native state grows application-level orchestration logic.
- Another frontend family requires a separate state model with different
  lifecycle assumptions.

## Dependencies
**Internal:** `crates/ipc`  
**External:** `thiserror`

## Related ADRs
- `ADR-001` active frontends and contract ownership.

## Usage Examples
```rust
use pentimento_frontend_core::{apply_native_ui_message, NativeUiState};

let mut state = NativeUiState::default();
apply_native_ui_message(&mut state, &pentimento_ipc::BevyToUi::CloseMenus);
```

## API Consumer Contract
- Consumers may clone `NativeUiState` and store it inside frontend-specific
  resources or bridges.
- `apply_native_ui_message` updates only framework-neutral frontend snapshot
  state; callers own rendering and side effects.

## Structured Producer Contract
- Produces in-process snapshot values only.
- Field semantics mirror the relevant `crates/ipc::BevyToUi` messages.
- No persisted artifact or versioned external schema is produced here.
