#!/usr/bin/env bash
# Engine/contract qualification. Native CEF rendering remains a separate gate.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
mode="${1:---all}"
case "$mode" in
    --all|--engine-only|--frontend-only) ;;
    *) echo "Usage: $0 [--all|--engine-only|--frontend-only]" >&2; exit 2 ;;
esac
if [[ "$mode" != --frontend-only ]]; then
    ./scripts/test-sculpt-geometry.sh
    scene_tests="$(cargo test --locked -p pentimento-scene --features sculpting,mesh_painting,mesh_editing,selection,wireframe,atmosphere --lib -- --list)"
    for suite in 'projection_painting::tests::' 'brush_ui::tests::' \
        'sculpt_geometry_sync_tests::exit_commits_dirty_geometry_and_reentry_preserves_brush_and_uv_corners: test'; do
        grep -Fq "$suite" <<< "$scene_tests"
    done
    cargo test --locked -p pentimento-scene --features sculpting,mesh_painting,mesh_editing,selection,wireframe,atmosphere --lib
    cargo check --locked -p pentimento-ipc --examples
fi
if [[ "$mode" != --engine-only ]]; then
    ./scripts/check-source-readmes.sh --all
    npm run verify
    npm run build
fi
