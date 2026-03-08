# ui/src/styles

## Purpose
This directory contains global browser styling that sets the baseline look and input affordances for the Svelte frontend.

## Contents
| File/Folder | Description |
|-------------|-------------|
| `global.css` | Shared theme, layout, and base-element styling loaded from `main.ts`. |

## Problem
The browser UI needs one global style layer for overlays, panels, and typography before component-level styling takes over.

## Constraints
- Must remain safe for offscreen rendering and Electron `file://` loading.
- Must not remove focus indicators without replacing them.

## Decision
Keep global styles minimal and push component-specific behavior back into the owning component files.

## Alternatives Rejected
- Duplicating base styles per component: rejected because it makes offscreen rendering regressions harder to audit.

## Invariants
- Focus-visible states remain present somewhere in the stack.
- Global styles do not encode host-specific behavior.

## Revisit Triggers
- Theme tokens or typography need to be shared with the Dioxus frontend.
- Global styles start accumulating component-specific overrides.

## Dependencies
**Internal:** `ui/src/main.ts`  
**External:** None

## Related ADRs
- `ADR-001` active frontends and contract ownership.

## Usage Examples
```ts
import './styles/global.css';
```

## API Consumer Contract
- None identified as of 2026-03-08.
- Reason: this directory exposes CSS only and has no callable API surface.
- Revisit trigger: shared style tooling or theme exports become machine-consumed by other packages.

## Structured Producer Contract
- None identified as of 2026-03-08.
- Reason: the directory does not generate manifests, schemas, or persisted metadata.
- Revisit trigger: design tokens become generated artifacts consumed outside the browser bundle.
