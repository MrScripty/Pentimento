# Maintained Wayland title font parser

## Status and owned requirement

Implemented; direct rendering/API/metric checks pass. Real Wayland compositor
screenshot, title-update, resize, output-scale and clean-exit acceptance remain
required until the hosted qualification job produces that evidence. Direct
pixmaps and compile success do not close this cross-process graphical claim.

The Linux Winit 0.30.13 decoration path used sctk-adwaita 0.10.1 -> ab_glyph
0.2.32 -> owned_ttf_parser 0.25.1 -> ttf-parser 0.25.1. The authoritative
[RUSTSEC-2026-0192](https://rustsec.org/advisories/RUSTSEC-2026-0192.html)
identifies ttf-parser as unmaintained, lists no patched release, and identifies
maintained skrifa as an alternative. Raising ttf-parser's version is therefore
not a disposition. This change does not suppress the audit or disable Wayland,
window titles, or image formats.

## Selected bounded replacement

- Vendor the official sctk-adwaita 0.10.1 source; preserve its public API,
  Wayland state/input/lifecycle and tiny-skia/Smithay types.
- Backport only the official upstream skrifa title renderer, retaining
  Smithay-client-toolkit 0.19 and tiny-skia 0.11 at Winit's boundary.
- Use exact skrifa 0.44.0, within upstream sctk-adwaita 0.12's supported range.
  Parser/metrics/variation/outline/kern semantics stay in maintained
  skrifa/read-fonts; no local font parser is introduced.
- The existing default `ab_glyph` feature is a compatibility forwarding name
  for the skrifa renderer, with no ab_glyph dependency. Explicit `skrifa`
  selects the same implementation. Crossfont precedence and the historical
  no-title feature configuration remain; the active app keeps titles enabled.
- Root `[patch.crates-io]` selects `vendor/sctk-adwaita`; the reviewed root
  lockfile owns application resolution. The separate vendored lock owns the
  regression/example build, including unselected optional crossfont/platform
  packages. That graph is not the active application's consumed graph.

An unconditional sctk-adwaita 0.12 upgrade was rejected because it changes the
Smithay 0.21/tiny-skia 0.12 API/type boundary Winit 0.30 consumes. Upstream's
unmodified skrifa renderer was not treated as a drop-in: it omits legacy kern
adjustments and allocates only advance/line bounds. This fork delegates first
supported horizontal non-variable kern lookup to read-fonts and includes the
actual ink bounds with a 1px guard, retaining the old nominal advance+2 and
line-height+2 box when ink fits.

## Visible behavior and limits

Retained: gsettings/fc-match font selection, 96/72 point-to-pixel conversion,
integer output scaling, regular bundled Cantarell fallback on unavailable or
invalid system font, case-insensitive Bold variable `wght=700`, control skipping,
empty-title absence, and blank whitespace with its advance width. Static Bold
comes from the selected system font; the renderer does not synthesize bold.

Like the old renderer, layout maps each Unicode scalar independently. It does
not promise complex-script shaping, ligatures, GPOS pair positioning, bidi,
color emoji, or per-character font fallback. Unsupported codepoints use the
font's missing glyph. Those are retained limitations, not newly verified
capabilities. Hinting/anti-aliasing pixels change. Italic overhang can widen the
title bitmap by 1–3px in the measured sample so ink is not cropped; header
centering/clipping still uses the resulting bitmap and protects buttons.
Invalid or unrepresentable scales produce no bitmap and recover on a valid
scale, rather than panicking or constructing overflowing dimensions.

## Evidence

Locked direct tests: 22 tests, covering public title updates/repeated updates,
color, empty/control/whitespace titles, malformed/incomplete system-font fallback,
Latin/Greek/Cyrillic/missing CJK/emoji glyphs, combining marks, 1x/2x/3x, padding,
legacy kern decoding/filtering, real variable-weight Bold, static regular/Bold
behavior, invalid scale recovery, and real draw_headerbar clipping before both
left and right buttons at narrow/wide widths and all three scales.

The recorded [180-case metric comparison](../vendor/sctk-adwaita/tests/title-metrics-baseline.csv)
used the unchanged official 0.10.1 backend with ab_glyph 0.2.32 and this fork,
10pt, six fixed titles, regular/Bold preferences, and scales 1/2/3:

- Cantarell, DejaVu Sans, Latin Modern Mono CFF, and Roboto variable: all 144
  measured bitmap widths/heights match the previous backend exactly.
- DejaVu Sans Oblique: 26/36 dimensions match; ten gain 1–3px of width to retain
  overhang ink, with identical height.
- Raster coverage differs due to the renderer change. No pixel-equivalence,
  universal font equivalence, or hardware rendering claim is made.
- The 72 bundled/Roboto rows are enforced against the legacy dimensions in a
  reproducible direct test; system-font rows are supplementary local evidence.

Commands, using the retained separate lock (production still uses root lock):

```sh
cargo test --locked --manifest-path vendor/sctk-adwaita/Cargo.toml --lib
cargo test --locked --manifest-path vendor/sctk-adwaita/Cargo.toml --no-default-features --features skrifa --lib
cargo check --locked --manifest-path vendor/sctk-adwaita/Cargo.toml --no-default-features
cargo build --locked --manifest-path vendor/sctk-adwaita/Cargo.toml --example wayland_title_smoke
cargo fmt --manifest-path vendor/sctk-adwaita/Cargo.toml -- --check
```

The no-default-feature check verifies a retained library configuration; it is
not the active application's resolution and does not replace title evidence.
Crossfont is unselected in the active application and has not been separately
runtime-qualified by this change.

### Required real graphical gate

The example `wayland_title_smoke` connects to a real Wayland compositor,
forces ClientOnly decorations, and has a fixed app ID. The hosted job must:

1. Start isolated headless Sway with wlroots pixman renderer; record actual
   output/session/renderer facts and the exact source/manifest/lock identities.
2. Observe the client via Sway get_tree, client decoration/frame acknowledgments,
   and a grim screenshot with visible title and controls.
3. Update the test control-file title, resize the window narrow/wide, and change
   the actual compositor output scale from 1 to 2. Capture and inspect title,
   button separation, clipping, scale, and resulting window screenshots/metadata.
4. Request compositor close, wait for successful client process exit, verify
   absence of that client from get_tree, then exit/reap the compositor.

Required Ubuntu packages: sway, grim, jq, libwayland-dev, libxkbcommon-dev,
pkg-config, fontconfig, fonts-dejavu-core, dbus-x11. Build/provisioning owns exact
runner package resolution; no user session or browser security flags are needed.
The qualified software-rendered headless environment proves the Wayland CSD
path, not hardware graphics or full Pentimento paint/sculpt runtime acceptance.

## Provenance, licenses and maintenance

Official source: sctk-adwaita 0.10.1 registry source, upstream commit
`d5f343c45c04d4a026db57f76760819b7325db00`; source API/Wayland implementation is
unchanged apart from title regression tests and six explicit-lifetime compiler
warning fixes. Official [skrifa renderer](https://github.com/PolyMeilex/sctk-adwaita/blob/master/src/title/skrifa_renderer.rs)
was retrieved 2026-10-13, Git blob `39bd772933fdade3a9b4866ea653b7a0c4c581de`.
Upstream's MIT LICENSE is retained. The acknowledged snui-derived rounded
header code has an independently verified [MPL-2.0 upstream license](https://gitlab.com/snakedye/snui/-/blob/742c61a42a500d3b10c7fb160d5437e9847af502/LICENSE).
This fork conservatively treats the complete `src/lib.rs` file containing that
code as MPL-covered Source Code Form, retains the original derivation notice,
and adds Exhibit A notice and snui attribution. The complete authoritative
Mozilla MPL-2.0 text is retained in `LICENSE-MPL-2.0`, SHA256
`3f3d9e0024b1921b067d6f7f88deb4a60cbe7a78e76c64e3f1d7fc3b779b9d04`.
Aggregate package metadata is `MIT AND MPL-2.0 AND OFL-1.1`; this describes
separately licensed code/font materials and provides no MIT-only option for
the covered file. Other upstream Rust modules retain their MIT terms; unrelated
Pentimento files are not relicensed. No geometry or rendering behavior changes
are part of this license/notice correction. See [file-scoped notices](../vendor/sctk-adwaita/THIRD_PARTY_NOTICES.md)
and [exact-version source availability requirements](../vendor/sctk-adwaita/SOURCE_AVAILABILITY.md).
Executable distributions must provide the exact modified covered source and
recipient access instructions before distribution. These bounded compliance
steps are not a blanket legal-clearance claim. No unrelated source was imported
from an unverified fork.

Bundled Cantarell is unchanged, SHA256
`d15f25bcdaba2e25f54c32d4e29e68fedeb1b7a1d8fbc83f7c4475916e93b55f`.
Its embedded copyright and SIL OFL 1.1 text are retained in Cantarell-OFL.txt.
The Roboto variable fixture is from official [Google Fonts](https://github.com/google/fonts/tree/main/ofl/roboto),
source metadata commit `91d5d3e5b81efa04a77925cc609fdcdd7ee663d1`, version 3.015,
SHA256 `d7598e12c5dbef095ff8272cfc55da0250bd07fbdecbac8a530b9b277872a134`.
Its OFL text/copyright is retained, SHA256
`061402327a96aadb0bfb694a960ed289ecd38d383e396243831ab81feb109c41`.

Local supplementary font SHA256 identities:

- DejaVu Sans: `57f73e11f51999432bf7ab22ce55b6f945d5eca1bf824404cfa9ec2e3718c84e`
- DejaVu Sans Oblique: `e2f09289f4276309a36b9a93e5a0ac64957ef3eb7158151b243d41f667151ee4`
- Latin Modern Mono CFF: `22deb6d3be3ffcb40b33f0e010afb256c677d97c2a2bdee1f5740cdc97751558`

Standalone manifest SHA256:
`5097e37290679a4e2f260b74b5d57195e2208f49bff31265c3d4323db73afb3b`.
Standalone lock SHA256:
`6c8dac661b2cbfca95f78c691cc3ae1905f1cf1ec2017ca7779cae493aaa0ec2`.
The root lock/application graph and rendered workflow evidence must be reviewed
for the final exact integration commit; these hashes do not replace that gate.

Pentimento maintainers own this bounded fork, future upstream security review,
and font-rendering regressions. Revisit on Winit's next decoration dependency
upgrade and any skrifa/read-fonts advisory. Remove the patch when a released
Winit-compatible maintained title backend passes these same direct and real
Wayland gates. Audit output remains enabled; this targeted change is not
blanket security, licensing, or release clearance.
