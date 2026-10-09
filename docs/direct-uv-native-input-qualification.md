# DirectUV native input and verification

The native app input route forwards chronological mouse and pressure-bearing
TouchInput contacts into the existing DirectUV editor and bounded history. The
shared layer editor uses this same route and selected UV layer; [its contract](shared-uv-layers-v3.md)
and [live preview](shared-uv-live.md) describe structural history and Canvas integration.

## Contact and history contract

The actual native app InputPlugin consumes chronological WindowEvents in
PreUpdate and publishes admitted original TouchInput events. A single contact
owns its stroke until physical release/cancel. Secondary contacts and companion
mouse events cannot steal, close or promote that owner. UI-origin contacts use
the existing CompositeBackend mouse route and remain UI-owned. Their coordinates
follow the existing CEF logical versus Capture/Overlay physical DPI contract.
Touch events flush prior throttled mouse moves before ownership; queued hover
cannot replay after UI touch Down. Ordinary UI, camera and selection pointer
consumers remain blocked during a contact.

Scene contacts carry the original ID and reported force into DirectUV. First
dab and moved dabs use pressure through the existing BrushEngine. Normalized and
calibrated finite values in [0,1] are accepted; malformed supplied force cancels
rather than silently becoming full pressure. Missing force uses ordinary full
pressure on Start/Move; End without force reuses the last pressure. Tilt and
altitude are not modeled. This is Bevy/winit TouchInput forwarding, not a claim
that every platform exposes a hardware stylus through that event type.

A held contact remains active without MouseButton Left. Explicit native cancel,
Escape, Cancel UI, focus loss and invalid samples roll back the pending owned
stroke. Crossing into UI commits only the last valid scene prefix and latches
the contact until release; returning to the viewport cannot start another
stroke. Mode exit cancels; an admitted press prefix prevents a queued backend
switch from reinterpreting input. A stationary press publishes its current
window cursor before Down. Open resets document ownership and history; a
canceled contact still physically held cannot author the reopened document.

Retained keyboard state observes raw releases even when UI filters actions.
Focus/window/document resets release all retained keys. Valid Ctrl+Z before
focus loss remains chronological; a later plain Z cannot inherit Ctrl/Shift.
Release then Undo in one batch commits and undoes that accepted stroke.

Accepted changed strokes enter bounded history; no-op/rejection/cancel retain
redo, and a new accepted changed stroke clears redo. Every Undo/Redo comparison
uses exact raw f32 bits and CPU display bytes; first-stroke Undo/Cancel also
checks original material binding. Existing 64 MiB/128-entry retained payload and
32 MiB pending-baseline bounds remain unchanged. Live surfaces, Image assets,
compositor/allocator/process memory remain outside that payload bound.

Geometry, UV continuity, nearest supported/unsupported occluder, owned-image
and file conflict guards remain unchanged. The preserved conservative external
geometry guard retains already admitted pixel bytes, expires pending/history
ownership and stops subsequent dabs; it does not promise rollback after an
external mesh edit and never rewinds that geometry. Owned Save/Open retains
atomic replacement and external-byte conflict refusal.

## Verification

App fixtures install the actual InputPlugin and shared UI dispatcher, production
scene plugins and a RecordingBackend implementing CompositeBackend. They forward
ordered native WindowEvents and typed Bevy messages into real CPU image/material
assets. Tests cover first-dab pressure, repeated strokes, exact history,
secondary/contact/mouse ownership, malformed force, no-op/cancel Redo retention,
focus/keyboard order, DPI/UI capture, stationary origin, queued mode changes,
external geometry, Save/Open/reentry, active Save refusal and external file bytes.
Scene/project tests, frontend contracts, Svelte diagnostics/build and existing
rendered browser suites cover the associated editor and persistence boundaries.

The DOM browser harness forwards pointer samples through the actual app input
route. It exercises held controls, chronological history, Escape/Cancel, owned
Save/Open with fresh history, synthetic pen pressure, overlapping pen strokes,
exact Undo/Redo and pointercancel rollback. Its inspector is explicitly labeled
as a CPU texture diagnostic. Owned project files and JPEG85 screenshots are
external evidence artifacts, not shipped UI or tracked source.

Reproduce after building the app test binary with its ordinary library environment:

```bash
PENTIMENTO_UI_URL=http://127.0.0.1:5173 \
PENTIMENTO_DIRECTUV_DRIVER_BIN=/absolute/path/to/pentimento-app-test-binary \
PENTIMENTO_DIRECTUV_DRIVER_TEST=input::direct_uv_native_tests::browser_driver \
PENTIMENTO_DIRECTUV_EVIDENCE=/absolute/external/evidence-directory \
node tests/ui/direct-uv-tool.mjs
```

Use a Vite server at the selected URL. The scene-only default driver remains
available for narrower scene qualification. Official installed Playwright-core
and system Chromium are used with bounded startup/request timeouts. UiDirty is
ignored by the CPU test transport; authored commands use the native dispatcher.

## Qualification limits

CPU/input fixtures verify app forwarding and the shared frontend trait route.
They do not establish native desktop CEF execution, compositor/GPU shading or
physical stylus hardware support. Bevy/winit TouchInput pressure forwarding does
not promise that every platform exposes stylus hardware through TouchInput;
tilt and altitude are not modeled.

PTex rendered painting/persistence is unsupported. Legacy single-surface owners
retain their mixed Canvas/DirectUV refusal. Explicitly enabling shared layers
migrates retained raw data into editable layers and unifies both authoring routes;
[shared UV layers](shared-uv-layers-v3.md) specifies that transition and its limits.
See [editor routing](direct-uv-ui-qualification.md) and [canonical v2](project-directuv-v2.md)
for inherited view, input, ownership and persistence behavior.
