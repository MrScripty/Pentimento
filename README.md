# Pentimento

Pentimento is a multi-frontend Bevy workspace with one active browser UI contract shared across three supported frontend paths: CEF, Electron, and Dioxus.

## Canonical Workflow

Use the root launcher for install, build, run, and verification:

```bash
./launcher.sh --help
./launcher.sh --install
./launcher.sh --build --frontend cef
./launcher.sh --build-release --frontend electron
./launcher.sh --run --frontend dioxus
./launcher.sh --test
```

## Active Frontends

| Frontend | Ownership Model | Status |
|----------|------------------|--------|
| `cef` | Native Bevy app + Chromium offscreen webview | Active |
| `electron` | Electron shell + Svelte UI + Bevy WASM | Active |
| `dioxus` | Native Bevy app + Rust-native Dioxus UI | Active |

Discontinued paths such as capture, overlay, and Tauri remain in the repository for historical context only and are not part of the canonical standards-aligned workflow.

## Support Matrix

| Platform | Status | Notes |
|----------|--------|-------|
| Linux x86_64 | Required | Canonical CI and launcher verification target. |
| Windows x86_64 | Unsupported | `crates/webview/src/platform_windows.rs` is still a stub. |
| macOS ARM / Intel | Unsupported | No active verification path today. |

The support decision and IPC ownership model are recorded in [ADR-001](docs/adr/ADR-001-active-frontends-and-contract-ownership.md).

## System Requirements

### Linux packages

```bash
sudo apt-get install -y \
  libasound2-dev \
  libgtk-3-dev \
  libudev-dev \
  libwayland-dev \
  libxkbcommon-dev \
  pkg-config
```

### Tooling

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli
```

Node.js 22+ is required for the Svelte and Electron tooling.

## Verification

`./launcher.sh --test` is the canonical local verification command. It currently enforces:

- active source-directory README coverage
- active frontend Rust formatting
- Svelte accessibility linting
- TypeScript typechecking for the browser and Electron shells
- Rust-to-JavaScript IPC acceptance coverage
- cargo checks for the Dioxus, CEF, and WASM frontend paths

## Project Structure

```text
crates/app/                Native Bevy application entrypoint
crates/app-wasm/           Electron/WASM Bevy entrypoint
crates/dioxus-ui/          Native Rust UI implementation
crates/ipc/                Shared frontend/backend message contract
crates/webview/            Platform host integrations for CEF and Dioxus
src-electron/              Electron shell compiled from TypeScript
ui/                        Svelte frontend
docs/adr/                  Recorded architecture decisions
```

## License

MIT
