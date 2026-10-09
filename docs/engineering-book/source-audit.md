# Pinned source audit

## Summary

The strongest current risk is that correctness is inferred from local data-structure checks that do not establish the promised geometric or replay semantics. The code has substantial useful machinery: editable half-edges, split/collapse/flip operations, chunk boundaries, tiled painting, compact stroke packets and local undo. These should be retained where their contracts can be repaired and tested.

The audit found five small counterexamples reproducible as source models, and several additional direct source observations. The original report of non-manifold sculpting has not yet been reproduced through the running application. It would be premature to name any one finding as its complete cause.

All GitHub links below are pinned to f819e593819004690c4465afbb9733cb78b731c2. These findings are about that commit, not a claim about future main.

## 1 The manifold validator cannot see every vertex fan

**Source observation and source-model result.** [check_manifold](https://github.com/MrScripty/Pentimento/blob/f819e593819004690c4465afbb9733cb78b731c2/crates/painting/src/half_edge/validation.rs#L194-L269) checks twin relationships and then compares counts returned by two traversal helpers. Both [get_vertex_faces and get_adjacent_vertices](https://github.com/MrScripty/Pentimento/blob/f819e593819004690c4465afbb9733cb78b731c2/crates/painting/src/half_edge/topology.rs#L84-L216) start at one outgoing half-edge, follow prev then twin, and stop on a boundary, repeated edge or 100 iterations.

Two closed tetrahedral surfaces sharing one vertex have two disconnected cycles in that vertex's link. Every edge can still have exactly two oppositely directed incident faces. The traversal starts in only one tetrahedron, counts three neighbors and three faces and never visits the other fan. The current-check source model accepts this fixture; an independent link-graph construction counts two components.

There are other weaknesses. Saturating subtraction turns any negative count difference into zero; zero is accepted even for a boundary vertex. The blanket rejection of valence below three also rejects valid boundary triangulations such as an isolated triangle. Consequently, a validator can have both false acceptance and false rejection. Self-intersection is explicitly not checked, and should be a separate geometric property rather than part of the definition of an abstract manifold.

**Proposed response:** derive incidence independently from live faces for an offline/test oracle. Check each edge's incidence and each vertex's complete link, distinguishing one cycle, one path and invalid links. Make boundary support explicit. Use local mutation preconditions for the hot path; do not blindly add a full mesh scan to every dab.

## 2 Position welding changes topology and loses corner attributes

**Source observation.** [from_bevy_mesh](https://github.com/MrScripty/Pentimento/blob/f819e593819004690c4465afbb9733cb78b731c2/crates/painting/src/half_edge/construction.rs#L48-L121) quantizes positions at a fixed scale of one million, chooses the first vertex in each bucket and redirects triangle indices. It retains UVs and normals on vertices, not face corners. [to_bevy_mesh](https://github.com/MrScripty/Pentimento/blob/f819e593819004690c4465afbb9733cb78b731c2/crates/painting/src/half_edge/construction.rs#L241-L280) emits the retained vertex UV for each face corner.

This is not a tolerance-based pairwise weld and not merely deduplication of identical data. It can join distinct touching sheets; nearby positions on opposite sides of a bucket edge remain separate; truncation behavior differs around zero. UV seam duplicates that were intentionally separate now share the canonical vertex's UV. Removing triangles with repeated welded indices does not restore lost seams or prove that the surviving mesh is a manifold.

**Proposed response:** separate topological vertices from corner-domain UV, normal and material attributes. Import should either retain declared connectivity or perform an explicitly scoped weld with a report. Establish units and tolerances from the asset contract. Test coincident disconnected components as well as UV spheres.

## 3 Construction can overwrite directed-edge ownership

**Source observation.** In [construction lines 186–199](https://github.com/MrScripty/Pentimento/blob/f819e593819004690c4465afbb9733cb78b731c2/crates/painting/src/half_edge/construction.rs#L186-L199), a directed edge is inserted into a HashMap without rejecting an existing owner; a reverse lookup may also relink an already paired edge. The constructor returns the resulting object without performing a comprehensive import validation. Invalid index and attribute-length inputs are also directly indexed earlier in the constructor rather than converted into typed import errors.

**Proposed response:** validate indices, attribute cardinalities, finite values, triangle degeneracy, repeated oriented edges and incidence before constructing the trusted editable mesh. Keep recovery separate from ordinary import success. Whether repair is automatic is a product decision because a repair may remove or split artist geometry.

## 4 The collapse flip guard inspects only one endpoint

**Source observation.** [would_cause_flip](https://github.com/MrScripty/Pentimento/blob/f819e593819004690c4465afbb9733cb78b731c2/crates/sculpting/src/tessellation/edge_collapse.rs#L463-L530) examines faces adjacent to v0 and replaces only v0 while predicting the new geometry. A collapse also redirects surviving faces incident to v1. Those faces need inspection. The test rejects negative normal dot products but not zero-area triangles, for which the dot product can be zero.

The current [common-neighbor test](https://github.com/MrScripty/Pentimento/blob/f819e593819004690c4465afbb9733cb78b731c2/crates/sculpting/src/tessellation/edge_collapse.rs#L234-L270) checks cardinality. Full simplicial link equality contains edges as well as vertices; a tetrahedral boundary is an important small fixture. A geometric placement objective cannot replace the topological eligibility test.

**Proposed response:** compute the surviving face set from both endpoint stars, exclude intentionally removed faces, simulate both endpoint substitutions, require nondegenerate oriented faces, and use the appropriate full link condition for the admitted mesh class. Verify that a rejected operation is atomic and leaves all authoritative state unchanged.

## 5 Compaction performs nondeterministic destructive repair

**Source observation and source-model result.** [compact](https://github.com/MrScripty/Pentimento/blob/f819e593819004690c4465afbb9733cb78b731c2/crates/painting/src/half_edge/modification.rs#L861-L988) iterates a HashSet of live face IDs. For duplicate directed edges it retains an earlier encountered owner and marks a later face for removal. Different legal iteration orders retain different geometry. The source-model probe demonstrates the two outcomes for two triangles sharing a directed edge.

Additionally, an edge ownership entry inserted before a face is later rejected remains in the temporary map during this scan. That can make the outcome depend on rejected intermediate faces. Compaction should not hide the origin of an invalid mutation by deleting geometry. Sorting would make this repair repeatable, but would not make it semantically correct or non-destructive.

**Proposed response:** distinguish storage compaction, which preserves the live abstract mesh, from an explicit repair operation with provenance and a declared choice policy. Capture a failing pre-compaction artifact before investigating the mutation that produced it. Stable logical IDs and remapping must cover paint, selection, undo, external references and topology revisions, not only chunk-local vertex arrays.

## 6 Release connectivity checks do not establish geometry validity

**Source observation.** [validate_connectivity](https://github.com/MrScripty/Pentimento/blob/f819e593819004690c4465afbb9733cb78b731c2/crates/painting/src/half_edge/validation.rs#L20-L104) returns unconditional success outside debug assertions. The [pipeline](https://github.com/MrScripty/Pentimento/blob/f819e593819004690c4465afbb9733cb78b731c2/crates/sculpting/src/pipeline.rs#L339-L402) invokes connectivity checks in debug sections around tessellation. These checks can be useful for diagnosis but are neither a release safety contract nor an independent manifold proof.

**Proposed response:** use local checked topology operations whose admitted inputs and atomic outputs maintain defined invariants. Keep the independent full oracle in tests and opt-in diagnostics unless measured risk justifies a broader runtime gate.

## 7 Sculpt normal encoding and decoding disagree

**Source observation and source-model result.** [SculptDab](https://github.com/MrScripty/Pentimento/blob/f819e593819004690c4465afbb9733cb78b731c2/crates/sculpting/src/types.rs#L94-L116) encodes azimuth using theta plus pi, but decodes a nonnegative azimuth without reversing that offset. For the exact unit normal +X, the model yields a decoded normal approximately −X with dot product −1. This is much larger than ordinary 8-bit quantization error.

**Proposed response:** define one spherical or octahedral encoding contract, its angular error metric, zero-vector behavior and byte fixtures. Keep protocol version compatibility explicit. Changing a codec silently can reinterpret persisted packets.

## 8 Sculpt packet headers capture a moving base

**Source observation and source-model result under the documented forward-delta interpretation.** [create_dab](https://github.com/MrScripty/Pentimento/blob/f819e593819004690c4465afbb9733cb78b731c2/crates/sculpting/src/brush.rs#L410-L477) advances base_position after each dab. create_packet later writes that field into the header. For a start at 0 and dabs at 0.1 and 0.2, the deltas are 10 and 10 at scale 100, but the serialized base is 0.2. A decoder starting at the documented base reconstructs 0.3 and 0.4.

**Proposed response:** separate packet origin from previous encoded position. Define whether rounding occurs from the last decoded position or the last input position, so error cannot silently accumulate. Use explicit packet sequence and end-of-stroke framing for overflow, empty strokes, cancellation and partial arrival. No actual network replay decoder was established by this audit, so this is a representation-contract defect rather than a measured two-peer failure.

## 9 Sculpt spacing double-counts short input travel

**Source observation and source-model result.** [update_stroke](https://github.com/MrScripty/Pentimento/blob/f819e593819004690c4465afbb9733cb78b731c2/crates/sculpting/src/brush.rs#L340-L386) adds the distance from last_dab_position to every new input. When no dab is emitted, that position remains unchanged. Samples at 0.02, 0.04 and 0.06 with spacing 0.1 accumulate 0.12 and emit a dab at 0.1, beyond the last input.

**Proposed response:** integrate segment lengths from the previous input point, carry a residual distance and interpolate each emitted dab along its actual input segment. Establish pressure/radius-dependent spacing and time-dependent flow separately. Test segmentation invariance of the same polyline at different input rates.

## 10 The painting log is local storage with future sync hooks

**Source observation.** [StrokeLog::append](https://github.com/MrScripty/Pentimento/blob/f819e593819004690c4465afbb9733cb78b731c2/crates/painting/src/log/storage.rs#L56-L82) pushes packets into an in-memory vector under a write lock and emits callbacks. It does not deduplicate packet identity or establish a replica-independent order. [iroh_key](https://github.com/MrScripty/Pentimento/blob/f819e593819004690c4465afbb9733cb78b731c2/crates/painting/src/log/storage.rs#L121-L128) has no chunk ordinal, while the log documentation permits multiple overflow packets sharing a stroke ID. [StrokeHeader](https://github.com/MrScripty/Pentimento/blob/f819e593819004690c4465afbb9733cb78b731c2/crates/painting/src/types.rs#L35-L70) has no author identity or topology revision; sculpt IDs also start at a local zero counter.

**Proposed response:** preserve the hooks but specify operation identity, idempotency, causal dependencies, complete stroke framing, authentication, replay inputs and target revision before choosing Iroh protocol integration. A future key-per-packet mapping would collide if it used only the existing key. This does not assert that such a remote store currently exists.

## 11 Current paint undo restores whole tile snapshots

**Source observation.** [undo](https://github.com/MrScripty/Pentimento/blob/f819e593819004690c4465afbb9733cb78b731c2/crates/painting/src/pipeline/undo.rs#L73-L104) restores captured tile pixels. That can implement local last-stroke undo in its current context. It is not selective collaborative undo: after another author modifies the same tile, restoring a stale tile can erase their contribution.

**Proposed response:** retain snapshot checkpoints as accelerators, but define collaborative undo as a new authored operation over a target operation or versioned group. Reconstruct affected tiles using a declared operation ordering and dependency policy. Smudge and topology edits require special treatment because their outputs depend on the state they read.

## 12 Projection capabilities and documentation are not aligned

**Source observation.** [live_projection_system](https://github.com/MrScripty/Pentimento/blob/f819e593819004690c4465afbb9733cb78b731c2/crates/scene/src/projection_painting.rs#L414-L453) checks dirty state and then has comments instead of projection work. [PtexTargetStub](https://github.com/MrScripty/Pentimento/blob/f819e593819004690c4465afbb9733cb78b731c2/crates/painting/src/projection_target.rs#L242-L321) logs warnings and returns no dirty output. Its comment says it panics, but its actual methods do not. A separate [MeshPtexSurface](https://github.com/MrScripty/Pentimento/blob/f819e593819004690c4465afbb9733cb78b731c2/crates/painting/src/mesh_surface.rs#L220-L320) exists, so it would be wrong to claim all Ptex-style storage is absent. The projection target path is the incomplete path.

The batch projector traverses nontransparent source pixels, tests target meshes and selects the nearest world-space hit. Its normal transform uses the forward affine linear transform, which does not preserve normals under general nonuniform scaling. UV Y conventions also differ between UvAtlasTarget and MeshUvSurface and need an actual end-to-end orientation fixture before naming a visible inversion bug.

**Proposed response:** reconcile capability states and use one projection contract for batch and live work. Test nearest visibility, thin shells, backfaces, overlapping UVs, nonuniform transforms, dirty-tile updates and UV orientation. Treat world-to-texture footprint estimation and seam-aware filtering as separate correctness requirements.

## 13 Depth view is a visualization path

**Source observation.** [depth_view](https://github.com/MrScripty/Pentimento/blob/f819e593819004690c4465afbb9733cb78b731c2/crates/scene/src/depth_view/mod.rs) is a fullscreen grayscale depth display with scene-bound normalization. The [shader](https://github.com/MrScripty/Pentimento/blob/f819e593819004690c4465afbb9733cb78b731c2/crates/scene/src/depth_view/shaders/depth_view.wgsl#L25-L43) assumes infinite reverse-Z perspective depth and reads MSAA sample zero. The Rust path also reads an orthographic near plane, but no projection-kind switch is present in this shader. That is an audit target if orthographic depth view is reachable.

**Proposed response:** keep display normalization out of the canonical depth representation. Define axial depth versus ray distance, camera intrinsics/extrinsics, validity masks, uncertainty and units before reconstruction or depth-guided painting. The inspected path is not evidence of TSDF fusion, camera tracking or calibrated reconstruction.

## 14 Canonical verification is not a geometry test suite

**Source observation.** README and ADR-001 designate launcher.sh as canonical. [run_verification_suite](https://github.com/MrScripty/Pentimento/blob/f819e593819004690c4465afbb9733cb78b731c2/launcher.sh#L472-L503) runs source documentation, formatting, npm verification, cargo check and cargo rustc commands. It does not run cargo test for the geometry crates. The older justfile has a workspace-test recipe, but it is not the canonical path.

**Proposed response:** add focused real Rust regressions at the owning crates, then wire the smallest relevant test suite into canonical verification after measuring its cost. A passing frontend compile must not be reported as proving sculpt topology, packet replay or two-peer convergence.

## Audit limits and next observation

This was a targeted inspection of topology, brush, packet, undo, projection and depth paths with their nearby contracts, not an exhaustive security or code audit. The five Python probes reproduce the reasoning of small source expressions. They are useful counterexamples and fixture specifications, but cannot detect Rust-specific wiring, Bevy scheduling, rendering or platform defects.

After the book architecture and design review, the next narrow companion implementation slice should move the normal round-trip, packet-origin and short-segment resampling cases into actual Rust tests and fix those bounded contracts. The first mesh slice should reproduce the disconnected-link acceptance and weld/seam cases using actual constructors. Full GUI and representative performance work remains separate evidence.
