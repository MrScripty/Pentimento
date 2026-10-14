# Wayland title qualification

The `title-smoke.sh` gate starts an isolated Sway headless compositor with the
explicit software `pixman` renderer and connects the actual SCTK ClientOnly
decoration example. It observes title updates, narrow/wide window geometry,
overlong-title clipping before protected close-control pixels, 1x/2x
scale configuration, compositor frame acknowledgments, title pixels and both
natural process exits. It does not qualify physical GPU performance.

Provision official Ubuntu packages `sway grim jq libwayland-dev libxkbcommon-dev
pkg-config fontconfig fonts-dejavu-core dbus-x11 python3-pil`. Build the reviewed
vendor example with its own committed lock, then invoke through
`dbus-run-session -- bash tests/wayland/title-smoke.sh`, setting absolute
`PENTIMENTO_WAYLAND_CLIENT` and `PENTIMENTO_WAYLAND_EVIDENCE` paths. The hosted
dependency repair workflow owns this environment-qualified gate.

`title-pixels.py` checks the captured real output and compositor geometry.
Direct renderer tests live with `vendor/sctk-adwaita`; fixture/unit tests are
supporting evidence and cannot replace this genuine compositor observation.

## Owned compositor shutdown receipt

After the client has exited with status zero and disappeared from the actual
compositor tree, `compositor-exit.py` authenticates the IPC socket's Linux
`SO_PEERCRED` PID against the compositor child this harness started (and its UID
against the caller). It obtains a live `GET_VERSION` acknowledgment, then sends
the complete, exact framed `RUN_COMMAND` payload `exit` on that same connection.
`compositor-exit.json` records that peer, version, request and reply state. This
receipt has status `requested`, never `passed` by itself.

Sway 1.9 queues replies asynchronously: its exit command calls
`sway_terminate(0)`, which terminates the Wayland event loop before the queued
reply necessarily flushes. See the official tagged sources for
[`exit.c`](https://github.com/swaywm/sway/blob/1.9/sway/commands/exit.c),
[`main.c`](https://github.com/swaywm/sway/blob/1.9/sway/main.c) and
[`ipc-server.c`](https://github.com/swaywm/sway/blob/1.9/sway/ipc-server.c), and its
[`IPC protocol`](https://github.com/swaywm/sway/blob/1.9/sway/sway-ipc.7.scd).
The helper therefore accepts either a valid successful exit reply or clean EOF
before any reply bytes, only after the authenticated complete request. It rejects
partial frames, resets, timeouts, malformed/rejected replies and wrong peers.

`compositor-shutdown.sh` then requires that same owned child to exit naturally
within 20 seconds and that the shell's actual `wait` return zero. It records the
PID and status in `compositor-wait.json`. Only after both gates does the harness
write the overall `process-result.json`; timeout/forced cleanup cannot produce a
passing process result. Existing real title, scale, pixel and client-close gates
remain prerequisites.

Run `PYTHONDONTWRITEBYTECODE=1 python3 tests/wayland/test_compositor_shutdown.py`
for both temporary UNIX-peer protocol fixtures and owned-child shutdown tests.
`ShutdownUnitTests` can be selected separately in environments that prohibit
UNIX sockets; those socket-free tests do not replace the complete hosted suite
or real Sway observation.
