# Evidence and staged repair plan

## Decision and stopping boundary

The first decision is whether Pentimento's current mesh and stroke architecture can be repaired in small, testable slices before any representation replacement. Source inspection already provides enough evidence to justify bounded correctness regressions. It does not justify a broad rewrite, a completed P2P claim or a performance ranking of meshes versus SDFs.

This research milestone stops when the current authority boundaries are described, concrete defects have reproducible fixture specifications, primary sources cover the proposed book and the next repair slice is explicit. That boundary is reached by the accompanying audit, outline, source map and five probes. Full book writing and in-application acceptance continue as later work.

The next book slice is to draft the geometry and stroke foundations, obtain independent mathematical/source review, and explain the design choices before companion implementation. A later bounded implementation slice is **real Rust stroke correctness regressions**: port the normal round-trip, packet-origin and short-segment resampling cases to their actual owners, repair the local contracts, record compatibility implications and run the affected crate tests. Do not change the topology architecture in that slice.

## Current acceptance state

- Pinned source audit and actual-code findings: complete for the declared targeted scope
- Five source-model counterexamples: executed successfully
- Full book chapters and primary derivations: proposed, not complete
- Production source repair: not started
- Compiled Pentimento tests: not run
- GUI sculpting reproduction: not run
- Two-peer synchronization and convergence: not run
- Representative performance benchmarks: not run
- Front and back covers, editable book and rendered PDF: not yet produced

## Invariants to agree and own

Geometry should own admitted topology, units, stable logical identities and mutation outcomes. Paint should own color/channel representation, parameterization and texture materialization. A stroke record should own resolved brush inputs and assets sufficient for its advertised replay promise. The collaboration layer should own identity, causality, authorization, synchronization and history semantics. Render buffers and spatial indices should remain derived state with explicit invalidation.

Some choices need user-facing product decisions before they become implementation commitments:

1. Whether editable sculpt meshes include open surfaces and intentionally disconnected touching parts
2. Whether concurrent topology edits must remain available during offline work, and how ambiguous merges should be shown
3. Whether undo is per-author selective undo, document-global last operation, or both
4. Whether canonical geometry/paint must be bit-identical across supported platforms, or which outputs permit numeric tolerance
5. Whether brush edits during a stroke affect that stroke immediately, and which preset/assets are frozen into history

Research and the bounded codec/resampler repairs do not require resolving every question immediately. Do not invent answers in a wire format that later makes a costly product choice irreversible.

## Staged work

### Stage A Make recorded strokes faithful

Exact proposed write set: sculpting brush and packet types, their focused unit tests and a short protocol note. Keep packet start and previous position separate. Repair azimuth round-trip semantics with version-aware treatment. Correct residual arc-length integration. Cover normal cardinal directions, quantization bins, delta overflow, empty/cancelled strokes, pressure changes and repeated input.

Acceptance: the actual Rust encoder/decoder contract reproduces emitted dab positions within its declared error bounds; equivalent piecewise-linear inputs produce the same distance-spaced dabs under the selected dynamics policy. A compiled focused test result is required. Python models are explanatory fixtures only.

### Stage B Establish an independent geometry oracle

Exact proposed write set: focused tests/reference fixtures near half-edge topology and import. Build incidence directly from live faces rather than reusing the mutation algorithm's ring traversal. Cover interior cycles, boundary paths, disconnected fans, repeated directed edges, duplicate faces, zero-area faces and invalid handles. Keep embedded self-intersection as a separately named test.

Acceptance: positive and negative fixtures are classified by their intended defect; valid open surfaces do not fail a closed-surface assumption accidentally. The oracle must not become expensive permanent production machinery without a demonstrated need.

### Stage C Repair import and atomic local mutation

Bound this into individually reviewable slices. First preserve corner-domain attributes and explicit welding semantics. Then repair collapse eligibility and both-endpoint orientation/area guards. Make compaction preserve live geometry, with separate explicit repair if retained. Address chunk mutation and remapping only after local operations have dependable tests.

Acceptance: operations preserve declared invariants, rejected operations are atomic, and no implicit cleanup deletes artist geometry. Export and topology revision references remain valid. A captured application failure, when available, becomes a regression rather than being replaced by generic sphere tests.

### Stage D Unify appearance and depth contracts

Give UV orientation and pixel-center rules one owner. Reconcile live projection and Ptex projection capability states with actual behavior. Add nonuniform transform and occlusion fixtures. Define depth units, projection convention and validity. Any depth reconstruction experiment is isolated until camera and confidence contracts are established.

