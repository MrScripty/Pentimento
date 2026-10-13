# Electron runtime qualification

This Linux x86_64 suite qualifies the installed Electron runtime and genuine
production Svelte/Bevy WASM assets. It supplements the focused shell tests in
`tests/ui/electron-shell.test.mjs`; simulated shell tests cannot establish
rendering or OS sandbox operation.

## Ownership and source

The suite is adapted from repository branch
`review/electron44-ubuntu22-qualification`, commit
`ebdf529af66643017aa5097c8e7a621dcb084061`. Its lineage, changes, and current
verification status are recorded in
[the qualification report](../../docs/plans/browser-runtime-upgrade/reports/electron-qualification.md).

- `runtime-smoke.cjs` launches either an isolated renderer probe or the actual
  compiled `src-electron/dist/main.js`. It records sandbox/preload state,
  production document identity, real WASM initialization, console failures,
  screenshots, and shutdown readiness.
  The environment probe observes `process.sandboxed` and `contextIsolated`
  inside its disposable preload. Production checks effective web preferences,
  successful real preload execution, and absent renderer `process`/`require`;
  it does not add privileged test APIs to the production preload.
- `frame-evidence.cjs` checks canvas-only BGRA captures against the canonical
  scene's red cube, green torus, blue sphere, and neutral ground/background.
  The UI overlay is hidden only while taking these captures. Blank, grayscale,
  transparent, or missing-object frames cannot satisfy the oracle.
  It also locates the real red object's interior for an Electron mouse click.
  After the pointer leaves, two captures must show new orange pixels concentrated
  on that object's original boundary, without an orange fill or unrelated scene
  changes. `selection-before.png`, `selection-after.png`, and JSON coordinates
  and pixel counts retain the evidence. Synthetic fixtures reject unchanged,
  wrong-object, wrong-color, fill, and global-tint frames.
- `runtime-errors.cjs` recognizes fatal browser logs and Bevy's formatted ERROR
  records, which its browser subscriber emits through `console.info`.
- `depth-evidence.cjs` requires the real WASM capability response: a disabled,
  unpressed depth control with a visible accessible OpenGL/WebGL reason.
  A control still waiting for bootstrap does not pass.
- `supervise.py` owns the Linux process family through exit. PID/start-time
  identity and subreaping prevent orphaned children from being mistaken for
  successful shutdown. Forced cleanup is failure, never success.
- `test_supervise.py` tests the supervisor with real disposable subprocesses.
- `../contracts/electron-render.test.mjs` provides positive and negative pixel
  fixtures for the scene oracle.

## Local focused checks

```bash
node --test tests/ui/electron-shell.test.mjs tests/contracts/electron-render.test.mjs
PYTHONDONTWRITEBYTECODE=1 python3 tests/electron/test_supervise.py
```

## Qualified runtime procedure

The authoritative CI procedure is
[`electron-runtime.yml`](../../.github/workflows/electron-runtime.yml). It uses
Ubuntu 22.04, a non-root account, Xvfb at 1920×1080×24, and explicitly selected
ANGLE/SwiftShader software graphics. It does not disable Electron's sandbox,
web security, or host security settings and does not enable unsafe SwiftShader
WebGL fallback. Missing graphics or sandbox capability blocks the qualification.

After `./launcher.sh --install` and `./launcher.sh --build --frontend electron`,
the equivalent production invocation is:

```bash
export PENTIMENTO_EVIDENCE_DIR="$(mktemp -d)"
electron="$(node src-electron/check-runtime.cjs)"
env -u ELECTRON_RUN_AS_NODE -u VITE_DEV_SERVER_URL \
  xvfb-run -a -s '-screen 0 1920x1080x24 -nolisten tcp' \
  python3 tests/electron/supervise.py production "$electron" \
  --use-gl=angle --use-angle=swiftshader tests/electron/runtime-smoke.cjs
```

Run the same command with phase `environment` and appended `--preflight` before
the production build to check basic sandbox operation. Use the supervisor for
both phases: direct execution intentionally fails its shutdown handshake.
Evidence is written to `PENTIMENTO_EVIDENCE_DIR`, or `electron-runtime-evidence`
under the working repository when unset. CI retains environment information,
the resolved lockfiles, build/runtime logs, JSON results and scene PNGs.

This is software-rendered startup/scene, selection-outline, and shutdown qualification. It does not
establish hardware GPU performance, stylus behavior, or paint/sculpt workflows.
