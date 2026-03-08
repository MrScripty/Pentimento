# ADR-001 Active Frontends and Contract Ownership

## Status
Accepted on 2026-03-08.

## Context
Pentimento currently ships three active frontend paths with materially different runtime boundaries:

- CEF: native Bevy process with browser UI rendered through the webview stack
- Dioxus: native Bevy process with a Rust-native UI renderer
- Electron: Svelte UI plus Bevy WASM inside an Electron shell

The previous project state had two gaps that kept the codebase drifting:

1. The support matrix was implicit, so Windows looked supported in code even though the webview backend was still stubbed.
2. The Rust and TypeScript IPC contracts were mirrored manually with no enforced ownership model.

## Decision
- Treat `crates/ipc` as the source of truth for the active frontend contract until a generated schema replaces it.
- Require every contract change to update the JavaScript consumer mirror and the Rust-generated acceptance sample used by `tests/contracts/ipc-contract.test.mjs`.
- Treat Linux x86_64 as the required supported platform for the active frontend stack today.
- Treat Windows x86_64 and macOS as explicitly unsupported for the active frontend stack until the missing native backend work is implemented and verified.
- Use `launcher.sh` as the canonical entrypoint for install, build, run, test, and release-smoke workflows.

## Consequences
### Positive
- One contract owner exists for cross-language changes.
- CI and hooks validate the actual active frontend surface instead of relying on ad hoc commands.
- The support promise is honest about current platform reality.

### Negative
- TypeScript still mirrors Rust manually for now.
- Windows users do not have a supported native path until the webview backend is implemented.

## Revisit Triggers
- A code generation workflow is adopted for Rust-to-TypeScript contract artifacts.
- The Windows webview backend grows beyond the current stub and can pass the canonical launcher verification.
- Another browser host replaces Electron or CEF as an active supported frontend.
