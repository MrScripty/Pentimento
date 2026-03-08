#!/usr/bin/env bash

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="${1:---staged}"

ACTIVE_ROOTS=(
    "ui/src"
    "crates/app-wasm/src"
    "crates/app/src/render/ui_dioxus"
    "crates/dioxus-ui/src"
    "crates/ipc/src"
    "crates/webview/src"
    "src-electron"
)

list_dirs() {
    local root="$1"
    find "${PROJECT_ROOT}/${root}" \
        \( -path '*/node_modules' -o -path '*/dist' \) -prune -o \
        -type d -print
}

ROOTS_TO_CHECK=()
case "$MODE" in
    --all)
        ROOTS_TO_CHECK=("${ACTIVE_ROOTS[@]}")
        ;;
    --staged)
        while IFS= read -r path; do
            for root in "${ACTIVE_ROOTS[@]}"; do
                if [[ "$path" == "$root" || "$path" == "$root/"* ]]; then
                    ROOTS_TO_CHECK+=("$root")
                fi
            done
        done < <(git -C "$PROJECT_ROOT" diff --cached --name-only --diff-filter=ACMR)
        ;;
    *)
        echo "Usage: ./scripts/check-source-readmes.sh [--staged|--all]" >&2
        exit 2
        ;;
esac

if [[ "${#ROOTS_TO_CHECK[@]}" -eq 0 ]]; then
    exit 0
fi

mapfile -t UNIQUE_ROOTS < <(printf '%s\n' "${ROOTS_TO_CHECK[@]}" | sort -u)

MISSING=()
for root in "${UNIQUE_ROOTS[@]}"; do
    while IFS= read -r dir; do
        if [[ ! -f "${dir}/README.md" ]]; then
            MISSING+=("${dir#${PROJECT_ROOT}/}")
        fi
    done < <(list_dirs "$root")
done

if [[ "${#MISSING[@]}" -gt 0 ]]; then
    printf 'Missing README.md in required source directories:\n' >&2
    printf '  %s\n' "${MISSING[@]}" >&2
    exit 1
fi
