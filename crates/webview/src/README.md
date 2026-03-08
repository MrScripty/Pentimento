# crates/webview/src

## Purpose
This directory contains the platform-specific host implementations that embed browser or native UI renderers into the Bevy application.

## Contents
| File/Folder | Description |
|-------------|-------------|
| `lib.rs` | Shared public entrypoints and backend selection glue. |
| `platform_linux.rs` | WebKitGTK capture backend for legacy/native browser embedding. |
| `platform_linux_cef.rs` | CEF backend used by the active Chromium-native frontend. |
| `platform_linux_dioxus.rs` | Dioxus renderer bridge for the native Rust UI path. |
| `platform_windows.rs` | Explicit unsupported stub for the current Windows path. |

## Problem
Pentimento needs host integrations for CEF and Dioxus while keeping platform-specific code out of the application layer.

## Constraints
- Platform-specific code must stay isolated by file.
- Active Linux backends must support the launcher verification flow.
- Windows is not yet feature-complete and must be represented honestly.

## Decision
Keep backend selection in `lib.rs` and isolate each platform/host combination in dedicated files, even when one implementation is still a stub.

## Alternatives Rejected
- Inline `cfg()` checks through the application layer: rejected because it hides platform boundaries.
- Pretending Windows is supported without a backend: rejected because it creates false guarantees.

## Invariants
- Host-specific code stays in dedicated platform files.
- Active frontends expose the same high-level Bevy-facing API.

## Revisit Triggers
- Windows support becomes an active requirement.
- CEF or Dioxus is replaced by another host technology.

## Dependencies
**Internal:** `crates/app`, `crates/ipc`, `crates/dioxus-ui`  
**External:** CEF, WebKitGTK, wry

## Related ADRs
- `ADR-001` active frontends and contract ownership.

## Usage Examples
```rust
let webview = pentimento_webview::CefWebview::new(size, html)?;
```

## API Consumer Contract
- Consumers call the public backend constructors from `lib.rs`.
- Runtime failures surface as `WebviewError`; callers must treat startup failure as fatal for the selected frontend.
- Ordering matters: initialize the backend before sending input or resize events.

## Structured Producer Contract
- This directory produces framebuffer buffers and event callbacks for the Bevy app, not persisted artifacts.
- Pixel format and event semantics are backend-specific but must remain consistent at the public API boundary.
- Any public shape change requires coordinated updates in `crates/app`.
