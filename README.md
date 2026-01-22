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

### WASM Mode Requirements (Electron/Tauri)

For Electron or Tauri WASM modes, you also need:

```bash
# Rust WASM target
rustup target add wasm32-unknown-unknown

# wasm-bindgen CLI
cargo install wasm-bindgen-cli

# For Tauri mode only (unmaintained)
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
| `--electron` | Electron | WASM | Chromium | Stable WASM mode for Linux |
| `--tauri` | Tauri (native) | WASM | WebKitGTK | ⚠️ **Unmaintained** - see Known Issues |

- `--capture` - Renders WebKitGTK offscreen and captures to texture (default)
  - Most compatible, works on all systems
  - Slightly higher overhead due to framebuffer capture
- `--overlay` - Uses transparent GTK window overlay
  - Better performance, compositor handles blending
  - Best on X11, may have positioning issues on Wayland
- `--cef` - Renders CEF (Chromium) offscreen and captures to texture
  - Downloads CEF binaries on first run (~150MB)
  - Chromium rendering engine instead of WebKitGTK
- `--electron` - Electron mode with Bevy running as WASM
  - Electron owns the window, Bevy renders via WebGL2 in a canvas
  - Svelte UI runs in the same webview as Bevy WASM
  - Uses Chromium (stable WebGL2, recommended for Linux WASM)
- `--tauri` - ⚠️ **Unmaintained** - Tauri mode with Bevy running as WASM
  - Tauri owns the window, Bevy renders via WebGL2 in a canvas
  - Svelte UI runs in the same webview as Bevy WASM
  - **Not functional on Linux** due to WebKitGTK WebGL bugs (see Known Issues)

**Examples:**
```bash
# Run with default capture mode
./launcher.sh

# Run with overlay mode
./launcher.sh --overlay

# Run with CEF mode (downloads binaries on first run)
./launcher.sh --cef

# Run with Electron mode (Bevy as WASM, recommended for WASM on Linux)
./launcher.sh --electron

# Run with Tauri mode (Bevy as WASM) - UNMAINTAINED
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
├── src-tauri/       # Tauri desktop app (unmaintained)
├── src-electron/    # Electron desktop app
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

### WASM Modes (Electron, Tauri)

Inverts the architecture - the desktop framework owns the window:

- Bevy compiles to WASM and renders via WebGL2 in a canvas element
- Svelte UI runs in the same webview alongside Bevy
- Communication via CustomEvents instead of native IPC
- Uses `Tonemapping::Reinhard` for WebGL2 compatibility (TonyMcMapFace requires tonemapping_luts which needs zstd, unavailable in WASM)

**Electron mode** (`--electron`) is recommended for WASM on Linux as it uses Chromium with stable WebGL2 support.

**Tauri mode** (`--tauri`) is unmaintained due to WebKitGTK 
WebGL bugs.

The Tauri team is exploring alternatives including Chromium/CEF integration and Servo. See the [Tauri discussion](https://github.com/orgs/tauri-apps/discussions/8524) for updates.

## Known Issues

### Tauri/WASM Mode - Unmaintained

**Status:** Tauri mode (`--tauri`) is **unmaintained** and not functional on Linux due to an upstream WebKitGTK bug.

WebKitGTK 2.40+ has known WebGL2 instability issues that cause context loss and crashes. This is a WebKitGTK bug, not a Pentimento or Tauri issue. The WASM build works correctly in Chrome/Firefox - only WebKitGTK is affected.

**Symptoms:**
- Window turns solid grey after a few seconds
- `WebLoaderStrategy::internallyFailedLoadTimerFired()` errors in console
- WebGL context lost

### Capture Mode - Unmaintained

This mode is fundamentaly flawed. Despite many agent hours and my own time developing methodologies for this mode, there was no way to make it work in a maner which results in a good UX. I cannot recomend this method for any serious use. 

- UI only updates when a new capture occurs; event-driven capture misses hover/animation updates and can feel frozen.
- A continuous capture heartbeat fixes responsiveness but is expensive and can tank frame rate. If you dont do this you have to roll your own system for keeping the process alive until all actions have completed.
- GTK/WebKit are single-threaded, so heavy capture or input pumping can stall Bevy if done synchronously.
- DPI scaling must be applied before rasterization (render at physical size, adjust viewport scale), or the UI will blur. WebKitGTK will not adapt on its own, but will scale the rendered result instead causing blurry UI.

It is possible to run WebKitGTK and Bevy completly seperate processes so that event loops are decoupled, but the complicaitons of handling frame transport negate pratical use cases. For example, in theory you can use DMA‑BUF/EGLImage to share GPU buffers accross context, but it is a Linux kernel mechanism that does not translate to other OS. It could work with good performance, but it is not sutable for the needs of Studio Whip or the vast majority of other applicaiotns. 

**Recommendation:** Use `--electron` for WASM mode (uses Chromium), or `--cef` for native Bevy rendering. `--overlay` might work but it is not consistent across platforms as it depends on the OS implementaiton of webview.


## License

MIT
