# DirectUV editor routing and verification

The existing painting panel and shared commands select Canvas projection or the
owned DirectUV editor. Shared UV layers add one selected receiver/layer to both
backends; see [shared UV layers](shared-uv-layers-v3.md) and [staged live preview](shared-uv-live.md).

## Editor contract

`PaintCommand::SetTarget { target: Canvas | DirectUv }` selects the existing
backend. Brush settings/catalog Save/Select operate on that backend's resource;
the other editor retains its brush. PaintBrushStateChanged includes authoritative
target, receiver name, active status, notice and bounded snapshot payload metrics.
Mode selection, brush edits and history refuse active stroke ownership. Direct
mode explicitly refuses Canvas source/layer/sampler/projection operations.

The source canvas is hidden and its view unlocked in Direct mode; live projection
is paused. Its original visibility/view properties are entity-owned and restored
on Canvas return or ordinary paint exit. Create/select/deselect/view changes are
refused while Direct owns the view. A pending Canvas projection command refuses
entry. Owned Open establishes fresh document ownership and fresh history.

Direct strokes use existing BrushEngine pressure/spacing in atlas pixels, with
radius equal to half the emitted diameter. Interpolation is confined to the same
face or an exact shared position-and-UV edge; missing hits, seams, disconnected
islands and unsupported/other receivers break interpolation. A nearer visible
unsupported triangle occludes supported receivers behind it. Hidden surfaces do
not receive paint. No mesh geometry, UV, global identity or sculpt tolerance is
changed by painting/history.

Window input is consumed chronologically, including focus changes, Escape and
Ctrl+Z/Ctrl+Shift+Z. Release followed by Undo in one batch commits then undoes
that accepted stroke. Escape or Cancel restores complete pre-stroke float pixels
and appearance binding. No-op/cancel/rejection retain redo; a new accepted changed
stroke clears redo. The existing app canvas shortcut consumes its events but
does not act in Direct mode or replay them on a later backend switch.

Snapshots retain at most 64 MiB across Undo/Redo and 128 entries, plus a pending
baseline at most 32 MiB. UI metrics describe those payloads and expirations, not
process memory. Live surfaces, image assets and compositor/allocator memory are
outside that bound. Conservative ownership conflicts preserve external edits,
disable history/upload and can require reopening. File Save preserves existing
external-byte conflict checks and atomic replacement.

## Verification

Production scene and app fixtures exercise editor routing, brush parameters,
ordered gestures, exact raw float-bit history, original/derived material and
image ownership, canonical Save/Open, and refusal of active or conflicted edits.
The frontend contract/unit tests, Svelte check, production build and rendered
browser suites cover the actual controls and authoritative backend responses.
Generated logs, dependencies, owned files and JPEG85 screenshots stay outside Git.

`tests/ui/direct-uv-tool.mjs` drives actual Svelte mode, numeric/color, saved
preset, Undo/Redo, Cancel and File controls through a Rust test driver. It checks
held-stroke control gating, overlapping strokes, chronological keyboard history,
Escape/Cancel, Save/Open and a fresh post-open edit, comparing raw float bits and
bound CPU display bytes. The copied canonical file is retained outside the owned
fixture before cleanup. Reopen preserves authoring/appearance data and starts
fresh history; Rust tests also compare the original typed material/texture
binding after first-stroke Undo/Cancel.

For actual app native event forwarding, build the app test binary and run:

```bash
PENTIMENTO_UI_URL=http://127.0.0.1:5173 \
PENTIMENTO_DIRECTUV_DRIVER_BIN=/absolute/path/to/pentimento-app-test-binary \
PENTIMENTO_DIRECTUV_DRIVER_TEST=input::direct_uv_native_tests::browser_driver \
PENTIMENTO_DIRECTUV_EVIDENCE=/absolute/external/evidence-directory \
node tests/ui/direct-uv-tool.mjs
```

Run a Vite server at the selected URL and provide the binary's ordinary platform
library environment. The harness uses installed official Playwright-core and
system Chromium, with bounded driver/request timeouts and no native binary
download. UiDirty is a GPU repaint marker ignored by the CPU test transport;
authored commands go through the real dispatcher. The scene-only driver exercises
production scene input and assets but does not establish app native forwarding.

## Qualification limits

The diagnostic DOM canvas displays bound CPU texture bytes and is explicitly
labeled as CPU evidence. Its pointer samples use the fixture camera's native
window-event ray; this is not the actual desktop viewport compositor. Original
typed tint/texture binding is verified in Rust asset state, not through browser
GPU material shading. Native desktop CEF execution, GPU extraction/shading and
physical stylus hardware remain unqualified.

PTex rendered painting/persistence is unsupported. Legacy single-surface owners
refuse simultaneous meaningful Canvas projection and DirectUV paint. Explicitly
enabled shared UV layers provide the migration and shared appearance owner
without flattening raw authoring pixels; DirectUV and Canvas Apply target the
same selected layer. Conservative cross-asset conflicts remain documented in
[the v2 ownership contract](project-directuv-v2.md) and the shared-layer contract.
