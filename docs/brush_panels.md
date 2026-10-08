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

## One-click source color sampling

**Sample canvas color** arms a single canvas click. Choose **Visible layers** to
sample the bottom-to-top visible layer composition with layer opacity, or
**Active layer** to read that layer's raw color even when hidden. Both return
straight linear RGB; visible sampling divides the premultiplied composite RGB
by its alpha, ignoring transparency rather than darkening the picked paint color.
The hex/color inputs continue to display sRGB. This samples source layers, not
lighting, materials, occlusion, or a pixel from the rendered UV receiver.

A successful click updates native brush color, preserves brush alpha, opacity and
Brush/Eraser, and disarms sampling. The consumed press remains owned until release;
a fresh press can paint even within the same ordered input batch. Transparent,
nonfinite or out-of-bounds pixels leave the color and history unchanged. A
transparent hit reports an error and keeps sampling armed for another click.
Escape, focus loss and leaving paint mode disarm the tool; an already accepted
color survives a later cancellation/focus event. A missing canvas hit is consumed
without painting. Source selection lasts for the editor session and is restored
from native state when the UI remounts; it is not part of a saved brush.

Sampling neither allocates stroke IDs nor creates pipelines, packets, dirty tiles
or undo entries, and preserves redo. Scene/pipeline stroke ownership refuses stale
arming/source commands; the UI disables and reconciles them. Actual subsequent
sampled-color strokes use the normal accepted stroke path, Escape rollback and
exact per-layer Undo/Redo. The color sampler does not change geometry guards,
projection rules or tolerances.

Local qualification covers layer math, real ordered Bevy input and painting
pipeline history, and rendered Chromium controls. Native CEF qualification remains
pending because the matching official runtime download is proxy-blocked (HTTP
403). Browser panel screenshots and CPU tests do not qualify native 3D rendering.

## Device-local custom brushes

Both panels can save the current brush by name and restore it with **Use paint
brush** or **Use sculpt brush**. The lists are separate, including their IDs;
the same name can exist once in each mode. Reusing a name replaces that entry
without changing its ID. Two differently named brushes with identical parameters
retain the explicitly saved/restored identity. Customization clears the displayed
current match. A UI reload requests the backend catalog again; application restart
loads it from disk, and selecting a preset restores its parameters.

Paint saves the existing engine brush model, including full pressure-size limits,
opacity, hardness and spacing, plus linear color and Brush/Eraser. Built-in presets
continue to reset only tip settings and retain the current color/tool. Sculpt saves
the existing editor settings: tool, mesh-local radius, strength, hardness, falloff
and the explicit auto-smoothing override. Grab stays continuous and retains its
stored amount for returning to a stamped brush. No geometry, tessellation policy,
history snapshots or projection settings are stored in a brush preset.

Saving and restoring are refused whenever the scene or a brush pipeline owns a
stroke, including stale enabled frontend commands. The panel disables preset
actions and reconciles the accepted choice. Restoring changes current brush
parameters without clearing undo/redo or replaying input. Successful paint recall
and built-in tip selection also disarm the one-click sampler, so the next fresh
click paints with the selected brush. A consumed sampler press stays consumed
until release. Saving or refused selection leaves sampler state unchanged; source
selection stays session-local and is not stored in the brush catalog.

One versioned backend-owned JSON catalog stores both modes. On Linux it is under
`$XDG_CONFIG_HOME/pentimento/brush-presets.json` (absolute XDG paths only), falling
back to `$HOME/.config`; macOS uses `~/Library/Application Support`, and Windows
uses `%APPDATA%`. `PENTIMENTO_BRUSH_PRESETS_PATH` can explicitly override the file
path. Saves use a stable advisory lock and atomic file replacement. Known external
edits block further preset operations until restart; editors ignoring the advisory
lock are not protected from every concurrent filesystem race. Corrupt/unsupported
catalogs are left unchanged, and write errors never report a successful save.

Limits are 64 presets per mode, 64 characters per name and a 1 MiB file. Invalid
numeric settings and duplicate names/IDs are refused. Builds without the sculpt
engine preserve the other mode's catalog and validate numeric/schema bounds;
sculpt effective tool defaults are additionally validated when that engine is
available. WASM/browser storage is unavailable and shown as such; there is no
separate frontend preset database, cloud sync or cross-frontend panel parity claim.

The combined brush feature branch retains the original auto-smoothing, custom
preset and sampler checkpoints through a normal merge. Its integrated input test
samples actual layered paint, saves that RGB, restores it from the real catalog,
then paints with the recalled brush and verifies exact layer/composite Undo/Redo
and Escape rollback. Recall preserves existing redo until a new accepted stroke.
A saved Grab smoothing override resumes on returning to a stamped brush. The
Chromium suite additionally exercises all three panels' controls together and
reloads authoritative state; it continues to distinguish UI protocol tests from
native renderer acceptance.

Local qualification includes actual saved/reloaded paint pixels and eraser undo,
saved sculpt parameters affecting guarded pipeline geometry with exact history,
concurrent-writer refusal, storage faults, active-stroke refusal and Chromium panel
interactions. Native CEF acceptance remains pending due to the official runtime
download's proxy 403 block.

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
