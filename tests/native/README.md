# Native CEF qualification

`cef-paint-sculpt.mjs` launches the built canonical CEF application, attaches to
its real renderer for DOM inspection, and sends every user action through X11
with xdotool. It never injects a mocked IPC transport or direct engine command.
Native window RGB readback is compared before/after strokes and written as JPEG
quality 85. JSON result markers, engine stroke-start logs, screenshots, source
commit and environment information are retained outside the repository.

The bounded `native-cef-qualification` hosted job installs official Ubuntu
packages, uses the checked npm lockfile, builds only CEF and its helper through
the launcher, and runs under Xvfb with Mesa's software Vulkan driver. The job
retains its generated Cargo.lock for exact dependency reproduction; Rust's lock
policy is unchanged. Unconditional WebKitGTK dependencies require the development
package even when selecting CEF. The old separately version-pinned setup-cef.sh
is not used: cef-dll-sys selects the runtime matching its Rust binding.

CEF's existing `--remote-debugging-port=0` option is enabled only on the test
process. The port is ephemeral and loopback bindings are verified with `ss`
before attachment. The process group, including the endpoint, is killed in the
script's finally block and bounded by the job timeout. No listener is enabled
by default and no security flags are weakened by the harness.

References checked for this path:
- [Official CEF binary distribution documentation](https://chromiumembedded.github.io/cef/general_usage#using-a-binary-distribution) links the Spotify CEF build CDN used by download-cef.
- [Chromium remote debugging socket implementation](https://github.com/chromium/chromium/blob/main/chrome/browser/devtools/remote_debugging_server.cc) binds 127.0.0.1 or ::1.
- The installed cef-dll-sys 143.7.1 binding documents port zero as ephemeral.

The test covers startup/bootstrap, actual Grab selection and radius update,
widget-drag capture, a visible sculpt stroke, Tab ownership, Exit/reentry,
canvas creation, paint settings/stroke/undo, and production live/apply events.
The history extension retains all original assertions and adds undo, redo,
active-stroke Escape rollback and redo-branch invalidation. Its pixel oracle uses
three unchanged captures at both endpoints, compares the pixels that actually
deformed, and requires both visible motion and restoration within measured frame
noise. Escape is tested only after visible active-stroke deformation is established.
The helper's synthetic negative tests cannot substitute for this native run.
It uses the default scene's sphere and default camera as a reproducible fixture.
A failed stage produces a failure result and screenshot; it must not be reported
as an interaction pass just because the window appeared.

This gate does not prove layered-stroke self-intersection freedom, all sculpt
tools, every imported mesh, scaled-object gizmo accuracy or historical replay.
Those require their own qualified tests. The manual acceptance checklist remains
in docs/paint-sculpt-qualification.md. Until hosted execution is inspected, this
harness is prepared source, not completed native acceptance.

## Software-rendered diagnostic qualification

The hosted runner sets `PENTIMENTO_CEF_RENDERING=software` for the qualification
process only. This passes `--disable-gpu --disable-gpu-compositing` to CEF, matching
[CEF's official software-OSR sample](https://github.com/chromiumembedded/cef/blob/master/tests/shared/browser/client_app_browser.cc).
Bevy continues to use Mesa Vulkan. Sandbox, security, certificate and network
settings are unchanged. This qualifies software-rendered native controls and 3D
interaction; it does not qualify hardware-accelerated CEF.

The harness fails within 30 seconds if the actual CEF framebuffer/first captured
paint is missing. It checks native toolbar and sculpt-panel pixels, as well as
DOM controls. Geometry capture waits for a post-finalization receipt matching the
specific new stroke ID; a Start or pre-finalization End marker is insufficient.
Cancellation also requires that exact transaction's cancellation receipt. The
original geometry pixel assertion and gesture timing are retained. Per-input
work timings are logged only in the qualification process, and any rejection is
reported explicitly. Image inspection remains required even when these gates pass.
