# Layered sculpt safety

The `SculptingPipeline` admits a complete surface before its first edit, checks local connectivity, valid facets and orientation after each adaptive split/collapse/flip and smoothing pass, and globally validates each complete dab and post-stroke repartitioning before returning control to the renderer. Topological incidence and geometric embedding are separate tests.

- Live half-edges must belong to valid face cycles. Global vertex links must be a single boundary path or interior cycle. Global identity maps join chunk copies, independently of UV charts.
- Triangle positions must be finite and non-degenerate. Duplicate facets, coplanar overlap, and triangle contacts outside an actual shared vertex/edge are rejected. Shared IDs permit the exact topological boundary, not a tolerance-sized overlap region.
- A BVH queries changed triangles against the entire candidate surface, including unmodified, distant chunks. Unchanged pairs reuse a previously admitted result. Complete incident links are rechecked around all vertices touched by topology changes.
- Deformations cannot flatten or reverse a face. A conservative swept-volume test also rejects non-adjacent faces that cross between accepted endpoints. This is not a general continuous collision detector: adjacent-face trajectories and topology-change trajectories are not continuously certified. Every returned/renderable dab and completed stroke is checked. Internal candidates remain unobservable until their transaction is admitted.
- The geometric predicates use f64 arithmetic on stored f32 positions, a local length-relative contact tolerance of 1e-7 plus a small coordinate-roundoff allowance, and an area degeneracy threshold of 1e-10 times squared edge length. This is conservative floating-point validation, not exact-arithmetic certification.

## Failure behavior

A failure restores the entire pre-stroke mesh, global IDs, chunk layout, spatial data and vertex budget. Every restored chunk is marked for a full GPU upload, since earlier valid dabs may already have been displayed. The rest of that stroke is ignored, and no stroke packets are emitted on completion. Initial invalid geometry is refused without deleting or repairing input faces. Callers receive a typed rejection reason.

Exact admission snapshots include the complete half-edge mesh (including its edge map) and both global identity maps. A changed snapshot is re-admitted, including zero-dab inputs and stroke completion. No probabilistic hash or caller-maintained revision is used as proof of validity.

The mutable mesh is borrowed exclusively during processing. Candidate edits, validation and rollback finish synchronously before `process_input` or `end_stroke` returns; rejected candidates cannot reach the renderer through those APIs. Geometry changed externally between inputs is compared with the last admitted surface, rather than trusted because it has valid half-edge pointers.

Adaptive compaction refreshes both directions of boundary references. Synchronization resolves a neighbor's current local vertex from its stable global ID. Flip-only edits also mark topology/GPU state dirty.

## Bounded work and qualification

Adaptive work is limited to 1–4 topology edits per dab, scaled by total face count. Further refinement continues on later dabs. Discrete checks cap candidate-pair work; swept checks cap subdivisions and reject unresolved close approaches. Exhaustion is a rejection, never a successful partial check. An entire-stroke snapshot and exact admission/geometry snapshots add memory proportional to mesh size.

Tests cover overlapping/opposing strokes, multiple chunks, UV seams, 128-valence open boundaries, actual refinement/coarsening, inverted faces, disconnected sheets, thin adjacent overlaps, initial invalid geometry, rollback/replay, flip-only updates and external edits. Existing import/UV/topology regressions remain applicable.

Run correctness checks:

    cargo test -p sculpting --features bevy

Run the manually selected repeated-stroke cost fixture:

    cargo test -p sculpting --features bevy --config 'profile.test.package.sculpting.opt-level=3' --config 'profile.test.package.painting.opt-level=3' --test layered_sculpt_safety measure_ -- --ignored --nocapture --test-threads=1

The cost fixture reports first and later dabs, full stroke times, and rejection counts for approximately 1k, 4k, 16k and 65k triangles. These engine measurements do not establish native interactive performance. Dense-mesh latency and the real render/input path must be qualified separately before claiming smooth interactive use.

The standalone low-level deformation/tessellation helpers are not complete stroke transactions. Application code must use `SculptingPipeline` for this guarantee.
