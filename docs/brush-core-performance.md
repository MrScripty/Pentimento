# Brush CPU performance and dirty-region correctness

This qualification covers the actual `painting` and `sculpting` Rust engines.
The comparison baseline is `cd85372`, using the same deterministic harness source,
compiler and release settings. It is separate from rendered UI, physical GPU,
pointer/stylus, and driver qualification.

## Reproduce

No new benchmark dependency or Cargo feature is required. From the repository:

```sh
export CARGO_PROFILE_RELEASE_LTO=false
export CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16
cargo run -p painting --release --locked --example core_profile -- 7
cargo run -p painting --release --locked --example uv_region_profile
cargo run -p sculpting --features bevy --release --locked --example core_profile -- 96 5
cargo test -p painting --features bevy --release --locked --lib --tests
cargo test -p sculpting --features bevy --release --locked --lib --tests -- --test-threads=2
```

The sculpt arguments are sphere segments and independent trials. Try `32 5`
and `160 5` for smaller/larger meshes. Setup, checksums, exact Undo/Redo checks,
and cancellation assertions are outside the timed phases. All collision safety
checks remain enabled. The painting harness times 128 dabs, 32 canvas frames,
or 16 composites per row; divide elapsed time by the reported operation count
for a per-operation cost. Canvas rows report the aggregate number of dirty tiles
across all trials in their final hexadecimal column, rather than a pixel hash.
The p95 column is the nearest upper order statistic of a small sample (the maximum
with the default trial counts), not a production tail-latency characterization.

For a baseline comparison, create a detached checkout at `cd85372`, copy only
`crates/painting/examples/core_profile.rs` and
`crates/sculpting/examples/core_profile.rs` into its corresponding directories,
and run the same commands/settings there with a separate `CARGO_TARGET_DIR`.
Do not share target directories across checkouts: newer cached path-crate artifacts
can otherwise contaminate a baseline built from older file timestamps. `uv_region_profile` compares full
composite-then-crop against the new region API in one build; do not copy that
example to the baseline, which has no region API. Avoid concurrent compiler work
when collecting timings. No generated logs, binaries, or benchmark data are tracked.

## Repairs and invariants

### Canvas presentation

Previously every call to `PaintingPipeline::take_dirty_tiles` recomposited the
entire canvas and marked every tile for upload, including unchanged frames. Scene
calls this every frame. A 1024-square canvas therefore produced 64 dirty tiles
per frame even when idle.

Now the layer stack compares exact layout metadata and composites only the union
of dirty source tiles. Idle calls return no tiles. Visibility, opacity, order,
layer creation/deletion and explicit full composites invalidate the full output.
Public mutable layer access conservatively marks the entire layer, because callers
may edit raw pixels without tracking them. Internal stroke, Cancel, Undo and Redo
use a private tracked accessor and preserve their small tile footprints. Cached
pixels outside the dirty region remain unchanged. The public raw composite-surface
accessor remains a derived-output API, not a source-layer edit.

### Shared UV presentation

`UvLayersDocument::composite_active_region` returns a clipped packed rectangle
without allocating a full-atlas intermediate. It shares exactly the existing
mask, blend, opacity and active Color/Mask override arithmetic. Regression tests
compare all four blends, both targets, clipping and transparent hidden RGB against
the original scalar compositor, down to float bits.

`has_visible_paint` makes the material-bound decision without a full float RGBA
composite. Its alpha calculation follows the same operation order. This is still
an atlas/layer scan in the empty or last-painted-pixel case; the region harness
explicitly includes both. Dirty tile inspection is non-consuming so the Scene
owner can retain changes until an image is ready. These CPU changes do not prove
that Bevy's renderer performs partial GPU texture uploads for changed Images.

### Sculpt admission and history

Immutable geometry-only surfaces share triangle storage. Replacing candidate
chunk geometry uses copy-on-write, so admitted/rollback surfaces cannot change.
When tessellation made no edit, the pre-deformation surface is already the exact
admitted surface and does not need a second all-face position refresh. Initial
history capture is moved into the rollback checkpoint instead of cloning it again.
Each accepted dab still receives the full existing geometric validation. An exact
mesh witness is captured once at the externally observable end of an input batch;
no external mutation is possible between dabs while the mesh is exclusively borrowed.
Stationary inputs still check the exact witness before deciding there are no dabs.

History acceptance validates snapshot identities, boundary relationships and
counters afresh. It reuses a geometric proof only after exact witness equality
against an admitted endpoint, including private half-edge maps and global identity
maps; otherwise the full existing surface validator runs. Undo and Redo continue
full fresh target validation. Geometry predicates, collision work caps and tolerances
are unchanged. Invalid counters, external edits, failed dabs and rebalance changes
cannot bypass the fallback/rollback path.

