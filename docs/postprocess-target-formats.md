# Post-process destination formats

The outline and depth fullscreen passes specialize their render pipelines using
`ViewTarget::main_texture_format()`, not a platform/HDR feature guess. Bevy 0.18.1
creates both ping-pong textures in that format and exposes them through
`post_process_write()`. The output is resolved (sample count 1), even when the
camera's source depth is multisampled.

Each pass owns its own `SpecializedRenderPipelines` cache. The selected pipeline
ID and exact format are components on the render-view entity. A node checks the
component's format and pipeline readiness before calling `post_process_write()`;
a skipped or compiling pass must not flip the ping-pong target. Preparation runs
in `PrepareBindGroups`, after view targets and prepass resources are prepared.

Depth's sampled texture is also prepared on the corresponding view entity,
replacing the previous first-camera global resource. Preparation clears stale
depth data when settings or prepass data are unavailable. Scene depth bounds and
outline ID-buffer ownership remain the application's existing main-camera model;
this change does not introduce general independent multi-camera scene settings.

Graph ordering, bind layouts, shader math, outline settings and HDR behavior are
unchanged. Depth input remains `texture_depth_multisampled_2d` with a multisampled
bind layout. Supporting single-sample depth is a separate change requiring input
selection, shader behavior and capability validation. This repair does not
advertise that support or enable depth on WebGL/OpenGL.

## Verification

CPU Rust regression tests use the production descriptor builders for RGBA SDR,
BGRA SDR and HDR, assert single-sample/no-blend output, enforce the exact-format
specialization key type, and check per-entity format association and rejection
of stale choices. They do not construct GPU pipelines or emulate engine caches.

Run on a build-capable executor:

```sh
cargo test --locked -p pentimento-scene --features selection target_format_tests
node --test tests/contracts/postprocess-target-format-source.test.mjs tests/contracts/depth-capability-source.test.mjs
```

Required separate runtime qualification:

- Normal-sandbox hosted SDR composition must render through edge detection with
  no target-format validation error, produce genuine scene pixels, and pass the
  existing interaction/recovery probes.
- Native GPU SDR/HDR coverage must render outlines and enabled depth, preserving
  scene/depth/outline/gizmo ordering and the existing supported depth input.
- Mixed-format views and SDR/HDR target changes must not reuse a wrong-format
  pipeline; a pending pipeline must skip without swapping the view target.

A structural/CPU pass is not a claim of native GPU or actual rendering coverage.