Acceptance: selected batch/live paths have equivalent materialization for the same input; expected occluders block paint; seams and tile boundaries are evaluated at appropriate footprints; depth round trips use the actual camera model.

### Stage E Specify collaborative editing before transport integration

Construct a small operation model with author/session identity, monotonically unique operation counters, chunk ordinals, dependencies, target revisions, brush/material versions and bounded decoding. Separate ephemeral previews from committed immutable chunks. Select causal ordering and semantic conflict policy deliberately.

Acceptance: a delivery-permutation simulator demonstrates convergence for its declared operation class; undo preserves unrelated contributions; invalid/stale references return explicit outcomes. Then verify real two-peer transport with the same operation semantics. Do not claim the simulator proves real network behavior.

### Stage F Compare representation and performance alternatives

With trusted correctness evidence, compare the repaired mesh path against a narrowly scoped sparse-SDF prototype and external retopology candidates. Match workload, quality target and output semantics. If an alternative cannot preserve required paint or collaboration semantics, record that before interpreting a speed advantage.

Acceptance: representative measurements, correctness checks and resource costs support a decision. No performance threshold is currently user-approved; proposed workloads are not established budgets.

## Experiments

### E01 Complete local mesh validity

**Claim:** every admitted live vertex has the correct complete link and every edge has the admitted incidence.  
**Evidence:** focused, environment not applicable, automated.  
**Inputs:** triangle disk, tetrahedron, closed triangulated sphere, cylinder with boundary, two tetrahedra sharing one vertex, three-face edge, duplicate oriented/reversed triangles and a high-valence star.  
**Oracle:** independently enumerate face incidence and construct links; known fixtures establish expected classification.  
**Output:** typed diagnostics and serialized minimal failing fixtures.  
**Boundary:** this does not prove absence of spatial self-intersections.

### E02 Mutation sequences and rejection atomicity

**Claim:** split/collapse/flip preserve admitted local and global invariants, and rejected operations do not alter authority.  
**Evidence:** focused plus integration, environment not applicable, automated.  
**Inputs:** seeded sequences over small valid meshes, with boundary/feature constraints and near-degenerate geometry.  
**Oracle:** E01 plus explicit expected local element counts and canonical live-face comparison.  
**Output:** operation trace, seed, pre/post mesh and first violating step. Shrink failures by deleting irrelevant operations and geometry.  
**Boundary:** exact byte hashes are appropriate only for canonical serialization; allocator or HashMap layout is not semantic identity.

### E03 Import and attribute round trips

**Claim:** declared welding preserves topological intent and corner data, and invalid inputs produce typed errors.  
**Inputs:** UV sphere seam, hard-normal cube, material boundary, coincident disconnected sheets, positions around quantization bucket boundaries, invalid indices, nonfinite positions and short attribute arrays.  
**Oracle:** compare face-corner attributes and admitted connectivity independently of render-vertex duplication.  
**Output:** mapping report and before/after seam visualization.  
**Boundary:** a pleasing screenshot is not proof of preserved corner values.

### E04 Chunk-boundary sculpting

**Claim:** partitioning, boundary synchronization, local topology edits and merging preserve one coherent surface and its identities.  
**Evidence:** integration and real GUI workflow.  
**Inputs:** repeat the same stroke path wholly inside a chunk, across one boundary and near multiple boundaries; force rebalance events.  
**Oracle:** compare to an unchunked reference for a supported restricted operation set; apply E01 on merged output.  
**Output:** boundary correspondences, displacement error, triangle/link diagnostics and recorded workflow.  
**Boundary:** position equality of duplicated boundary vertices is necessary but does not prove conforming boundary connectivity.

### E05 Brush sampling and packet replay

**Claim:** the serialized stroke faithfully represents the dabs that were applied.  
**Inputs:** short segments, corners, irregular timestamps, pressure changes, long jumps, overflow, repeated packets, missing packets and normal cardinal directions.  
**Oracle:** the emitted canonical dab list before packing, independently decoded under the documented quantization contract.  
**Output:** per-field maximum and distribution of error, packet bytes and exact fixture versions.  
**Boundary:** direct f32 equality is not automatically appropriate for approximate geometry, but IDs, sequence and version must agree exactly.

### E06 Projection, visibility and texture filtering

**Claim:** paint reaches the intended visible surface with the admitted sampling quality.  
**Inputs:** analytic plane, sphere, steep grazing face, foreground occluder, thin double wall, nonuniformly scaled mesh, overlapping UV chart, UDIM edge and Ptex adjacency.  
**Oracle:** analytic correspondences and a high-sample CPU reference for small scenes, plus exact visible-surface identity checks.  
**Output:** texel error, missed/incorrectly painted regions, seam profiles and source/target footprints.  
**Boundary:** the reference's own numerical error and sampling limit must be stated; no universal PSNR threshold is selected yet.

