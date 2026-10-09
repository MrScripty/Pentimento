#!/usr/bin/env bash

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="${1:---check}"
RUSTFMT_ARGS=()

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
    "${PROJECT_ROOT}/crates/egui-ui/src"
    "${PROJECT_ROOT}/crates/frontend-core/src"
    "${PROJECT_ROOT}/crates/ipc/src"
    "${PROJECT_ROOT}/crates/painting/src"
    "${PROJECT_ROOT}/crates/scene/src"
    "${PROJECT_ROOT}/crates/webview/src"
)

# Cargo resolves both explicit editions and edition.workspace inheritance.
# Keep checking every source file in ACTIVE_DIRS, including unreferenced files.
METADATA="$(cargo metadata --manifest-path "${PROJECT_ROOT}/Cargo.toml" --no-deps --format-version 1)"
EDITIONS="$(printf '%s' "$METADATA" | node --input-type=module -e '
    import { readFileSync } from "node:fs";
    const metadata = JSON.parse(readFileSync(0, "utf8"));
    for (const pkg of metadata.packages) {
        console.log(`${pkg.manifest_path}\t${pkg.edition}`);
    }
')"
declare -A CRATE_EDITIONS
while IFS=$'\t' read -r manifest edition; do
    CRATE_EDITIONS["$manifest"]="$edition"
done <<< "$EDITIONS"

FILE_COUNT=0
for dir in "${ACTIVE_DIRS[@]}"; do
    manifest="${dir%/src}/Cargo.toml"
    edition="${CRATE_EDITIONS[$manifest]:?Missing Cargo edition for $manifest}"
    FILES=()
    while IFS= read -r -d '' file; do
        FILES+=("$file")
    done < <(find "$dir" -type f -name '*.rs' -print0)
    if [[ "${#FILES[@]}" -gt 0 ]]; then
        rustfmt --edition "$edition" "${RUSTFMT_ARGS[@]}" "${FILES[@]}"
        FILE_COUNT=$((FILE_COUNT + ${#FILES[@]}))
    fi
done

if [[ "$FILE_COUNT" -eq 0 ]]; then
    echo "No Rust files found in active frontend directories" >&2
    exit 1
fi
