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
