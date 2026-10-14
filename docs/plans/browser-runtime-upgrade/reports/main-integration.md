# Main integration qualification

Date: 2026-10-13. First integrated source: d1fc564. Its runtime/frontend
qualification passed; strict audit blocked merge. The subsequent approved
[dependency repair](dependency-repair.md) needs its own final-head qualification.

## Source and integration boundary

The candidate `integration/qualified-runtime-stack` reconciles main
`50eae5db65fba44b86f39f95f65782144ae3e130` with runtime stack
`0d654286fcf1fd56c444dd12e02b9e7753106d5a`, preserving published ancestry.
The runtime stack descends through the native brush workspace and Bevy 0.20
migration proposals. It was 19 commits ahead and 72 commits behind current main;
its earlier green jobs therefore could not qualify this main integration.
Twenty-four textual conflicts required reconciliation, and automatic merges
also required review. Main's optional frontend gates, document and input
correctness, locked verification, warning checks, protected Electron runtime,
ordinary sandbox observation and owned-process shutdown are retained.

Automatic merge duplication of depth imports, IPC variants, TypeScript unions
and contract switch cases was removed. Source guards were adapted to Bevy 0.20's
Core3d/GPU-resource systems while retaining early-return, initialization
exclusion, per-view exact format, readiness and ping-pong ordering assertions.
Independent review found and repaired a guard false-positive gap. Electron
bootstrap fixtures preserve install failure propagation and the exact
wasm-bindgen CLI gate. The protected provisioner uses the independently verified
44.7.0 archive digest and Python 3.10-compatible bounded SHA256 streaming for the
selected Ubuntu 22.04 runner. No new insecure browser flags are introduced.

## Resolution and typed crossings

Pentimento's own Cargo resolver reconciled the runtime lock against the
integrated manifests. Root lock SHA256:
`fd9a2b11b914b5cdb376ccd5b65c911d262ca4f90f2f013272769964acd88ca1`.
Default and hot-reload `cargo metadata --locked --offline --format-version 1`
both resolve under Rust 1.99.0 / Cargo 1.99.0.

Independent comparison with the pre-integration runtime lock found zero added
package identities and no retained version/source/checksum changes. Main's
unused direct app/webview edges were removed, with six now-unused Windows-family
identities: windows/core/implement/interface 0.58.0, windows-result 0.2.0 and
windows-strings 0.1.0. Other Windows edge spelling changes are disambiguation only.

Reviewed producer/consumer package identities in both metadata graphs:

- Blitz paint and UI share registry AnyRender 0.14.0.
- anyrender_vello 0.15.0 and UI share registry Vello 0.11.0.
- Adapter, Vello, UI and Bevy-render 0.20.0 share registry wgpu 30.0.1.
- Dioxus/native-dom share registry core/html 0.7.10; resolver/devtools are also
  registry 0.7.10. Hot-reload adds expected optional edges, no package identities.
- Blitz/UI layout crossings share git Taffy
  `2d936b7701f64b0fe95e50f1bfdcfc65a9f88f6f`. Separate registry Taffy and
  registry/git Parley packages are on Bevy-owned paths.
- All four Blitz packages resolve exactly to
  `74fe1abf090732524c86d96e6cdb6f95d8f8d6dc`.

The audit's exact-lock guard admits this independently reviewed graph for
scanning. It does not assert security acceptance. `--deny warnings`, the pinned
scanner/database, same-run provenance and incomplete-coverage failure remain.
The previous audit belongs to a different lock. Local cargo-audit 0.22.2
acquisition returned registry CONNECT 403; no new local audit result exists.
Do not infer the candidate's advisory result from retained package inventory.

## Current local supporting evidence

Environment: Linux x86_64 cloud workspace, Rust/Cargo 1.99.0, Node 24.19.0,
npm 11.9.0. These are local checks, not the Node22/npm10 hosted install claim.

- Root source README coverage, active Rust formatting, Bash syntax and diff
  checks pass.
- Svelte accessibility/typecheck and production UI build pass.
- Complete Node contract/UI suite: 151 passed, zero failed. This executes the
  actual Rust-generated IPC samples, including document generations and masks.
- Experimental egui widgets: 28 passed. Frontend-core library: 4 passed.
- Painting library/integration suites: 138 passed.
- Sculpting library/integration suites: 146 passed, 2 pre-existing manual cost
  tests ignored.
- Complete Scene library with sculpting, painting/editing, selection, wireframe
  and atmosphere: 157 passed, 3 pre-existing manual tests ignored.
- WASM check passes. Scene's reduced-feature build emits existing feature-boundary
  warnings; this compile result is not the strict warning acceptance claim.
- Focused Electron Node suite: 45 passed. Provisioning: 17 passed. Process
  supervisor: 4 passed. Shell typecheck, CJS/Python syntax and YAML parse pass.
- Default/hot-reload Dioxus checks and its library tests terminate on absent
  system `fontconfig.pc`; the native app egui test terminates on absent
  `libudev.pc`. These are unavailable environment evidence, not accepted
  frontend results.

No fake pkg-config data or weaker renderer/sandbox mode is used to obtain a
passing local native frontend check. The final integrated full launcher suite,
Dioxus default/hot-reload and warning checks, CEF/Electron rendered workflows,
and strict audit must be assessed on this candidate's actual hosted head.

## Historical stack evidence, not integrated-head acceptance

The runtime stack head `0d654286` passed
[verify](https://github.com/MrScripty/Pentimento/actions/runs/38422940543),
[Electron](https://github.com/MrScripty/Pentimento/actions/runs/38422940544),
[native CEF](https://github.com/MrScripty/Pentimento/actions/runs/38422940516),
[paint/sculpt](https://github.com/MrScripty/Pentimento/actions/runs/38422940514), and
[sculpt geometry](https://github.com/MrScripty/Pentimento/actions/runs/38422940512).
These include the native release/history ordering repair, but predate the
current main reconciliation and cannot replace its exact-head qualification.

Independent reviews found no concrete production regression in reviewed app,
input, egui, Scene depth/outline, GPU lifecycle and dependency crossings. This
review conclusion is supporting evidence; pending required runtime, full
frontend and security claims remain unsatisfied.

## Actual d1fc564 hosted qualification

The first integrated source passed [full launcher verify](https://github.com/MrScripty/Pentimento/actions/runs/38478308859), [Electron](https://github.com/MrScripty/Pentimento/actions/runs/38478308919), [native CEF](https://github.com/MrScripty/Pentimento/actions/runs/38478308816), [paint/sculpt](https://github.com/MrScripty/Pentimento/actions/runs/38478308780) and [geometry](https://github.com/MrScripty/Pentimento/actions/runs/38478308885). Electron/native screenshots, selection/depth/input/stroke/history and lifecycle evidence were inspected; these are software-graphics observations, not hardware performance qualification.

[Cohort resolution/focused checks](https://github.com/MrScripty/Pentimento/actions/runs/38478308963) passed on actual head d1fc564 and exact lock fd9a2b11..., including default/hot-reload/library/app/warning checks. Its complete strict audit failed with zero listed vulnerabilities, two unsound warnings (glib/rand) and four unmaintained warnings (fxhash/paste/proc-macro-error/ttf-parser). Scanner stderr was empty and coverage complete. These findings caused the approved compatible repair; no merge or security acceptance followed from the runtime passes.
