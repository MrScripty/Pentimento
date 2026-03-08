#!/usr/bin/env bash

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="${1:---check}"
RUSTFMT_ARGS=(--edition 2024)

case "$MODE" in
    --check)
        RUSTFMT_ARGS+=(--check)
        ;;
    --write)
        ;;
    *)
        echo "Usage: ./scripts/rustfmt-active.sh [--check|--write]" >&2
        exit 2
        ;;
esac

ACTIVE_DIRS=(
    "${PROJECT_ROOT}/crates/app/src"
    "${PROJECT_ROOT}/crates/app-wasm/src"
    "${PROJECT_ROOT}/crates/cef-helper/src"
    "${PROJECT_ROOT}/crates/dioxus-ui/src"
    "${PROJECT_ROOT}/crates/ipc/src"
    "${PROJECT_ROOT}/crates/painting/src"
    "${PROJECT_ROOT}/crates/scene/src"
    "${PROJECT_ROOT}/crates/webview/src"
)

FILES=()
for dir in "${ACTIVE_DIRS[@]}"; do
    while IFS= read -r -d '' file; do
        FILES+=("$file")
    done < <(find "$dir" -type f -name '*.rs' -print0)
done

if [[ "${#FILES[@]}" -eq 0 ]]; then
    echo "No Rust files found in active frontend directories" >&2
    exit 1
fi

rustfmt "${RUSTFMT_ARGS[@]}" "${FILES[@]}"
