# Retained legacy frontends

WebKit capture/overlay and Tauri are discontinued (see
[ADR-001](../docs/adr/ADR-001-active-frontends-and-contract-ownership.md)).
This independent Cargo workspace preserves their source and entry points; it
is outside the supported CEF, Electron, Dioxus and experimental egui workflow.

The root workspace must not depend on these manifests. `legacy/app` and
`legacy/webview` compile the original `crates/app/src/main.rs` and
`crates/webview/src/lib.rs` with WebKit enabled. Keeping one source owner avoids
copying the application, renderer, input or IPC contract. The retained standalone
`crates/frontend-webkit`, `crates/frontend-overlay` and `src-tauri` packages
explicitly belong to this workspace.

## Entry points

From the repository root, build the existing Svelte UI first (`npm run build`).
On Linux, install the GTK 3/WebKitGTK 4.1 development libraries, plus the native
Bevy requirements documented in the root README. Then use:

```sh
# Original native capture/overlay app, with the same binary and environment selectors
PENTIMENTO_COMPOSITE=capture cargo run --locked --manifest-path legacy/Cargo.toml -p pentimento-legacy --features webkit
PENTIMENTO_COMPOSITE=overlay cargo run --locked --manifest-path legacy/Cargo.toml -p pentimento-legacy --features webkit

# Original capture example and standalone backends
cargo run --locked --manifest-path legacy/Cargo.toml -p pentimento-webview-legacy --example capture_test
cargo check --locked --manifest-path legacy/Cargo.toml -p pentimento-frontend-webkit -p pentimento-frontend-overlay

# Original Tauri app (frontend assets and native Tauri dependencies are still required)
cargo run --locked --manifest-path src-tauri/Cargo.toml -p pentimento-tauri
```

`just legacy-capture`, `just legacy-overlay`, `just legacy-build` and
`just legacy-check` are conveniences for these explicit manifests. Tauri's
configuration, build script, Rust entry points and frontend-dist path remain
unchanged. Its WASM assets must be prepared by the historical Tauri workflow.
No legacy command is added to `launcher.sh` or the active support matrix.

## Dependency and verification boundary

`legacy/Cargo.lock` is independent from the root lockfile and is tracked.
It deliberately retains GTK/WebKit/Tauri dependencies and their known audit
findings. Isolation is not a fix or security approval for those findings. Do
not copy its lockfile into the supported workspace or disable the root audit.
Legacy changes require their own lock review; use `--locked` for normal builds.

Workspace membership and retained entrypoint paths are checked through real
`cargo metadata --no-deps` consumers and the focused frontend dependency
contracts. These checks establish resolution boundaries and source ownership,
not a GUI support promise. Native compilation requires the actual GTK/WebKit
system development libraries; missing libraries must be reported as a blocker,
not replaced with fake pkg-config files or a skipped native check.
