# Dependency regression source

`lib.rs` is a test-only consumer of the maintained token-pasting macro spellings
and the retained pulp runtime-dispatch API. It checks generated identifiers and
exact dispatched/scalar arithmetic, including short and non-vector-aligned tails.
No production implementation lives here.

Run `cargo test --locked -p dependency-regressions --lib` from the repository root.
See [the package README](../README.md) for codec tests, independent decoder gates,
assembly prerequisites and the limits of recorded evidence.
