# crates/dioxus-ui/src/components

## Purpose
This directory contains the reusable Dioxus widgets that make up the native Rust frontend.

## Contents
| File/Folder | Description |
|-------------|-------------|
| `toolbar.rs` | Top toolbar for global actions and mode toggles. |
| `side_panel.rs` | General scene/material controls for the native UI path. |
| `paint_side_panel.rs` | Paint-layer, brush, and projection controls. |
| `paint_toolbar.rs` | Paint-mode toolbar actions. |
| `slider.rs` | Shared slider primitive used by multiple controls. |

## Problem
The Dioxus path needs componentized native UI pieces that can evolve without turning the whole native frontend into one large file.

## Constraints
- Components must consume bridge state instead of inventing their own backend contract.
- Native widgets must remain renderable by Blitz without browser assumptions.

## Decision
Keep the native UI split into small, domain-oriented components and share only the minimum primitives such as sliders and pickers.

## Alternatives Rejected
- Mirroring the Svelte component tree file-for-file: rejected because the native renderer has different composition pressure.

## Invariants
- Components remain bridge-driven and do not own transport setup.
- Shared primitives stay generic enough for multiple panels.

## Revisit Triggers
- One panel grows large enough to justify its own submodule.
- A shared primitive starts embedding domain-specific backend knowledge.

## Dependencies
**Internal:** `crates/dioxus-ui/src/app.rs`, `crates/ipc`  
**External:** Dioxus

## Related ADRs
- `ADR-001` active frontends and contract ownership.

## Usage Examples
```rust
rsx! {
    toolbar::Toolbar {}
    paint_side_panel::PaintSidePanel {}
}
```

## API Consumer Contract
- Components are consumed by the native Dioxus app tree, not by external callers.
- Props and state flow remain internal to this crate.
- Reuse assumes the caller already owns the bridge/document lifecycle.

## Structured Producer Contract
- None identified as of 2026-03-08.
- Reason: these components consume state and emit in-process callbacks only.
- Revisit trigger: a component starts producing serialized presets or saved panel layouts.
