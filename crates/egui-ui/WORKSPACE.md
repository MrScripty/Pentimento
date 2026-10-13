# Native brush workspace

The experimental egui frontend separates frequent brush controls, brush selection
and layer editing into stable, visible areas. It still sends only existing IPC
commands and renders accepted backend state.

**Evidence boundary:** the optional captures are UI-only CPU rasterizations of
actual egui output with fixture snapshots. They do not render a Bevy scene and
do not qualify the full native app, GPU or physical stylus.

## Layout

- Global toolbar: document menu, current mode, Add, camera reset and depth view.
- Context toolbar above the viewport: Paint target and Brush/Eraser, radius,
  opacity, hardness, spacing and target-specific Undo/Redo; Sculpt radius,
  strength, hardness and stroke Undo/Redo.
- Brush shelf: visible built-in paint preset tiles or all eight supported sculpt
  tools, plus a visible saved-brush catalog and save/recall controls. At widths
  below 1100 points it moves below the viewport and uses two columns. Larger
  windows put it on the left. It never becomes a collapsed list.
- Right dock: accepted brush color/tip and compact Canvas/UV stacks in Paint;
  tool description, visible falloff choices and autosmoothing in Sculpt.
  Layer rows select their backend identity and toggle visibility. One properties
  editor and operations bar operate on the acknowledged active layer. Stacks
  scroll independently, with a maximum height, instead of growing one full
  property form per layer.
- Startup: visible Paint Canvas creation and mesh-selection/Ctrl+Tab guidance.
  IPC does not expose a sculpt mode-entry command, so the UI does not fabricate
  an entry button or inject synthetic keyboard events.

Canvas order is bottom-to-top in the snapshot and is reversed for presentation.
UV snapshots are already top-to-bottom; move commands convert to owner indices.
Saved-brush tiles choose a candidate; Use explicitly recalls it, and the current
saved brush changes only after acknowledgement. Existing active-stroke,
projection-preview, conflict, rename, modal and document lifecycle guards remain.
Sculpt stroke history is not advertised as nondestructive authoring layers.

The layout follows concrete conventions from the official Krita manual:

- [Preset docker](https://docs.krita.org/en/reference_manual/dockers/brush_preset_docker.html):
  visible brush selection rather than a hidden chooser.
- [Layers docker](https://docs.krita.org/en/reference_manual/dockers/layers.html):
  compact stack, selected-layer controls, and an operations bar.

Only capabilities supported by Pentimento's existing contract are exposed;
there are no invented preview thumbnails, sculpt layers or unwired actions.

## Verification

Run the CPU widget suite:

```sh
cargo test -p pentimento-egui-ui --lib --locked
```

The suite runs actual egui frames and pointer/key interactions. In addition to
existing lifecycle and history regressions it covers 640×720, 800×900 and
1440×900 layouts, disjoint panel capture regions, visible primary controls,
resize/repeated actions, real numeric toolbar keyboard edits with accepted-state
reconciliation, authoritative selected-layer targeting and reorder
indices, mode-entry guidance, owner locks, and bounded 200-layer/64-preset views.
It asserts typed dispatch and accepted state, not a substitute layer engine.
Production native-controller/Scene tests are a separate integration gate.

Optional reproducible UI evidence uses the actual egui tessellation, clip
rectangles and font atlas with a small test-only CPU renderer:

```sh
PENTIMENTO_EGUI_EVIDENCE_DIR=/tmp/pentimento-egui-evidence \
  cargo test -p pentimento-egui-ui --lib capture_actual_egui_workspaces_when_requested
```

This writes Paint, Sculpt and DirectUV wide/narrow PPM images outside the source
checkout. The captures use deterministic fixture snapshots. The center is blank
because no Bevy scene is rendered. They are UI layout evidence, not screenshots
of a running Bevy app, physical GPU/stylus acceptance, or performance evidence.
The optional renderer adds no Cargo dependency and is excluded from release
builds. Keep generated images outside Git.
