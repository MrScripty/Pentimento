# Dependency repair qualification

Date: 2026-10-13. State: verifying, not merge or security acceptance.

## Accepted boundary

The first current-main integration, `d1fc564e4d9aa039573661ca6530dd2bd1cea435`,
passed full launcher, CEF, Electron, paint/sculpt and geometry workflows.
Its complete strict audit failed with two unsound and four unmaintained
warnings. The maintainer approved moving discontinued frontends into a
separate legacy workspace while preserving source and entry points, and
continuing compatible dependency fixes. No blanket warning waiver was approved
or introduced.

The root workspace owns supported CEF/Electron/Dioxus and experimental egui.
`legacy/Cargo.toml` owns retained capture/overlay, standalone WebKit/overlay
and Tauri consumers. Its app/webview manifests compile the original source
with historical feature forwarding; original binary/library/example names
and Tauri configuration remain. New Cargo-consumer tests establish both real
workspace boundaries and source entrypoints. Native consumer build and genuine
WebKit capture are required separate hosted observations, not inferred from
metadata. Legacy's independent lock retains historical advisory debt; it is
not shipped as a newly security-qualified supported frontend.

## Finding disposition

- Unsound glib 0.18.5 (RUSTSEC-2024-0429) remains in the retained GTK3 graph;
  no compatible audited registry 0.18 patch exists. Active selected frontends
  did not call this package, but the former full lock included it.
- Unsound rand 0.7.3 (RUSTSEC-2026-0097), unmaintained fxhash 0.2.1
  (RUSTSEC-2025-0057) and proc-macro-error 1.0.4 (RUSTSEC-2024-0370) remain
  legacy-only. Rand's former PHF consumer used fixed-seed SmallRng without
  the advisory's log feature; this was contextual evidence, not an exemption.
- Unmaintained paste 1.0.15 (RUSTSEC-2024-0436) is replaced in the retained
  pulp 0.22.3/rav1e 0.7.1 APIs by upstream pastey maintenance patches.
  No image format is disabled; real codec/SIMD regressions protect behavior.
- Unmaintained ttf-parser 0.25.1 (RUSTSEC-2026-0192) is replaced at the
  SCTK title-renderer boundary with maintained skrifa/read-fonts. Winit 0.30,
  SCTK 0.19 and tiny-skia 0.11 remain compatible; Wayland/titles stay enabled.

The repaired root default/hot graph contains none of those six identities.
Root lock SHA256 is
`731218f189bd6c088b48aab080a7c64f7626c28bafbf19454603ecf5062a5847`.
Independent review found no unrelated retained-version refresh. The unchanged
strict scanner/database, `--deny warnings`, complete-coverage rejection and
same-run provenance must still qualify this exact source/lock.

## Provenance and licensing

Verified official registry archives and upstream maintenance patches are
recorded with exact modified-file lists and retained MIT/BSD/PATENTS notices.
SCTK's acknowledged snui-derived `src/lib.rs` is conservatively retained under
MPL-2.0 with full license, Exhibit A/file-level notice and attribution; other
MIT modules and OFL fonts keep their terms. No MIT-only or application-wide
relicensing claim is made. Executable distributions must include the recorded
legal materials and give recipients access to the exact modified source for
that build. Current CI evidence contains screenshots/logs, not a new executable
distribution. See the owned vendor notices and source-availability instructions.

## Supporting local evidence

- Eighteen focused real Cargo membership/entrypoint/feature contracts pass.
- Complete local npm verification passes accessibility/typecheck/production
  source formatting and 156 actual Node tests, including generated Rust IPC.
- Codec/macro/native SIMD regressions: five passed. Independent FFmpeg EXR
  float/alpha decoding passed. Earlier FFmpeg 7 AVIF alpha/color observations
  passed, but do not qualify the final portable avifdec procedure.
- Regression Clippy with `-D warnings`, formatting and exact vendor comparison
  pass. An unchanged upstream rav1e dead-code warning is visible; it is not
  suppressed and does not replace application-owned warning gates.
- SCTK direct renderer/API tests: 22 passed under both title feature selections;
  no-title API check and real Wayland smoke executable build pass.
- Font comparisons record 180 cases: 144 ordinary/CFF/variable dimensions match;
  ten italic cases gain 1–3px to preserve ink previously clipped. No universal
  pixel-equivalence claim is made; deterministic clipping/button tests protect
  the actual renderer boundary.
