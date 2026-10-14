# Post-process destination formats

The outline and depth fullscreen passes specialize their render pipelines using
`ViewTarget::main_texture_format()`, not a platform or HDR-feature guess. Bevy
0.20 creates both ping-pong textures in that format and exposes them through
`post_process_write()`. The post-process destination is single-sample even when
the camera's input depth is multisampled.

Each pass owns a `SpecializedRenderPipelines` cache. Pipeline ID and exact format
belong to the render-view entity. Its render system checks the prepared format
and pipeline readiness before calling `post_process_write()`; a skipped or
compiling pass cannot flip the ping-pong target. Preparation runs in
`RenderSystems::PrepareBindGroups`, after view targets and prepass resources
exist. GPU resource recovery clears device-bound preparation.

Depth's sampled texture is prepared on its corresponding view entity rather
than selected from the first camera. Missing or disabled prepass data clears
stale preparation. Depth settings/bounds and the outline ID buffer remain the
application's existing main-camera model; this does not introduce independent
multi-camera scene state.

The Bevy 0.20 render schedule retains tonemapping → depth → outline ordering.
Supported native depth still uses the existing multisampled texture-load shader.
It is only queued when enabled, and never constructed on the unsupported GL
backend. See [depth capability](depth-view-capability.md) for admission and UI
behavior. Single-sample or browser depth rendering is not implemented here.

The outline ID camera is a data pass: it uses an sRGB attachment with explicit
shader write/sample compensation, no tone mapping, no dithering, and no MSAA.
This avoids unsupported alternate texture views without merging 24-bit entity
identities. The roundtrip invariant and original failure evidence are in
[WebGL rendering](plans/browser-runtime-upgrade/reports/webgl-rendering.md).

## Verification and lineage

Format specialization and its four CPU tests are adapted from repository commit
`534d31d` to current Bevy 0.20 APIs. The tests use production descriptor builders
for RGBA SDR, BGRA SDR and HDR, assert single-sample/no-blend output, enforce the
exact-format specialization key, and check per-view association and stale-key
rejection. They do not construct GPU pipelines.

```sh
cargo test --locked -p pentimento-scene --features selection target_format_tests
cargo test --locked -p pentimento-scene --features selection id_encoding_tests
```

The runtime-upgrade checks require visible production WebGL scene pixels and
selection outlines, plus repeated native Vulkan depth toggling and restoration
using that application's default view format. They do not exercise every SDR/HDR
format or a live target-format transition. Broader qualification of those cases
remains pending. CPU descriptor coverage of multiple formats must not be
represented as passing GPU acceptance for that entire matrix.
