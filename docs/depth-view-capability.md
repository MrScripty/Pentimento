# Optional depth view: capability contract

Depth view is an optional visualization. Its current shader reads a multisampled depth texture with WGSL `textureLoad`. Bevy 0.18.1 / wgpu 27's GLSL backend cannot translate that operation. The existing WebGL2 application therefore failed during eager pipeline initialization even with depth view disabled.

## Backend authority

`DepthViewCapability` starts unavailable. `DepthViewPlugin::finish` reads the actual renderer's `RenderAdapterInfo.backend`, after Bevy's renderer finishes initialization. OpenGL/WebGL (`Gl`) is unavailable on every target, including native GL. Missing, unknown and no-op adapters fail closed. Vulkan, Metal, DX12 and BrowserWebGPU retain the existing implementation. Availability here means this implementation is admitted on that backend; it is not proof that every adapter, texture configuration or native rendering path has been visually qualified.

Unavailable renderers retain `DepthViewLabel`, the no-op view node, and the Tonemapping → DepthView → outline → EndMainPassPostProcessing ordering. They do not install depth-only prepass/effect/bounds systems, preparation systems or the depth pipeline. The feature's WGSL, MSAA and target format are unchanged. Other features may independently need prepasses.

## IPC and frontend behavior

- `GetDepthViewState` returns `DepthViewState { available, enabled, reason }`.
- Startup also publishes state. Each Svelte mount, Dioxus bridge creation and egui runtime's first frame explicitly queries state, recovering snapshots missed before subscription.
- `SetDepthView` on an unavailable renderer returns `DepthViewRejected { reason }` followed by current state. Neither enable nor disable requests mutate scene settings, shadows, AO or prepasses.
- Supported requests update settings and return authoritative state. Repeated requests do not mark settings changed or recapture already-disabled effects.
- Controls start disabled, explain unavailability, and display only backend-confirmed enabled state. They do not toggle optimistically.
- WASM drains the shared outbound queue into its real CustomEvent bridge. This replaces its previous log-only handling of depth commands.

## Evidence and remaining qualification

The genuine application diagnostic at https://github.com/MrScripty/Pentimento/actions/runs/37125921942 loaded WASM and created scene objects with ANGLE/SwiftShader, WebGL2 / GLES3, backend `Gl`. It then failed on `depth_view_pipeline`: WGSL depth `textureLoad` unsupported in GLSL. This was not a successful render.

Source evidence, not measured texture descriptors: the main camera uses `Camera3d::default()` without an MSAA override; Bevy 0.18.1 defaults to Sample4. Its depth prepass uses Depth32Float with the camera's sample count, and this pipeline binds a multisampled depth texture. Postprocess destinations are single-sample. Both existing depth and outline pipelines hard-code Rgba16Float while Bevy chooses view texture format from HDR state. That independent format assumption remains an adjacent blocker/risk; this change deliberately does not rewrite it or claim it works.

Required independent validation before declaring runtime success:

1. Run IPC contract tests and compile native active frontends plus WASM selection build using the repository's dependency cohort and lock policy.
2. Run scene capability tests and native frontend tests. Structural Node checks are guardrails, not substitutes for these tests or GPU execution.
3. Re-run the same genuine production Electron diagnostic, with normal sandbox/context isolation. Verify actual WASM, visible PBR geometry, selection outlines, disabled depth control and explicit reason, resize/interaction, normal close, and owned-process cleanup. Record adapter, actual camera MSAA, depth texture format/sample count and destination format. Do not use a fixture canvas or a new CSP/security workaround.
4. On a supported native GPU, render depth on/off with normal MSAA, check near/far grayscale output, outlines, repeated requests, and shadow/AO restoration.

No browser depth visualization is claimed. A full browser color-depth rendering path and CSP changes remain separate proposals.
