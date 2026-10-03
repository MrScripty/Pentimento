# Research book outline

## Purpose and central argument

**Title:** Pentimento Geometry and Collaborative Painting  
**Author:** Puma

The book explains how to build an editable, paintable surface whose history can be synchronized and replayed. Geometry, appearance and collaboration share identities and dependencies: a paint sample refers to a surface; remeshing changes that surface; undo changes which operations contribute to the current result. Treating these as unrelated subsystems makes a convincing single-user demo easier and reliable collaboration harder.

The book will derive the algorithms, show implementation-oriented Rust examples and include small reproducible experiments before larger representative benchmarks. Every chapter ends with the invariants its algorithm preserves, failure cases it cannot solve, a concrete Pentimento integration point and an evidence checklist. Artist-facing button sequences are outside scope.

The outline is a research blueprint. Algorithms described below are candidates or teaching material, not a promise that Pentimento already implements them. The pinned audit is the authority for current behavior. Source IDs refer to [the source map](source-map.md).

## Part One Geometry with explicit contracts

### 1 What the editor must preserve

Start with the editable document rather than a renderer. Separate the abstract surface, its embedding in three-dimensional space, corner attributes, material channels, brush operations and derived GPU buffers. Define identity across a mutation and distinguish the primary document from acceleration structures and caches. Describe closed surfaces and surfaces with boundary as different supported contracts.

Develop a worked example: two authors paint opposite sides of a thin sculpted shell, one author remeshes it, and the other later undoes a stroke. Show why a triangle array index cannot by itself identify the intended paint target. Introduce topology epochs and correspondences as proposals to evaluate, not hidden implementation assumptions.

Implementation artifact: a compact domain diagram and Rust type sketch for MeshRevision, SurfacePoint, CornerAttribute and OperationId. Figure: one object shown as topology, embedding, appearance and operation history. Evidence: import–edit–export round trips preserve declared attributes and identity semantics.

### 2 Manifolds, links and numerical predicates

Define stars and links of simplices. For a triangle surface, build the link of a vertex from the opposite edge of every incident face. Explain why a single cycle identifies an interior manifold neighborhood and a single path identifies a boundary neighborhood under the stated simplicial assumptions. Demonstrate that edge valence alone is insufficient using two tetrahedra touching at one vertex.

Derive Euler characteristic as a useful global consistency check, then show that it is not a complete local validity test. Separate orientation, duplicate simplices, degeneracy, self-intersection, watertightness and non-manifold incidence. Explain scale-aware error bounds and why a guessed epsilon cannot repair a wrong combinatorial predicate. Read S01, S02 and S03.

Implementation artifact: independent incidence-based reference validator with precise diagnostics, initially test-only. Figures: disk, boundary half-disk, bow-tie link and three-face edge. Evidence: known-good and one-defect fixtures, including valid valence-two boundary corners and high-valence vertices.

### 3 Half-edge storage and atomic topology changes

Explain how origin, next, prev, twin and face relations encode a surface. Distinguish absent boundary twins from deleted elements. Derive the local rewiring for split, collapse and flip, including exact element counts under each admitted precondition. Make invalidation of handles, selections, acceleration structures and corner data explicit.

Use a mutation transaction: gather a local patch, validate eligibility, compute replacements, validate the candidate patch, then commit. Evaluate whether a small edit journal or copy-on-write patch is the simplest rollback mechanism. Storage compaction must preserve live geometry; repair must be a separately named operation with a report. Show the current HashSet-dependent deletion as a counterexample to that separation.

Implementation artifact: a patch-level mutation API and a small stable-ID map. Figure: before/after rewiring with invariants rather than a long unannotated pointer listing. Evidence: rejected mutation leaves the document unchanged; split and collapse preserve expected topology; compaction preserves geometry and all external references. Sources S01–S03 and pinned Pentimento code.

### 4 Dynamic topology around a brush

Derive a size field from world-space brush support, projected screen size and local curvature. Use a local quadratic surface approximation to motivate why geometric approximation error depends on curvature and edge length; state approximation assumptions. Separate a user's persistent shape from view-dependent tessellation policy.

Study split/collapse hysteresis, candidate priority queues, stale candidate invalidation, protected feature curves and boundary constraints. Contrast remeshing before deformation with deformation before remeshing and identify what a correspondence must preserve. Investigate chunk boundary topology rather than only matching duplicated positions.

