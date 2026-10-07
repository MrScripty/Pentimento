# Sculpt input sampling and packet origins

This is a brush-engine repair, separate from mesh topology and UV-corner work.

## Input sampling

Spacing consumes the distance between successive input samples exactly once.
The engine keeps the previous input position separately from the last emitted
dab and carries residual arc length across segments. A dab after a corner lies
on the segment being consumed. Stationary samples do not re-count old movement.
Tiny floating-point spacing discrepancies can snap to the current endpoint;
emission never extrapolates beyond that endpoint.

Zero spacing remains continuous: each input emits one dab and resets spacing
residuals. Dab pressure, normal and timestamp still use the current input sample.
Spacing uses that sample's effective radius. If spacing drops below the pending
residual, the engine emits once at the new segment's start and resets that old
residual. This includes a stationary pressure drop; it does not replay earlier
segments to place retroactive dabs. This explicit phase-reset policy is covered
by regression and is not a claim of varying-pressure subdivision invariance or
time-based flow.

## Position packet contract

The existing v1 format declares forward deltas relative to the preceding decoded
dab, starting at the packet header's fixed-point base. The engine now preserves
that base throughout a packet and tracks the decoded position separately.

- Header origin is quantized once to integer 1/1000 object-space units. The cursor
  starts from those exact integers' decoded value; the float is never quantized
  again for header serialization.
- Signed delta components are rounded to 1/100 units relative to the decoded
  cursor, preventing sub-quantum movement from accumulating unbounded drift.
- A delta outside the supported ±127 range starts a new packet at the current
  input. An oversized first jump rebases without emitting an empty packet.
- Live deformation still receives the full-precision sampled position.

The header layout, scale factors, eight-byte dab layout and version remain
unchanged. Newly generated position bytes deliberately change; persisted packets
created with the previous moving-origin/truncating implementation are not
repaired or certified. The scalar reconstruction tests exercise the declared
position contract, not an implemented two-peer replay stack. Normal decoding,
brush metadata completeness, topology revisions, packet identity/order, and
collaborative undo remain separate work.

## Regressions

Run `cargo test -p sculpting --lib sampling_regression_tests` and the complete
sculpting suite. The tests execute `SculptBrushEngine` directly, covering short
segments, stationary input, constant-pressure subdivision invariance, polyline
corners, stable packet origins, an oversized first jump, multiple overflow
packets, a thousand sub-quantum movements, and empty/cancelled stroke isolation.

The nine regressions all failed on the pinned main source
`f819e593819004690c4465afbb9733cb78b731c2`. After the repair, the complete default
sculpting library suite passed 51 tests. Both runs rebuilt the actual path crates
in isolation from other checkouts' cached artifacts, using Rust 1.92.0. Interactive
GUI input and historical/cross-version replay were not exercised by these runs.

Existing limits remain: there is no per-input dab-count budget for arbitrarily
long segments/tiny positive spacing, and header coordinates outside the signed
32-bit fixed-point domain are not supported or newly validated by this patch.
The finite-input guard is not a claim to solve those separate admission limits.

Independent review additionally found a non-idempotent float-origin case near
x = 8268.646. Its regression failed against the first repair (header 8268648
instead of 8268647). The integer-origin follow-up passes that start/rebase case
and the explicit reduced-spacing policy test; the complete Bevy-enabled sculpting
library suite passes 60 tests. Clippy completes with no diagnostics in the
changed sampling code; inherited crate/test warnings remain.
