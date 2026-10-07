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
It uses the default scene's sphere and default camera as a reproducible fixture.
A failed stage produces a failure result and screenshot; it must not be reported
as an interaction pass just because the window appeared.

This gate does not prove layered-stroke self-intersection freedom, all sculpt
tools, every imported mesh, scaled-object gizmo accuracy or historical replay.
Those require their own qualified tests. The manual acceptance checklist remains
in docs/paint-sculpt-qualification.md. Until hosted execution is inspected, this
harness is prepared source, not completed native acceptance.
