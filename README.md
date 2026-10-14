# Pentimento

Pentimento is a multi-frontend Bevy workspace with a shared frontend/backend
contract across supported CEF, Electron, and Dioxus frontends plus an
experimental native egui implementation.

## Canonical Workflow

Use the root launcher for install, build, run, and verification:

```bash
./launcher.sh --help
./launcher.sh --install
./launcher.sh --build --frontend cef
./launcher.sh --build --frontend egui
./launcher.sh --build-release --frontend electron
./launcher.sh --run --frontend dioxus
./launcher.sh --run --frontend egui
./launcher.sh --test
```

The root `package-lock.json` and `src-electron/package-lock.json` are tracked
inputs to reproducible npm installs. Routine bootstrap uses
`./launcher.sh --install`, which runs `npm ci` without regenerating either
lockfile. Deliberate dependency updates must regenerate and review the affected
lockfile alongside its manifest using the Node.js 22/npm 10 CI toolchain. Do not
delete or regenerate lockfiles to work around a failed install.

## Frontend Paths

| Frontend | Ownership Model | Status |
|----------|------------------|--------|
| `cef` | Native Bevy app + Chromium offscreen webview | Active |
| `electron` | Electron shell + Svelte UI + Bevy WASM | Active |
| `dioxus` | Native Bevy app + Rust-native Dioxus UI | Active |
| `egui` | Native Bevy app + `bevy_egui` overlay UI | Experimental |

Discontinued capture, overlay, and Tauri paths are retained in the independent
[legacy workspace](legacy/README.md), with their source and explicit entry points
preserved. Their dependencies and lockfile are outside the canonical
standards-aligned workflow and the root workspace dependency audit.

## Support Matrix

| Platform | Status | Notes |
|----------|--------|-------|
| Linux x86_64 | Required | Canonical CI and launcher verification target. |
| Windows x86_64 | Unsupported | `crates/webview/src/platform_windows.rs` is still a stub. |
| macOS ARM / Intel | Unsupported | No active verification path today. |

The support decision, experimental egui status, and IPC ownership model are
recorded in [ADR-001](docs/adr/ADR-001-active-frontends-and-contract-ownership.md).

## System Requirements

### Linux packages

```bash
sudo apt-get install -y \
  libasound2-dev \
  libgtk-3-dev \
  libudev-dev \
  libwayland-dev \
  libwebkit2gtk-4.1-dev \
  libxkbcommon-dev \
  pkg-config
```

### Tooling

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli
```

Node.js 22.12.0+ is required for the Svelte and Electron tooling.

The browser runtimes are pinned to CEF 154.0.34 (Rust bindings 154.5.0) and
Electron 44.7.0, verified latest stable on 2026-10-13. `./launcher.sh --install`
explicitly downloads Electron after `npm ci`. CEF's locked build provisions its
matching distribution, helper, and resources; `./scripts/setup-cef.sh` can build
that helper separately. Do not reuse an older `CEF_PATH` distribution. The
launcher rejects mismatched or incomplete adjacent CEF resources rather than
searching stale build caches. See the [upgrade evidence](docs/plans/browser-runtime-upgrade/plan.md)
for verification status and remaining acceptance requirements.

The Electron runtime CI separately builds genuine production UI/WASM, checks the
ordinary Linux sandbox and preload isolation, and records canvas-only scene
captures for visual review. Its test-only Linux supervisor records descendant
PID/start-time identities before shutdown and verifies observed process exits;
a `will-quit` event alone is not considered completed shutdown.

## Verification

`./launcher.sh --test` is the canonical local verification command. Canonical launcher
builds/checks and CI checks use the committed root `Cargo.lock` with `--locked`; dependency
changes require explicit lock review. See [dependency lock provenance and known
audit findings](docs/cargo-lock-baseline.md). This reproducibility baseline is
not security clearance or a runtime support guarantee.

The verification suite currently enforces:

- active source-directory README coverage
- active frontend Rust formatting
- CPU sculpt topology, UV-seam, tessellation, and render-asset synchronization tests
- combined projection, brush-control, and sculpt mode-transition engine tests
- sculpting library unit tests, including the v1 normal codec and brush packets
- Svelte accessibility linting
- TypeScript typechecking for the browser and Electron shells
- production UI asset generation before Rust checks that embed `dist/ui`
- Rust-to-JavaScript IPC acceptance coverage
- warning-free cargo checks for the CEF, Dioxus, egui, WASM, and shared native
  UI crates

For the focused integration gate and required rendered CEF acceptance, see
[paint/sculpt qualification](docs/paint-sculpt-qualification.md).

Local File Save/Open and the bounded, lossless editable document subset are
described in [project format v1](docs/project-format-v1.md).

## Project Structure

```text
crates/app/                Native Bevy application entrypoint
crates/app-wasm/           Electron/WASM Bevy entrypoint
crates/dioxus-ui/          Native Rust UI implementation
crates/egui-ui/            Experimental native egui UI implementation
crates/frontend-core/      Shared native frontend snapshot and contract helpers
crates/ipc/                Shared frontend/backend message contract
crates/webview/            Platform host integrations for CEF and Dioxus
src-electron/              Electron shell compiled from TypeScript
ui/                        Svelte frontend
docs/adr/                  Recorded architecture decisions
```

## License

MIT

Vendored compatibility forks and fonts retain their own MIT/BSD/MPL-2.0/OFL
terms and notices; the project license does not replace those obligations.
See [vendor ownership and licenses](vendor/README.md) and the SCTK
[exact-source distribution instructions](vendor/sctk-adwaita/SOURCE_AVAILABILITY.md).
