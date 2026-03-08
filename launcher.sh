#!/usr/bin/env bash

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP_BIN="pentimento"
CEF_HELPER_BIN="pentimento-cef-helper"
DEFAULT_FRONTEND="cef"
ACTION=""
FRONTEND="$DEFAULT_FRONTEND"
RUN_ARGS=()

LAUNCHER_STATE_ROOT="${PENTIMENTO_LAUNCHER_STATE_ROOT:-${PROJECT_ROOT}/.launcher-state}"
PENTIMENTO_LAUNCHER_ISOLATE_STATE="${PENTIMENTO_LAUNCHER_ISOLATE_STATE:-1}"

usage() {
    cat <<'EOF'
Pentimento launcher for active frontend workflows.

Usage:
  ./launcher.sh --help
  ./launcher.sh --install
  ./launcher.sh --build [--frontend <cef|dioxus|electron>]
  ./launcher.sh --build-release [--frontend <cef|dioxus|electron>]
  ./launcher.sh --run [--frontend <cef|dioxus|electron>] [-- <args>]
  ./launcher.sh --run-release [--frontend <cef|dioxus|electron>] [-- <args>]
  ./launcher.sh --test
  ./launcher.sh --release-smoke [--frontend <cef|dioxus|electron>]

Actions:
  --install        Install repo-managed dependencies without touching lockfiles
  --build          Build development artifacts for the selected frontend
  --build-release  Build optimized release artifacts for the selected frontend
  --run            Build and run the selected frontend in development mode
  --run-release    Run the existing release artifact for the selected frontend
  --test           Run the canonical verification suite for active frontends
  --release-smoke  Launch the release artifact briefly and fail on unhealthy startup
  --help           Show this help text and exit

Options:
  --frontend <name>  Select the active frontend: cef, dioxus, or electron
                     Default: cef

Managed state:
  Dev/test/smoke flows default to isolated repo-local state under:
    .launcher-state/
  Override with:
    PENTIMENTO_LAUNCHER_STATE_ROOT=/custom/path
    PENTIMENTO_LAUNCHER_ISOLATE_STATE=0

Examples:
  ./launcher.sh --install
  ./launcher.sh --build --frontend cef
  ./launcher.sh --build-release --frontend electron
  ./launcher.sh --run --frontend dioxus
  ./launcher.sh --run-release --frontend cef -- --scene docs/example.scene
  ./launcher.sh --test
  ./launcher.sh --release-smoke --frontend electron

Exit codes:
  0   Success
  1   Operation failed
  2   Usage error
  3   Missing dependency during runtime preflight
  4   Missing release artifact for --run-release
  130 Interrupted
EOF
}

fail_usage() {
    local message="$1"
    echo "[error] ${message}" >&2
    usage >&2
    exit 2
}

set_action() {
    local candidate="$1"
    if [[ -n "$ACTION" ]]; then
        fail_usage "exactly one action flag must be selected"
    fi
    ACTION="$candidate"
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --help)
            set_action "help"
            shift
            ;;
        --install)
            set_action "install"
            shift
            ;;
        --build)
            set_action "build"
            shift
            ;;
        --build-release)
            set_action "build-release"
            shift
            ;;
        --run)
            set_action "run"
            shift
            ;;
        --run-release)
            set_action "run-release"
            shift
            ;;
        --test)
            set_action "test"
            shift
            ;;
        --release-smoke)
            set_action "release-smoke"
            shift
            ;;
        --frontend)
            [[ $# -ge 2 ]] || fail_usage "--frontend requires a value"
            FRONTEND="$2"
            shift 2
            ;;
        --)
            shift
            if [[ "$ACTION" != "run" && "$ACTION" != "run-release" ]]; then
                fail_usage "-- may only be used with --run or --run-release"
            fi
            RUN_ARGS=("$@")
            break
            ;;
        *)
            fail_usage "unknown flag: $1"
            ;;
    esac
done

if [[ -z "$ACTION" ]]; then
    fail_usage "exactly one action flag must be selected"
fi

case "$FRONTEND" in
    cef|dioxus|electron)
        ;;
    *)
        fail_usage "--frontend must be one of: cef, dioxus, electron"
        ;;
esac

trap 'exit 130' INT

check_cargo() {
    command -v cargo >/dev/null 2>&1
}

install_cargo() {
    echo "Install Rust and Cargo via https://rustup.rs" >&2
    return 1
}

