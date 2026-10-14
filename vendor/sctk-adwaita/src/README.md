# Adwaita decoration implementation

Copied from official sctk-adwaita 0.10.1. Frame configuration, Wayland objects,
button handling, input, subsurface placement, and the public API remain the
upstream implementation. Explicit lifetime syntax fixes compiler warnings
without changing types. The tests at the end of lib.rs observe real header
composition and ensure title ink cannot overwrite the buttons.

The title implementation is documented in [title/README.md](title/README.md).

`lib.rs` has an explicit MPL-2.0 source-file notice for acknowledged snui-derived
rounded-header geometry, with original SCTK MIT contribution notices retained.
This notice correction does not change geometry. Other source modules retain
their upstream licenses; see ../THIRD_PARTY_NOTICES.md for the scoped boundary.
