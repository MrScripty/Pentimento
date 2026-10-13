# Native paint/sculpt responsiveness and workspace

This candidate builds on `cloud/bevy20-egui-upgrade` at
`cd85372493d8e9067ad5530d842480540d26dfa0`. It preserves Bevy **0.20.0**,
bevy_egui **0.43.0-rc.1**, egui **0.36.2**, and the committed dependency lock.
Main was still Bevy 0.18 at investigation time; this is a stacked development
candidate, not a dependency downgrade or a main merge.

## Completed checkpoints

1. Reproduce CPU problems and preserve a deterministic baseline.
2. Remove unnecessary canvas/compositor and sculpt snapshot work without
   weakening transaction, geometry, layer, mask or history contracts.
3. Replace collapsed native brush/layer forms with visible contextual controls.
4. Exercise real egui widgets and Scene/core lifecycle tests; retain repeatable
   CPU benchmark and UI-only capture tools. Publish a draft for review.

## Reproduced problems and changes

### Canvas repainting while idle

`PaintingPipeline::take_dirty_tiles` used to composite the complete layer stack
and mark every tile dirty on every call, including unchanged frames. The Scene
calls it every frame. A 1024² canvas therefore submitted 64 tiles per idle frame.
The core now preserves source/composite invalidation and only recomposites changed
tiles. Layer order, opacity, visibility and public mutable pixel access still
invalidate all affected output; undo/cancel preserve exact restoration.

Dirty tiles are now durable across the main/render boundary. Pending uploads
coalesce the latest pixels per tile and survive missing render resources or an
unprepared `GpuImage`. The render owner acknowledges a revision only after queueing
its writes. An old acknowledgement cannot erase newer stroke/cancel/undo bytes.
Deleted canvases release pending data. Shared immutable tile bytes avoid copying
whole pending buffers during extraction. A checked, untracked CPU image copy is
patched for recovery without requesting duplicate whole-image uploads.

This is CPU and queue-contract evidence. Actual GPU completion, physical device
loss/recovery, high-rate tablet input and end-to-end frame latency require native
runtime qualification on an appropriate machine.

### Sculpt picking and safety-preserving history

Sculpt picking used to allocate a converted copy of all indices, scan every
triangle for the hit, then search again by approximate position to find its
normal. Two shells within 0.001 units could therefore use the wrong triangle's
normal. Rotation-only normal conversion was also wrong for nonuniform scale.
Picking now retains the exact winning triangle in one allocation-free pass,
uses inverse-transpose normals, and safely skips malformed/degenerate triangles.
It reads the current mesh asset each time, so cancel/history replacement cannot
leave a stale acceleration cache. The path remains O(triangles); it is not a BVH.

Sculpt optimizations share immutable admitted surfaces and reduce duplicate
snapshot/refresh work. Exact geometry witnesses and fresh identity/counter checks
allow endpoint proof reuse only when the geometry is identical. Every changed dab
still runs the existing geometric safety checks; Undo/Redo still validate restored
geometry afresh. See [core measurements and proof boundaries](brush-core-performance.md).
No nondestructive sculpt authoring-layer backend has been invented.

### Shared UV display composition

Layered UV uploads composite and convert only the dirty tile rectangles, patching
the image and exact owner witness together. Original texture sampling uses full
atlas coordinates even at clipped edge tiles. Material binding remains correct
when erasing the final visible pixel. Layer metadata, masks, undo and cancellation
continue to invalidate complete output when required. Missing image assets retain
dirty state for retry.

`has_visible_paint` may still scan full visible layer buffers for sparse/empty
atlases; benchmarks explicitly include those cases. Bevy can still upload the
whole changed UV Image asset. CPU incremental composition is not a claim of
incremental UV GPU texture writes.

## Native workspace

The egui frontend now has a contextual top options toolbar, a visible preset/tool
shelf, and compact Canvas/UV layer stacks with one acknowledged active-layer
properties editor. Smaller windows move the shelf below the viewport. Frequent
brush and layer controls are not hidden in collapsible forms. Startup exposes
Paint Canvas and explicit mesh-selection/Sculpt shortcut guidance.

