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
