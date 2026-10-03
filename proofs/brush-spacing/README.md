# Fixed-spacing phase accounting: first Lean milestone

**Status: source candidate, not yet Lean-compiled or kernel-verified.** This
isolated package is a specification for review, not a correctness claim about
Pentimento's Rust implementation. The current preparation environment has no
Lean/lake and less than 1 GB free disk. No local toolchain, dependency cache,
or heavy build was installed. Hosted verification must pass before describing
these theorems as checked.

## Mathematical contract

For fixed real spacing `h > 0`, prior residual `0 ≤ a < h`, newly traversed
arc length `L ≥ 0`, and previously emitted count `n : Nat`:

- `q = floor ((a + L) / h)` (natural floor, equal to integer floor here)
- `a' = a + L - q * h`
- `n' = n + q`

The source supplies proofs of residual bounds `[0,h)`, distance conservation,
count from zero, two-segment composition, zero-length identity, an exact-boundary
reset, and no repeated emission of a consumed boundary. Bounds and composition
actually require only `a ≥ 0`; zero-length identity also needs `a < h`.

`start (b : Fin 2)` applies initial-dab policy `b = 0` or `b = 1` once, on the
first input of a nonempty stroke. Subsequent segments use `advance`, never
`start`. A one-point stroke is nonempty even if its geometric length is zero.
An empty stroke has no start event and emits nothing. No end event forces a
last dab. `count_with_initial_policy` is `b + floor (L / h)`; the initial dab
consumes no arc length.

The model uses NEW segment arc length. It does not use distance from the last
emitted dab. Splitting a segment preserves count and phase for a fixed `h`.
This does not justify replacing a curved path by a chord of different length.

## Scope and explicit exclusions

This is one small independent proof package, not a generic proof framework.
There is no new Rust, JavaScript, WASM, Electron or application-runtime dependency.
No production brush loop or other project was modified.

Not established:

- refinement of Rust/f32 behavior, roundoff, termination of Rust loops, NaN/Inf
- geometric dab placement, directions, normals, pressure interpolation
- pressure-dependent/changing spacing, zero or negative spacing
- skipped sub-threshold input movement, paint's `0.001` guards
- networking, codecs, packetization, undo or collaborative determinism

Later work must decide production policy and separately implement/test a
refinement. A green proof job does not fix the regressions below.

## Concrete regression mapping

Inspected baseline:
[`f819e593819004690c4465afbb9733cb78b731c2`](https://github.com/MrScripty/Pentimento/commit/f819e593819004690c4465afbb9733cb78b731c2).
Use constant pressure/size so `h` stays fixed; straight, increasing x and y=0
make the short traces unambiguous. These are exact-arithmetic models of the
inspected statements, not executed Rust regressions.

### Sculpt: counted distance overlaps

[`crates/sculpting/src/brush.rs`, `update_stroke`](https://github.com/MrScripty/Pentimento/blob/f819e593819004690c4465afbb9733cb78b731c2/crates/sculpting/src/brush.rs#L358-L382)
adds the distance from `last_dab_position` to `distance_since_dab`. When no dab
was emitted, the previous residual is therefore included again.

With `h=10`, start x=0, then inputs x=3,6,9:

- Correct new lengths are 3,3,3: `(count,residual)` = `(0,3),(0,6),(0,9)`.
- Old overlapping distances are 3,6,9: states are `(0,3),(0,9),(1,8)`.
- The old loop emits at x=10 before the input reaches x=10.

Lean fixture `sculpt_no_early_crossing` asserts the correct endpoint `(0,9)`.
This illustrates a phase-accounting defect; geometric correctness is not proved.

### Paint: short segments discard residual

[`crates/painting/src/brush.rs`, `stroke_to`](https://github.com/MrScripty/Pentimento/blob/f819e593819004690c4465afbb9733cb78b731c2/crates/painting/src/brush.rs#L193-L249)
adds new distance, but assigns `distance - current_distance` after the loop.
If no dab was generated, `current_distance` stays zero and the old residual is
lost. With `h=10`, initial dab at x=0, then x=3,6,10:

- Correct states are `(1,3),(1,6),(2,0)`.
- Old states are `(1,3),(1,3),(1,4)`.

Lean fixture `paint_preserves_carry` asserts `(2,0)`.

## Reproducibility and verification

Pins, verified against the official repositories:

- Lean: [`leanprover/lean4:v4.19.0`](https://github.com/leanprover/lean4/releases/tag/v4.19.0)
- mathlib: [`c44e0c8ee63ca166450922a373c7409c5d26b00b`](https://github.com/leanprover-community/mathlib4/commit/c44e0c8ee63ca166450922a373c7409c5d26b00b), official v4.19.0,
  whose lean-toolchain specifies the exact same Lean release
- `lake-manifest.json` records mathlib plus all eight inherited revisions from
  that release's official manifest; it is source-prepared, not Lake-generated
- hosted bootstrap: official elan v4.1.2; checkout action pinned to full commit

The proposed `.github/workflows/lean-brush-spacing.yml` is a separate read-only
job on Ubuntu 24.04, limited to changes in this proof package/workflow, with a
30-minute cap. It downloads the toolchain and mathlib only on the hosted runner,
checks that resolving dependencies did not mutate the manifest, builds the
proofs, audits their transitive axioms, and runs audit negative controls. It
has no secrets, uploads, publishing step, or integration with frontend builds.
A manually dispatched run is also available once the workflow is published.

Run from this directory on a machine with the pinned toolchain and adequate disk:

```sh
python3 scripts/check_sources.py
lake exe cache get
lake build
lake env lean AxiomAudit.lean
python3 scripts/test_axiom_audit.py
python3 scripts/check_fixtures.py
```

`AxiomAudit.lean` examines kernel dependencies of every public `Pentimento.*`
declaration. Only `propext`, `Classical.choice`, and `Quot.sound` are allowed.
Custom axioms, unfinished proof axioms and native-evaluation axioms fail closed.
Negative controls deliberately introduce each forbidden category and require
an explicit audit rejection. There is no `native_decide` in the proof modules.
Rational fixtures use ordinary `decide`, whose resulting proof is kernel-checked.

Local preparation checks that DID run:

- immutable-pin and proof-source policy check
- 10 independent exact-rational fixtures
- 1,200 bounded exact-rational composition/conservation cases
- both old-loop divergence models
- Python syntax checks and `git diff --check`

NOT run locally: Lean elaboration, kernel build, axiom audit and its negative
controls, hosted Actions, Rust tests. The Python oracle is supporting evidence,
not a substitute for Lean checking, and it does not prove Rust refinement.
