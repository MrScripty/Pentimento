# Electron production qualification

Status on 2026-10-13: implemented; local focused evidence passed; real renderer
acceptance remains blocked locally and pending in CI. This report does not mark
BR3 accepted.

## Source lineage

Reused the repository's existing qualification work from
`origin/review/electron44-ubuntu22-qualification`, exact commit
`ebdf529af66643017aa5097c8e7a621dcb084061` (2026-10-03):

- `.github/workflows/electron-runtime.yml`
- `tests/electron/runtime-smoke.cjs`
- `tests/electron/frame-evidence.cjs`
- `tests/electron/supervise.py`
- `tests/electron/test_supervise.py`
- `tests/contracts/electron-render.test.mjs`

This is a port of existing evidence, not a replacement canvas or smoke harness.
The production test requires the current branch's compiled Electron main and
preload plus real `dist/ui/wasm/pentimento_wasm.js` and
`pentimento_wasm_bg.wasm`; it does not substitute WASM or load a dev server.

## Adaptation to this upgrade

- Read expected Electron identity from `src-electron/package.json` instead of
  the source branch's old `44.5.1` literal. Record runtime versions and arguments.
- Use this branch's read-only `check-runtime.cjs` before starting the binary;
  preserve explicit installation through `launcher.sh --install`.
- Reuse this branch's focused shell/preflight tests instead of importing the
  older, duplicate bootstrap implementation and tests.
- Use Electron 44's current console-message event object, preserving fatal
  console/panic/context-loss rejection without deprecated argument fallback.
- Remove the source harness's invalid isolated-world evaluation of the preload's
  lexical `process` polyfill. The disposable environment preload directly
  reports `process.sandboxed` and `contextIsolated`; production independently
  checks effective web preferences, its actual minimal preload marker, and
  absence of renderer Node globals. No production test-only API is added.
  These are distinct claims, not a direct production-process sandbox attestation.
  The lexical scope is defined by
  [Electron 44.7.0's preload implementation](https://github.com/electron/electron/blob/v44.7.0/lib/sandboxed_renderer/preload.ts).
- Retain the source oracle's pixel thresholds and two canvas-only captures.
  The current `ScenePlugin` still defines the red cube, green torus, blue sphere,
  and neutral ground expected by that oracle.
- Preserve process ownership, shutdown handshake, orphan rejection, and
  PID/start-time tracking. Give each supervisor fixture its own evidence path,
  including when CI sets a shared artifact destination.
- Keep Ubuntu 22.04 and the previously selected
  `--use-gl=angle --use-angle=swiftshader` test graphics configuration. Explicitly
  reject `--enable-unsafe-swiftshader`, sandbox bypasses, and disabled web
  security. No host security setting is changed.
- Capture exact environment, lockfiles, build logs, runtime logs, JSON, and PNG
  evidence under the runner's temporary directory. Assert that all three
  application lockfiles remain unchanged through installation and build.
- Let the launcher install the matching locked `wasm-bindgen` CLI instead of
  installing an unversioned CLI first. Omit the source branch's unrelated npm
  audit gate from this rendering job; dependency advisory checks are not
  renderer-acceptance evidence.

## Evidence and limits

Passed locally:

1. `node --test tests/ui/electron-shell.test.mjs tests/contracts/electron-render.test.mjs`:
   11 tests passed, covering shell isolation/configuration and asset path,
   missing/stale binary preflight, and strict positive/negative canvas fixtures.
2. `PYTHONDONTWRITEBYTECODE=1 python3 tests/electron/test_supervise.py`:
   four real-process fixtures passed, including orphaned-child rejection and
   cleanup, missing-handshake rejection, and PID reuse distinction.
3. Electron `44.7.0` was installed explicitly, passed read-only preflight, and
   reported `v44.7.0` with exit status zero. Shell compilation/typecheck passed.
4. JavaScript syntax, workflow YAML parsing, README coverage, and whitespace
   checks passed.

Actual local environment probe used the installed binary, the retained
software-graphics switches, isolated writable XDG config/cache/data directories,
and the process supervisor. It failed before renderer creation with:

```text
Missing X server or $DISPLAY
The platform failed to initialize. Exiting.
```

The supervisor recorded main exit `-5`, no shutdown handshake, observed process
family exit, and `success: false`. No forced cleanup was needed. The executor
has neither Xvfb nor a configured display. Its available Rust toolchain has only
the native Linux target and no `wasm-bindgen` CLI, so the genuine WASM build and
production renderer test have not run locally. These missing prerequisites are
supplied by the CI workflow; no sandbox/security workaround was attempted.

CI remains pending for this branch's exact eventual commit. Passing source-
branch checks, local pixel fixtures, binary version output, or shell typechecks
do not substitute for the new commit's sandbox probe, real WASM build, strict
nonblank scene capture, and graceful process-family shutdown.

The qualified environment is software-rendered Linux x86_64. It cannot establish
hardware GPU behavior/performance, stylus responsiveness, unsupported platforms,
or the separate CEF paint/sculpt workflow claim.
