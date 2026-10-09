# Staged live shared UV projection

Live UV preview provides an editable projection preview on the same selected receiver and UV layer used by DirectUV. The preview is committed only by Apply.

## Layer controls

The actual Svelte UV panel has Create, select, name, Raise/Lower, show/hide, opacity, paint lock, duplicate, delete and bounded UV Undo/Redo. Stack order is top first in the UI and bottom first in storage. Both reorder directions are tested through rendered controls. Locked layers still permit structural edits; locked or hidden layers reject authoring. The last layer cannot be deleted. Authoritative backend responses reconcile rejected control drafts. DirectUV generic history labels now say UV edit when shared layers include metadata or Apply history.

Receiver selection covers existing installed PaintableMesh UV owners. Automatic admission of newly added geometry is an existing separate limitation. UV editing does not alter sculpt geometry guards.

## Live preview contract

In Canvas projection, enable Live UV preview for the selected unlocked, visible layer. Admission pins receiver/entity/layer, source canvas, saved projection camera/context, geometry stamps and mesh asset revision. Source strokes, source Undo/Redo and source-stroke Cancel regenerate a transient mapped preview over the **committed** selected layer baseline. Repeated refreshes are idempotent. Raw layer documents and UV history remain unchanged while previewing; only the existing derived display uploader uses the working preview.

Apply refuses an active source stroke and validates a frozen source/mapping snapshot. It commits the final preview once with the existing UV pixel history operation, then pauses live preview. It never composites over an already displayed preview. Enable live again for the next staged edit. Apply without live retains the existing explicit stamp behavior. Only accepted non-no-op commits clear UV Redo.

Cancel UV preview or disable Live discards the preview and retains Canvas edits and UV history/Redo. This differs from Cancel current stroke/Escape, which keeps existing source-stroke rollback semantics. A Cancel after queued Apply in the same batch cancels the still-uncommitted preview. Cancel without a live session does not change legacy projection events or mode.

UV layer/receiver/UV history/DirectUV changes are refused while preview owns its pinned target. Canvas brush settings and source history remain available. Preview ownership is distinct from actual stroke activity; an idle preview does not disable source Undo or Open. Paint mode exit, focus loss, source removal/switch, mapping/geometry changes or owner conflict discard the session without a UV commit. Foreign image/workspace bytes are preserved; cancellation synchronously rechecks current ownership before scheduling baseline restoration. Standard sticky external-edit conflicts remain in force.

## Persistence and memory

Save refuses every uncommitted preview, including visually empty or quantization-equivalent previews. Successful Open replaces the project atomically and clears preview/pending payload before publishing fresh state. v3 stores only committed UV pixels/metadata; it rebuilds display and opens with fresh histories and paused live projection. No serializer revision or replay stream is introduced.

Live expected UV pixels are capped at 32 MiB, and frozen source plus geometry admission at 32 MiB. They are reported together with other pending history payload. Apply shares the updated source admission Arc with live provenance after pinned mapping validation; it does not add another retained source copy or double-count that snapshot. Default 1024² source/receiver fits these ceilings. The existing 64 MiB/128-entry retained history ceiling remains. Live document pixels, workspace, acknowledged images, mapping cache/current source, transient projection/composite allocations and allocator overhead are additional memory; accounted pending payload is not an RSS or peak-allocation limit.

## Qualification

Actual app dispatcher/native window forwarding and production CPU asset tests cover repeated source strokes, source Undo/Redo, Apply once, UV Undo/Redo, both cancellation meanings, locked/hidden layer refusal, mid-operation layer/receiver/target switches, same-batch Apply cancellation, persistence/Open cleanup, mapping/external ownership conflicts and default 1024² admission. Rendered Svelte tests use the same app driver and a labeled external CPU texture diagnostic. They do not qualify native CEF, GPU extraction/material shading or physical stylus hardware. JPEG screenshots use quality 85 and all generated files remain outside Git.

Normal, Multiply, Screen and Overlay use the same [isolated UV-stack compositor](shared-uv-blends.md) for committed and staged layer pixels. [Basic grayscale masks](shared-uv-masks.md) share this same path and pin the Color/Mask target. Other blend modes, a continuously armed post-Apply workflow and automatic admission of newly added geometry as receivers are not implemented.
