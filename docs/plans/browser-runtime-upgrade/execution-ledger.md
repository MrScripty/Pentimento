# Execution ledger

## 2026-10-13: discovery and migration

Inspected PR #27 and created a separate worktree at its exact head. Found CEF 143.7 declarations, stale independent CEF 144 setup archive, and Electron ^38.0.0 (locked 38.8.6). Verified official current stable CEF154 and Electron44 metadata. Bevy is exactly 0.20.0 and is retained.

The active CEF adapter is `crates/webview/src/platform_linux_cef.rs`. The standalone adapter is separate. Existing native qualification strictly checks visible toolbar framebuffer content; that assertion remains required. Local Rust 1.97.1 and Node 24.19.0 are available; native system development libraries are missing.

## 2026-10-13: implemented dependency and acquisition boundary

Pinned cef154.5.0 centrally for host/helper and matched the independent adapter. Cargo changed only cef, cef-dll-sys and download-cef. Upstream154 copies its runtime beside built executables; the launcher now validates that concrete artifact against Cargo.lock instead of selecting the first stale build-cache directory. The separate CEF144 unchecked installer now delegates to the locked helper build, without deleting a shared cache. Host and helper initialize the same versioned CEF API and report unavailable support.

Pinned Electron44.7.0 and updated its generated lock. Explicit runtime installation is separate from read-only readiness; launcher preflight and invocation do not trigger Electron's new lazy downloader. Corrected compiled shell→production UI path and retained isolated sandboxed preload. Focused contract tests cover both acquisition boundaries.

Diagnosed the CEF startup failure using prior CI's actual startup and failure images: the initial native image was black but the later image displayed the toolbar. The corrected test waits for the existing pixel oracle, under the same deadline, rather than assuming a CPU upload means the frame was presented. All later strict native assertions remain intact. [Diagnosis and evidence](reports/cef-readiness.md).

Local helper build, actual library loading, adapter API source check, Electron executable identity, frontend static checks/contract tests and production Svelte build passed. [Detailed local evidence](reports/local-checks.md). Full native/production acceptance remains pending representative CI.

## 2026-10-13: independent review

Independent review found and verified fixes for failure masking when Bash disables errexit inside a conditional installer, preservation of the launcher’s missing-release exit status, and an inherited Electron smoke assumption that the preload’s lexical process was a global. Focused regressions cover the installer and status corrections; production preload privilege was not expanded. Final independent checks:78 Node tests and4 process-supervisor tests passed, along with runtime metadata preflights, syntax, and whitespace checks. No unresolved introduced code defect remained. Full native and Electron rendered CI acceptance is still required.

The standalone CEF adapter also passed a real-distribution cargo check without dox. Its pre-existing seven unused/private-interface warnings remain outside this migration; the full active app is locally blocked at missing GLib development metadata.

## 2026-10-13: first published qualification and bounded renderer repair

Published draft PR #28 stacked on PR #27 at remote commit719ac6bfcf7a78035f3be8b22ee76bbca8b0c73e (same tree as local e1d38d19d744db3e32402b0d20982827be1e41a9). Electron runtime qualification passed its sandbox probe and production WASM build, then failed the strict rendered-scene check. Actual renderer logs identify unsupported WebGL alternate texture views, mismatched postprocess attachment formats, and unsupported depth textureLoad shader generation. The SwiftShader deprecation warning alone was not treated as the cause.

Re-opened the plan for a bounded, independently reviewed renderer/capability repair. Native depth remains supported; previously ignored browser depth commands will report unavailable capability instead of optimistic success. No sandbox/security weakening is authorized or introduced. The full CEF154 app/helper build and native input/capture regressions passed on the first published head; strict runtime interaction acceptance is still running.

The official Rust wasm32 target and exact wasm-bindgen-cli0.2.129 were installed locally. The canonical Electron launcher build completed successfully with real WASM, Svelte production output and Electron TypeScript. Because renderer edits began during that run, final artifact identity requires a clean incremental rebuild after the coordinated diff settles.

