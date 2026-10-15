# Pentimento

Paint, project and sculpt in one 3D workspace. Pentimento brings layered 2D canvases and editable meshes together, so you can sketch a surface, project that work onto a model and keep shaping the scene.

The editor includes canvas brushes, projection painting, mesh-editing and sculpting tools, lighting controls and local project Save/Open. It is development software; the [project-format guide](docs/project-format-v1.md) explains what a saved document preserves and which features are still limited.

## Run on Linux

Linux x86_64 is the supported platform. Install Rust stable, Node.js 22.12+ and the [Linux build prerequisites](docs/getting-started.md), then:

```sh
git clone https://github.com/MrScripty/Pentimento.git
cd Pentimento
./launcher.sh --install
./launcher.sh --run --frontend cef
```

The launcher builds the app and prepares its matching browser runtime. A working display and Vulkan-capable renderer are required.

CEF is the primary native editor. The same launcher also offers `electron` and `dioxus`; `egui` is experimental. Windows and macOS are currently unsupported. See [setup and frontend choices](docs/getting-started.md).

## Development

```sh
./launcher.sh --test
```

Use `./launcher.sh --help` for release builds and other actions. Keep the committed Cargo and npm lockfiles; the launcher installs from them.

- [Painting and sculpting](docs/paint-sculpt-qualification.md)
- [Project files](docs/project-format-v1.md), [DirectUV](docs/project-directuv-v2.md) and [shared UV layers](docs/shared-uv-layers-v3.md)
- [Frontend architecture](docs/adr/ADR-001-active-frontends-and-contract-ownership.md)
- [Dependency baseline](docs/cargo-lock-baseline.md)
- [Retained legacy frontends](legacy/README.md)

## License

[MIT](LICENSE). Vendored libraries and fonts retain their [own licenses and notices](vendor/README.md).