Implementation artifact: budgeted local remesher with explicit stopping reasons and deterministic tie-breaking where replay requires it. Figure: footprint, size field and mutation frontier. Evidence: repeated strokes across chunk boundaries, thin sheets, poles, saddle regions and camera changes. Sources S03, S04 and S18.

### 5 Surface deformation and brush kernels

Develop a radial brush weight w(r), pressure mapping and finite displacement field. For smoothing, derive uniform and cotangent Laplacians and discuss shrinkage, boundary handling, stability and negative weights. Treat grab as a displacement constraint and compare normal displacement, flattening, pinching and volume-aware alternatives.

Explain that normals used for a stroke may be evaluated on the evolving surface or captured at a defined point in the stroke. That decision changes repeated-dab behavior and replay inputs. Distinguish surface-distance support from Euclidean spheres that unintentionally reach the other side of a thin object.

Implementation artifact: pure kernel evaluation separated from neighborhood selection and mutation application. Figures: geodesic versus Euclidean brush influence, normal field and smoothing stencil. Evidence: flat-plane invariance, symmetry, bounded displacement, thin-shell isolation and step-size sensitivity. Additional primary differential-geometry references must be read before final derivations are accepted.

## Part Two Alternative representations and deliberate retopology

### 6 Voxels, signed distance fields and hybrid editing

Define occupancy, scalar fields, signed distance and the zero isosurface. Explain the conditions under which the sign is meaningful for an imported surface. Derive Boolean field combinations and discuss the loss of the exact distance property after min/max composition. Compare dense grids, sparse blocks, octrees and VDB-style storage without asserting a universal winner.

Evaluate additive sculpt fields, narrow-band maintenance, resampling error, thin features and memory growth. A volume is an alternative authority for shape, not a guarantee of a usable extracted mesh. A hybrid design needs explicit correspondence and attribute transfer when converting back to an editable surface. Sources S05–S07.

Implementation artifact: small analytic SDF fixtures and a restricted brush-field prototype. Figure: a cutaway surface beside distance samples and a sparse active band. Evidence: sphere and box error versus resolution, thin-shell survival, memory footprint and extraction consistency. All timing comparisons await a representative environment.

### 7 Extracting surfaces without hiding ambiguity

Compare primal isosurface extraction and dual contouring. Derive the dual-contouring quadratic error function from Hermite samples: minimize the sum of squared distances to sample tangent planes. Discuss singular systems, bounded cell placement and feature preservation.

Explain ambiguity and topology policies rather than presenting a marching lookup table as a complete correctness proof. Examine transitions between resolutions and crack prevention. Separate topology of the sampled field, topology of its continuous interpolation and topology of the extracted mesh. Source S06 and additional primary ambiguity-resolution references to be selected during chapter research.

Implementation artifact: a small extractor that reports ambiguous cases. Figures: corner signs, tangent-plane intersection and adaptive-level transition. Evidence: analytic surfaces, adversarial sign configurations, watertight edge incidence and vertex-link checks.

### 8 Error-controlled decimation

Derive the quadric of a plane p: Qp = p pᵀ in homogeneous coordinates. Accumulate vertex quadrics and minimize x̄ᵀ(Qa + Qb)x̄ subject to chosen boundary or feature constraints. Explain singular systems and endpoint/midpoint fallback candidates. Eligibility is a topological and geometric decision; the error objective only ranks eligible contractions.

Extend the discussion to UVs, normal discontinuities, material boundaries and texture error. Compare the current tangent-biased midpoint placement against a constrained QEM baseline with identical safety rules. Do not claim that QEM alone preserves manifoldness or topology. Source S04, plus S01 and S03.

Implementation artifact: deterministic candidate ranking and recorded rejected-contraction reasons. Figures: quadrics as intersecting plane constraints, feature-protected collapse. Evidence: point-to-surface distance, normal error, seam displacement, material preservation and component/genus invariants where promised.

### 9 Retopology and surface correspondence

Define the desired result before the method: edge flow, feature alignment, quad-dominant or all-quad topology, singularities, density, deformation readiness and subdivision behavior. These cannot be summarized by triangle count. Explain cross fields, orientation consistency, parameterization and integer constraints at a level suitable for implementing or integrating a remesher.

