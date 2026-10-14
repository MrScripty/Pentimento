# Image and macro dependency regressions

## Purpose

This unpublished, test-only workspace package verifies the retained image codec
and SIMD APIs after the owned `pulp`/`rav1e` pastey backports. Production crates
continue to consume the same image APIs and default format features.

## Invariants and evidence

- Both token-pasting invocation spellings and generated identifier transforms work
- Pulp's real dispatched SIMD and scalar implementations match exact arithmetic,
  including empty, short, and non-vector-aligned tails
- Real OpenEXR float/alpha encoding and decoding retain dimensions and pixel values
- Real AVIF RGB/RGBA encoding remains available
- Representative PNG, JPEG, BMP, TIFF, PNM, TGA, QOI, and WebP encode/decode paths work

The separate ignored external-decoder tests require `ffmpeg` with the EXR
decoder and `avifdec` from the official `libavif-bin` package with dav1d
decoding support. The AVIF gate explicitly selects `--codec dav1d --depth 8`
and independently decodes the complete RGB/RGBA image into PNG. It asserts
dimensions, RGB within an explicit eight-level lossy per-channel tolerance,
and exact alpha. The EXR gate checks float/alpha planes exactly through
FFmpeg's independent decoder (6.1+). These gates do not claim conformance for
arbitrary codec inputs or every supported target. Temporary encoded and
decoded files are deleted when each external test completes.

The selected AVIF CLI is documented in the
[official libavif v1.0.4 source](https://github.com/AOMediaCodec/libavif/blob/v1.0.4/apps/avifdec.c).
FFmpeg 6.1 exposes only the primary AVIF item, so it is not used as a complete
RGBA oracle. libavif owns AVIF parsing/alpha association, dav1d owns independent
AV1 decoding, and image only reads the produced PNG.

## Entry points

Run from the repository root with its committed lockfile:

```sh
cargo test --locked -p dependency-regressions
cargo test --locked -p dependency-regressions --test independent_decoders -- --ignored
cargo clippy --locked -p dependency-regressions --all-targets -- -D warnings
cargo fmt -p dependency-regressions --check
```

Where NASM 2.14+ is installed, also compile and execute rav1e's retained
assembly-backed paths (the feature unifies with image's real encoder):

```sh
cargo test --locked -p dependency-regressions --features asm
cargo test --locked -p dependency-regressions --features asm --test independent_decoders -- --ignored
```

Source ownership, backport provenance, complete licenses, maintenance and
retirement criteria are recorded in [vendor/README.md](../../vendor/README.md).
These focused gates complement the root graph/audit and supported frontend
checks; they do not imply the entire application's rendered acceptance passed.

## Recorded local evidence (2026-10-13)

Linux x86_64, Rust/Cargo 1.99.0, root locked resolution: the five ordinary
regressions, package formatting, and package Clippy with `-D warnings` passed.
The final independent EXR gate passed with FFmpeg 7.1.5. Before replacing the
AVIF oracle for Ubuntu 24.04 portability, FFmpeg 7.1.5/libdav1d independently
decoded both AVIF color and exact alpha planes successfully.

The final libavif `avifdec` gate compiled, but was not executed locally because
`avifdec` is unavailable here. NASM is also unavailable locally; assembly-feature
commands were not run. Those claims require execution in the provisioned hosted
gate before acceptance. No missing tool is treated as successful verification.

Retained rav1e reports one existing `dead_code` warning for
`src/lrf.rs::RestorationPlaneOffset`, an unused internal record. It is unchanged
and no lint allowance was added. The prior toolchain spelling/custom-cfg warnings
were repaired mechanically; codec algorithms and values are unchanged.
