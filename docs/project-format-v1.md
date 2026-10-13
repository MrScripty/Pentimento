# Local project format v1

This describes the first format and its version-specific limits. The editor reads
v1, v2 and v3 using the same codec. It writes v2 without shared UV layers and v3
when shared layers exist. [The DirectUV extension](project-directuv-v2.md) adds
explicit single-surface authoring data; [shared UV layers](shared-uv-layers-v3.md)
add the shared layer representation and appearance owner. PTex rendered painting
and persistence remain unsupported.

File → Save, Save As, and Open use absolute local `.pentimento.json` paths in the native editor. The Svelte dialog sends real shared IPC commands and waits for the engine's success or failure receipt. Browser-only previews disable file operations until the native capability handshake. New creates a fresh empty document after explicit confirmation; Export remains disabled.

The first canonical format is self-contained UTF-8 JSON with `format: "pentimento-project"` and `version: 1`. It uses the existing scene, mesh, layer, and brush models rather than an asset database. Serialization and parsing preserve finite f32 values exactly, including editable pixel values. Runtime Entity IDs, asset handles, caches, active strokes, replay packets, and undo history are omitted.

## Saved document

- Stable document object IDs, names, selectable IDs, object/plane/mesh/stroke identity counters, transforms, and visibility. Reopening reseeds the existing primitive-name counter before another Add Object.
- Exact triangle mesh position, normal, UV0/UV1, tangent, and float-color arrays; index width, order, and unindexed geometry.
- Sculpt authoring half-edge topology, UV seams, tombstones, all chunk/global identity maps, and allocation counters, separately from the exact current render mesh. Sculpt re-entry reuses these identities while the saved render mesh still matches.
- Canvas layers with their IDs, order, active layer, next ID, names, visibility, opacity, and lossless RGBA float pixels; current validated paint/sculpt brush settings.
- Each receiver's separate per-canvas projection layers and original base appearance. These are restored without flattening, double blending, or resurrecting cleared projection. Supported material properties and embedded CPU image bytes are preserved.
- Main orbit view, lighting, and ambient occlusion settings when present.

Open replaces document geometry only after detached parsing, reference validation, resource preflight, and existing sculpt safety admission all succeed. It clears old queued editor/input commands and local undo/replay state, restores identities and authoring layers, and sends fresh UI state. Live projection opens paused, as the visible receipt explains. A new accepted paint/sculpt edit then uses the existing real local Undo/Redo pipeline.

Save refuses active strokes or unsettled scene messages. Bevy retains messages briefly after processing, so an immediate Save may ask the user to wait and retry. For live projection, a read-only check compares fresh source composition, current root transforms/visibility, effective camera, culling, geometry, and cache revision against the completed projection snapshot. Save does not replay an edit or freeze an older receiver after Undo or another mapping change. A refused Save leaves the file untouched.

## Explicit v1 limits

Parsing is capped at 64 MiB before reading the file into memory. A document permits at most 256 objects, one million topology records, 4,194,304 aggregate editable pixels, and 32 MiB of embedded image bytes. Meshes permit 250,000 vertices and 750,000 triangle indices; canvas stacks permit 64 layers and dimensions up to 1048×1048. Embedded images are single-level CPU RGBA8/BGRA8, sRGB or linear, at most 4096×4096, with the default sampler.

Unsupported versions, missing CPU assets, non-triangle or morph meshes, unsupported attributes/material features, parented authoring objects, and painted DirectUV/PTex mesh-paint storage are rejected explicitly. These are format boundaries, not silent conversion paths. Ordinary canvas/projection paint and sculpt authoring are supported. Images and project pixel data are lossless; JPEG is used only for qualification screenshots.

Sculpt restoration calls the existing safety snapshot admission and preserves its tolerances. The existing welded import is also used to recognize untouched raw source primitives whose first sculpt entry already welded them; this does not add a new geometric tolerance. Changed raw position/corner-UV negative controls must still fail admission.

## File ownership and failure behavior

Save records the bytes of the owned file and checks them again before replacement. External changes block same-path overwriting; Reopen or Save As to a new path is required. Saving to an unowned existing path, a symlink, or a non-regular file is refused. A sibling advisory lock serializes cooperative writers. A uniquely created sibling temporary file preserves owned-file permissions before receiving project bytes, then is fully written and synced before atomic rename; failed/interrupted pre-commit saves remove the temporary file and preserve the prior file. The local path/state change only after success.

The sibling lock remains available for later operations. Parent-directory durability and behavior on unusual filesystems are subject to the host filesystem; the format does not claim crash-proof storage on every platform. There are no external asset paths, recovery journal, autosave, or persisted undo history in v1.

## Qualification

Rust owned-file tests exercise production shared Save/Open dispatch, exact authoring identities and pixel values, rendered receiver images, malformed/over-limit files, failed saves, external-edit conflicts, active-stroke refusal, scene/input reset, and real first edits with Undo/Redo after reopen. Native command-batch tests cover Save after queued object/canvas creation and the successful Open document boundary. The native input fixture proves the full project dialog region prevents a canvas stroke.

Chromium tests exercise the actual Svelte controls, capability gates, pending/failure/success receipts, focus handling, and emitted LayoutUpdate regions. Their engine receipts are synthetic and do not establish native CEF file I/O or GPU acceptance. Native CEF qualification remains pending. Build output, owned fixtures, logs, and JPEG85 evidence belong outside Git.

## New document boundary

File > New Project always confirms discarding unsaved changes and all local Undo/Redo history. The editor does not claim precise dirty tracking. Cancel and Escape send no New command. The modal freezes the exact decimal document generation at creation; later state updates cannot retarget the confirmation. Native admission rejects unconfirmed or stale requests, active editing ownership, unapplied document commands, and admitted scene pointer presses without replacing geometry, file ownership, or history.

A confirmed idle New prepares a validated empty version-2 document and uses the same atomic replacement primitive as Open. It resets authoring objects and identity counters, paint/UV/sculpt histories and selection, brushes, camera and lighting defaults, pending scene/native input, and paused projection. It releases the old file owner (including any external-edit conflict) without writing or deleting that file. Save As can then choose a fresh local path. External-edit handling for Save/Open is unchanged.

The native controller flushes its buffered canvas prefix before admission and drops the remainder of the old command batch after successful replacement. Native forwarding observes the replacement generation, clears gesture/motion/modifier state, and suppresses contacts held across the boundary until their physical release/end. Fresh presses can then edit the new document. These controller/input tests compile production source in a headless harness; they do not qualify the native GUI. The CEF SDK remains unavailable, and no CEF download retry or browser sandbox workaround is used for this candidate.
