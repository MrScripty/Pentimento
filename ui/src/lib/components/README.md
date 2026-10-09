# ui/src/lib/components

## Purpose
This directory holds the active Svelte components that make up Pentimento's browser UI surface.

## Contents
| File/Folder | Description |
|-------------|-------------|
| `Toolbar.svelte` | Top-level scene controls, menus, and view toggles. |
| `SidePanel.svelte` | Material, lighting, and ambient-occlusion controls. |
| `AddObjectMenu.svelte` | Keyboard-accessible add-object dialog used by the active viewport workflows. |
| `BrushPanels.svelte` | Persistent backend brush subscriptions and mode-specific panel composition. |
| `PaintBrushPanel.svelte` | Round brush/eraser, one-click canvas color sampling, presets, tip parameters, canvas undo and UV projection actions. |
| `SculptBrushPanel.svelte` | Supported deformation tools, radius, strength, hardness and falloff. |
| `BrushControl.svelte` | Accessible bounded slider plus exact numeric input. |
| `PaintToolbar.svelte` | Legacy shortcut component, no longer mounted by the active Svelte app. |

## Problem
The browser UI needs modular components for scene controls without letting each component own its own transport or host-detection logic.

## Constraints
- Components must stay compatible with offscreen capture and Electron/WASM.
- Accessibility regressions must be caught by `svelte-check --fail-on-warnings`.
- Gesture-heavy hosts mean overlays and menus must not steal input unpredictably.

## Decision
Keep components thin and bridge-driven: interaction state stays local, while backend mutations flow through the shared bridge.

## Alternatives Rejected
- One giant component tree file: rejected because review and accessibility fixes become too hard to localize.
- Transport logic inside each component: rejected because it recreates contract drift and lifecycle bugs.

## Invariants
- Interactive controls use semantic buttons/inputs and explicit accessible names.
- Components do not talk to host globals directly.

## Revisit Triggers
- A component exceeds reviewable size again and needs further decomposition.
- UI state starts duplicating bridge-owned backend state.

## Dependencies
**Internal:** `ui/src/lib/bridge.ts`, `ui/src/lib/types.ts`  
**External:** Svelte 5

## Related ADRs
- `ADR-001` active frontends and contract ownership.

## Usage Examples
```svelte
<Toolbar {renderStats} mode={editMode} />
<SidePanel />
<AddObjectMenu {show} {position} onClose={closeMenu} />
```

## API Consumer Contract
- Parent components provide current visibility and callback props where required.
- Menu/dialog components own focus entry and restoration for their own subtree.
- Outbound backend mutations always flow through `bridge`.

## Structured Producer Contract
- None identified as of 2026-03-08.
- Reason: components consume bridge contracts but do not produce independently persisted structured artifacts.
- Revisit trigger: a component starts generating saved layouts, presets, or other machine-consumed data.

Brush controls never reset backend settings on mount. `RequestBrushState` restores current values after reload; authoritative snapshots also reflect keyboard adjustments. See [brush panels](../../../../docs/brush_panels.md) for support scope and testing.

The Edit menu consumes the same authoritative edit mode and history availability
as the brush panels. Undo/Redo emit one existing paint or sculpt command and are
disabled for unsupported modes, empty history and active sculpt transactions.
Paint availability already includes the backend's active-transaction refusal.
Native shortcuts remain native; the menu adds no browser shortcut handler.