### E07 Calibrated depth and reconstruction

**Claim:** depth samples map to the intended geometry with stated uncertainty.  
**Inputs:** known planes/spheres, perspective and orthographic cameras, reverse-Z conventions, missing regions, range noise, pose perturbations and outliers.  
**Oracle:** analytic geometry and known synthetic camera transforms.  
**Output:** position/normal error, uncertainty coverage, unknown-space classification and extracted topology diagnostics.  
**Boundary:** synthetic calibration evidence does not establish a real sensor's calibration or a learned depth model's metric accuracy.

### E08 Decimation and retopology

**Claim:** simplification/retopology meets a declared shape and attribute quality target while preserving required features.  
**Inputs:** smooth organic surface, crease, thin sheet, tiny handle, high-curvature detail, UV seam and materials.  
**Oracle:** dense source surface queries plus exact protected-feature and topology checks.  
**Output:** surface-distance distribution, normal error, feature drift, parameterization distortion, singularities and transfer error.  
**Boundary:** sampled distance is an estimate, not a certified Hausdorff bound unless a verified bounding procedure is used.

### E09 Convergence under network disorder

**Claim:** replicas with equivalent admitted operations and dependencies materialize the same canonical state.  
**Evidence:** simulated protocol contract first, then real two-peer system test.  
**Inputs:** reordered, duplicated, dropped/retried and delayed chunks; disconnection, reconnection, concurrent strokes, stale revisions and unsupported versions.  
**Oracle:** canonical reducer over the complete admitted operation set and explicit rejection outcomes.  
**Output:** operation frontier, missing dependencies, convergence time, content identifiers and divergence trace.  
**Boundary:** convergence does not prove that a conflict resolution preserves intended shape or stroke order.

### E10 Collaborative undo and cold replay

**Claim:** undo changes the specified contribution and preserves unrelated admitted contributions, including after restart.  
**Inputs:** overlapping authors, smudge over earlier paint, undo/redo while partitioned, a topology edit followed by paint, missing historical brush assets and old checkpoints.  
**Oracle:** declared semantic policy and canonical reconstruction with the selected visibility/dependency changes.  
**Output:** final tiles/mesh, preserved contribution checks and explicit unavailable/unsupported outcomes.  
**Boundary:** a stale local tile snapshot is not the oracle for collaborative history.

### E11 Resource and security limits

**Claim:** untrusted packets cannot trigger unbounded allocation, infinite replay or unauthorized document changes.  
**Inputs:** oversized lengths, invalid enum values, nonfinite numeric data, cyclic/invalid dependencies, conflicting IDs, missing chunks, stale capabilities and slow consumers.  
**Oracle:** bounded decoder and document-authorization policy; ordinary well-formed peers must remain usable.  
**Output:** typed rejection, retained resource bounds and recovery evidence.  
**Boundary:** transport identity alone does not establish edit permission; cryptography should use established implementations.

### E12 Representative performance

**Claim:** the selected workflow meets an agreed performance budget on an agreed supported configuration.  
**Current status:** unavailable until hardware, workloads and budget authority are specified.  
**Proposed dimensions:** mesh vertices/faces and active patch size; number/resolution/channels of paint tiles; brush support and dab rate; participant count; latency/jitter/loss and relay use; operation-history length and checkpoint distance.  
**Metrics:** input-to-local-preview, remote-preview, commit-to-convergence, frame p50/p95/p99, topology time, bytes per useful dab, resident memory, upload bandwidth and cold replay duration.  
**Method:** pin baseline and candidate commits, release flags, compiler/dependency lockfiles, CPU/GPU/driver, process limits, warm/cold conditions and sampling policy. Save raw samples. Report distribution and variability, not only an average.  
**Boundary:** the five tiny Python probes are not performance evidence.

## Deliverables and reproducibility

The future companion directory should contain deterministic synthetic mesh/depth generators, minimal recorded stroke traces, wire fixtures, reference algorithms, actual Rust tests and experiment manifests. Prefer established harnesses and simple scripts over a new general benchmark framework. Every permanent test or tool should have a named failure it prevents and a maintenance/removal criterion.

The editable manuscript is the source of prose and equations. Generated figures derive from companion inputs; measurements derive from raw result files. A final build should regenerate the PDF, inspect every page at readable scale, verify references and figures, and package a source archive alongside the editable document. Covers are required on both ends. No final delivery should confuse this first research milestone with the completed book.
