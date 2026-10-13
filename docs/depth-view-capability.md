# Optional depth view: capability and frontend contract

Depth view is an optional visualization. In the Bevy 0.20 / wgpu 30 renderer, its shader reads a multisampled depth texture with WGSL `textureLoad`. The GLSL/WebGL backend cannot translate this operation. An eagerly queued shader can therefore fail application startup even while depth view is off. Browser depth was previously a logged command with an optimistic toolbar toggle; this repair reports its actual unavailability.

## Backend authority

`DepthViewCapability` starts unavailable. `DepthViewPlugin::finish` resolves the actual `RenderAdapterInfo.backend` after renderer initialization:

- OpenGL/WebGL (`Gl`) is unavailable, on both browser and native targets.
- Missing, unknown and no-op adapters fail closed.
- Vulkan, Metal, DX12 and BrowserWebGPU retain depth support.

Admission on one of these backends is not evidence that every adapter or texture configuration has been visually qualified. Supported native depth operation remains a separate rendered acceptance requirement.

Unavailable renderers keep the depth render ordering point, but install no depth-only prepass, effect/bounds update, bind-group preparation or depth pipeline. Supported renderers specialize the pipeline for the actual view format and queue it only for an enabled depth view. The Bevy 0.20 `Core3d` systems retain tonemapping → depth → outline ordering. See [postprocess target formats](postprocess-target-formats.md) for the related renderer work.

## IPC and frontend behavior

`crates/ipc` owns the shared contract; the TypeScript mirror and Rust-generated acceptance samples change together.

- `GetDepthViewState` returns `DepthViewState { available, enabled, reason }`.
- `DepthViewRejected { reason }` reports an unsupported request. It is followed by the authoritative current state. Rejected enable and disable requests do not mutate settings or trigger shadow/AO/prepass changes.
- Supported `SetDepthView { enabled }` requests update settings and return authoritative state. Repeated requests leave an unchanged settings resource untouched, preventing repeated effect-state capture.
- All controls start disabled, explain unknown/unavailable capability, and wait for backend acknowledgement before changing their enabled display.
- Each Svelte toolbar mount subscribes before querying. The native bridge coalesces a pending depth query until IPC readiness, discards it on disposal, and never replays depth mutations. Remounting queries again without reusing stale capability.
- Each Dioxus bridge creation and egui runtime's first frame requests depth state. The shared native snapshot suppresses stale `enabled: true` whenever capability is unavailable. egui also synchronizes direct scene-setting changes through the same capability-authoritative state function each frame.
- WASM handles the query and mutation with the same admission helper as native dispatch. It drains `OutboundUiMessages` through the actual CustomEvent bridge, including the startup state broadcast.

No browser color-depth replacement or browser depth-rendering success is claimed.

## Regression coverage and qualification

- `tests/ui/depth-view.test.ts`: initial unknown state, synchronous late-mount recovery, unavailable capability, repeated requests without optimistic state, rejection, stale enabled suppression, cleanup and remount.
- `tests/ui/bridge-lifecycle.test.ts`: native and WASM message delivery; delayed native IPC readiness; coalesced depth queries; no delayed mutation; disposal/remount.
- `tests/contracts/ipc-contract.test.mjs` with the Rust-generated samples: unsupported and supported depth state, rejection and query serialization.
- Shared native snapshot, Dioxus bridge and egui library tests: startup admission and backend acknowledgement.
- Native dispatcher tests: query, repeated supported toggles, rejected enable/disable, and missing capability. Native egui integration tests exercise actual toolbar clicks and direct scene-setting synchronization, including unavailable controls.
- Scene tests cover backend admission and native shadow/AO restoration. These CPU/state tests do not substitute for renderer qualification.

The [browser runtime upgrade plan](plans/browser-runtime-upgrade/plan.md) and [Electron qualification report](plans/browser-runtime-upgrade/reports/electron-qualification.md) track exact-head build and runtime evidence. This upgrade's bounded rendered acceptance requires real WASM/PBR pixels and selection outlines on WebGL with a disabled depth control and explicit reason, plus repeated supported-native depth on/off with varied grayscale output and restoration of the previous scene. The native smoke does not isolate near-versus-far ordering or individual shadow/AO settings; those broader visual claims remain unqualified. CPU tests separately cover shadow/AO state restoration. Do not infer quantitative depth accuracy or a complete adapter/format matrix from these checks.

## Implementation lineage

The contract is adapted from `2022d754` and the Dioxus RSX ownership correction `415a3fc`, with their qualification history on `origin/test/depth-capability-qualification` and `origin/review/electron-depth-runtime-composition`. Current brush/workspace state, delayed CEF bootstrap behavior, native per-frame synchronization and Bevy 0.20 APIs are retained rather than replacing whole frontend files with that older implementation.
