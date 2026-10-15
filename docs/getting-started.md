# Running Pentimento from source

The supported target is Linux x86_64. CEF is the primary native editor; Electron uses the Svelte interface with the WASM engine, and Dioxus provides a native Rust interface. The egui frontend is experimental. Windows and macOS are currently unsupported.

## Prerequisites

Install Rust stable through rustup and Node.js 22.12 or newer. The Ubuntu verification workflows install these frontend development packages:

```sh
sudo apt-get update
sudo apt-get install -y \
  libasound2-dev libgtk-3-dev libudev-dev libwayland-dev \
  libwebkit2gtk-4.1-dev libxkbcommon-dev pkg-config
```

The native CEF workflow also installs `cmake` and `ninja-build`. Running the desktop editor needs a working display and Vulkan-capable renderer. Headless CI's software graphics and X11 test tools are qualification infrastructure, not a hardware recommendation.

From the repository root:

```sh
./launcher.sh --install
./launcher.sh --run --frontend cef
```

`--run` builds development artifacts before starting the selected app. A separate `--build` step is unnecessary for that flow.

The install action uses `npm ci` for the root and Electron packages, explicitly installs Electron's runtime, adds the WASM target and installs the `wasm-bindgen-cli` version matching the workspace dependency. It asks you to install missing Rust/Node tools rather than installing those system tools for you. It does not install OS packages.

CEF builds provision the distribution, helper and resources matching the locked bindings. Do not point `CEF_PATH` at an older installation; incomplete or mismatched adjacent resources are rejected. [Browser runtime details and dated evidence](plans/browser-runtime-upgrade/plan.md).

## Other frontends

```sh
./launcher.sh --run --frontend electron
./launcher.sh --run --frontend dioxus
./launcher.sh --run --frontend egui
```

Each command selects a separate interface. Feature parity and runtime acceptance differ; egui remains experimental. The [architecture decision](adr/ADR-001-active-frontends-and-contract-ownership.md) records these boundaries.

Development, test and smoke actions use isolated state under `.launcher-state/` by default. Set `PENTIMENTO_LAUNCHER_STATE_ROOT` to choose another location, or `PENTIMENTO_LAUNCHER_ISOLATE_STATE=0` to use host state.

## Builds and checks

```sh
./launcher.sh --build --frontend cef
./launcher.sh --build-release --frontend cef
./launcher.sh --run-release --frontend cef
./launcher.sh --test
```

`--run-release` requires an existing release build; `--release-smoke` builds and briefly launches one. See `./launcher.sh --help` for all options.

Keep root `Cargo.lock`, root `package-lock.json` and `src-electron/package-lock.json` tracked. Routine bootstrap preserves them. Review dependency and lockfile changes together; deleting a lockfile is not an installation fix. [Dependency provenance and known audit findings](cargo-lock-baseline.md).

The aggregate suite checks active frontends, shared contracts and engine behavior. It is separate from [rendered native painting/sculpting acceptance](paint-sculpt-qualification.md) and does not by itself establish a hardware performance or security guarantee.