Compare Instant Meshes and QuadriFlow using their actual published objectives and limitations. Automatic output must still be assessed against Pentimento's declared semantic constraints. A retopology operation needs a forward/backward correspondence for paint, masks, sculpt layers and selections. Sources S08 and S09; Mudbox behavior S19 is a requirements reference only.

Implementation artifact: retopology adapter contract with revision identity, cancellation and transfer report. Figure: a shape with direction field, singularities, output edges and transfer rays. Evidence: feature curves, parameterization distortion, singularity count and texture/normal transfer error.

## Part Three Appearance and depth

### 10 Texture domains and attribute ownership

Compare vertex color, corner attributes, UV atlases, UDIM tiles and per-face textures. Explain that UV seams are discontinuities in a parameterization, not necessarily boundaries of the surface. Develop a face-corner representation that keeps a topological weld from destroying UV charts.

Define tile addressing, negative/out-of-range UV policy, pixel-center conventions, channel formats and texture revision ownership. Explain Ptex adjacency and cross-face filtering requirements; a dictionary of per-face squares does not automatically implement the Ptex format or its filtering semantics. Sources S10, S11 and S20.

Implementation artifact: SurfaceAddress enum and channel-aware storage interface with one owner for UV orientation. Figures: welded topological vertex with distinct UV corners, UDIM layout and cross-face texture support. Evidence: seam round trips, non-square tiles, atlas edges and texture-coordinate orientation.

### 11 Projection painting as visibility and resampling

Derive camera rays from intrinsics and extrinsics; intersect rays with triangles using barycentric coordinates. Treat projection as both a visibility problem and a resampling problem. Compare forward splatting from paint pixels with inverse gathering from destination texels; each has different hole, overlap and filtering behavior.

Derive the local Jacobian between the source image and the texture domain. Its singular values characterize the anisotropic footprint. Account for grazing angles, depth discontinuities, backfaces, nearest-surface tests and nonuniform object transforms. Paint through an occluder only under an explicitly selected policy. Sources S12–S14 and S20.

Implementation artifact: shared batch/live projection kernel with dirty-region scheduling and a visibility provenance record. Figures: projector rays, occluder, surface tangent basis and texture ellipse. Evidence: checkerboards on curved surfaces, thin double walls, occlusion edges and nonuniform scale.

### 12 Compositing, seams and filtering

Derive premultiplied-alpha over compositing and show a numeric example where reversing stroke order changes the result. Separate linear-light arithmetic from encoding for display. Distinguish opacity per dab, opacity per stroke, flow per unit time and brush accumulation semantics.

Explain mipmaps and anisotropic/EWA filtering in relation to a projected brush footprint. Cover chart gutters, dilation, seam-neighbor transport and matching filters at multiple resolutions. Do not use a seam-padding field as proof that padding is populated. Sources S11, S12 and S15.

Implementation artifact: reference compositor and golden numeric cases independent of GPU implementation. Figures: source/destination sampling footprints, premultiplied color, seam gutter across mip levels. Evidence: order-sensitive blends, transparent edges, energy/coverage behavior, mip seams and CPU/GPU agreement within a declared tolerance.

### 13 Depth is a measurement with a camera model

Separate hardware depth-buffer values, axial camera depth, distance along a ray, normalized display depth and displacement maps. Derive back-projection X = z K⁻¹[u,v,1]ᵀ for axial pinhole depth, with the assumptions about calibration and units stated. Use the inverse projection matrix for general projection conventions instead of reusing one perspective-only formula.

Develop uncertainty propagation: a pixel/depth covariance maps through the back-projection Jacobian to a three-dimensional covariance. Explain confidence, missing measurements, outliers, discontinuities and why unknown space is not known empty space. Introduce camera-pose uncertainty and the limits of monocular relative depth before any learned-model integration.

Implementation artifact: typed depth image with intrinsics, pose, units, validity and uncertainty. Figures: axial versus radial depth and a growing uncertainty ellipsoid. Evidence: analytic planes, perspective/orthographic/reverse-Z round trips, near/far conventions and invalid pixels. Source S13 and reconstruction sources S16–S17; camera-calibration references will be added before chapter completion.

### 14 Reconstruction and depth-guided edits