The first CEF154 runtime check completed at the later brush-panel scroll stage. It established visible toolbar pixels (92,160/92,160), painted sculpt controls, actual Grab/radius changes, widget isolation, an accepted sculpt operation on1,216 faces and19,634 changed native pixels. It then failed to scroll downward to a brush-history control. Inspection traced internal DOM-style positive-down wheel input being forwarded with the wrong sign to CEF. A minimal adapter-boundary correction and regression are required before claiming native qualification. Run: https://github.com/MrScripty/Pentimento/actions/runs/38404508780; evidence: https://github.com/MrScripty/Pentimento/actions/runs/38404508780/artifacts/11818848263.

## 2026-10-13: reviewed repair and local regression gates

The first published head's [aggregate verification](https://github.com/MrScripty/Pentimento/actions/runs/38404508737), [paint/sculpt engine integration](https://github.com/MrScripty/Pentimento/actions/runs/38404508779), and [sculpt geometry](https://github.com/MrScripty/Pentimento/actions/runs/38404508751) all passed. The two runtime failures above remain historical evidence, not passing acceptance.

The repair specializes actual per-view postprocess formats, preserves all24-bit outline IDs through compensated sRGB storage, and reports real depth support to every frontend. Independent review corrected delayed native capability-query replay; local test execution corrected the new egui test's ownership of texture deltas. The CEF wheel fix is shared by both adapters. Canonical verification now runs complete scene/frontend library suites and native depth-dispatch/toolbar tests rather than relying only on compilation.

Local results before publishing the repair: real production WASM packaging passed on a serialized retry; scene94/94, frontend-core4/4 and egui28/28 passed; full frontend verification and process-supervisor regressions passed. The real standalone CEF adapter recheck passed with its unchanged pre-existing warnings. Dioxus library execution and the full native application remain locally blocked by missing fontconfig/GTK development metadata and are required in hosted qualification. See [local checks](reports/local-checks.md) for boundaries.

The runtime harness now observes the browser's resolved unavailable-depth reason and exercises native supported depth twice through physical toolbar input, varied grayscale scene pixels and calibrated restoration. Native baseline evidence deliberately follows native wireframe/atmosphere defaults rather than imposing WASM's material palette. Near/far depth ordering, isolated visual shadow/AO effects, a complete SDR/HDR transition matrix and real hardware/stylus behavior remain unqualified.

## 2026-10-13: second published head and evidence-calibrated assertions

Published the reviewed repair at713653f0804255636af5b89e6e6eb1cc12f6e311, treea824de470481cde6cde3611bcf8e687c7014fa96. The hosted [paint/sculpt integration](https://github.com/MrScripty/Pentimento/actions/runs/38410829892) and [geometry](https://github.com/MrScripty/Pentimento/actions/runs/38410829829) passed. [Aggregate verification](https://github.com/MrScripty/Pentimento/actions/runs/38410830026) found a rendered-test selector ambiguity caused by two legitimate status messages; the scoped assertion is corrected without removing either accessibility status.

[Electron qualification](https://github.com/MrScripty/Pentimento/actions/runs/38410829861) passed the ordinary sandbox, genuine WASM build, authoritative unavailable-depth reason and strict canonical scene captures. Real mouse input selected the cube and rendered its thin orange outline without shader errors. The new silhouette oracle incorrectly excluded dark red faces. Independent inspection and replay of [the retained artifact](https://github.com/MrScripty/Pentimento/actions/runs/38410829861/artifacts/11822966280) localized all1,842 orange pixels after the bounded geometry-classifier correction, with zero orange interior fill and zero meaningful outside changes. The original acceptance thresholds remain unchanged. The full job is still recorded as failed until rerun, including its unreached graceful-shutdown handshake.

The local executor reset after publication; a fresh checkout was recovered and verified against the exact remote commit/tree. Previously published source and hosted runs were preserved. Local Node dependencies and the same Rust1.97.1 toolchain were restored for final checks.

The same source head713653f subsequently passed [full native CEF qualification](https://github.com/MrScripty/Pentimento/actions/runs/38410829970), including actual toolbar presentation, repeated supported depth toggles, wheel scrolling and the unchanged strict paint/sculpt/history assertions. [Retained native evidence](https://github.com/MrScripty/Pentimento/actions/runs/38410829970/artifacts/11821199796) is authoritative for that head. The final correction changes only test observation and evidence documents; the application/runtime source remains unchanged. Full local verification on the corrected tests passes102 tests plus formatting, accessibility and type checks. Final hosted aggregate and Electron shutdown acceptance still require the next exact-head run.
