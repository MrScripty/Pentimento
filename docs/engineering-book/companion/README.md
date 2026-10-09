# Companion source model probes

Run:

```sh
python3 source_model_probes.py
```

The script uses the Python standard library only and takes negligible runtime on ordinary development hardware. It asserts and prints five counterexamples derived from Pentimento commit f819e593819004690c4465afbb9733cb78b731c2:

1. The sculpt normal codec maps +X close to −X.
2. A moving packet base shifts a documented forward-delta replay.
3. Repeated short input segments produce a dab beyond the pointer.
4. Two tetrahedral vertex fans pass the modeled validator but have a disconnected link.
5. Duplicate-directed-edge face deletion picks a different survivor with different face traversal orders.

The recorded JSON is [source-model-results.json](source-model-results.json). These probes ran successfully on 3 October 2026 UTC. The assertions confirm the modeled counterexamples, not that Pentimento is correct. No application latency or throughput is measured. Python arithmetic is not an exhaustive emulation of Rust f32 operations; the selected examples use large, structural errors rather than precision-boundary behavior.

The graph oracle is independent of the ring-walk subject: it builds the vertex link directly from the triangle incidence list. A connected cycle is the expected closed interior link. The model intentionally covers closed fixtures and rejects attempts to use it as a general boundary-mesh validator.

Port each relevant fixture to its real Rust owner before accepting a production repair. Retain source models as explanatory book material only while they still have teaching value; do not turn them into a second production implementation.
