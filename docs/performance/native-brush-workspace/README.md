# CPU benchmark receipts

Recorded 2026-10-13 on the same Linux x86_64 cloud CPU, Rust 1.97.1,
release optimization, LTO off, 16 codegen units. Before is
`cd85372493d8e9067ad5530d842480540d26dfa0`; after is the candidate containing
these receipts. Baseline/candidate core binaries had isolated Cargo target
directories and identical harness source and ran sequentially after compilation.

The operation counts describe whole timed blocks, not per-dab time. The original
small-stroke and idle blocks emitted 2,048 dirty tiles; the candidate emitted nine
and zero respectively. `scene.txt` contains explicit per-record parameters.
Pixel and global-position hashes match. Timing samples and geometry hashes are
bounded evidence, not proof for every mesh or a physical GPU/stylus measurement.

See [methodology and remaining costs](../../native-brush-performance.md) and
[core harness details](../../brush-core-performance.md). Reproduce with
`./scripts/profile-native-brushes.sh --all`; use an isolated target directory for
each historical checkout. Full native runtime qualification remains outstanding.
