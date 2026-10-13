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
