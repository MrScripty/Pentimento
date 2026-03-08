# src-electron

## Purpose
This directory contains the Electron shell that hosts the Svelte UI and Bevy WASM runtime.

## Contents
| File/Folder | Description |
|-------------|-------------|
| `main.ts` | Electron main-process window bootstrap compiled to `dist/main.js`. |
| `preload.ts` | Secure preload bridge that exposes the Electron runtime marker. |
| `package.json` | Shell-specific build and runtime scripts. |
| `tsconfig.json` | TypeScript config for the Electron shell sources. |

## Problem
Pentimento needs a Chromium host for the WASM frontend path without letting shell code drift away from the typechecked source.

## Constraints
- Must keep `contextIsolation: true` and `nodeIntegration: false`.
- Runtime entrypoint must be generated from the TypeScript source of truth.
- The shell has to load `dist/ui/index.html` for packaged flows.

## Decision
Use `main.ts` and `preload.ts` as the only source files and compile them to `dist/` before launch.

## Alternatives Rejected
- Keeping hand-edited `.js` and `.ts` copies side by side: rejected because runtime drift already occurred.

## Invariants
- `package.json` points at `dist/main.js`.
- The preload bridge exposes only the minimal runtime marker needed by the UI.

## Revisit Triggers
- Electron needs a richer preload API than the current boolean marker.
- Packaging or code signing is added to the repo.

## Dependencies
**Internal:** `crates/app-wasm`, `ui/dist` output, `launcher.sh`  
**External:** Electron, TypeScript

## Related ADRs
- `ADR-001` active frontends and contract ownership.

## Usage Examples
```bash
npm --prefix src-electron run build
./launcher.sh --run --frontend electron
```

## API Consumer Contract
- The shell is launched by `launcher.sh` after the UI bundle and WASM artifacts exist.
- Renderer code should rely on the preload marker rather than enabling Node integration.
- Missing compiled shell artifacts should be treated as a build error, not auto-generated at runtime.

## Structured Producer Contract
- Produces the `dist/main.js` and `dist/preload.js` artifacts consumed by Electron.
- Output paths are stable for the launcher and package metadata.
- Entry-point path changes require coordinated updates to `package.json` and launcher verification.