check_npm() {
    command -v npm >/dev/null 2>&1
}

install_npm() {
    echo "Install Node.js 22+ and npm from https://nodejs.org/" >&2
    return 1
}

check_rustup() {
    command -v rustup >/dev/null 2>&1
}

install_rustup() {
    echo "Install rustup via https://rustup.rs" >&2
    return 1
}

check_root_node_modules() {
    [[ -d "${PROJECT_ROOT}/node_modules" ]]
}

install_root_node_modules() {
    (
        cd "$PROJECT_ROOT"
        npm ci
    )
}

check_electron_node_modules() {
    [[ -x "${PROJECT_ROOT}/src-electron/node_modules/.bin/electron" ]]
}

install_electron_node_modules() {
    (
        cd "${PROJECT_ROOT}/src-electron"
        npm ci
    )
}

check_wasm_target() {
    rustup target list --installed | grep -qx 'wasm32-unknown-unknown'
}

install_wasm_target() {
    rustup target add wasm32-unknown-unknown
}

check_wasm_bindgen() {
    command -v wasm-bindgen >/dev/null 2>&1
}

install_wasm_bindgen() {
    cargo install wasm-bindgen-cli
}

check_timeout() {
    command -v timeout >/dev/null 2>&1
}

install_timeout() {
    echo "Install GNU coreutils so the timeout command is available." >&2
    return 1
}

ensure_dependency() {
    local name="$1"
    local check_fn="$2"
    local install_fn="$3"

    if "$check_fn"; then
        echo "[ok] ${name} already satisfied"
        return 0
    fi

    echo "[install] ${name} missing; installing"
    if ! "$install_fn"; then
        echo "[error] ${name} install failed"
        return 1
    fi

    if "$check_fn"; then
        echo "[done] ${name} installed"
        return 0
    fi

    echo "[error] ${name} install failed"
    return 1
}

require_dependency() {
    local exit_code="$1"
    local name="$2"
    local check_fn="$3"
    local help_text="$4"

    if "$check_fn"; then
        return 0
    fi

    echo "[error] Missing dependency: ${name}. ${help_text}" >&2
    exit "$exit_code"
}

setup_managed_state_env() {
    local scope="$1"

    if [[ "$PENTIMENTO_LAUNCHER_ISOLATE_STATE" != "1" ]]; then
        echo "[state] using host state"
        return 0
    fi

    local state_dir="${LAUNCHER_STATE_ROOT}/${scope}"
    mkdir -p "${state_dir}/xdg-state" "${state_dir}/xdg-data" "${state_dir}/xdg-cache"
    export XDG_STATE_HOME="${state_dir}/xdg-state"
    export XDG_DATA_HOME="${state_dir}/xdg-data"
    export XDG_CACHE_HOME="${state_dir}/xdg-cache"
    echo "[state] using isolated state at ${state_dir}"
}

native_feature_args() {
    case "$FRONTEND" in
        cef)
            printf '%s\n' "--features" "cef"
            ;;
        dioxus)
            printf '%s\n' "--features" "dioxus"
            ;;
        *)
            return 1
            ;;
    esac
}

build_ui() {
    require_dependency 1 "repo npm dependencies" check_root_node_modules "Run ./launcher.sh --install."
    (
        cd "$PROJECT_ROOT"
        npm run build
    )
}

build_electron_shell() {
    require_dependency 1 "Electron npm dependencies" check_electron_node_modules "Run ./launcher.sh --install."
    (
        cd "${PROJECT_ROOT}/src-electron"
        npm run build
    )
}

