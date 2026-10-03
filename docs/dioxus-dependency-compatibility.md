# Dioxus renderer dependency compatibility

## Candidate scope

Keep `blitz-dom`, `blitz-paint`, `blitz-traits`, and `dioxus-native-dom`
on the same exact Blitz revision, `f83b2bc049aae41275b248e8709586a0b267591e`.
This replaces four floating `main` references without changing caller APIs or
migrating Pentimento's graphics stack. The initial candidate is based on
Pentimento `e8fe9d5a61309151225eb125eb09eb8decc1fb60`.

This historical development snapshot is a compatibility candidate, not a
previously verified green Pentimento release, supported upstream release, or
security-qualified dependency set. A pin prevents silent source movement; it
does not establish maintenance or security support.

## Contracts to preserve

- Pentimento's Dioxus component/VirtualDom and `dioxus-native-dom` must use a
  compatible registry Dioxus core 0.7 identity.
- `blitz-paint::paint_scene` must accept the AnyRender 0.7 `PaintScene` implemented
  by Pentimento's `anyrender_vello` adapter.
- That adapter's Vello `Scene` must share Pentimento's Vello 0.7 identity.
- Vello and Pentimento's direct wgpu dependency must use the wgpu 27 device
  identity supplied by Bevy 0.18. Unrelated older wgpu versions elsewhere in the
  workspace are not automatically defects; inspect actual crossing edges.
- Existing document node IDs and layout-field access must remain valid. Do not
  paper over an incompatible cohort with casts or disconnected API changes.

The [candidate workspace manifest](https://github.com/DioxusLabs/blitz/blob/f83b2bc049aae41275b248e8709586a0b267591e/Cargo.toml)
declares AnyRender 0.7, wgpu 27 and registry Dioxus 0.7.3-compatible dependencies.
The [paint crate](https://github.com/DioxusLabs/blitz/blob/f83b2bc049aae41275b248e8709586a0b267591e/packages/blitz-paint/Cargo.toml)
uses AnyRender but does not directly depend on Vello. The upstream workspace's
Vello 0.6 declaration is therefore not proof of the active paint adapter type.
The [historical upstream lock](https://github.com/DioxusLabs/blitz/blob/f83b2bc049aae41275b248e8709586a0b267591e/Cargo.lock)
records `anyrender_vello 0.7.1 -> vello 0.7.0 -> wgpu 27.0.1` and
`anyrender_vello 0.7.1 -> anyrender 0.7.0`. This is reference evidence only:
Pentimento must generate and review its own root lockfile.

The candidate also has pinned upstream Taffy and Parley git dependencies. They
must be recorded in the generated graph and lock review; do not replace them
or infer identity compatibility from version numbers alone.

## Independently floating Dioxus dependencies

`dioxus-asset-resolver` and optional `dioxus-devtools` still use Dioxus `main` in
this narrow candidate. PR6's hosted build resolved direct asset-resolver
0.8.0-alpha.1 at `b2ed8c328bff51ea5d4e42225a3d1ef7675bad2c`, alongside registry
asset-resolver/devtools 0.7.10. This is separate from the four Blitz pins.

The current asset-provider boundary calls `native::serve_asset(&str)` and
consumes response bytes, with no Dioxus core type passed across that boundary.
The optional `hot-reload` feature only enables devtools; there are currently no
Rust references to its APIs. Git devtools nevertheless brings its own git
core/signals/devtools-types cohort when enabled. A default build alone does not
qualify that feature. Any move to a registry version or a new explicit pin
requires its own source/behavior review, including asset root semantics.

## Qualification requirements

Before declaring this repair complete:

1. Generate Pentimento's own Cargo.lock on an isolated hosted runner from the
   exact candidate. Record source SHA, rustc/cargo versions, lock checksum,
   complete metadata and dependency trees. Do not copy Blitz's lockfile.
2. Review actual package IDs, versions and sources at the crossings above,
   including default and hot-reload graphs, both git dependency families and
   upstream transitive git pins. Multiple versions require edge-level review.
3. Run focused UI/frontend checks and the hot-reload check with `--locked`.
4. Review the generated lockfile and explicitly decide its version-control and
   canonical launcher/CI enforcement before claiming reproducibility. A
   generated-but-uncommitted lock is a probe artifact, not a finished repair.
5. Run the repository's complete canonical `./launcher.sh --test` against the
   same reviewed lock and exact final commit. Focused checks do not substitute
   for that gate or GPU/runtime rendering tests.
6. Review security advisories and upstream support separately. Record unresolved
   findings rather than treating compilation or a clean audit as proof of
   support or absence of vulnerabilities.

No hosted resolution, build, security audit, or runtime qualification is implied
by this document. Preserve failed logs as evidence and stop for review before
broadening the dependency or caller changes.
