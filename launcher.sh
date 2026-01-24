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
        --tauri)
            COMPOSITE_MODE="tauri"
            shift
            ;;
        --electron)
            COMPOSITE_MODE="electron"
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
            echo "  --tauri     Use Tauri mode (Bevy WASM in webview) - UNMAINTAINED"
            echo "  --electron  Use Electron mode (Bevy WASM in Chromium)"
            echo "  --help, -h  Show this help message"
            echo ""
            echo "Compositing modes:"
            echo "  capture - Renders WebKitGTK offscreen, captures to texture (default)"
            echo "            Most compatible, works on all systems"
            echo "  overlay - Uses transparent GTK window overlay"
            echo "            Better performance, may have issues on some systems"
            echo "  cef     - Renders CEF (Chromium) offscreen, captures to texture"
            echo "            Downloads CEF binaries on first run (~150MB)"
            echo "  tauri   - Tauri owns window, Bevy runs as WASM (UNMAINTAINED - WebKitGTK bug)"
            echo "  electron - Electron owns window, Bevy runs as WASM, uses Chromium"
            echo "             Stable WebGL2 rendering via Chromium"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

# Tauri mode has a completely different build/run process
if [ "$COMPOSITE_MODE" = "tauri" ]; then
    echo "Building Pentimento in Tauri mode..."

    # Build Svelte UI
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

    # Build Bevy WASM
    echo "Building Bevy WASM..."
    if [ "$RELEASE" = true ]; then
        cargo build --target wasm32-unknown-unknown --release -p pentimento-wasm
        WASM_FILE="target/wasm32-unknown-unknown/release/pentimento_wasm.wasm"
    else
        cargo build --target wasm32-unknown-unknown -p pentimento-wasm
        WASM_FILE="target/wasm32-unknown-unknown/debug/pentimento_wasm.wasm"
    fi

    # Run wasm-bindgen
    echo "Running wasm-bindgen..."
    mkdir -p dist/wasm
    wasm-bindgen "$WASM_FILE" --target web --out-dir dist/wasm --out-name pentimento_wasm

    # Copy WASM to where Tauri can serve it (production build)
    mkdir -p dist/ui/wasm
    cp dist/wasm/* dist/ui/wasm/

    # Also copy to Vite public dir for dev server
    mkdir -p dist/wasm-public/wasm
    cp dist/wasm/* dist/wasm-public/wasm/

    if [ "$BUILD_ONLY" = true ]; then
        echo "Build complete (Tauri mode)"
        echo "WASM output: dist/wasm/"
        echo "UI output: dist/ui/"
        exit 0
    fi

    # Run Tauri app
    echo "Launching Pentimento (Tauri mode)..."
    if [ "$RELEASE" = true ]; then
        cargo tauri build
        # Find and run the built binary
        TAURI_BINARY=$(find target/release/bundle -name "pentimento*" -type f -executable 2>/dev/null | head -1)
        if [ -n "$TAURI_BINARY" ]; then
            exec "$TAURI_BINARY"
        else
            echo "Built Tauri bundle is in target/release/bundle/"
        fi
    else
        # Kill any existing Vite processes on port 5173
        fuser -k 5173/tcp 2>/dev/null || true

        # Tauri will start Vite via beforeDevCommand
        cargo tauri dev
    fi
    exit 0
fi

# Electron mode has a completely different build/run process
if [ "$COMPOSITE_MODE" = "electron" ]; then
    echo "Building Pentimento in Electron mode..."

    # Build Svelte UI
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

    # Build Bevy WASM (with selection feature for Electron/Chromium)
    echo "Building Bevy WASM..."
    if [ "$RELEASE" = true ]; then
        cargo build --target wasm32-unknown-unknown --release -p pentimento-wasm --features selection
        WASM_FILE="target/wasm32-unknown-unknown/release/pentimento_wasm.wasm"
    else
        cargo build --target wasm32-unknown-unknown -p pentimento-wasm --features selection
        WASM_FILE="target/wasm32-unknown-unknown/debug/pentimento_wasm.wasm"
    fi

    # Run wasm-bindgen
    echo "Running wasm-bindgen..."
    mkdir -p dist/wasm
    wasm-bindgen "$WASM_FILE" --target web --out-dir dist/wasm --out-name pentimento_wasm

    # Copy WASM to UI directory
    mkdir -p dist/ui/wasm
    cp dist/wasm/* dist/ui/wasm/

    # Also copy to Vite public dir for dev server
    mkdir -p ui/public/wasm
    cp dist/wasm/* ui/public/wasm/

    # Setup Electron (install dependencies and download binary if needed)
    echo "Setting up Electron..."
    cd src-electron

    # Check if Electron binary exists
    ELECTRON_BIN="node_modules/electron/dist/electron"
    if [ ! -f "$ELECTRON_BIN" ]; then
        echo "Electron not found. Installing dependencies and downloading Electron binary..."
        rm -rf node_modules package-lock.json
        npm install

        # Verify Electron was downloaded
        if [ ! -f "$ELECTRON_BIN" ]; then
            echo "Error: Failed to download Electron binary"
            exit 1
        fi
        echo "Electron binary downloaded successfully"
    fi
    cd ..

    if [ "$BUILD_ONLY" = true ]; then
        echo "Build complete (Electron mode)"
        echo "WASM output: dist/wasm/"
        echo "UI output: dist/ui/"
        echo "Electron: src-electron/"
        exit 0
    fi

    # Run Electron app using the bundled binary
    echo "Launching Pentimento (Electron mode)..."
    # Unset ELECTRON_RUN_AS_NODE to ensure Electron runs as Electron, not Node.js
    unset ELECTRON_RUN_AS_NODE
    cd src-electron
    if [ "$DEV_MODE" = true ]; then
        # Dev mode: Start Vite server, then Electron
        cd ../ui
        npm run dev &
        VITE_PID=$!
        sleep 2
        cd ../src-electron
        VITE_DEV_SERVER_URL="http://localhost:5173" ./node_modules/electron/dist/electron .
        kill $VITE_PID 2>/dev/null || true
    else
        ./node_modules/electron/dist/electron .
    fi
    exit 0
fi

# Build UI if not in dev mode (native modes)
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

# Build Rust application (native modes)
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
