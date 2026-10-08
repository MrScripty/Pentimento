# Paint and sculpt integration qualification

The canonical user surface is the native CEF host with Svelte controls. CPU
engine tests, a rendered browser with a mocked IPC transport, a headless Bevy GPU
probe, and the complete CEF app establish different things. Passing one does not
qualify the others. Keep the PR in draft until the required hosted checks and
native acceptance below have run against its exact final commit.

## Scope and lineage

This integration combines the topology/UV-corner repair, projection engine and
backend-owned paint/sculpt panels. A separately attributed brush follow-up also
repairs current-input spacing and fixed packet origins; historical packet replay
is still unqualified. It includes the separately attributed PR #15
pixel-coverage winding prerequisite. PR #2's normal-packet decoder correction is
not a dependency: live sculpt deformation receives the original input normal,
not the decoded packet. The decoder/replay repair is excluded and must not be
reported as fixed by this integration.

The original public PR18 baseline did not implement sculpt history or atomic
cancellation. The separately attributed Dot reconstruction now implements local,
bounded per-stroke snapshot undo/redo, complete checkpoint cancellation, current-
policy detached restore validation, and authoritative history controls. See
[the reconstruction lineage and contract](sculpt-history-dot-reconstruction.md).
Its CPU tests establish geometry/state behavior; they do not establish a native
CEF visual pass. The unrecovered cloud candidate `1a4601a` remains a separate
artifact and its results must not be attributed to this reconstruction. Snapshot
history does not qualify historical packet replay.

## Repeatable engine and frontend gates

Use an isolated checkout at the commit being qualified. Do not share a Cargo
build directory concurrently between checkouts. If reusing one sequentially,
clean the workspace path crates first so prior-checkout artifacts cannot satisfy
tests. Retain third-party dependency artifacts if desired.

```sh
export CARGO_BUILD_JOBS=1
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_INCREMENTAL=0
./scripts/test-paint-sculpt-tools.sh --engine-only
./scripts/test-paint-sculpt-tools.sh --frontend-only
```

The first command runs the complete painting and sculpting libraries, the real
geometry/pipeline integration tests, the complete sculpt-enabled scene library
(including projection and panel command tests), and IPC example compilation. Discovery guards
prevent accidental omission of the major suites. A hosted Ubuntu 24.04 workflow
runs this engine gate without requiring a display or native CEF dependencies.
The frontend command runs README coverage, active Rust formatting, Svelte
accessibility, Svelte/Electron typechecking, the real Rust/JS IPC contract tests,
pure UI tests, and a production Svelte build. Frontend dependencies must already
be installed. Neither command installs dependencies or modifies dependency policy.

The npm install blocker is repaired by checked root and src-electron lockfiles,
without changing declared or already installed dependency versions. Fresh
`npm ci` lock consistency was verified in isolated directories. This does not
itself qualify the aggregate multi-frontend Rust job. The dedicated native CEF
workflow and its exact coverage are described in tests/native/README.md.

## Browser-rendered control gate

On a permitted Linux host with Node 22+, repository npm dependencies and an
installed Chromium executable:

```sh
npm run dev -- --host 127.0.0.1 --port 5187
# In another shell, from this checkout:
CHROMIUM_PATH=/usr/bin/chromium \
PENTIMENTO_EVIDENCE_DIR=/tmp/pentimento-brush-evidence \
  npm run test:ui:browser
```

This launches and inspects the actual Svelte controls, captures JPEG quality 85,
and verifies emitted commands with a mocked IPC bridge. It covers numeric input,
color, presets, erase/undo, live/apply, all sculpt tools, falloff, backend snapshots,
mode remounting, keyboard capture, layout regions and narrow windows. Sculpt
history cases cover authoritative undo/redo availability, transaction lockout and
history-limit notices. These browser cases are distinct from real engine history.
It does not
prove native input arbitration or any 3D rendering. Do not bypass denied browser
socket/security restrictions; record the blocked stage and run on an allowed host.

## Native CEF gate

Use the supported Linux x86_64 machine with a working display and Vulkan-capable
renderer. Install the Linux prerequisites listed in README (including GTK, ALSA,
udev and Wayland development libraries), Node 22+, Rust, and the required CEF
runtime through the canonical setup. Use the checked npm lockfiles for a fresh installation. Then:

```sh
./launcher.sh --build --frontend cef
./launcher.sh --run --frontend cef
```

Record commit, OS, GPU/driver, CEF version, window scale and command exit status.
Capture display evidence as JPEG quality 85 outside the repository. Inspect each
capture; logs alone cannot establish correct visible output.

