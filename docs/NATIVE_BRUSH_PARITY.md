# Native Paint, Sculpt and Layers parity

The comparison uses the current Svelte PaintBrushPanel, SculptBrushPanel,
SavedBrushPresets and UvLayerPanel, with Bevy 20 baseline
136d1ff12a329e48709bd9f7bb8d01bc20c80f12. Native widgets continue to send commands
through the existing shared controller and Scene APIs. This followup changes no
geometry algorithms, safety tolerances, history implementation or dependency.

## Completed controls

- Paint offers editable `#RRGGBB` sRGB input alongside the existing linear RGB
  picker. Committed edits convert once to linear RGB; malformed input sends no
  command. Focusing an unchanged field preserves the precise accepted color.
  A round tip preview reflects acknowledged color, hardness and opacity.
- Saved Paint and Sculpt brushes now support choosing an entry and explicitly
  using it. The current saved name follows backend acknowledgement. Name entry
  permits 64 characters; local replacement and catalog limits are explained.
- Undo/Redo labels identify Canvas strokes, legacy surface strokes or shared UV
  edits. Canvas Apply identifies the active UV layer when enabled. UV history
  accounting remains visible in both shared-layer targets.
- Ordinary Canvas brush controls and Sculpt tool/radius/strength/hardness/falloff
  controls follow Svelte's active-stroke behavior. Existing Direct UV,
  autosmooth, presets and history restrictions remain. Cancel UV preview can
  restore the UV receiver while an independent Canvas stroke remains active.
- Sculpt tool descriptions and brush-adjustment/navigation hints are present.

All current UV layer operations already existed in native egui: receiver and
Enable, create/select/rename/reorder, visibility/opacity/paint lock, four blend
modes, duplicate/delete, mask add/remove/enable, Color/Mask target and Undo/Redo.
The implementation preserves their preview, active-owner and external-conflict
handling. No replacement layer engine was added.

## Sculpt authoring-layer boundary

Neither the current Svelte Sculpt panel nor `SculptCommand` has nondestructive
sculpt authoring layers. Committed-stroke undo snapshots restore complete mesh
state; they do not provide independently visible, reorderable or editable sculpt
layers. Layered-geometry safety tests also do not establish an authoring-layer API.

A separate backend workstream should first define the sculpt-layer composition
model and its interaction with adaptive topology/global identities. It then needs
persistence, composition-time safety validation, external-edit conflict policy
and an atomic history contract, followed by explicit Scene commands/state and
matching Svelte/native controls. Adding a Layers button without that backend
would not implement the feature.

## Verification scope

`cargo test -p pentimento -p pentimento-egui-ui --features pentimento/egui
--lib --tests --locked --offline -- --test-threads=2` passed **161 App tests
and 18 widget tests, zero failures**; one manual browser driver remains ignored.
`cargo build -p pentimento --features egui --locked --offline` linked successfully.
Independent source review passed, including the strict hex parser, accounting
and active preview-cancellation corrections.

The production App tests exercise actual egui widgets, the shared dispatcher and
real Paint/Sculpt/Projection resources and assets. Added flows cover hex entry
through accepted Direct UV pixels and exact Undo/Redo; UV mask document/image
roundtrips; active Sculpt setting changes with exact cancellation; and active
Canvas preview cancellation retaining its independent source owner.
Presentation tests cover strict hex validation/linear conversion/precision,
accepted preview geometry, explicit preset Use and active ownership restrictions.
Actual native Xvfb/Mesa llvmpipe Vulkan smoke accepted `#808080`, updated the
round preview, and saved its linear RGB (0.21586047 per channel) to an isolated
preset file. Choosing that preset preserved Eraser mode until Use restored Brush
mode and the acknowledged saved name. Software rendering required slow key/click
pacing across frames; the preset popup opened via keyboard Space. Popup opening
via synthetic mouse and fast input remain unqualified. Native runtime
evidence, the owned preset file, test receipts and JPEG85 screenshots remain
outside Git.

Physical GPU/stylus behavior and lighting pixel parity are not established by a
software Vulkan smoke. Dioxus/CEF executable runtime remains separately limited
as described in BEVY20_MIGRATION.md.
