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
        --help|-h)
            echo "Pentimento Launcher"
            echo ""
            echo "Usage: ./launcher.sh [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --dev       Run in development mode (uses Vite dev server for UI)"
            echo "  --build     Build only, don't run"
            echo "  --release   Build and run in release mode"
            echo "  --help, -h  Show this help message"
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
    cargo build --release -p pentimento
    BINARY="target/release/pentimento"
else
    cargo build -p pentimento
    BINARY="target/debug/pentimento"
fi

if [ "$BUILD_ONLY" = true ]; then
    echo "Build complete: $BINARY"
    exit 0
fi

# Run the application
echo "Launching Pentimento..."
if [ "$DEV_MODE" = true ]; then
    export PENTIMENTO_DEV=1
    echo "(Development mode - UI served from Vite dev server)"
    echo "Start Vite in another terminal: cd ui && npm run dev"
fi

exec "$BINARY" "$@"
