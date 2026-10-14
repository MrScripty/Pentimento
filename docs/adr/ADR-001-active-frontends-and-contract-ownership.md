# ADR-001 Active Frontends and Contract Ownership

## Status
Accepted on 2026-03-08.

## Context
Pentimento currently ships three active supported frontend paths plus one
experimental native path with materially different runtime boundaries:

- CEF: native Bevy process with browser UI rendered through the webview stack
- Dioxus: native Bevy process with a Rust-native UI renderer
- Electron: Svelte UI plus Bevy WASM inside an Electron shell
- egui: native Bevy process with a `bevy_egui` overlay UI (experimental)

The previous project state had two gaps that kept the codebase drifting:

1. The support matrix was implicit, so Windows looked supported in code even though the webview backend was still stubbed.
2. The Rust and TypeScript IPC contracts were mirrored manually with no enforced ownership model.

## Decision
- Treat `crates/ipc` as the source of truth for the active frontend contract until a generated schema replaces it.
- Require every contract change to update the JavaScript consumer mirror and the Rust-generated acceptance sample used by `tests/contracts/ipc-contract.test.mjs`.
- Treat Linux x86_64 as the required supported platform for the active frontend stack today.
- Allow experimental frontend paths to reuse the same contract and launcher
  workflow, but require docs and verification to mark them explicitly until
  parity and support commitments are promoted.
- Treat Windows x86_64 and macOS as explicitly unsupported for the active frontend stack until the missing native backend work is implemented and verified.
- Use `launcher.sh` as the canonical entrypoint for install, build, run, test, and release-smoke workflows.

## Consequences
### Positive
- One contract owner exists for cross-language changes.
- CI and hooks validate the actual active frontend surface instead of relying on ad hoc commands.
- Experimental frontends can land without creating a second IPC ownership model
  or a hidden build path outside the launcher.
- The support promise is honest about current platform reality.

### Negative
- TypeScript still mirrors Rust manually for now.
- Windows users do not have a supported native path until the webview backend is implemented.

## Revisit Triggers
- A code generation workflow is adopted for Rust-to-TypeScript contract artifacts.
- The Windows webview backend grows beyond the current stub and can pass the canonical launcher verification.
- Another browser host replaces Electron or CEF as an active supported frontend.
- The egui path reaches supported parity and should be promoted from
  experimental to active.

## Retained legacy dependency boundary (2026-10-13)

Discontinued WebKit capture/overlay and Tauri are preserved in an independent
`legacy/Cargo.toml` workspace. The root app and webview manifests no longer
register WebKit features or GTK/WebKit dependency edges, and Tauri is no longer
a root workspace member. Explicit legacy manifests compile the retained app
and webview source; standalone WebKit/overlay and Tauri manifests select that
workspace. See [retained entry points](../../legacy/README.md).

The boundary owns dependency resolution and historical entry points only.
`crates/ipc` still owns the shared contract, and shared application/render/input
source retains one owner. A supported frontend change does not require touching
legacy manifests unless the shared source contract changes; a historical native
dependency update changes only the legacy graph. Removing these separate
manifests would force retained GTK/WebKit/Tauri dependencies back into the active
resolver or remove the explicitly retained entry points. This is the reason for
the extra build manifests; no duplicate application implementation is introduced.

The root audit remains enabled and applies to the supported workspace lockfile.
The separately tracked legacy lock retains its own advisories; this isolation
does not clear those advisories or promote the discontinued frontends to support.
