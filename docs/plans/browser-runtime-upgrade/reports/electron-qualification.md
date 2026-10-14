# Electron production qualification

Status on 2026-10-13: the complete Electron qualification passed on source head
`e368821a274bd6a544e132500887f81786126c43`, including genuine scene rendering,
two stable selection-outline frames and graceful process-family shutdown.
The earlier rendering and oracle failures remain documented in
[WebGL rendering](webgl-rendering.md). A subsequent native Canvas ordering
repair is in progress; its new source head needs its own qualification rerun.
This passing Electron slice does not by itself mark aggregate BR3 accepted.

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
- Require actual WebGL selection-outline evidence: locate a red-object interior
  from the native capture, send an Electron mouse click, park the pointer away,
  and require two stable orange-boundary frames. Keep before/after PNGs and
  input/pixel metrics; reject fills, unrelated scene changes, unchanged frames,
  and wrong-object/color borders. The combined focused suite now has 21 passing
  tests. Hosted execution passed on the source head identified below.
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

## Initial local evidence and limits

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
has neither Xvfb nor a configured display. At this initial check its Rust
toolchain also lacked the WASM target and `wasm-bindgen` CLI. Those build
prerequisites were subsequently installed and a canonical local WASM build
completed; the real local renderer remains unavailable. No sandbox/security
workaround was attempted.

The first exact-head hosted run exposed real texture-format and depth-shader
failures, as detailed in the linked WebGL report. The next run rendered correctly
but exposed a shadowed-face silhouette error in the new test. Those historical
failures were resolved before the successful full qualification below.

## Verified hosted qualification

[Run 38414235793, job 115354262783](https://github.com/MrScripty/Pentimento/actions/runs/38414235793/job/115354262783)
completed successfully on source head `e368821a274bd6a544e132500887f81786126c43`.
The retained `environment.log` records PR merge checkout
`e25f035a7d65259311421cb85f9f83954365d293`; GitHub commit metadata verifies that
its tree `64d9f9ac1a31ff9cb28b201380eb7d91a4cd9b50` equals the source head's tree.
[Artifact 11824680772](https://github.com/MrScripty/Pentimento/actions/runs/38414235793/artifacts/11824680772)
contains the inspected production/selection PNGs, JSON results, process-family
records, environment, lockfiles and build/runtime logs. The downloaded ZIP's
SHA-256 matches the artifact digest:
`956b20e6b382a7beedfdbccd45fb841f6c73f9b44d7dfa91e8dd830ca5e57e35`.

The artifact establishes:

- Electron 44.7.0 / Chromium 152.0.7977.130 on Ubuntu 22.04 Linux x86_64,
  with Xvfb and the existing ANGLE/SwiftShader software graphics switches.
  The canonical production Svelte and real Bevy WASM build passed.
- The ordinary environment preload reported `sandboxed: true` and
  `contextIsolated: true`. Production reported sandbox/isolation enabled,
  Node integration disabled, and absent renderer `process`/`require` globals.
  No sandbox bypass, unsafe SwiftShader flag or host security change was used.
- Both strict canvas captures contain the canonical red cube, green torus,
  blue sphere and neutral ground. The full production PNG visibly contains
  the genuine scene and UI; no dev server or substitute WASM was involved.
- The actual WASM depth capability reached the disabled, unpressed control
  with its visible accessible OpenGL/WebGL-unavailable reason.
- A real Electron click at DIP `(960,464)`, followed by parking the pointer
  away, produced two accepted consecutive selection frames. All 1,842 new
  orange pixels localized to the cube boundary, with zero orange interior
  pixels and zero meaningful changes among 1,225,275 outside pixels.
  The inspected after PNG shows the clean cube outline.
- `production.json` records `success: true`, `willQuit: true`,
  `wasmInitialized: true`, and empty `errors` and `network` arrays. No Bevy
  ERROR was observed. Nonfatal software-renderer warnings remain distinct.
- Both environment and production supervision record main exit 0, a completed
  shutdown handshake, observed process-family exit, no remaining processes,
  no forced cleanup, and `success: true`.

These results are tied to the stated source tree. They do not automatically
qualify the subsequent native Canvas ordering repair or any new source head;
that head still requires its own rerun.

The qualified environment is software-rendered Linux x86_64. It cannot establish
hardware GPU behavior/performance, stylus responsiveness, unsupported platforms,
or the separate CEF paint/sculpt workflow claim.
