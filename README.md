# Pentimento

A stand-alone feature demo for Studio-Whip, exploring Combined full featured Svelte 5 GUI + native Bevy 0.18 3D viewport with native GPU rendering.

A number of different methods are tested to determine the best way to get the best of both worlds for desktop applications. 

## Quick Start

```bash
just setup    # Install dependencies
just dev      # Development mode (UI hot reload)
just build    # Release build
just run      # Run release
```

## Compositing Modes

| Mode | Architecture | UI Engine | Status |
|------|--------------|-----------|--------|
| `--cef` | Native Bevy + offscreen Chromium | CEF | Recommended |
| `--electron` | WASM Bevy in Electron | Chromium | Recommended for WASM |
| `--overlay` | Native Bevy + GTK overlay | WebKitGTK | X11 only |
| `--capture` | Native Bevy + offscreen WebKitGTK | WebKitGTK | Unmaintained |
| `--tauri` | WASM Bevy in Tauri | WebKitGTK | Unmaintained |

```bash
./launcher.sh --cef              # Recommended: native Bevy + Chromium
./launcher.sh --electron         # WASM mode with Chromium
./launcher.sh --release --cef    # Release build
./launcher.sh --dev              # Dev mode (run `cd ui && npm run dev` first)
```

## System Requirements

### Linux

```bash
# Ubuntu/Debian
sudo apt-get install -y \
    libasound2-dev libudev-dev libxkbcommon-dev libwayland-dev \
    libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev \
    librsvg2-dev pkg-config
```

### WASM Modes (Electron/Tauri)

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli
```

Node.js 22+ required for Svelte UI.

## Project Structure

```
crates/
├── app/         # Native Bevy application
├── app-wasm/    # WASM Bevy build
├── scene/       # Shared 3D scene
├── webview/     # Offscreen webview (WebKitGTK, CEF)
├── ipc/         # Message protocol
└── diffusion/   # Texture streaming
src-electron/    # Electron shell
ui/              # Svelte frontend
```

## Architecture

**Native modes** (cef, overlay, capture): Bevy owns the window. UI runs in webview, composited as texture or overlay.

**WASM modes** (electron, tauri): Desktop framework owns the window. Bevy compiles to WASM, renders via WebGL2 alongside Svelte.

## Known Issues

### Capture Mode (Unmaintained)

Fundamentally flawed approach:
- Event-driven capture misses hover/animation updates
- Continuous capture tanks frame rate
- GTK/WebKit single-threaded, can stall Bevy

### Tauri Mode (Unmaintained)

WebKitGTK 2.40+ has WebGL2 instability causing context loss. Not a Pentimento bug. Works in Chrome/Firefox.

## License

MIT
