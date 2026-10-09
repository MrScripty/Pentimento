# Shared UV texture layers and canonical project v3

DirectUV strokes and Canvas projection share one UV appearance owner and one explicitly selected receiver/layer. Shared layers are enabled explicitly for an installed UV receiver; the existing single-surface editor remains available for legacy documents.

## Editor contract

Select a UV receiver in the existing paint panel and explicitly enable UV texture layers. Create, name, select, reorder, show/hide, change opacity, lock paint, duplicate and delete layers. Keep at least one layer. A new layer is inserted above the selected layer. Lock prevents content painting; it permits structural edits. Hidden layers refuse content painting. Receiver selection remains available when another receiver is conflicted.

DirectUV freezes receiver and layer identity for a stroke. Canvas Apply commits a mapped snapshot to the same selected layer, on only the selected receiver. Each accepted Apply is one UV history entry; repeated explicit requests are intentional stamps. Source Canvas Undo continues to edit the source. The explicit UV Undo/Redo controls work in either authoring mode and recover layer structure, pixels and active selection together. Selection alone preserves Redo. Accepted authoring edits clear global UV Redo; no-op, rejected and cancelled edits preserve it.

Apply refuses active source/UV transactions and pending scene presses. An accepted request captures the source pixels, receiver/layer, source entity, existing mapping context, scene geometry stamps and mesh asset revision. Source or mapping changes before commit refuse the whole pending request batch; they never stamp a different source. Intervening paint commands are refused until Apply settles. Multiple identical requests share one pending snapshot. Successful Open clears old requests and their payload before emitting the new document state.

## Rendering and migration

Raw UV layers are linear **premultiplied** RGBA. The first layer compositor is Normal source-over; opacity scales all four associated channels once. The composite is placed over the original material/texture, unpremultiplied for the derived RGBA8 sRGB display, and written by the existing mesh uploader. Projection retains its existing geometry, occlusion and UV mapping. Its uploader skips shared receivers and its setup reuses the shared derived image. No sculpt geometry or guard tolerance changes are included.

Canvas Apply remains an explicit commit. Live UV preview pins a transient projection to the selected layer, with explicit Apply/Cancel and pause after Apply; see [the live preview contract](shared-uv-live.md). Normal and selected-layer brush/erase are qualified; layer Multiply, Screen, masks and other advanced modes are not implemented controls.

Enable imports exact retained per-canvas projection snapshots below the exact DirectUV raw texture. Snapshots become ordinary independent UV layers; subsequent source edits require Apply. The new compositor is explicit and may change legacy appearance. Neither migration nor Save normalizes raw pixels. Transparent hidden RGB is retained, skipped in display and masked only at texels actually painted. Invalid/unassociated nontransparent pixels refuse migration. Different-size nonempty projection snapshots refuse; only bitwise-empty mismatched snapshots may be omitted when adopting DirectUV dimensions.

Migration validates unique installed receiver/entity/mesh/storage/image identities, acknowledged Direct/projection displays, original images, descriptors, raw projection layers and valid UV0 geometry. Only a proven legacy projection ownership transition can clear its old mixed-owner admission block. Foreign bytes, replaced owners, duplicate IDs and foreign projection texels preceding partial uploads refuse. Other external-edit conflict handling remains conservative and sticky. History never stores or restores geometry.

## Persistence and budgets

Canonical v3 stores bottom-to-top layer order, active layer and receiver, raw float pixels, names, visibility, opacity, paint lock, seam padding, identity high-water mark and the explicit compositor tag. It retains typed original material and texture data. Derived display images are rebuilt. Files without shared layers still write v2; v1/v2 reads remain available. Validation and preparation precede installation. Invalid files and owned-file conflicts leave the current document and file unchanged. Open installs fresh histories and pauses live projection.

Retained UV history has a global **64 MiB accounted payload** and **128 entry** ceiling, including legacy mesh history and all shared receivers. Metadata edits retain layouts/names without copying every layer's pixels. Pixel edits retain changed layer before/after payloads. Per-core limits also apply. Cross-receiver trimming removes legacy history first, then histories of other receivers before the current receiver; it does **not** promise globally oldest eviction. Delete recovery expires when its history is evicted.

Pending stroke baselines, Apply snapshots and live preview pixel payloads each have a 32 MiB ceiling. A live session also retains one source/mapping admission capped at 32 MiB, shared with pending Apply when applicable; combined accounted live payload is at most 64 MiB; admission prevents concurrent authoring ownership. Apply allows at most 128 queued requests sharing a snapshot. Status reports retained and pending payloads and evictions. Layer limits are 64 layers, 4,194,304 aggregate pixels per receiver, dimensions up to the existing 1048 project boundary, and 256-byte names. Existing project file/aggregate image and topology budgets still apply. These are authoring/history payload limits, not a process-memory or allocation-free promise: live layers, working surfaces, acknowledged original/derived images, projection cache, transient composites, serialization and allocator overhead consume additional memory.

## Qualification

`input::direct_uv_native_tests::shared_layers*` drives the real app dispatcher, native window forwarding and production Canvas, projection and mesh painting plugins with CPU assets. It covers mixed authoring, color order, selected-layer erase, metadata/deletion recovery, global history bounds, external conflicts, frozen Apply chronology and saved camera, exact v3 persistence, atomic refusal, old-request cleanup, legacy migration and foreign partial-upload refusal.

`tests/ui/shared-uv-layers.mjs` drives rendered Svelte controls through that actual app driver. It requires native-input-forwarding receipts, exercises DOM mouse and synthetic pressure-pen events, and performs owned-file Save/Open. Its texture is an external CPU diagnostic. Native CEF, actual GPU extraction/shading and physical stylus hardware are unqualified. Screenshots use JPEG quality 85. All builds, dependencies, fixture files and evidence remain outside Git.

Run with official dependencies and an external `CARGO_TARGET_DIR`; use `cargo test -p pentimento --features egui`, `cargo test -p painting --lib`, `cargo test -p pentimento-scene --all-features --lib`, frontend checks/build/tests and the shared browser harness. The test harnesses write evidence to an explicitly supplied directory outside the source checkout. Test counts and generated evidence belong to the qualification report for the exact tested source.
