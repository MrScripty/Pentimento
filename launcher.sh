#!/bin/bash
# Pentimento Launcher
# Builds and runs the Bevy + Svelte compositing application

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Parse arguments
DEV_MODE=false
BUILD_ONLY=false
RELEASE=false
COMPOSITE_MODE="capture"
CARGO_FEATURES=""

while [[ $# -gt 0 ]]; do
    case $1 in
        --dev)
            DEV_MODE=true
            shift
            ;;
        --build)
            BUILD_ONLY=true
            shift
            ;;
        --release)
            RELEASE=true
            shift
            ;;
        --capture)
            COMPOSITE_MODE="capture"
            shift
            ;;
        --overlay)
            COMPOSITE_MODE="overlay"
            shift
            ;;
        --cef)
            COMPOSITE_MODE="cef"
            CARGO_FEATURES="--features cef"
            # Note: cef-rs will download CEF 143 automatically during build
            # The setup-cef.sh script downloads CEF 144 which is incompatible
            # with the version check in cef-dll-sys build.rs
            shift
            ;;
        --help|-h)
            echo "Pentimento Launcher"
            echo ""
            echo "Usage: ./launcher.sh [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --dev       Run in development mode (uses Vite dev server for UI)"
            echo "  --build     Build only, don't run"
            echo "  --release   Build and run in release mode"
            echo "  --capture   Use capture compositing mode (default)"
            echo "  --overlay   Use overlay compositing mode (transparent window)"
            echo "  --cef       Use CEF (Chromium) compositing mode"
            echo "  --help, -h  Show this help message"
            echo ""
            echo "Compositing modes:"
            echo "  capture - Renders WebKitGTK offscreen, captures to texture (default)"
            echo "            Most compatible, works on all systems"
            echo "  overlay - Uses transparent GTK window overlay"
            echo "            Better performance, may have issues on some systems"
            echo "  cef     - Renders CEF (Chromium) offscreen, captures to texture"
            echo "            Downloads CEF binaries on first run (~150MB)"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

# Build UI if not in dev mode
if [ "$DEV_MODE" = false ]; then
    echo "Building Svelte UI..."
    if [ -d "ui" ]; then
        cd ui
        if [ ! -d "node_modules" ]; then
            echo "Installing npm dependencies..."
            npm install
        fi
        npm run build
        cd ..
    fi
fi

# Build Rust application
echo "Building Pentimento..."
if [ "$RELEASE" = true ]; then
    cargo build --release -p pentimento $CARGO_FEATURES
    BINARY="target/release/pentimento"
    BINARY_DIR="target/release"
else
    cargo build -p pentimento $CARGO_FEATURES
    BINARY="target/debug/pentimento"
    BINARY_DIR="target/debug"
fi

# For CEF mode, also build the helper binary
if [ "$COMPOSITE_MODE" = "cef" ]; then
    echo "Building CEF helper binary..."
    if [ "$RELEASE" = true ]; then
        cargo build --release -p pentimento-cef-helper
        HELPER_BINARY="target/release/pentimento-cef-helper"
    else
        cargo build -p pentimento-cef-helper
        HELPER_BINARY="target/debug/pentimento-cef-helper"
    fi

    # Verify helper was built
    if [ ! -f "$HELPER_BINARY" ]; then
        echo "Error: Failed to build CEF helper binary"
        exit 1
    fi
    echo "CEF helper binary built: $HELPER_BINARY"
fi

if [ "$BUILD_ONLY" = true ]; then
    echo "Build complete: $BINARY"
    exit 0
fi

# Run the application
echo "Launching Pentimento ($COMPOSITE_MODE mode)..."

# Set environment variables
export PENTIMENTO_COMPOSITE="$COMPOSITE_MODE"

# For CEF mode, find and set LD_LIBRARY_PATH to the CEF directory
if [ "$COMPOSITE_MODE" = "cef" ]; then
    # CEF libraries are downloaded by cef-dll-sys to target/*/build/cef-dll-sys-*/out/cef_linux_x86_64
    if [ "$RELEASE" = true ]; then
        BUILD_TYPE="release"
    else
        BUILD_TYPE="debug"
    fi

    CEF_LIB_DIR=$(find "target/$BUILD_TYPE/build" -type d -name "cef_linux_x86_64" 2>/dev/null | head -1)
    if [ -n "$CEF_LIB_DIR" ] && [ -f "$CEF_LIB_DIR/libcef.so" ]; then
        echo "Found CEF libraries at: $CEF_LIB_DIR"
        export LD_LIBRARY_PATH="$CEF_LIB_DIR${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
    else
        echo "Warning: Could not find CEF libraries. The application may fail to start."
        echo "Try running './launcher.sh --cef --build' first."
    fi
fi

if [ "$DEV_MODE" = true ]; then
    export PENTIMENTO_DEV=1
    echo "(Development mode - UI served from Vite dev server)"
    echo "Start Vite in another terminal: cd ui && npm run dev"
fi

exec "$BINARY" "$@"
