# Pentimento

Combine full featured Svelte 5 GUI's with a native Bevy 0.18 3D GPU viewport for advanced graphics applications.

This is a stand-alone feature demo for Studio-Whip, exploring multiple methods of utalizing modern web tech to design frontends while retaining the performance and capabilities of native GPU graphics engines.

## System Requirements

### Linux Dependencies

Install the required development libraries:

```bash
# Ubuntu/Debian
sudo apt-get update
sudo apt-get install -y \
    libasound2-dev \
    libudev-dev \
    libxkbcommon-dev \
    libwayland-dev \
    libgtk-3-dev \
    libwebkit2gtk-4.1-dev \
    libayatana-appindicator3-dev \
    librsvg2-dev \
    pkg-config

# Fedora
sudo dnf install -y \
    alsa-lib-devel \
    systemd-devel \
    libxkbcommon-devel \
    wayland-devel \
    gtk3-devel \
    webkit2gtk4.1-devel \
    libappindicator-gtk3-devel \
    librsvg2-devel
```

### Node.js

Node.js 22+ is required for the Svelte UI build.

### Tauri Mode Additional Requirements

For Tauri mode, you also need:

```bash
# Rust WASM target
rustup target add wasm32-unknown-unknown

# wasm-bindgen CLI
cargo install wasm-bindgen-cli

# Tauri CLI
cargo install tauri-cli
```

## Building

```bash
# Install dependencies
just setup

# Development mode (with UI hot reload)
just dev

# Build for release
just build

# Run release build
just run
```

## Launcher Options

The `launcher.sh` script provides options for building and running:

```bash
./launcher.sh [OPTIONS]
```

**Build Options:**
- `--dev` - Development mode (uses Vite dev server for UI hot reload)
- `--build` - Build only, don't run
- `--release` - Build and run in release mode

**Compositing Modes:**

| Mode | Window Owner | Bevy Target | UI Engine | Use Case |
|------|--------------|-------------|-----------|----------|
| `--capture` | Bevy (native) | x86_64 | WebKitGTK | Default, most compatible |
| `--overlay` | Bevy (native) | x86_64 | WebKitGTK | Better performance on X11 |
| `--cef` | Bevy (native) | x86_64 | Chromium | Chromium rendering engine |
| `--tauri` | Tauri (native) | WASM | WebKitGTK | Cross-platform, simpler deployment |

- `--capture` - Renders WebKitGTK offscreen and captures to texture (default)
  - Most compatible, works on all systems
  - Slightly higher overhead due to framebuffer capture
- `--overlay` - Uses transparent GTK window overlay
  - Better performance, compositor handles blending
  - Best on X11, may have positioning issues on Wayland
- `--cef` - Renders CEF (Chromium) offscreen and captures to texture
  - Downloads CEF binaries on first run (~150MB)
  - Chromium rendering engine instead of WebKitGTK
- `--tauri` - Tauri mode with Bevy running as WASM
  - Tauri owns the window, Bevy renders via WebGL2 in a canvas
  - Svelte UI runs in the same webview as Bevy WASM
  - Simpler architecture, cross-platform via Tauri

**Examples:**
```bash
# Run with default capture mode
./launcher.sh

# Run with overlay mode
./launcher.sh --overlay

# Run with CEF mode (downloads binaries on first run)
./launcher.sh --cef

# Run with Tauri mode (Bevy as WASM)
./launcher.sh --tauri

# Release build with overlay mode
./launcher.sh --release --overlay

# Development mode (start Vite separately: cd ui && npm run dev)
./launcher.sh --dev
```

## Project Structure

```
Pentimento/
├── crates/
│   ├── app/         # Main native Bevy application
│   ├── app-wasm/    # Bevy WASM build for Tauri mode
│   ├── scene/       # Shared 3D scene (used by native and WASM)
│   ├── webview/     # Offscreen webview management
│   ├── ipc/         # Message protocol
│   └── diffusion/   # Texture streaming
├── src-tauri/       # Tauri desktop app
├── ui/              # Svelte frontend
└── dist/
    ├── ui/          # Built Svelte output
    └── wasm/        # Built Bevy WASM output
```

## Architecture

The application supports four compositing modes with two different architectural approaches:

### Native Modes (capture, overlay, cef)

Bevy owns the native window and GPU rendering. Svelte runs in a webview:

- **Capture mode**: WebKitGTK renders offscreen, framebuffer is captured and composited as a Bevy texture
- **Overlay mode**: Transparent GTK window overlays the Bevy window, desktop compositor handles blending
- **CEF mode**: Chromium (CEF) renders offscreen, framebuffer is captured and composited as a Bevy texture

### Tauri Mode

Inverts the architecture - Tauri owns the window:

- Bevy compiles to WASM and renders via WebGL2 in a canvas element
- Svelte UI runs in the same webview alongside Bevy
- Communication via CustomEvents instead of native IPC
- Uses `Tonemapping::Reinhard` for WebGL2 compatibility (TonyMcMapFace requires tonemapping_luts which needs zstd, unavailable in WASM)

## License

MIT
