# Pentimento Geometry and Collaborative Painting

## Engineering research book by Puma

This project develops the mathematics, algorithms, invariants and implementation decisions behind a reliable collaborative sculpting and painting system. It is written for engineers building Pentimento. It is not an artist-facing usage guide.

The first research milestone is a pinned source audit, a substantial chapter outline, a primary-source map and an experiment plan. The full book will include an editable manuscript, a rendered and visually reviewed PDF, illustrated front and back covers, and reproducible companion material. This milestone does not claim that the full book, a production repair or an application benchmark is finished.

The central recommendation from the audit is to establish geometry and replay correctness before choosing a replacement surface representation or accelerating network transport. Several small, concrete defects can be isolated without committing Pentimento to a broad rewrite. Dynamic topology, voxels and signed distance fields remain alternatives to be tested against the same user-visible contracts.

## Read in this order

1. [Source audit](source-audit.md) identifies what the current code actually implements, its correctness gaps and the limits of the evidence.
2. [Book outline](book-outline.md) maps the theory and implementation work to chapters, derivations and figures.
3. [Source map](source-map.md) identifies primary references and exactly what each can support.
4. [Evidence and repair plan](evidence-plan.md) defines the experiments, staged repairs and unresolved product decisions.
5. [Companion probes](companion/README.md) reproduces five source-model counterexamples with no external dependencies.

## Baseline and evidence vocabulary

- Repository: [MrScripty/Pentimento](https://github.com/MrScripty/Pentimento)
- Inspected commit: [f819e593819004690c4465afbb9733cb78b731c2](https://github.com/MrScripty/Pentimento/tree/f819e593819004690c4465afbb9733cb78b731c2)
- Research date: 3 October 2026 UTC
- Coding standards consulted: MrScripty/Coding-Standards at dcc56f26e884ade260770beceba2501d3746200d, selecting Core, Router, discovery/planning, implementation, documentation, verification and independent oracles, performance and replay guidance relevant to this work.

Every result uses one of these labels:

- **Source observation:** visible behavior or control flow in pinned source.
- **Source-model result:** an executed, deliberately small model of inspected arithmetic or graph traversal; it is not compiled Pentimento.
- **Hypothesis:** a plausible application consequence still requiring an in-repository reproducer.
- **Proposed:** a design or experiment that has not been implemented or accepted.
- **Measured:** reserved for a named experiment run with recorded environment, inputs, configuration and results.

No Pentimento build, GUI workflow, two-peer sync session or application performance experiment was run in this milestone. Lightweight Python probes were run. No production source was changed.

## Editorial and implementation boundaries

Preserve P2P collaborative editing as a design goal. Do not describe an append-only local log as a completed CRDT. Do not conflate reliable transport with deterministic replay, manifold topology with embedded non-self-intersection, decimation with retopology, or a depth visualization with depth reconstruction.

Blender source and developer documentation can establish public implementation facts. Mudbox, Mari and 3DCoat documentation can establish documented behavior and useful comparison requirements, but it does not establish proprietary internal algorithms. Illustrations should be original explanatory drawings, not copies of commercial product artwork.

The proposed repository destination is docs/engineering-book/ for manuscript and research materials, with an eventual active implementation plan under docs/plans/geometry-and-replay-correctness/. The latter must clearly distinguish research proposals from admitted implementation decisions. No pre-existing book build convention was found in the inspected repository tree.