Derive weighted TSDF fusion from accumulated weighted signed distances: Dnew = (W D + w d)/(W + w), with an explicit truncation and weight policy. Explain which choices remain order-independent in exact arithmetic and which clamps, rounding, truncation or pose changes break that property. Reconstruction also needs alignment; fusion is not a camera tracker.

Compare TSDF fusion with Poisson/screened Poisson reconstruction of oriented points. Discuss scan boundaries, missing regions and confidence. Integrate depth with projection visibility, brush support and sculpt displacement without confusing measured shape with inferred fill. Sources S16, S17 and S24.

Implementation artifact: synthetic calibrated depth generator and small reconstruction experiment. Figures: two depth observations, signed bands, weight accumulation and confidence-aware paint. Evidence: shape and normal error against known geometry, noise/outlier sweeps, pose-error sensitivity and topology after extraction.

## Part Four A brush engine that can be replayed

### 15 Sampling, dynamics and adjustable presets

Model input as timestamped position, pressure, tilt and other supported channels. Derive arc-length resampling with a residual carried across input segments. Interpolate pressure and orientation at emitted positions. Treat time-based flow as an independent integral so a stationary airbrush and a distance-spaced brush have explicit behavior.

Develop configurable response curves, stable low-pass filtering, spacing, hardness, aspect ratio, orientation, jitter, texture stamps and symmetry. Capture the random generator algorithm and seed when randomness affects replay. Presets should contain values and versioned assets rather than an ID whose meaning silently changes. Sources S21 and pinned brush code.

Implementation artifact: input-to-dab function with no renderer dependency and a serializable resolved brush definition. Figures: the same path sampled at different rates, residual-distance carry and pressure curve. Evidence: segmentation invariance, stationary flow, pressure jumps, sharp corners, zero-duration input and reproducible jitter.

### 16 Packet formats and deterministic materialization

Define packet origins, previous decoded positions, units, rounding, overflow and packet sequence. Derive quantization error bounds separately for position, radius, pressure and normal encoding. Explain why encoding final dabs can simplify replay at a bandwidth cost, while encoding raw device input requires binding the entire brush-engine state.

Compare exact cross-platform canonical state with tolerance-based render agreement. GPU floating-point behavior, parallel reductions and traversal order cannot be assumed identical. Bind the mesh revision, texture revision, brush/asset version and operation dependencies required to reproduce the promised result. Sources S22–S23 and the repository replay audit.

Implementation artifact: versioned wire schema with byte fixtures, bounded decoder and reference materializer. Figures: one stroke split into ordered chunks and the checkpoint closure required for replay. Evidence: round trips, overflow splits, missing/repeated packets, unsupported versions and native/WASM cross-runs.

## Part Five Collaboration as an editing contract

### 17 Causality, convergence and visible intent

Distinguish a transport log, an operation set, a CRDT and a deterministic renderer. Derive happens-before and causal dependencies, then specify a deterministic ordering for concurrent noncommutative operations. A convergent operation set plus a pure canonical reducer can converge, but late operations may force replay and visibly change a previous preview.

Use two colored translucent strokes to demonstrate noncommutativity. Use smooth, smudge and collapse to show operations that read and change state. Explain why wall-clock milliseconds cannot establish causal order or global uniqueness. Sources S22 and S23.

Implementation artifact: small replicated operation-set simulator with delivery permutations and a declared canonical render order. Figures: event DAG, two legal arrival orders and one agreed materialization. Evidence: duplicates, reordering, partitions, reconnection and equivalent operation sets. Do not call a proposed protocol verified until these tests cross its real boundaries.

### 18 P2P transport, streaming and bounded resources

Map live presence, ephemeral preview dabs, committed stroke chunks, checkpoints and texture/mesh assets onto separately budgeted data flows. Evaluate Iroh gossip, blobs and docs according to current official contracts. Transport authentication is not document authorization. Durable missing-data recovery must be separate from transient broadcast.

Develop backpressure, per-document limits, cancellation, chunk retransmission, interest regions and checkpoint discovery. Derive a workload model with authors, dabs per second, bytes per dab and relay/fan-out overhead. Measure bandwidth and tail latency rather than assuming compressed packets imply a fast system. Source S25 and version-pinned Iroh documentation to be established for implementation.

Implementation artifact: two-peer harness, loss/delay/duplication simulator and packet accounting. Figures: reliable content path beside ephemeral preview path. Evidence: commit durability after restart, bounded queues, slow peers, stale sessions, malformed packets and authorization failures.

