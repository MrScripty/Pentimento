#!/usr/bin/env bash
# Provision the exact CEF distribution selected by Cargo.lock and build its helper.
# cef-dll-sys owns download integrity, layout and copying resources next to binaries.
set -euo pipefail
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"
profile=debug
args=(--locked -p pentimento-cef-helper)
case "${1:-}" in
    '') ;;
    --release) profile=release; args+=(--release) ;;
    *) echo 'Usage: ./scripts/setup-cef.sh [--release]' >&2; exit 2 ;;
esac
cargo build "${args[@]}"
target_dir="$(cargo metadata --locked --no-deps --format-version 1 | node -e 'let s="";process.stdin.on("data",c=>s+=c).on("end",()=>console.log(JSON.parse(s).target_directory))')"
node scripts/cef-runtime.mjs "${target_dir}/${profile}" "${PROJECT_ROOT}/Cargo.lock"
echo 'CEF runtime and helper are ready. Use ./launcher.sh --run --frontend cef.'
