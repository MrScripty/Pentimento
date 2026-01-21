# Pentimento

A desktop application combining Bevy 0.18 3D rendering with a Svelte 5 UI in a single native window.

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
- `--capture` - Renders WebKitGTK offscreen and captures to texture (default)
  - Most compatible, works on all systems
  - Slightly higher overhead due to framebuffer capture
- `--overlay` - Uses transparent GTK window overlay
  - Better performance, compositor handles blending
  - Best on X11, may have positioning issues on Wayland
- `--cef` - Renders CEF (Chromium) offscreen and captures to texture
  - Downloads CEF binaries on first run (~150MB)
  - Chromium rendering engine instead of WebKitGTK

**Examples:**
```bash
# Run with default capture mode
./launcher.sh

# Run with overlay mode
./launcher.sh --overlay

# Run with CEF mode (downloads binaries on first run)
./launcher.sh --cef

# Release build with overlay mode
./launcher.sh --release --overlay

# Development mode (start Vite separately: cd ui && npm run dev)
./launcher.sh --dev
```

## Project Structure

```
Pentimento/
├── crates/
│   ├── app/         # Main Bevy application
│   ├── webview/     # Offscreen webview management
│   ├── ipc/         # Message protocol
│   └── diffusion/   # Texture streaming
├── ui/              # Svelte frontend
└── dist/ui/         # Built Svelte output
```

## Architecture

Bevy owns the native window and GPU rendering. Svelte runs in a webview. Three compositing modes are supported:

- **Capture mode**: WebKitGTK renders offscreen, framebuffer is captured and composited as a Bevy texture
- **Overlay mode**: Transparent GTK window overlays the Bevy window, desktop compositor handles blending
- **CEF mode**: Chromium (CEF) renders offscreen, framebuffer is captured and composited as a Bevy texture

## License

MIT