### 19 Undo, checkpoints and compaction of history

Define who may undo which operation and how redo interacts with a new branch of work. Compare operation visibility changes, semantic inverse operations and reconstruction from checkpoints. Distinguish local tile snapshot rollback from collaborative selective undo.

Checkpoint identity must include the operation frontier and every versioned input that affects reconstruction. Garbage collection needs a policy for offline peers; an indefinitely offline peer and unbounded undo history imply different retention costs from a bounded session. These are product decisions, not transport implementation details. Sources S22–S23.

Implementation artifact: authored undo operations and affected-region rebuild. Figures: local contribution removal with another author's stroke preserved; checkpoint DAG. Evidence: undo after remote overlap, undo while offline, tombstone replay, checkpoint cold load and explicit history-retention limits.

### 20 Concurrent topology and preservation of intent

Analyze three credible policies: serialize topology transactions within a mesh revision while keeping paint responsive; replay deterministic topology operations from an immutable base and canonical order; or branch topology and reconcile through an explicit correspondence/merge workflow. Regional leases may limit conflicts but change offline behavior. None should be smuggled in as an unquestioned consequence of P2P.

Define a stroke's surface anchoring semantics: object/world point, barycentric point on a specific face revision, persistent surface coordinate or a projection instruction. Test which intent survives remeshing and deformation. A nearest-point fallback can jump between thin layers and therefore needs confidence and ambiguity handling.

Implementation artifact: topology-revision protocol and transfer result that can report ambiguous, unavailable and rejected outcomes. Figures: concurrent collapse and paint, invalid old face references and alternative resolution results. Evidence: identical final operation sets, deterministic accepted topology, paint-transfer error and user-visible conflict outcomes.

### 21 Performance engineering and the repair program

Create the measurement model before selecting data structures. Separate input-to-local-preview latency, remote-preview latency, committed convergence, topology time, texture upload time, frame time, memory and cold replay. Profile representative release builds using recorded workloads and compare each proposed optimization with a baseline.

Present the staged repairs from the source audit, then use measured results to choose CPU/GPU splits, spatial indexing, tile sizes, parallelism and caching. Include CPU reference paths as test oracles where justified, not as permanent duplicated production authority. Treat a shader preview, a compiler pass and a two-peer editing session as different evidence.

Implementation artifact: reproducible experiment manifest, raw results and scripts that regenerate plots. Figures: latency distribution, memory-versus-scale curves and a correctness/performance trade-off map. Evidence: before/after confidence intervals or suitable variability reporting, correctness preserved and limitations disclosed.

## Appendices

- A: notation, units, coordinate conventions and numerical tolerances
- B: minimal mesh fixtures and independent validity oracle
- C: stroke, packet, material and topology revision schemas
- D: derivations and implementation-oriented Rust listings
- E: reproducible experiment commands, environments and result schemas
- F: source audit, immutable citations and an index of claims
- G: licenses and provenance for incorporated code, datasets and illustrations

## Illustration and cover program

The front cover will depict a sculptural surface transitioning through a clean half-edge mesh, a sparse signed-distance cutaway and a restrained field of colored paint samples. The composition should explain the book's subject rather than imitate a commercial sculpting product. Place the title and author name Puma as clean, readable typography.

The back cover will use a related original illustration: three peers contributing differently colored strokes to the same evolving surface, with a small visual treatment of an operation history. Its copy should state the engineering audience, mathematical and implementation focus, and collaborative scope. It must not invent an ISBN, publisher, endorsement or measured performance claim.

Scientific figures should be generated from companion data or code, with labels, units and accessible contrast. A visual aesthetic is not evidence: diagrams showing a proposed system must be labeled proposed. Cover art can use image generation; topology diagrams and measured plots should use precise code-native drawing where that gives better control. Both covers and every manuscript page must be rendered and visually inspected before final delivery.

## Completion criteria for the book

Each finished chapter has primary citations read at the depth of its claims; definitions and assumptions adjacent to derivations; runnable listings or precisely scoped pseudocode; at least one illuminating counterexample or experiment; and a current-versus-proposed Pentimento integration note. The final manuscript has a consistent notation glossary and references, editable source and an accessible rendered PDF. All performance results include raw data and environment records. Unrun experiments remain proposed and are not drawn as result charts.