1. Create a source canvas and UV sphere. Paint distinct asymmetric color marks
   in all four quadrants. Apply once and inspect UV orientation. Include front
   and rear objects, a non-UV object, and front/back/no-culling materials.
   Culled faces must neither receive paint nor occlude visible eligible surfaces.
2. Enable live projection. Draw at least two new strokes and confirm the rendered
   mesh changes after each, not just the CPU image asset. Change opacity, erase,
   undo, and verify stale live pixels disappear. Disable live projection; new
   strokes should remain unapplied until Apply is pressed. Repeated Apply must
   not increase opacity on an unchanged source.
3. Pan/zoom the canvas, leave and return to canvas view, and switch canvases.
   Verify source placement, brush state and active-canvas undo. Inspect existing
   material texture/color handling and alpha edges against a contrasting base.
4. Select the sphere and enter sculpt mode with Ctrl+Tab. Compare all eight
   tools, small/large radius, strength endpoints, soft/hard falloff and each curve.
   Exercise F and Shift+F adjustments. Check deformation at seams and poles.
5. Make a stroke that changes topology; immediately exit sculpt. Confirm the
   final deformation remains, re-enter, and inspect UV corners and connectivity.
   Verify the same customized brush state remains selected. Try repeated exits,
   mode switches while a stroke is active, and pointer/focus interruption.
   While sculpting, plain Tab and Add Canvas must not activate another brush.
   Rejected mode entries must show a dismissible error even outside paint/sculpt.
6. For sculpt history, establish a visible deformation, then undo and redo it.
   Capture at least three unchanged frames at each endpoint to calibrate pixel
   noise. Check restoration over the pixels that actually deformed, and require
   an actual transition from the prior endpoint. A small whole-viewport average
   difference alone cannot establish restoration. While a new stroke is still
   active, first establish visible deformation, then press Escape and verify its
   complete rollback with the same regional/noise checks. Undo and make a new
   accepted stroke; redo must become unavailable. Verify history buttons remain
   disabled during active transactions. At release and Exit, a final rejected
   stroke must roll back and emit `sculpt_stroke_rejected` to the user.
7. Drag every slider off the panel into the viewport and release both inside and
   outside it. No paint/sculpt stroke should start from the widget drag. Focus
   numeric/color controls and press shortcuts; viewport tools should stay idle.
   Confirm F/Shift+F adjustments with a click held into a drag; that press must
   never become a sculpt stroke. Resize and test a scaled display. Check for
   clipped controls and stale capture.

A native failure is a release blocker even if the mocked browser harness or CPU
suites pass. Report exact failing step, input sequence, expected/actual behavior,
and the inspected image or log. Do not merge temporary visualization branches.

## Native pointer and keyboard ordering

Native forwarding consumes Bevy's ordered `WindowEvent` stream. Typed messages
remain available to scene and egui readers. Consecutive pointer moves coalesce,
but each button, wheel, keyboard or focus-loss event flushes its preceding move.
Buttons and scroll packets therefore use their chronological coordinates;
keyboard events retain their position relative to focus-changing clicks.
Idle hover remains throttled to 16 ms; held drags and event barriers flush sooner.

A button pressed in a reported UI rectangle owns capture until that same button
releases. Other button releases cannot clear it. Capture blocks the entire frame
containing a UI gesture, including a complete down/up followed by viewport moves.
Hover alone does not latch a later viewport-origin press. Focus loss releases
held frontend buttons, clears local ownership/AltGraph state, and blocks browser
scene pointer input until window focus returns. egui retains
ownership of its own pointer/keyboard flags.

CEF/Capture/Overlay block scene pointer input during layout bootstrap. The shared
UI sends initial rectangles immediately and coalesces later RAF/timer refreshes;
the bridge sends the latest pending rectangles within its 16 ms timer without
continually restarting that timer. Teardown cancels both refresh paths and guards
queued focus callbacks. Native diagnostics identify receipt of the first layout.
Dioxus and egui do not wait for the Svelte reporter.

Focused Bevy input regressions cover sliders, buttons, text focus, fast movement,
panel-boundary drags, independent buttons, release-frame capture, wheel origins,
DPI scaling, focus loss, typed-reader preservation and duplicate forwarding.
JavaScript tests cover stalled RAF, fallback races, latest-layout delivery,
bootstrap replay and cleanup. These regressions supplement the actual native
CEF suite; its original assertions and input sequence remain the qualification
criteria. The history extension preserves all 16 original native assertion lines
and adds undo, redo, cancellation and redo-branch checks. History restoration
also requires deformation-region motion and return within measured unchanged-
frame noise; cancellation cannot pass without a visibly deformed active stroke.
Synthetic oracle regressions prove that no-op and partial restoration fail these
criteria, but they do not replace executing the actual CEF application. Native CEF
qualification remains open until that exact final build passes on a permitted host.