build_wasm_bundle() {
    local profile="$1"
    local cargo_args=()
    if [[ "$profile" == "release" ]]; then
        cargo_args+=(--release)
    fi

    require_dependency 1 "rustup" check_rustup "Install rustup, then rerun ./launcher.sh --install."
    require_dependency 1 "wasm32 target" check_wasm_target "Run ./launcher.sh --install."
    require_dependency 1 "wasm-bindgen" check_wasm_bindgen "Run ./launcher.sh --install."

    (
        cd "$PROJECT_ROOT"
        cargo build --target wasm32-unknown-unknown "${cargo_args[@]}" -p pentimento-wasm --features selection
    )

    local wasm_file="${PROJECT_ROOT}/target/wasm32-unknown-unknown/${profile}/pentimento_wasm.wasm"
    mkdir -p "${PROJECT_ROOT}/dist/wasm" "${PROJECT_ROOT}/dist/wasm-public/wasm"
    wasm-bindgen "$wasm_file" \
        --target web \
        --out-dir "${PROJECT_ROOT}/dist/wasm" \
        --out-name pentimento_wasm
    cp "${PROJECT_ROOT}"/dist/wasm/* "${PROJECT_ROOT}/dist/wasm-public/wasm/"
}

publish_wasm_assets() {
    mkdir -p "${PROJECT_ROOT}/dist/ui/wasm"
    cp "${PROJECT_ROOT}"/dist/wasm/* "${PROJECT_ROOT}/dist/ui/wasm/"
}

build_native_app() {
    local profile="$1"
    local cargo_args=()
    local feature_args=()

    if [[ "$profile" == "release" ]]; then
        cargo_args+=(--release)
    fi

    mapfile -t feature_args < <(native_feature_args)

    (
        cd "$PROJECT_ROOT"
        cargo build "${cargo_args[@]}" -p "$APP_BIN" "${feature_args[@]}"
    )

    if [[ "$FRONTEND" == "cef" ]]; then
        (
            cd "$PROJECT_ROOT"
            cargo build "${cargo_args[@]}" -p "$CEF_HELPER_BIN"
        )
    fi
}

build_frontend() {
    local profile="$1"

    case "$FRONTEND" in
        cef|dioxus)
            build_ui
            build_native_app "$profile"
            ;;
        electron)
            build_wasm_bundle "$profile"
            build_ui
            publish_wasm_assets
            build_electron_shell
            ;;
    esac
}

setup_cef_runtime_env() {
    local profile="$1"
    local cef_lib_dir
    cef_lib_dir="$(find "${PROJECT_ROOT}/target/${profile}/build" -type d -name "cef_linux_x86_64" 2>/dev/null | head -n 1 || true)"

    if [[ -z "$cef_lib_dir" || ! -f "${cef_lib_dir}/libcef.so" ]]; then
        echo "[error] Missing CEF runtime libraries. Build the CEF frontend first with ./launcher.sh --build --frontend cef." >&2
        exit 3
    fi

    export LD_LIBRARY_PATH="${cef_lib_dir}${LD_LIBRARY_PATH:+:${LD_LIBRARY_PATH}}"
}

native_binary_path() {
    local profile="$1"
    printf '%s/target/%s/%s\n' "$PROJECT_ROOT" "$profile" "$APP_BIN"
}

electron_binary_path() {
    printf '%s/src-electron/node_modules/.bin/electron\n' "$PROJECT_ROOT"
}

require_release_artifacts() {
    case "$FRONTEND" in
        cef|dioxus)
            local native_binary
            native_binary="$(native_binary_path "release")"
            if [[ ! -x "$native_binary" ]]; then
                echo "[error] Release artifact missing: ${native_binary}" >&2
                echo "Run ./launcher.sh --build-release --frontend ${FRONTEND}" >&2
                exit 4
            fi
            ;;
        electron)
            local electron_bin
            electron_bin="$(electron_binary_path)"
            if [[ ! -x "$electron_bin" ]]; then
                echo "[error] Electron runtime missing. Run ./launcher.sh --install." >&2
                exit 4
            fi
            if [[ ! -f "${PROJECT_ROOT}/src-electron/dist/main.js" ]]; then
                echo "[error] Electron release shell missing: src-electron/dist/main.js" >&2
                echo "Run ./launcher.sh --build-release --frontend electron" >&2
                exit 4
            fi
            if [[ ! -f "${PROJECT_ROOT}/dist/ui/index.html" ]]; then
                echo "[error] UI release artifact missing: dist/ui/index.html" >&2
                echo "Run ./launcher.sh --build-release --frontend electron" >&2
                exit 4
            fi
            if [[ ! -f "${PROJECT_ROOT}/dist/ui/wasm/pentimento_wasm_bg.wasm" ]]; then
                echo "[error] WASM release artifact missing: dist/ui/wasm/pentimento_wasm_bg.wasm" >&2
                echo "Run ./launcher.sh --build-release --frontend electron" >&2
                exit 4
            fi
            ;;
    esac
}

run_native_binary() {
    local profile="$1"
    local binary
    binary="$(native_binary_path "$profile")"

    export PENTIMENTO_COMPOSITE="$FRONTEND"

    if [[ "$FRONTEND" == "cef" ]]; then
        setup_cef_runtime_env "$profile"
    fi

    exec "$binary" "${RUN_ARGS[@]}"
}

run_electron_app() {
    local electron_bin
    electron_bin="$(electron_binary_path)"
    unset ELECTRON_RUN_AS_NODE

    cd "${PROJECT_ROOT}/src-electron"
    exec "$electron_bin" . "${RUN_ARGS[@]}"
}

run_verification_suite() {
    require_dependency 1 "cargo" check_cargo "Install Rust and Cargo first."
    require_dependency 1 "npm" check_npm "Install Node.js 22+ and npm first."
    require_dependency 1 "repo npm dependencies" check_root_node_modules "Run ./launcher.sh --install."
    require_dependency 1 "Electron npm dependencies" check_electron_node_modules "Run ./launcher.sh --install."
    require_dependency 1 "wasm32 target" check_wasm_target "Run ./launcher.sh --install."
    require_dependency 1 "wasm-bindgen" check_wasm_bindgen "Run ./launcher.sh --install."

    (
        cd "$PROJECT_ROOT"
        ./scripts/rustfmt-active.sh --check
        npm run typecheck
        cargo test -p pentimento-ipc
        cargo check -p pentimento --features dioxus
        cargo check -p pentimento --features cef
        cargo check --target wasm32-unknown-unknown -p pentimento-wasm
    )
}

run_release_smoke() {
    local status

    build_frontend "release"
    setup_managed_state_env "release-smoke-${FRONTEND}"

    case "$FRONTEND" in
        cef|dioxus)
            export PENTIMENTO_COMPOSITE="$FRONTEND"
            if [[ "$FRONTEND" == "cef" ]]; then
                setup_cef_runtime_env "release"
            fi
            timeout -k 5s 15s "$(native_binary_path "release")" >/dev/null 2>&1 || status=$?
            ;;
        electron)
            unset ELECTRON_RUN_AS_NODE
            (
                cd "${PROJECT_ROOT}/src-electron"
                timeout -k 5s 15s "$(electron_binary_path)" . >/dev/null 2>&1
            ) || status=$?
            ;;
    esac

    status="${status:-0}"
    if [[ "$status" -eq 124 ]]; then
        echo "[ok] release smoke for ${FRONTEND} remained healthy for 15s"
        return 0
    fi

    echo "[error] release smoke for ${FRONTEND} exited with status ${status}" >&2
    return 1
}

case "$ACTION" in
    help)
        usage
        ;;
    install)
        ensure_dependency "cargo" check_cargo install_cargo
        ensure_dependency "npm" check_npm install_npm
        ensure_dependency "rustup" check_rustup install_rustup
        ensure_dependency "repo npm dependencies" check_root_node_modules install_root_node_modules
        ensure_dependency "Electron npm dependencies" check_electron_node_modules install_electron_node_modules
        ensure_dependency "wasm32 target" check_wasm_target install_wasm_target
        ensure_dependency "wasm-bindgen" check_wasm_bindgen install_wasm_bindgen
        ;;
    build)
        require_dependency 1 "cargo" check_cargo "Install Rust and Cargo first."
        require_dependency 1 "npm" check_npm "Install Node.js 22+ and npm first."
        build_frontend "debug"
        ;;
    build-release)
        require_dependency 1 "cargo" check_cargo "Install Rust and Cargo first."
        require_dependency 1 "npm" check_npm "Install Node.js 22+ and npm first."
        build_frontend "release"
        ;;
    run)
        require_dependency 3 "cargo" check_cargo "Install Rust and Cargo first."
        require_dependency 3 "npm" check_npm "Install Node.js 22+ and npm first."
        build_frontend "debug"
        setup_managed_state_env "run-${FRONTEND}"
        case "$FRONTEND" in
            cef|dioxus)
                run_native_binary "debug"
                ;;
            electron)
                run_electron_app
                ;;
        esac
        ;;
    run-release)
        require_dependency 3 "cargo" check_cargo "Install Rust and Cargo first."
        require_dependency 3 "npm" check_npm "Install Node.js 22+ and npm first."
        require_release_artifacts
        setup_managed_state_env "run-release-${FRONTEND}"
        case "$FRONTEND" in
            cef|dioxus)
                run_native_binary "release"
                ;;
            electron)
                run_electron_app
                ;;
        esac
        ;;
    test)
        setup_managed_state_env "test"
        run_verification_suite
        ;;
    release-smoke)
        require_dependency 1 "timeout" check_timeout "Install GNU coreutils."
        run_release_smoke
        ;;
esac
