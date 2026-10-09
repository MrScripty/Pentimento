# Sculpt geometry integrity

## Contracts in this repair

- `HalfEdgeMesh::from_bevy_mesh` preserves declared index connectivity. It checks
  triangle topology, attribute lengths/finite values, index bounds, nondegenerate
  triangles, unique directed-edge ownership and facets (including reversed
  duplicates), and complete manifold vertex links.
  Invalid input returns an error rather than panicking or deleting faces.
- `from_bevy_mesh_welded` is an explicit sculpt-import policy. It retains the
  previous 1e-6 object-space, truncation-based position quantization. It is not
  a pairwise-distance weld or an inference of artist intent. A collapsed face
  or non-manifold weld is rejected. Callers can retry the original indexed
  mesh through the connectivity-preserving importer.
- UV0 belongs to face corners. Position-welded topology therefore retains both
  sides of a UV seam. Splits interpolate UVs separately on each side; partition,
  merge, compaction and render export carry the corner values. Collapse touching
  a UV-seam vertex and flip across a UV seam are conservatively rejected.
  This does not introduce general custom-attribute, UV1, material-slot, or
  split-normal preservation.
- Complete manifold checking builds each vertex link from every live face,
  independently of an outgoing-half-edge traversal. An interior link must be
  one cycle and a boundary link one path. Boundaries and valence greater than
  100 are supported. Geometric self-intersection is a separate property.
- `try_compact` removes only tombstones and unused vertices, with stable ID
  remapping. Malformed live faces and duplicate directed edges cause an atomic
  error. The compatible `compact` wrapper logs that error and returns identity
  maps, leaving geometry unchanged; it never chooses a face to delete.
- Triangle collapse checks the full simplicial link condition, including shared
  link edges (the tetrahedron counterexample). Placement prediction inspects
  surviving faces at both endpoints and rejects zero-area, flipped or non-finite
  results. Local twin pairing is restored before the next operation.
- Render vertices are face corners, not topological vertices. Export supplies a
  render-to-topology mapping. Scene synchronization composes it with chunk IDs
  and patches every render copy during position-only updates, preserving UVs.

## Reproducible CPU verification

Run `./scripts/test-sculpt-geometry.sh`. The canonical launcher also invokes it,
with a separate `sculpt-geometry` workflow so frontend installation failures do
not hide the geometry tests.

The focused suites cover:

1. Two closed tetrahedral fans sharing one vertex, valid boundary triangles,
   complete boundary traversal, and a 130-face vertex fan.
2. UV-corner loss, unintended touching-component welding in the default import,
   duplicate directed edges, invalid indices/attributes, and zero-area faces.
3. Non-destructive and atomic compaction; rejected seam edits; split interpolation
   on both UV charts; and exact UV0 corner conservation on a Bevy UV sphere.
4. Destination-only collapse inversions/degeneracy, full tetrahedron link
   rejection, and twelve successful split/collapse/compact cycles on a closed mesh.
5. Multi-chunk UV-sphere partition/merge, a real `SculptingPipeline` stroke with
   adaptive tessellation and deformation, and headless Bevy `App` render-asset
   synchronization for all render copies of a welded vertex.

## Evidence and limits

At the pinned starting source `f819e593819004690c4465afbb9733cb78b731c2`, the nine
initial import/manifold/compaction/traversal regressions were executed against
the real Rust crate: all nine failed. The repaired owning-crate suite executes
these tests alongside the additional seam/mutation regressions.

Local qualification used Rust 1.92.0 with one compiler job and debug info disabled.
The repaired painting suite passed 99 tests; the feature-enabled sculpting suite
passed 49 unit tests and all four new integration tests. The headless Bevy scene
synchronization regression passed with the active sculpt/selection feature set.
These are new CPU results, not the unrelated normal-codec PR's test count.

CPU tests establish geometry, application scheduling and mesh-asset contents.
They do not establish interactive pointer behavior, GPU presentation, performance
on an artist's production mesh, self-intersection freedom, or collaborative
replay. The v1 normal-decoder repair is separate and is not included here.
No generated images, build products or test logs belong in Git.
