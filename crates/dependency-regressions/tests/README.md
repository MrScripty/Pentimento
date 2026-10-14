# Codec regression tests

`image_formats.rs` exercises retained OpenEXR float/alpha, AVIF RGB/RGBA and the
other default image format paths. `independent_decoders.rs` requires the official
FFmpeg EXR decoder and libavif `avifdec` with dav1d, and checks independent decoded
pixels rather than accepting successful encoder exit alone.

Run the ordinary and required external-decoder commands in
[the package README](../README.md). The external tests are ignored by the ordinary
Rust test command and must be invoked explicitly with `--ignored`. Missing decoder
tools are a gate failure, never a skip or successful result.
