# ui/src/lib

## Purpose
This directory contains the browser-side interop layer and reusable primitives that let the Svelte frontend talk to Bevy without embedding host-specific code in every component.

## Contents
| File/Folder | Description |
|-------------|-------------|
| `bridge.ts` | Runtime transport for native IPC injection and Electron/Tauri custom-event delivery. |
| `types.ts` | TypeScript mirror of the active Rust IPC contract. |
| `components/` | Shared UI pieces used by `App.svelte`. |

## Problem
The UI needs one stable place to encode host detection, message parsing, and outbound command emission so components stay declarative.

## Constraints
- Must match the Rust contract names and shapes from `crates/ipc`.
- Must work in native browser hosts and Electron/WASM.
- Must keep DOM observers and timers owned by the bridge layer.

## Decision
Use `bridge.ts` as the single runtime transport owner and keep `types.ts` as the mirrored contract boundary for the Svelte app.

## Alternatives Rejected
- Inline `window` IPC calls in components: rejected because it duplicates transport logic and makes cleanup brittle.
- JSON contract literals per component: rejected because it hides drift until runtime.

## Invariants
- Components send commands through `bridge.ts`, not raw `window` globals.
- `types.ts` mirrors `crates/ipc` and is treated as a consumer contract, not an independent schema.
- `setupAutoMarkDirty()` returns the only supported cleanup handle for the DOM observer and resize listener it owns.

## Revisit Triggers
- A generated Rust-to-TypeScript contract pipeline replaces manual mirroring.
- Another frontend host requires a new transport mode.

## Dependencies
**Internal:** `ui/src`, `crates/ipc`, `tests/contracts`  
**External:** Browser DOM APIs

## Related ADRs
- `ADR-001` active frontends and contract ownership.

## Usage Examples
```ts
import { bridge } from './bridge';

bridge.setDepthView(true);
bridge.subscribe((message) => console.log(message.type));
```

## API Consumer Contract
- Consumers subscribe through `bridge.subscribe()` and receive parsed `BevyToUi` messages.
- Outbound calls serialize `UiToBevy` messages immediately; callers should not assume retries.
- Invalid inbound JSON is rejected at the bridge boundary and logged.
- App bootstrap owns `setupAutoMarkDirty()` startup/teardown and must dispose the bridge on module restart.

## Structured Producer Contract
- `types.ts` defines the stable field names the browser UI expects from Rust.
- `bridge.ts` emits JSON that matches `UiToBevy`.
- Contract changes require updates in `crates/ipc/examples/contract_samples.rs` and `tests/contracts/ipc-contract.test.mjs`.
