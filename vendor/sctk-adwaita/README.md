# Bounded maintained-title backport

This is the official `sctk-adwaita` **0.10.1** source with the title renderer
backported from the official 0.12.0 `skrifa` implementation. It remains API/type
compatible with Winit 0.30's Smithay-client-toolkit 0.19 and tiny-skia 0.11.
It is a Pentimento-owned compatibility fork, not a new upstream release.

The historical default/`ab_glyph` feature forwards to `skrifa`; it no longer
selects or depends on the `ab_glyph`, `owned_ttf_parser`, or `ttf-parser` crates.
The explicit `skrifa` feature selects the same renderer. Crossfont selection
and no-title configuration retain their historical precedence and public API.

See [qualification and provenance](../../docs/wayland-title-font-qualification.md)
for behavior, maintained-dependency selection, licenses, limitations, test
commands, lock identities, and the remaining real Wayland acceptance gate.

The root application `Cargo.lock` owns production resolution. This directory's
`Cargo.lock` owns the separate, locked regression/example build, including
unselected optional crossfont/platform packages. Never substitute that optional
graph for evidence about the active application graph.

## License boundaries

The aggregate package license is `MIT AND MPL-2.0 AND OFL-1.1`. The complete
`src/lib.rs` file is distributed as MPL-2.0 Source Code Form because it includes
acknowledged snui-derived rounded-header code. Existing MIT contribution
notices remain intact; other Rust modules retain MIT and font materials retain
OFL-1.1. This is not an MIT-only licensing option for the covered file and does
not relicense unrelated application files.

See [third-party notices](THIRD_PARTY_NOTICES.md) and
[executable source availability](SOURCE_AVAILABILITY.md) before distribution.
