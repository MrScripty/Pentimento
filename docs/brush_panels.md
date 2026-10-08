# Brush panels and input contract

## Supported surface

The default `./launcher.sh --frontend cef` Svelte frontend now has dedicated
projection-paint and sculpt panels. Shared scene command handling is reusable by
the native Dioxus/egui and WASM hosts; this change does not claim panel parity
across those other frontends. Electron's existing partial command dispatcher is
not upgraded here.

- Paint: the six engine presets, round brush/eraser, radius in canvas pixels,
  opacity, hardness (edge falloff), dab spacing, and sRGB color input converted to
  the engine's linear RGB. No unsupported textured/elliptical tips are advertised.
- Paint settings/undo are backend-owned. Radius changes both pressure limits so
  actual dabs change size. The UI displays the real full-pressure radius rather
  than the previously misleading base-size field. Customized presets are labeled.
- Canvas undo affects the active canvas. Live/apply buttons emit the actual
  projection events; UV rendering correctness depends on the projection-engine
  implementation. There is no PTex promise.
- Sculpt: Push, Pull, Grab, Smooth, Flatten, Inflate, Pinch and Crease, with mesh-local
  radius, strength, hardness and five supported falloff curves. Tool-specific
  engine behavior is retained; customization survives tool and mode changes.
- Sculpt auto smoothing: 0–100% tangent-plane smoothing after stamped dabs.
  Existing defaults remain 50% for Push/Pull/Flatten/Inflate/Pinch/Crease and 0%
  for Smooth until customized. One custom amount then follows stamped brushes
  across tool and mode changes. Grab stays continuous with smoothing disabled;
  returning to a stamped brush restores the custom amount.
- Auto smoothing cannot change during an owned sculpt stroke. The native
  dispatcher refuses stale commands, and disabling the control resets pending
  drafts to the accepted value. Changing an amount leaves redo available;
  a new accepted stroke clears redo. History restores geometry snapshots and
  does not rewind current brush settings or claim self-contained input replay.
- A backend snapshot updates the panel after hotkeys, reload and mode changes.
- Browser rectangles block viewport pointer input; dragging a widget remains
  captured until release. Widget focus blocks viewport keyboard shortcuts.
- Canvas view retains its orbit lock but allows Shift+middle-drag pan and scroll
  zoom. Plain Tab no longer also handles Ctrl+Tab / Shift+Tab mode shortcuts.

## Limitations and next requirement

Sculpt has transactional per-stroke Undo/Redo and Escape cancellation; see
[the history contract](sculpt-history-contract.md). Projection canvas stroke
Redo and real source visibility are documented in
[receiver inspection and history](projection-target-history.md). In Live mode,
source Undo/Redo also refresh the UV target; with Live off, use Apply again.

The sculpt engine measures radius in mesh-local units. Nonuniform object scale
can also make the existing world-space brush gizmo differ from the deformation
footprint; scaled-transform sculpt interaction is not qualified by this change.

## Validation

- `npm run lint:a11y`, `npm run build`, `npm test`.
- `cargo test -p pentimento-scene --features sculpting brush_control` and
  `cargo test -p pentimento-scene --features sculpting brush_ui` exercise the real
  command dispatcher, paint pixels/dab count and sculpt vertex displacement.
- `cargo test -p sculpting grab_respects_strength` checks Grab's strength control.
- `npm run dev -- --host 127.0.0.1 --port 5187`, then `npm run test:ui:browser` runs
  the rendered Svelte/IPC interaction harness using an installed Chromium. Set
  `CHROMIUM_PATH` or `PENTIMENTO_UI_URL` when needed. Screenshots are JPEG quality
  85 under `PENTIMENTO_EVIDENCE_DIR` (default `/tmp/pentimento-brush-evidence`),
  never in Git. The harness is not a 3D renderer; engine tests are separate.

Manual renderer acceptance still requires a supported native CEF run: create a
canvas; paint small/large and soft/hard strokes; erase/undo; apply/live-project to
a UV mesh; select a mesh and Ctrl+Tab into sculpt; compare tools and falloff;
change F/Shift+F values; exit/re-enter; drag every slider across the viewport and
release over/outside the panel without producing an unintended stroke.

The auto-smoothing successor has local guarded scene/pipeline and Chromium
control coverage. Native auto-smoothing qualification remains pending: the
matching official CEF download is blocked by the executor's proxy (HTTP 403).
Browser controls and CPU geometry tests do not constitute native acceptance.
