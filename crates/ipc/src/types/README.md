# crates/ipc/src/types

## Purpose
This directory contains the structured payload types carried inside the top-level IPC messages.

## Contents
| File/Folder | Description |
|-------------|-------------|
| `scene.rs` | Scene graph, transforms, layout regions, and add-object payloads. |
| `settings.rs` | App settings, lighting, ambient occlusion, diffusion, and node graph payloads. |
| `material.rs` | Material properties and texture slot metadata. |
| `mod.rs` | Public type re-exports. |

## Problem
The frontend contract needs structured payloads that can be shared across multiple message variants without duplicating field definitions.

## Constraints
- Field names are mirrored by TypeScript consumers.
- Defaults in Rust must remain meaningful for native and browser frontends.

## Decision
Keep payload structs organized by domain and re-export them through `mod.rs`.

## Alternatives Rejected
- Embedding anonymous JSON blobs in messages: rejected because it hides field semantics and default behavior.

## Invariants
- Payload structs stay serializable with serde.
- Stable field names remain coordinated with `ui/src/lib/types.ts`.

## Revisit Triggers
- The schema becomes versioned or generated.
- A payload domain grows large enough to merit a nested module tree.

## Dependencies
**Internal:** `crates/ipc/src/messages.rs`, `ui/src/lib/types.ts`  
**External:** serde, serde_json

## Related ADRs
- `ADR-001` active frontends and contract ownership.

## Usage Examples
```rust
use pentimento_ipc::LightingSettings;

let settings = LightingSettings::default();
```

## API Consumer Contract
- Consumers receive these structs only through serialized IPC messages.
- Optional fields and defaults are defined here and mirrored in TypeScript.
- Consumers should treat missing required fields as invalid payloads.

## Structured Producer Contract
- Struct field names such as `moon_phase`, `azimuth_angle`, and `pollution` are stable contract fields.
- Default semantics live in Rust and must remain mirrored in the UI bridge.
- Contract updates require synchronized Rust, TypeScript, and sample-test changes.
