# Owned dependency backports

## Purpose and owner

Pentimento maintainers own these bounded dependency backports. The image-codec
patches remain until compatible published parents consume a maintained
token-pasting implementation. They preserve the
retained `pulp` 0.22.3 and `rav1e` 0.7.1 public APIs, source algorithms, default
features, assembly sources, and supported image-format capabilities.

The root `[patch.crates-io]` selects these actual parent sources. Their manifests
and imports refer to the real `pastey` package. No crate named `paste` is aliased,
relabelled, or excluded from auditing to hide RUSTSEC-2024-0436.

## Provenance and exact changes

[`provenance.json`](provenance.json) records the original crates.io archive
SHA-256, release source revision, official backport reference, and modified files.
The copied release archives are `pulp-0.22.3.crate` and `rav1e-0.7.1.crate`.
For the image-codec copies, only registry/cache markers, original manifests
superseded by Cargo's normalized manifests, and stale nested lockfiles are omitted.
Published image-codec sources, examples, embedded assembly/test material and
legal notices remain intact. The SCTK entry separately records its complete
archive-to-fork added/modified/omitted/unchanged file sets.

- `pulp`: adopt the two-line dependency/import migration in upstream
  [601de0c071ba32df3fd7ff33fc4218c9bd712aba](https://github.com/sarah-quinones/pulp/commit/601de0c071ba32df3fd7ff33fc4218c9bd712aba).
  The normalized manifest expresses `pastey = "0.2"` and the sole source change
  is `use pastey::paste`.
- `rav1e`: backport the dependency/macro-path migration in upstream
  [c247d53ae43dd1312dbd90117c45e4c0ee6b06ce](https://github.com/xiph/rav1e/commit/c247d53ae43dd1312dbd90117c45e4c0ee6b06ce).
  All existing `paste::item!` paths become `pastey::item!`. Upstream originally
  selected pastey 0.1; this backport uses its compatible maintained 0.2 successor
  alongside pulp, with compile/behavior coverage in the regression package.
  Additional local Rust 1.99 compatibility updates use primitive associated
  numeric constants, make two already-elided lifetimes explicit, and declare
  valid upstream custom cfg names without disabling cfg checking. These do not
  change codec algorithms, numeric values, runtime features, or public APIs.

The original versions intentionally remain unchanged for image/exr/ravif
consumer compatibility; these are locally owned modified sources, not claims
that those crates.io releases have been republished. `pastey` is resolved under
its genuine crates.io name/version/checksum in the root lockfile. Its
[maintainer's migration instructions](https://github.com/AS1100K/pastey#macros-for-all-your-token-pasting-needs)
identify it as a drop-in successor. Its license is MIT OR Apache-2.0.

## Maintained Wayland title fork

[`sctk-adwaita` 0.10.1](sctk-adwaita/README.md) retains Winit's SCTK 0.19 and
tiny-skia 0.11 API/type boundary while backporting the official maintained skrifa
title renderer. The historical `ab_glyph` feature is forwarding only; no old
font parser or audit exception is retained. Its verified official release
archive and renderer blob, exact write set, manifest/lock/font identities, and
omission reasons are in [provenance.json](provenance.json).

Pentimento maintainers own this fork and revisit it at the next Winit decoration
upgrade or skrifa/read-fonts advisory. Retire it when a released compatible
maintained backend passes the same direct/font and real Wayland gates. See
[qualification](../docs/wayland-title-font-qualification.md) for measured raster/
metric limits and remaining exact-commit graphical acceptance.

The package combines separate MIT, MPL-2.0 and OFL-1.1 material. Its aggregate
`MIT AND MPL-2.0 AND OFL-1.1` metadata provides no MIT-only option for the
MPL-covered `src/lib.rs` file. Complete licenses/attribution and
[exact-version executable source availability](sctk-adwaita/SOURCE_AVAILABILITY.md)
are retained; unrelated application files are not relicensed.

## Why retain these codec parents

The published compatible parents still require the unmaintained `paste`
package. Merely updating `image` does not establish removal. Updating the codec
parents across incompatible API boundaries would expand the migration; disabling
EXR, AVIF, default formats, or native SIMD would remove supported capability.
This backport changes macro ownership and mechanically updates retained source
spellings for the current Rust toolchain; it retains the real codecs.
The advisory remains enabled; the final graph and audit must demonstrate removal.

## Legal notices and distribution

The copied authoritative texts are retained verbatim:

- [pulp MIT license](pulp-0.22.3/LICENSE), copyright 2021 sarah
- [rav1e BSD-2-Clause license](rav1e-0.7.1/LICENSE), copyright 2017–2023 the rav1e contributors
- [rav1e Alliance for Open Media Patent License 1.0](rav1e-0.7.1/PATENTS)

These terms permit the recorded modification and redistribution. Source
redistribution must retain those complete notices and the patent license.
Binary distributions containing these implementations must reproduce the BSD
notice/disclaimer and patent license in their accompanying legal materials;
retain pulp's MIT copyright and permission notice with substantial copies.
Pentimento's MIT project license does not replace these third-party terms.

## Verification and retirement

See [dependency-regressions](../crates/dependency-regressions/README.md) for real
image-codec, macro and native SIMD tests and independent decoding commands.
The final root lock, supported frontend checks, and enabled audit are additional
integration gates owned by the root dependency update.

At the next parent/image update, inspect the resolved graph and published source.
Replace these patches with compatible published parents once their real package
identities no longer depend on paste and all regression/frontend gates pass.
Remove the patch declarations and vendored sources together; retain the codec
regressions while they protect supported format/SIMD behavior. Do not mutate the
registry cache, broaden features away, or maintain a second resolution source.
