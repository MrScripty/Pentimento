# src-electron

## Purpose
This directory contains the Electron shell that hosts the Svelte UI and Bevy WASM runtime.

## Contents
| File/Folder | Description |
|-------------|-------------|
| `check-install.cjs` | Non-downloading installed runtime/version readiness check. |
| `dist/` | Compiled Electron main/preload output consumed at runtime. |
| `main.ts` | Electron main-process window bootstrap compiled to `dist/main.js`. |
| `preload.ts` | Secure preload bridge that exposes the Electron runtime marker. |
| `check-runtime.cjs` | Read-only verification of the pinned Linux x86_64 runtime installation. |
| `package.json` | Shell-specific build and runtime scripts. |
| `tsconfig.json` | TypeScript config for the Electron shell sources. |

## Problem
Pentimento needs a Chromium host for the WASM frontend path without letting shell code drift away from the typechecked source.

## Constraints
- Must keep `sandbox: true`, `contextIsolation: true`, and `nodeIntegration: false`.
- Runtime entrypoint must be generated from the TypeScript source of truth.
- The shell has to load `dist/ui/index.html` for packaged flows.

## Decision
Use `main.ts` and `preload.ts` as runtime sources and compile them to `dist/` before launch.
The bootstrap checker is a separate CommonJS tooling script, not a duplicate
runtime entrypoint. Electron 44 requires external Node.js 22.12+; the launcher
explicitly runs the installed `install-electron` command during bootstrap.

## Alternatives Rejected
- Keeping hand-edited `.js` and `.ts` copies side by side: rejected because runtime drift already occurred.

## Invariants
- `package.json` points at `dist/main.js`.
- The preload bridge exposes only the minimal runtime marker needed by the UI.

## Revisit Triggers
- Electron needs a richer preload API than the current boolean marker.
- Packaging or code signing is added to the repo.

## Dependencies
**Internal:** `crates/app-wasm`, root `dist/ui` output, `launcher.sh`  
**External:** Electron, TypeScript

The Electron runtime is pinned to **44.7.0** (Chromium **152.0.7977.130**,
Node.js **24.21.0**), the latest stable release checked on 2026-10-13.
The host Node.js used for npm tooling must be **22.12.0 or newer**.

Electron 42 and later no longer download the runtime during npm installation.
`./launcher.sh --install` explicitly bootstraps the binary through
`npm --prefix src-electron run install:runtime` after installing the locked npm
dependencies. Build and run preflight must verify the actual runtime, rather
than only the npm CLI shim. `npm --prefix src-electron run check:runtime --silent`
checks the npm package version, binary version, platform path, and executable
file without importing Electron or triggering a download; it prints the verified
binary path on success. Do not disable Chromium's sandbox to work around a
host that cannot run it.

The shell uses `BrowserWindow`, `app`, and `contextBridge`; the Electron 39–44
API removals do not require replacing those calls. Production loading resolves
the repository's `dist/ui/index.html` from compiled `src-electron/dist/main.js`.
The preload remains a sandbox-compatible CommonJS module exposing only a boolean.

Sources: [44.7.0 release](https://releases.electronjs.org/release/v44.7.0),
[breaking changes](https://www.electronjs.org/docs/latest/breaking-changes), and
[`electron` npm metadata](https://www.npmjs.com/package/electron/v/44.7.0).

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
- Shell verification is owned by the root launcher/typecheck flow; ad hoc local test scripts are not part of the supported workflow.
- `tests/ui/electron-shell.test.mjs`, included in root `npm test`, covers the
  production asset path, development URL, window lifecycle, isolation options,
  minimal preload contract, and missing/stale runtime rejection. These focused
  tests mock Electron and do not replace sandboxed runtime smoke or rendered
  WASM acceptance.
- The [Electron qualification suite](../tests/electron/README.md) builds and
  observes the real production shell/WASM with strict canvas and sandbox checks
  in its supported CI environment.

## Structured Producer Contract
- Produces the `dist/main.js` and `dist/preload.js` artifacts consumed by Electron.
- Output paths are stable for the launcher and package metadata.
- Entry-point path changes require coordinated updates to `package.json` and launcher verification.
