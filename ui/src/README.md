# ui/src

## Purpose
This directory holds the Svelte entrypoint tree for the browser UI used by CEF and Electron, and for the embedded asset bundle loaded by native modes.

## Contents
| File/Folder | Description |
|-------------|-------------|
| `App.svelte` | Root composition layer for toolbar, side panel, add-object menu, and paint UI visibility. |
| `lib/` | Shared IPC bridge, contract mirrors, and reusable components. |
| `main.ts` | Browser bootstrap that mounts the app and installs dirty-marking behavior. |
| `styles/` | Global CSS loaded once for the whole frontend. |

## Problem
Pentimento needs one browser UI implementation that can survive three hosts: offscreen CEF, Electron/WASM, and the native asset embedding path used by the Bevy app.

## Constraints
- Must tolerate native IPC injection and Electron/WASM event transport.
- Must keep contract names aligned with `crates/ipc`.
- Must avoid DOM-only assumptions that break offscreen rendering.

## Decision
Keep the browser frontend rooted in a small `main.ts` + `App.svelte` entrypoint and push host-specific behavior into `lib/bridge.ts`.

## Alternatives Rejected
- Separate CEF and Electron UI trees: rejected because contract drift had already appeared with a single tree.
- Host-specific DOM patches in `main.ts`: rejected because lifecycle cleanup belongs with the bridge and components that own it.

## Invariants
- `main.ts` remains the only browser bootstrap entrypoint.
- Host detection flows through bridge abstractions, not scattered global checks in components.

## Revisit Triggers
- A second browser UI shell appears that cannot share the existing bridge.
- The app needs route-level code splitting or multiple entrypoints.

## Dependencies
**Internal:** `ui/src/lib`, `crates/ipc`, `src-electron`  
**External:** Svelte 5, Vite

## Related ADRs
- `ADR-001` active frontends and contract ownership.

## Usage Examples
```ts
import './styles/global.css';
import App from './App.svelte';

new App({ target: document.getElementById('app')! });
```

## API Consumer Contract
- Browser hosts must expose either native IPC globals or Electron/Tauri runtime markers before the bridge starts sending messages.
- `App.svelte` expects the bridge contract from `ui/src/lib`.
- Missing bridge wiring degrades to console warnings instead of throwing at startup.

## Structured Producer Contract
- The UI emits `UiDirty` and `LayoutUpdate` messages through the bridge.
- Layout region field names must stay aligned with `LayoutInfo` in `crates/ipc`.
- Contract changes require updating the Rust sample producer and the JS acceptance test.