The design uses the official Krita [preset docker](https://docs.krita.org/en/reference_manual/dockers/brush_preset_docker.html)
and [layers docker](https://docs.krita.org/en/reference_manual/dockers/layers.html)
patterns, restricted to Pentimento's actual commands. See the
[native workspace guide](../crates/egui-ui/WORKSPACE.md) for controls, responsive
layout, and a reproducible actual-egui tessellation/font-atlas CPU capture tool.
Those captures use fixture state and an empty center: they are **UI layout
captures, not screenshots of the running Bevy scene**.

## Reproduce

Use a fresh target directory for each source revision. Cargo may reuse path-crate
outputs incorrectly for historical worktrees with preserved timestamps; do not
reuse baseline and candidate leaf artifacts. Run binaries sequentially after
builds finish, on the same host with the same profile and workload. Do not compare
a development build against an optimized one.

```sh
# Repeatable CPU benchmark suite (release, LTO off, 16 codegen units).
./scripts/profile-native-brushes.sh --all

# Production Scene/engine regressions, without a display.
cargo test --release --locked -p painting -p sculpting -p pentimento-scene \
  --features painting/bevy,sculpting/bevy,pentimento-scene/sculpting,pentimento-scene/mesh_painting,pentimento-scene/mesh_editing,pentimento-scene/selection,pentimento-scene/wireframe,pentimento-scene/atmosphere \
  --lib --tests -- --test-threads=2
cargo test --locked -p pentimento-egui-ui --lib
./scripts/check-source-readmes.sh --all
npm run verify
npm run build

# On a machine with the documented Linux native prerequisites:
./launcher.sh --build-release --frontend egui
./launcher.sh --run-release --frontend egui
```

The profiling wrapper prints its exact Rust/profile settings. Core examples emit
workload size, phase timings and output checksums; the sculpt example additionally
hashes output geometry. Scene benchmarks compile the original picking routine
alongside its replacement and compare identical outputs. The UV benchmark compares
full composition against dirty-region publication, including painted, sparse and
erased backgrounds. Timing tests are opt-in, never brittle CI speed thresholds.

## Verification receipt

Recorded on 2026-10-13, Linux x86_64 cloud CPU, Rust 1.97.1. The results below apply to this candidate and the accompanying reproduction
harness.

Passed locally:

- 277 painting/sculpting core unit/integration tests; two existing manual timing
  tests ignored. Three new canvas regressions demonstrably fail on isolated
  original `cd853724` source.
- 145 combined Scene tests; three manual tests ignored. Includes render-asset
  synchronization, chronological batched input, repeated/cancelled strokes,
  history, projects, masks, clipped UV patches, upload retry and stale receipts.
- 27 actual egui widget/layout tests, including numeric toolbar edits and
  acknowledged state, layer operations, modal/popup input capture, resize and
  640×720 / 800×900 / 1440×900 layouts.
- `npm run verify`: formatting, Svelte accessibility (zero errors/warnings),
  Svelte/Electron type checks and all 58 Node contract/UI tests. Node 24.19/npm
  11.9 were available locally; CI retains its Node 22/npm toolchain.
- `npm run build`, active-source README coverage, shell syntax and diff checks.

The canonical engine gate now includes painting integration regressions and
native egui widget tests. Existing crate feature guards are retained.

The production native app `--no-run` qualification was attempted and **blocked**
by missing system development metadata: `glib-sys` could not find `glib-2.0.pc`
(`pkg-config glib-2.0 >= 2.70`). This workspace also lacks the full documented
GTK/WebKit native development setup. The complete App adapter tests, linked app,
native scene screenshots, physical GPU/tablet responsiveness and canonical
all-frontend launcher gate are therefore not claimed as passed. Existing
Bevy-0.20 migration deprecation/debug-only warnings are still emitted in core
checks. No dependencies or lockfiles were changed to hide this limitation.

## Measured CPU results

Same-host sequential medians, optimized release with LTO disabled and 16 codegen
units. Core before/after binaries use isolated target directories and identical
harness source; painting pixel and sculpt global-position hashes agree. Scene
comparisons use reference/current implementations in the same test executable.
These are bounded CPU workloads, not application frame rates.

| Workload | Before | After |
|---|---:|---:|
| Canvas 1024², 32 idle frames | 50.795 ms, 2,048 tiles | 0.001 ms, 0 tiles |
| Canvas 1024², 32 small-stroke frames | 49.720 ms, 2,048 tiles | 0.273 ms, 9 tiles |
| Sculpt Push finish/history, 9,024 faces | 57.225 ms | 4.387 ms |
| Sculpt Push finish/history, 25,280 faces | 172.089 ms | 15.288 ms |
| Sculpt picking, 32,768 triangles, 128 rays | 38.974 ms | 27.564 ms |
| UV 1024², two layers, one dirty tile, painted background | 98.553 ms | 1.574 ms |
| Same UV workload, sparse last visible pixel | 97.614 ms | 3.598 ms |
| Same UV workload, fully erased | 95.110 ms | 3.679 ms |

Residual cost is important: 25,280-face warm Push/Smooth dabs still take roughly
21–24 ms each before picking, mesh publication and rendering. Large models and
adaptive topology need further profiling; this candidate does not establish a
60 Hz dense sculpting budget. The ordinary launcher build/run uses the unoptimized
development profile; the numbers above do not establish its responsiveness. Use
`--build-release` / `--run-release` when qualifying the optimized native app.
