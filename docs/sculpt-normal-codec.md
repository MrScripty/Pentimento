# Sculpt normal codec correction

## Decision and compatibility

`SculptDab::encode_normal` is the authority for the existing v1 normal-hint
byte. Its low nibble bins `(atan2(y, x) + PI) / TAU`; decoding must therefore
subtract `PI` after reconstructing the azimuth. This change repairs that
inverse, retaining the existing encoder rather than introducing a new schema.

The encoder, eight-byte dab layout, polar quantization, radius encoding,
packet fields, and header version remain unchanged. Stored bytes are preserved;
there is no history rewrite, migration, or dual decoder. The same existing v1
byte can now decode to a different direction: away from the north pole, the
previous decoder reversed the horizontal component. Old and corrected decoders
therefore do not have identical semantics even though the header remains v1.
Historical replay and mixed-version peer replay are **not qualified** by this
repair. Any future supported replay/sync path must make its own explicit
compatibility decision before relying on these packets.

The existing floor/clamp behavior is deliberate scope preservation, not a
claim of optimal quantization. In particular, the south pole is clamped to
polar bin 15 (one bin short of PI), and the positive azimuth seam is clamped
to bin 15 while the negative seam uses bin 0. The 256 byte values do not
represent 256 distinct directions because all north-pole bins coincide.

## Observable scope

The production `SculptDab` tests cover cardinal and mixed directions, both
signed-zero seam endpoints and nearby inputs, the poles, all 256 decoded
bytes being finite unit vectors, literal encoder/layout golden bytes, and a
deterministic 129-by-257 spherical sample against a conservative angular
bound. The bound is one polar bin plus one azimuth bin (33.75 degrees, plus
floating-point tolerance); it is not a nearest-neighbor or maximum-error
optimization claim. Invalid/non-finite input normals are outside this repair.

The `SculptBrushEngine` regression exercises begin/update/end, then decodes
actual completed packet dabs. Stationary input with zero spacing isolates
normal encoding from known, separate resampling and packet-origin defects.
It also checks that immediate `DabResult.normal` is the original input normal.
In-tree live deformation uses that original normal; no in-tree production
consumer currently calls `decode_normal`. These tests do not establish a
visible sculpting/tearing fix, topology correctness, UV correctness, or replay.

## Verification

`./launcher.sh --test` now includes `cargo test -p sculpting --lib` before
frontend verification. The separate `sculpting` job in the existing hosted
`verify` workflow runs that same suite even if unrelated frontend checks fail.
It checks formatting only for the two changed Rust source files and runs
Clippy for the sculpting library/tests under the existing warning policy.

For focused defect evidence, run:

```sh
cargo test -p sculpting --lib normal_hint_ -- --nocapture
```

Run the new tests against the original decoder first, then the corrected
source. A build failure is not red regression evidence: the baseline must
reach and fail the direction assertions, while encoder/layout and finite-unit
checks remain passing. After correction, run the full sculpting library suite.
Bevy-linked compilation belongs on a hosted runner for this change; local
formatting or launcher wiring checks do not substitute for Rust test execution.
