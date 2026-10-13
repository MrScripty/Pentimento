# Native CEF presentation readiness

Checked 2026-10-13 UTC against PR #27 head
`93f67c7d86c045cf298395b172acb0aa1ece14a6`.

## Observed failure

- [Native qualification run 38393731625](https://github.com/MrScripty/Pentimento/actions/runs/38393731625)
  failed at `cef_framebuffer_readiness` with `toolbar: native framebuffer does
  not show the UI region (0/92160 painted pixels)`.
- The retained [workflow artifact 11812395367](https://github.com/MrScripty/Pentimento/actions/runs/38393731625/artifacts/11812395367)
  is named `native-cef-a4d8e380e52776439cbcff37f6fcb361323a87e9`; the artifact
  metadata identifies the source head above. The artifact name uses the tested
  pull-request merge SHA.
- `app.log` records CEF readiness at 12:11:31.770925 UTC, a fully transparent
  initial capture, and a 1920×1080 capture with 396,928 nontransparent pixels at
  12:11:33.754626 UTC. DOM controls and layout also became ready.
- `native-startup.jpg` is entirely black. `failure.jpg`, captured subsequently
  by the same native X11 path in the error handler, visibly contains the toolbar,
  side panel, and scene wireframes. Both images were inspected. As a diagnostic
  cross-check, decoded JPEG toolbar pixels above intensity 15 were 0/92,160 in
  the startup image and 91,920/92,160 in the later image. Qualification continues
  to use uncompressed native RGB pixels, not JPEG-derived measurements.
- The runner used Ubuntu 24.04, Xvfb, and Vulkan llvmpipe from Mesa
  25.2.8. The configuration log confirms the CEF software rendering arguments
  `--disable-gpu --disable-gpu-compositing` were passed.

## Diagnosis and limits

The old failure is a startup presentation-readiness race. It is not evidence
that CEF permanently fails to render the toolbar: the subsequent failure image
shows it rendered. The initially black image includes the whole native window,
not just the browser overlay.

The production path is `crates/webview/src/platform_linux_cef.rs`, through
`CefWebview` in `crates/webview/src/lib.rs`, to
`crates/app/src/render/mod.rs::upload_texture_data`. The CEF paint callback
copies BGRA into an owned buffer; Bevy receives that buffer as image data.
The first-painted receipt is emitted after assigning `image.data`. This proves
the CPU image was populated, not that Bevy's GPU work reached the X11 surface.
The main-thread parked-pointer receipts likewise do not prove presentation.

Native rendering startup, including asynchronous pipeline compilation on the
software adapter, is a plausible source of the delay. The retained logs do not
identify the exact render-thread or driver operation responsible. No CEF
runtime fix or graphics-driver fix is claimed from this evidence.

`PENTIMENTO_CEF_RENDERING` already works in the qualification harness: it is
translated to argv by `cefRenderingArguments`, forwarded through `launcher.sh`,
and consumed by `cef::args::Args::new()`. Adding Rust environment handling is
unnecessary for this path. BGRA and the full-frame copy agree with the
[official CEF RenderHandler contract](https://github.com/chromiumembedded/cef/blob/master/include/cef_render_handler.h).
The installed CEF 154 bindings initialize ABI structure sizes in their
`Default` implementations; this investigation found no justification for
changing those defaults to address the observed blank startup frame.

## Bounded correction

The startup harness retains its existing first-painted CEF receipt and four
fresh parked-pointer update barrier. It then waits for the actual native X11
toolbar to satisfy the existing pixel assertion, under one 30-second condition
deadline. Repeated captures do not start another pointer-barrier deadline.

The original oracle is unchanged: an RGB pixel is painted only when its maximum
channel exceeds 15, and more than 15% of the sampled visible UI region must be
painted. The passing screenshot and pixel statistics come from the same frame.
An always-blank surface still fails. Later sculpt, paint, history, native input,
panel, and menu assertions are unchanged. No sandbox, certificate, origin,
network, or other security setting changed.

Changed files:

- `tests/native/readiness.mjs`: bounded presentation helper reusing the existing
  assertion and wait function.
- `tests/native/cef-paint-sculpt.mjs`: startup awaits visible native toolbar
  presentation; pointer parking is factored without changing its contract.
- `tests/ui/native-readiness.test.ts`: delayed presentation and always-blank
  deadline regressions. The delayed fixture rejects both black and exactly
  15%-painted frames before accepting a frame above the original threshold.

## Verification

- `node --test --experimental-strip-types tests/ui/native-readiness.test.ts`:
  17 tests passed, including the two new regressions.
- `node --check tests/native/cef-paint-sculpt.mjs`: passed.
- `git diff --check` for the three changed test files: passed.

These deterministic helper tests prove the polling/oracle contract only. They
do not replace native qualification of the upgraded CEF runtime. The full
actual-app X11 workflow must pass on the final commit, and its screenshots and
result receipts must be inspected before native acceptance is reported.

## CEF 154 first-head qualification and wheel correction

The first runtime-upgrade head was
`719ac6bfcf7a78035f3be8b22ee76bbca8b0c73e`, tested as pull-request merge
`a5a2283e4d17ff7830de2d53112f1199cfef3649` in
[run 38404508780](https://github.com/MrScripty/Pentimento/actions/runs/38404508780).
The [retained artifact 11818848263](https://github.com/MrScripty/Pentimento/actions/runs/38404508780/artifacts/11818848263)
contains the exact resolved lockfile, which records both `cef` and
`cef-dll-sys` as `154.5.0+154.0.34`.

Observed results:

- The canonical app and subprocess helper compiled successfully; native input
  and framebuffer-receipt tests passed.
- The corrected startup condition passed with 92,160/92,160 painted toolbar
  pixels. The startup, failure, and before/after sculpt images were inspected.
- Native selection, sculpt entry, Grab selection, radius changes, rendered
  sculpt controls (302,309/302,400 pixels), and widget-drag isolation passed.
- Sculpt transaction 0 completed as accepted with 1,216 faces, with 19,634
  changed native pixels. These establish real CEF 154 operation through the
  native input, IPC, backend, and presented-frame path for those actions.
- Qualification then failed at `sculpt_history` with `Timed out: native wheel
  scroll to requested brush control`. History actions, subsequent painting,
  and the complete workflow did not pass. These results do not qualify later
  renderer/depth changes or the final combined head.

The failure image shows the scrollable brush panel still at its top, with the
history controls below the visible area. The input-boundary direction error is
established by the source chain:

1. X11 wheel button 5 produces Winit vertical line delta -1. Bevy preserves it.
2. `crates/app/src/input/mouse.rs` scales line deltas by 40, then negates the
   vertical value for Pentimento's DOM-style routing, producing +40 for down.
3. Both CEF adapters previously forwarded that +40 directly to CEF.
4. The [official cefclient Linux OSR ScrollEvent implementation](https://github.com/chromiumembedded/cef/blob/master/tests/cefclient/browser/browser_window_osr_gtk.cc)
   uses -40 for down and +40 for up. Thus a request to scroll down was forwarded
   as up, consistent with the observed panel remaining at the top.

The bounded fix adds `frontend-core::cef_mouse::wheel_deltas`, used by both the
active webview and standalone CEF adapters. It negates the vertical float
before integer conversion. Horizontal direction and magnitude, global event
routing, scroll visibility assertions, input mechanisms, deadlines, and all
pixel thresholds remain unchanged. The native harness additionally retains
passive DOM wheel receipts and scroll position diagnostics; it never dispatches
DOM events or changes scroll position through JavaScript.

Focused verification after the correction:

- `cargo test --locked -p pentimento-frontend-core cef_mouse:: -- --nocapture`:
  3 tests passed. Coverage includes up/down, unchanged horizontal direction,
  fractional pixel deltas, zero, and safe saturation for an extreme delta.
- Direct `rustc --test` execution of the same helper: 3 tests passed.
- Native-readiness JavaScript regressions: 17 tests passed.
- JavaScript syntax, new-helper Rust formatting, and affected-file diff checks:
  passed.

The old run has no DOM wheel receipts, so it cannot independently show every
wheel event received by the renderer. The source direction error is fixed, but
actual scrolling, history, painting, and final-head native acceptance remain
unverified until the next complete native qualification run is inspected.

## Added native Vulkan depth qualification

The combined renderer/capability changes also require actual native depth
rendering evidence. The CEF harness now performs two physical X11 toolbar
enable/disable cycles before sculpting, restoring depth off before the original
sculpt and paint scenarios.

Before enabling depth, it establishes a stable, nonblank and spatially varied
scene across three consecutive native captures. Native defaults deliberately
enable the wireframe overlay and atmospheric lighting with EV13 exposure;
WASM omits those features. Therefore the native baseline does not impose WASM's
material color-population assumptions. It requires distributed visible scene
pixels, not an early toolbar-only image or uniform clear surface.
The baseline captures and each enabled/disabled native image are retained.

For each toggle, the toolbar's `aria-pressed` is the state acknowledgement: the
depth controller updates it only on backend `DepthViewState`, not optimistically
on click. Presentation is established independently. The scene-only pixel
oracle excludes toolbar, panels, and the parked cursor, and requires:

- more than 5% visibly gray pixels and at least four meaningfully populated
  intensity bins, excluding a black/flat clear surface;
- less than 1% chromatic pixels, excluding the ordinary colored scene; and
- more than 5% of scene pixels changed from the established off-state.

Disabling depth must restore the native scene and the same pixels
that changed, within the existing unchanged-frame-noise-calibrated restoration
oracle. Two cycles exercise repeat restoration without relaxing the existing
sculpt/history assertions. Each new readiness/presentation condition uses the
existing 30-second deadline. Input remains X11; DOM work only reads state and
bounds or installs passive diagnostics.

`tests/ui/native-depth-pixels.test.mjs`, native readiness regressions, and the
existing history oracle tests passed together: 28/28. Negative coverage rejects
toolbar-only/flat baselines, blank/flat depth, ordinary color frames,
unchanged grayscale, and overlay-only changes. These are supporting tests;
native Vulkan depth compilation, rendered on/off behavior, and final-head
acceptance still require the hosted run to pass.

The first-head CEF screenshots show a wireframe-dominant native scene with dark
material fill. Wireframe is intentionally enabled, the native lighting differs
from WASM, and the retained log contains no rendering validation or shader
error. This observation alone does not establish a PBR rendering defect. The
new baseline proves readiness of nonblank native scene pixels, not full PBR
material/lighting correctness; grayscale depth output and restoration have
their own independent assertions.

As a compatibility cross-check, the actual first-head JPEGs were decoded to RGB
and passed through the native baseline helper. `native-startup.jpg` has 29,308
painted scene pixels in six occupied tiles and 56 color bins;
`sculpt-before-stroke.jpg` has 48,744 painted scene pixels in six tiles and 316
bins, each out of 1,508,350 sampled pixels. These lossy-image checks confirm the
new baseline is compatible with the observed native defaults, not a replacement
for the harness's uncompressed readback. A single-line negative is also rejected.
The depth oracle establishes broad varying grayscale output and reversible
presentation; it does not claim a quantitative near/far depth-accuracy proof.
