# crates/dioxus-ui/src

## Purpose
This directory contains the Rust-native UI application used by the Dioxus frontend path.

## Contents
| File/Folder | Description |
|-------------|-------------|
| `app.rs` | Top-level Dioxus app composition and state wiring. |
| `bridge.rs` | Channel bridge between Bevy-side events and the Dioxus document. |
| `components/` | Reusable native UI widgets and panels. |
| `renderer.rs` | Renderer integration layer for the Dioxus/Blitz pipeline. |
| `document.rs` | Document model and update path used by the renderer. |

## Problem
The Dioxus frontend needs a Rust-native UI path that can track the same backend state as the browser UI without carrying Chromium or WebKit.

## Constraints
- Must align with the same IPC vocabulary as the browser frontend.
- Must cooperate with Bevy's render lifecycle and non-send resources.
- Native UI changes cannot assume browser DOM semantics.

## Decision
Keep Dioxus as a dedicated crate with a bridge/document/renderer split so the Bevy integration code in `crates/app` stays thin.

## Alternatives Rejected
- Embedding Dioxus directly inside `crates/app`: rejected because it would mix renderer internals into app orchestration.
- Separate IPC schema for Dioxus: rejected because it would reintroduce frontend drift.

## Invariants
- The bridge remains the only runtime ingress for backend-to-Dioxus messages.
- Renderer-specific resources stay inside this crate instead of leaking into generic IPC code.

## Revisit Triggers
- The document model diverges enough to justify sub-crates.
- Another native Rust UI renderer replaces Blitz/Vello.

## Dependencies
**Internal:** `crates/ipc`, `crates/app/src/render/ui_dioxus`  
**External:** Dioxus, dioxus-native, Vello

## Related ADRs
- `ADR-001` active frontends and contract ownership.

## Usage Examples
```rust
use pentimento_dioxus_ui::DioxusBridge;

let (bridge, handle) = DioxusBridge::new();
```

## API Consumer Contract
- Consumers use the public bridge and renderer entrypoints from `lib.rs`.
- Message ordering follows the Bevy event stream; no replay guarantee exists beyond the current process lifetime.
- Invalid inbound state should be rejected at the caller boundary before it reaches this crate.

## Structured Producer Contract
- This crate produces renderer-facing document updates and UI event envelopes, not persisted artifacts.
- Field names and enum variants must remain aligned with `crates/ipc`.
- Contract changes require coordinated updates in the Dioxus bridge and the browser contract mirror.
