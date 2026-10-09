# Canonical project v2: owned DirectUV data

The canonical JSON codec writes `version: 2` for documents without shared UV
layers. The editor reads versions 1, 2 and 3; shared-layer documents write v3
using [the shared UV representation](shared-uv-layers-v3.md). It preserves all v1 geometry, sculpt topology/global identities, layer
identities, projection layers, views, brushes, and owned-file conflict semantics.
It adds one representation: the existing DirectUV CPU authoring surface. There
is no alternate serializer, texture baking conversion, or GPU readback.

Each v2 UV-atlas PaintableMesh requires `mesh_uv`, its stable mesh ID and actual
mesh UV0, plus document `mesh_brush`. The surface includes exact finite RGBA f32
pixels, dimensions, seam padding, original linear color cache, and whether the
derived paint display is bound. Original typed material color and original CPU
image bytes remain in the existing material representation. The separate direct
painter's preset, color and blend mode are saved independently of the canvas
brush. Derived images are checked before Save and rebuilt synchronously before
Open succeeds; setup retains the restored authoring pixels. First Cancel/Undo
restores the original material color and original texture binding, including a
tinted textured material. It does not flatten that original into editable paint.

Admission requires same-size, single-level 2D RGBA8 sRGB images with default
sampler and no custom view for DirectUV's original and derived display. Save
refuses missing CPU bytes, descriptor edits, stale/composed-image mismatches,
active transactions, backend switches with retained work, and simultaneous
meaningful DirectUV and canvas-projected layers on one receiver. These restrictions
match the legacy single-surface renderer's ownership and compositor; they do not convert or
silently omit authored work. The compositor's existing formulas are unchanged.

The v1 aggregate decoded pixel budget (4,194,304) also counts DirectUV pixels.
The existing 64 MiB file, 256-object, mesh/topology and embedded-image budgets
remain. V2 UV surfaces are required; omission cannot be interpreted as unpainted.
V1 extension fields are refused, while actual unpainted v1 UV components can
Open, initialize a fresh surface, accept real edits, and Save as v2. Builds
without mesh painting reject v2 DirectUV fields explicitly.

## Local stroke history and editor routing

Production MeshPaintEvent processing owns a transaction on the starting entity
and mesh ID. Start snapshots an admitted surface; Move targets that owner;
End records only changed, accepted pixels; Cancel and invalid final pixels roll
back. No-op/rejected/canceled strokes preserve redo. A new accepted stroke clears
redo globally. Open starts with fresh local history and emits no replay data.

The renderer plugin and world APIs implement the real engine feature:

```rust
undo_mesh_paint(world: &mut World, entity: Entity) -> bool
redo_mesh_paint(world: &mut World, entity: Entity) -> bool
```

They validate current ownership before exchanging pixels and binding state.
The next Update settles the derived Image/material. Save during that interval
refuses stale display bytes. Resource-level exchange is private. Editor adapters can
read `undo_count(mesh_id)`, `redo_count(mesh_id)`, `has_active_stroke()`,
`history_bytes()`, `pending_history_bytes()`, `history_limit_bytes()`,
`evicted_history_strokes()` and `history_conflicted(mesh_id)` for honest controls
and feedback. The existing PaintBrushPanel now selects Canvas projection or
DirectUV surface through a shared PaintCommand::SetTarget. Its existing brush,
color, catalog, Undo/Redo controls reach the selected backend; authoritative
PaintBrushStateChanged includes target, active status, conflict notices and
snapshot payload metrics. No frontend brush store or separate panel is added.
The source canvas is hidden and live projection paused in DirectUV. Source
visibility is bound to its original entity and restored on mode change or exit.
Canvas creation/selection/view changes are refused while DirectUV owns that view;
pending projection commands and active transactions refuse backend switches.

DirectUV consumes chronological window pointer/focus/key events and admitted
frontend scene-pointer batches. Release before Ctrl+Z in one batch commits then
undoes that stroke. Escape and the shared Cancel control roll back the transaction.
A brush uses the existing BrushEngine in atlas pixel units, including spacing and
pressure. Missing hits break interpolation. Interpolation crosses a face boundary
only when the faces share an exact position-and-UV edge; seams/disconnected islands
receive local dabs without an atlas connecting line. No geometry tolerance changes.
Canvas layers, sampler and projection commands remain Canvas-only. DirectUV strokes
blend into a single owned editable surface while shared layers are disabled.
Explicitly enabling shared layers imports retained raw pixels into independently
editable layers; DirectUV and Canvas Apply then target the selected shared layer.

Retained before/after pixel snapshots share a 64 MiB raw-byte cap across undo and
redo and a 128-entry metadata cap. An active baseline is separately capped at
32 MiB: retained plus pending snapshot payload never exceeds 96 MiB. Begin refuses
a stroke whose before/after pair cannot fit. Only an accepted changed commit
evicts oldest entries; Begin, Cancel and no-op do not evict. Metrics report actual
retained/pending payload, not a total process-memory estimate. Live authoring
surfaces, image assets, derived compositor buffers and allocator overhead are
separate; undo history is bounded but document/render memory is not that budget.

External mutable surface access invalidates its active/history ownership before
returning bytes. Entity deletion/reuse, mesh/image replacement, duplicate mesh IDs,
storage changes and any mesh-asset mutation conservatively invalidate DirectUV
history. This includes unrelated geometry changes; there is no geometric hash
collision or new sculpt tolerance. Conflicting material/image descriptor/byte
edits block history and uploads instead of overwriting external work. Such image
conflicts remain blocked until fresh document/resource ownership is established.
The image change-tick guard is also conservative: an unrelated image/canvas write
while a DirectUV history upload is pending can cause that sticky conflict. It
preserves the external image and refuses stale Save. The editor reports the
conflict and requires fresh document ownership before further editing. Shared
layers additionally track exact acknowledged original/display bytes; their
external-edit and migration contract is described in `shared-uv-layers-v3.md`.
Save retains the canonical owned-file original-byte checks and atomic replacement.

## Qualification and concrete remaining boundary

Owned Rust pipeline tests drive the production plugin and shared Save/Open IPC
with actual CPU Image bytes bound to materials. They exercise repeated reopen,
edits/Undo/Redo, first Cancel/Undo appearance, transparent float-bit preservation,
seam/brush identity, two distinct surfaces, fresh history, active refusal, file
conflict/Save As, unsupported images, exact pixel-budget and history-budget limits,
metadata eviction, malformed files, and external/live/synchronous owner changes.
Existing sculpt safety and topology tests remain required and unchanged.

PTex's scene upload remains an explicit placeholder and projection PTex hits are
unsupported. Per-face CPU serialization alone cannot establish a usable rendered
reopen/edit/history feature. Saves with retained PTex faces refuse clearly; PTex
render/storage integration remains unsupported. Browser qualification of File UI
does not establish native CEF, GPU, or desktop tool qualification; none is claimed
here. The current Svelte-to-Rust CPU qualification is documented in
[direct-uv-ui-qualification.md](direct-uv-ui-qualification.md). Native app input forwarding is exercised by CPU fixtures described in
`direct-uv-native-input-qualification.md`; native CEF/GPU execution and physical
stylus hardware remain unqualified.
