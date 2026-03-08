# crates/app-wasm/src

## Purpose
This directory contains the WASM entrypoint used when Pentimento runs inside Electron.

## Contents
| File/Folder | Description |
|-------------|-------------|
| `lib.rs` | WASM app bootstrap and Bevy plugin setup. |
| `bridge.rs` | Custom-event transport between browser JavaScript and Bevy WASM. |

## Problem
Electron needs Bevy compiled to WASM while keeping the message contract consistent with the native frontends.

## Constraints
- Must target `wasm32-unknown-unknown`.
- Must use browser-compatible event transport instead of native IPC globals.
- Must stay aligned with `crates/ipc`.

## Decision
Expose a minimal WASM app plus a browser-event bridge and keep host-specific Electron code out of this crate.

## Alternatives Rejected
- Embedding Electron-specific logic in the WASM crate: rejected because the shell already owns that boundary.

## Invariants
- Browser-side messaging stays in `bridge.rs`.
- Contract field names remain aligned with the native IPC schema.

## Revisit Triggers
- Another WASM host replaces Electron.
- The browser bridge requires persistent buffering or replay semantics.

## Dependencies
**Internal:** `crates/ipc`, `crates/scene`, `src-electron`  
**External:** Bevy WASM, wasm-bindgen, web-sys

## Related ADRs
- `ADR-001` active frontends and contract ownership.

## Usage Examples
```rust
#[wasm_bindgen(start)]
pub fn start() {
    pentimento_wasm::run();
}
```

## API Consumer Contract
- The browser host loads the generated WASM bundle and dispatches custom events for UI-to-Bevy messages.
- Consumers must initialize the module before sending bridge events.
- Invalid message payloads should be rejected by the JavaScript side before they reach WASM.

## Structured Producer Contract
- The crate emits browser custom events and consumes JSON payloads that mirror `UiToBevy`/`BevyToUi`.
- Contract changes require updates in the Electron shell and browser bridge code.
- No persisted artifacts are produced beyond the build output bundle.
