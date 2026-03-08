# crates/ipc/src/commands

## Purpose
This directory groups the command enums that carry frontend intent back into Bevy.

## Contents
| File/Folder | Description |
|-------------|-------------|
| `mod.rs` | Shared command re-exports plus camera/object/material commands. |
| `gizmo.rs` | Transform-gizmo mode and axis commands. |
| `mesh_edit.rs` | Mesh-edit mode, selection, and tool commands. |
| `paint.rs` | Paint canvas, brush, and layer-stack commands. |

## Problem
Frontend input needs distinct command families without overloading one giant enum file.

## Constraints
- Command families must remain serializable through the top-level IPC envelope.
- Enum labels are consumed directly by TypeScript and the native Dioxus bridge.

## Decision
Split specialized command domains into focused files and re-export them from `mod.rs`.

## Alternatives Rejected
- One monolithic commands file: rejected because paint, gizmo, and mesh editing already evolve independently.

## Invariants
- Command names remain stable unless all consumers are updated together.
- Domain-specific enums stay under this directory rather than leaking into unrelated crates.

## Revisit Triggers
- Another domain grows large enough to deserve a dedicated command file.
- Generated bindings make the current manual split unnecessary.

## Dependencies
**Internal:** `crates/ipc/src/messages.rs`, `ui/src/lib/types.ts`  
**External:** serde

## Related ADRs
- `ADR-001` active frontends and contract ownership.

## Usage Examples
```rust
use pentimento_ipc::PaintCommand;

let command = PaintCommand::SetLayerOpacity { layer_id: 2, opacity: 0.5 };
```

## API Consumer Contract
- Consumers only see these commands when wrapped by `UiToBevy`.
- Enum labels are stable consumer-facing strings once serialized.
- Invalid command payloads should be rejected before mutating scene state.

## Structured Producer Contract
- Serialized command variants use serde enum tagging semantics.
- Variant names such as `SetDepthView`, `SetTool`, and `AddLayer` are consumer-visible.
- Renames require synchronized updates across Rust, TypeScript, and contract tests.
