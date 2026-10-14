# Third-party licenses and file boundaries

## SCTK Adwaita source and contributions

The base source is official sctk-adwaita 0.10.1, upstream revision
`d5f343c45c04d4a026db57f76760819b7325db00`, by Bartłomiej Maryńczak and
contributors. Its original MIT license and copyright notice are retained
verbatim in [LICENSE](LICENSE). The title renderer is adapted from official
sctk-adwaita's skrifa renderer, identified in the qualification document.

## MPL-covered source file

The upstream `rounded_headerbar_shape` implementation in `src/lib.rs`
acknowledges derivation from [snui](https://gitlab.com/snakedye/snui), by Bryan
Ndjeutcha and contributors. snui's authoritative license is MPL-2.0:
[immutable LICENSE revision](https://gitlab.com/snakedye/snui/-/blob/742c61a42a500d3b10c7fb160d5437e9847af502/LICENSE),
introduced 2021-11-08. This identifies the project/contributors; it does not
invent an additional copyright notice absent from the source license.

The complete `src/lib.rs` source file containing the acknowledged covered code
is distributed under MPL-2.0, including this fork's modifications to that file.
Its Exhibit A notice is at the start of the file. The original derivation
comment and upstream MIT contribution/license notices remain intact.
A complete [MPL-2.0 text](LICENSE-MPL-2.0) is provided verbatim from the
[license steward's official text](https://www.mozilla.org/media/MPL/2.0/index.txt).
SHA256: `3f3d9e0024b1921b067d6f7f88deb4a60cbe7a78e76c64e3f1d7fc3b779b9d04`.

This is a conservative source-file compliance boundary, not a claim that the
rest of the application has been relicensed. Other separately located upstream
Rust modules retain their MIT terms. The larger-work boundary does not remove
the MPL source/notice obligations for the covered file.

## Font material

The embedded Cantarell-Regular.ttf remains under SIL OFL 1.1, with its original
embedded copyright and complete license text retained in
[src/title/Cantarell-OFL.txt](src/title/Cantarell-OFL.txt).
The Google Fonts Roboto variable regression fixture remains under SIL OFL 1.1,
with copyright and complete terms in [tests/fonts/Roboto-OFL.txt](tests/fonts/Roboto-OFL.txt).
Source identities and fixture SHA256 values are in the root qualification doc.

## Aggregate package metadata

`MIT AND MPL-2.0 AND OFL-1.1` describes the obligations of these separate
materials when distributed together. It is not a choice among licenses and
does not mean each file is independently offered under all three. In particular,
it does not permit MIT-only distribution of the MPL-covered source file.

See [SOURCE_AVAILABILITY.md](SOURCE_AVAILABILITY.md) for executable-distribution
source access and notice requirements. Preserve these notices, all included
license texts, and the exact modified covered source in source distributions.
