# Pentimento build automation

# Default recipe
default: dev

# Install all dependencies
setup:
    npm install
    rustup target add x86_64-pc-windows-gnu

# Development mode (UI hot reload)
dev:
    #!/usr/bin/env bash
    set -e
    # Start Vite dev server in background
    npm run dev &
    VITE_PID=$!
    trap "kill $VITE_PID 2>/dev/null" EXIT
    sleep 2
    # Run Bevy app in dev mode
    PENTIMENTO_DEV=1 cargo run -p pentimento

# Build UI only
build-ui:
    npm run build

# Build Rust only
build-rust:
    cargo build --release -p pentimento

# Build everything for release
build: build-ui build-rust

# Build for Windows (cross-compilation)
build-windows:
    npm run build
    cross build --release -p pentimento --target x86_64-pc-windows-gnu

# Run the release build
run:
    cargo run --release -p pentimento

# Run tests
test:
    cargo test --workspace

# Type check Svelte
check-ui:
    npm run check

# Check Rust
check-rust:
    cargo check --workspace

# Check everything
check: check-rust check-ui

# Lint Rust
lint:
    cargo clippy --workspace -- -D warnings

# Format Rust code
fmt:
    cargo fmt --all

# Clean build artifacts
clean:
    cargo clean
    rm -rf dist/
    rm -rf node_modules/

# Install cross for Windows builds
install-cross:
    cargo install cross --git https://github.com/cross-rs/cross
