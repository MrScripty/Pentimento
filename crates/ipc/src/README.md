# crates/ipc/src

## Purpose
This directory defines the shared frontend/backend contract used by the active CEF, Dioxus, and Electron paths.

## Contents
| File/Folder | Description |
|-------------|-------------|
| `messages.rs` | Top-level `BevyToUi` and `UiToBevy` enums. |
| `commands/` | Command enums for camera, gizmo, paint, and mesh-edit actions. |
| `types/` | Structured payload types for scene data, settings, and materials. |
| `input.rs` | Shared serialized input events used by frontend hosts. |
| `error.rs` | Contract-layer error type. |

## Problem
All active frontends need one shared vocabulary for messages, settings, and input events or the codebase immediately drifts across languages and hosts.

## Constraints
- Rust is the current source of truth.
- TypeScript consumers must mirror the same field names and enum labels.
- Changes ripple across Bevy, Svelte, Dioxus, Electron, and tests.

## Decision
Keep the shared schema in this crate and require every consumer update to flow from here outward.

## Alternatives Rejected
- Frontend-specific schemas: rejected because the browser and Dioxus paths already share most semantics.
- JSON literals embedded in app code: rejected because review cannot spot drift reliably.

## Invariants
- `messages.rs` remains the top-level contract entrypoint.
- Stable field names are coordinated with `ui/src/lib/types.ts`.

## Revisit Triggers
- A code generator replaces the current manual TypeScript mirror.
- Persisted artifacts require versioned schema migration.

## Dependencies
**Internal:** `ui/src/lib/types.ts`, `crates/app`, `crates/dioxus-ui`  
**External:** serde, serde_json

## Related ADRs
- `ADR-001` active frontends and contract ownership.

## Usage Examples
```rust
use pentimento_ipc::{BevyToUi, UiToBevy};

let msg = UiToBevy::SetDepthView { enabled: true };
```

## API Consumer Contract
- Consumers serialize and deserialize `BevyToUi` and `UiToBevy` exactly as defined here.
- Unknown or malformed payloads should be rejected at the boundary before state mutation.
- Compatibility is maintained by updating the TypeScript mirror and acceptance sample in lockstep.

## Structured Producer Contract
- `serde(tag = "type", content = "data")` is the stable message envelope for active frontends.
- Enum labels and field names are part of the consumer contract.
- When the contract changes, update `ui/src/lib/types.ts`, `crates/ipc/examples/contract_samples.rs`, and `tests/contracts/ipc-contract.test.mjs`.