- Eleven positive/negative synthetic pixel-oracle tests pass. They distinguish
  blank/stale title evidence, missing/overwritten controls, clipped bounds,
  incorrect scale/resize, missing client and retained post-close client. They
  are oracle tests, not real graphical acceptance.

Native legacy checks are unavailable locally due missing real GTK/WebKit
development metadata. Compositor/avifdec/NASM tooling is absent; one authorized
official local provisioning retry failed during sandbox startup. No fake
metadata, alternative denied download route or skipped-success fallback was
used. Cargo-audit 0.22.2 now installs successfully; a complete pinned-database
audit still belongs to the exact-head hosted procedure.

## Required hosted acceptance

The final PR head must pass the unchanged full launcher, Dioxus focused/strict
audit, rendered CEF/Electron and paint/sculpt workflows, plus the new
dependency repair workflow:

1. NASM-backed macro/codec tests and independent libavif dav1d whole-RGBA
   decoding, including exact alpha, plus independent EXR float planes.
2. Real isolated headless Sway/pixman ClientOnly title rendering: title updates,
   narrow/wide/overlong-title captures, protected close pixels, 1x/2x actual
   compositor scale, observed frame acknowledgments and natural client/
   compositor exits with post-close compositor absence.
3. Real native retained workspace consumer builds and a genuine WebKit capture
   with nonzero pixels and bounded timeout.

Every job records its actual candidate SHA, lock/toolchain facts and artifacts.
Required unavailable/failed observations block acceptance. Software graphics
does not establish hardware GPU, stylus or interactive performance qualification.

## First repair execution and bounded follow-up (2026-10-14)

Published candidate `0591f8adf5074b7fe3b9d9c60def01fe7dbc94f7` passed the
[strict pinned audit](https://github.com/MrScripty/Pentimento/actions/runs/38493211493).
The inspected audit artifact records that exact source and root lock above,
scanner 0.22.2, database `ef6173cbc5c50ec8166f9a5b28f07834144373ee`, exit 0,
937 locked dependencies, zero vulnerabilities, zero warnings, an empty ignore
list and empty stderr, with no incomplete-coverage receipt. Electron runtime,
paint/sculpt and geometry workflows also passed. These observations qualify
that earlier source, not an unpublished follow-up or the entire PR.

The new gates exposed three packaging/observation/cohort defects, and the
focused warning gate exposed one feature-ownership defect:

- Existing `dist/` ignore rules excluded nine genuine rav1e assembly Rust
  modules from the published Git tree although they existed locally. Narrow
  vendored-source exceptions restore every reviewed module. A new regression
  compares actual Git-index blobs against complete pinned release-archive
  inventories and documented deltas, including negative missing-source,
  license/font and expanded-omission cases. Filesystem presence alone no longer
  establishes published-source completeness.
- At scale 2, Sway's logical output becomes 640x400 but its floating client
  retained x=400, y=280 and size 480x240. The original capture was correctly
  rejected as clipped. The harness now observes actual output geometry, asks
  the compositor to recenter, and requires successful, fully-contained actual
  client geometry before capture. Strict pixel predicates remain unchanged or
  strengthened. The original four unscaled captures pass; title update changes
  1,568 pixels and protected close-control changes are exactly zero.
- The separate legacy lock mixed Tauri 2.9.5/runtime-wry 2.9.3 with runtime
  2.12.1. The official historical host tag pairs runtime 2.9.2, explaining
  the native trait/API failure. Further review also identified known host
  security advisories. A separate focused maintained-cohort repair must retain
  the native consumer source/configuration and qualify it; legacy remains
  explicitly security-unqualified. See [the cohort evidence](../../../../legacy/README.md).
- Channel/capture imports now follow WebKit/CEF ownership; shared IPC/dirty
  imports follow WebKit/CEF/Dioxus ownership. No warning allowance was added.
  Featureless locked Webview Clippy with `-D warnings` passes locally.

The follow-up additionally passes five tracked-source completeness regressions,
fifteen synthetic Wayland oracle/geometry tests and full local Node verification
(162 tests), plus warning-free featureless Webview Clippy.
Its final published source must repeat the full strict audit and supported
frontend gates, and pass actual assembly/independent decoder, scaled compositor/
natural shutdown and retained native consumer/capture observations before merge.
