#!/usr/bin/env bash
# CPU geometry and render-asset regressions. No display or GPU is required.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
cargo test -p painting --features bevy --lib
cargo test -p sculpting --features bevy --lib --tests
# Explicit discovery guards prevent feature/filter changes from silently running zero tests.
geometry_tests="$(cargo test -p sculpting --features bevy --test geometry_regressions -- --list)"
for test in \
    partition_and_merge_preserve_uv_sphere_corners \
    collapse_prediction_checks_destination_only_faces_and_zero_area \
    repeated_split_collapse_compaction_keeps_a_closed_surface \
    real_sculpt_pipeline_deforms_tessellates_and_exports_without_seam_loss; do
    grep -Fxq "${test}: test" <<< "$geometry_tests"
done
scene_tests="$(cargo test -p pentimento-scene --features sculpting --lib sculpt_geometry_sync_tests -- --list)"
grep -Fq 'sculpt_geometry_sync_tests::gpu_sync_keeps_uv_corners_and_patches_every_render_copy: test' <<< "$scene_tests"
cargo test -p pentimento-scene --features sculpting --lib sculpt_geometry_sync_tests
