# Pentimento

A desktop application combining Bevy 3D rendering with a Svelte 5 UI in a single native window.

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

Bevy owns the native window and GPU rendering. Svelte runs in an offscreen webview (via wry/WebKitGTK). The UI framebuffer is captured on-demand and composited as a texture over the 3D scene.

## License

MIT
