# The launcher owns all supported frontend workflows.
default: dev

setup:
    ./launcher.sh --install

dev frontend="cef":
    ./launcher.sh --run --frontend {{frontend}}

build-ui:
    npm run build

build-rust frontend="cef":
    ./launcher.sh --build-release --frontend {{frontend}}

build frontend="cef":
    ./launcher.sh --build-release --frontend {{frontend}}

run frontend="cef":
    ./launcher.sh --run-release --frontend {{frontend}}

test:
    ./launcher.sh --test

check-ui:
    npm run check

check-rust:
    cargo check --locked --workspace

check: check-rust check-ui

lint:
    cargo clippy --locked --workspace -- -D warnings

fmt:
    ./scripts/rustfmt-active.sh --write

clean:
    cargo clean
    rm -rf dist/
    rm -rf node_modules/

# Discontinued frontends require their explicit independent workspace.
legacy-build: build-ui
    cargo build --locked --manifest-path legacy/Cargo.toml -p pentimento-legacy --features webkit

legacy-capture: build-ui
    PENTIMENTO_COMPOSITE=capture cargo run --locked --manifest-path legacy/Cargo.toml -p pentimento-legacy --features webkit

legacy-overlay: build-ui
    PENTIMENTO_COMPOSITE=overlay cargo run --locked --manifest-path legacy/Cargo.toml -p pentimento-legacy --features webkit

legacy-check:
    cargo check --locked --manifest-path legacy/Cargo.toml --workspace