Smoothing still scans every live face to gather complete cross-chunk one-rings,
including disconnected local fans belonging to a valid global fan. It avoids a
per-triangle temporary allocation and only computes position/normal contributions
for selected rings. Stable global ordering and the original arithmetic are retained.
Target writes resolve selected global IDs in every chunk instead of scanning every
local vertex. This does not introduce nondestructive sculpt authoring layers.

## Measurements

Measured on October 13, 2026, Linux x86_64, AMD EPYC 9V74, Rust 1.97.1.
Baseline and candidate were built in separate target directories, with release
LTO disabled and 16 codegen units. Their binaries were then run sequentially
without concurrent compiler work. These are one cloud host's medians; they are
not physical GPU/stylus measurements or artist-production-mesh guarantees.

| Workload | Original median | Candidate median |
| --- | ---: | ---: |
| 1024² canvas, 32 unchanged frames | 50.795 ms | 0.001 ms |
| 1024² canvas, 32 small-stroke frames | 49.720 ms | 0.273 ms |
| 9,024-face Push, eight warm dabs | 59.041 ms | 58.102 ms |
| 9,024-face Smooth, eight warm dabs | 71.718 ms | 61.993 ms |
| 9,024-face Push, one eight-dab input batch | 49.688 ms | 42.464 ms |
| 9,024-face Smooth, one eight-dab input batch | 59.584 ms | 46.121 ms |
| 9,024-face Push, finish/history acceptance | 57.225 ms | 4.387 ms |
| 9,024-face Smooth, finish/history acceptance | 59.039 ms | 5.080 ms |
| 25,280-face Push, eight warm dabs | 186.641 ms | 165.955 ms |
| 25,280-face Smooth, eight warm dabs | 227.490 ms | 187.934 ms |
| 25,280-face Push, finish/history acceptance | 172.089 ms | 15.288 ms |
| 25,280-face Smooth, finish/history acceptance | 176.209 ms | 13.734 ms |

Canvas rows use seven trials. Before the repair, both canvas workloads emitted
2,048 dirty tiles per trial; afterward idle emitted zero and the small stroke
emitted nine. The original idle, small-stroke and metadata-dirty regression tests
all fail when rebuilt against the isolated original source; all pass on the candidate.

Sculpt rows use five trials. These fixtures are welded UV spheres, partitioned
into four/16 chunks, with radius 0.2, strength 0.015, no adaptive tessellation,
no autosmooth or rebalance. Separate integration tests cover adaptive edits,
autosmooth, rollback, rejected geometry and rebalancing. Global-position checksums
match the baseline exactly for Push and Smooth at both sizes, and the harness
asserts exact complete-state Undo/Redo and cancellation outside the timed sections.

The unchanged 128-dab raster controls measured 13.826/13.764 ms for circles and
9.183/9.172 ms for UV ellipses. Full 512² four-layer UV composition (16 repetitions)
measured 57.466/52.504 ms for Normal and 107.007/99.228 ms for masked/Overlay.
All four pixel checksums match. No meaningful dab-raster speedup is claimed.

The separate regional UV example, on a 1024² four-Normal-layer document, measured
133.305 ms for eight full-composite-then-crop operations and 1.799 ms for eight
128² region operations, with exact output bits. These figures exclude material
binding scans. Sixteen binding checks took 78.281 ms with all layers empty and
57.112 ms with only the final top-layer pixel painted. Those are explicitly linear
worst-case scans, not constant-time lookups.

Cold initial geometry admission remains substantial: the 25,280-face candidate
first dab took 105.929 ms (Push) or 110.451 ms (Smooth). Warm dabs there still cost
about 20.7–23.5 ms each before application/rendering work. Exact no-dab witness
checks remain approximately 62 ms per 32 inputs at that size. This repair greatly
reduces idle painting and stroke-end stalls but does not establish fully responsive
large-mesh sculpting. Spatial safety/broad-phase work, initial admission, unchanged
input witnesses, and renderer/GPU costs remain separate profiling targets.
Ordinary development-profile launcher runs are also not represented by these
optimized-release timings.

Final core verification: 133 painting unit tests plus five integration tests;
82 sculpting unit tests plus 57 integration tests. All 277 passed, with the two
pre-existing manual timing tests ignored. New guards cover immutable Surface
copy-on-write, counter edits outside a cached geometry witness, triangle-query
parity/malformed cycles, idle and incremental canvas updates, partial edge tiles,
metadata/public edits, Cancel/Undo/Redo, regional UV blends/masks, and erasing the
last visible masked pixel. The existing sculpt topology, UV seam, swept collision,
external edit, complete-neighborhood smoothing and history suites also pass.
