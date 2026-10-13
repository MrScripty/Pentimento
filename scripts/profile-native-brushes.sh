#!/usr/bin/env bash
# Repeatable CPU microbenchmarks; no display/GPU/stylus qualification.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
mode="${1:---all}"
case "$mode" in
    --all|--painting|--sculpting|--scene) ;;
    *) echo "Usage: $0 [--all|--painting|--sculpting|--scene]" >&2; exit 2 ;;
esac
# Keep benchmark builds practical and make the exact profile explicit. Override
# these to measure the shipping LTO profile separately; don't mix comparisons.
export CARGO_PROFILE_RELEASE_LTO="${CARGO_PROFILE_RELEASE_LTO:-false}"
export CARGO_PROFILE_RELEASE_CODEGEN_UNITS="${CARGO_PROFILE_RELEASE_CODEGEN_UNITS:-16}"
echo "Profile: release; lto=$CARGO_PROFILE_RELEASE_LTO; codegen_units=$CARGO_PROFILE_RELEASE_CODEGEN_UNITS"
rustc --version
if [[ "$mode" == --all || "$mode" == --painting ]]; then
    cargo run --release --locked -p painting --example core_profile
    cargo run --release --locked -p painting --example uv_region_profile
fi
if [[ "$mode" == --all || "$mode" == --sculpting ]]; then
    cargo run --release --locked -p sculpting --features bevy --example core_profile
fi
if [[ "$mode" == --all || "$mode" == --scene ]]; then
    features=sculpting,mesh_painting,mesh_editing,selection,wireframe,atmosphere
    listing="$(cargo test --release --locked -p pentimento-scene --features "$features" --lib -- --list)"
    for test in \
        sculpt_picking::tests::benchmark_sculpt_picking \
        mesh_painting_system::incremental_upload_tests::benchmark_layered_uv_upload; do
        grep -Fxq "$test: test" <<< "$listing"
        cargo test --release --locked -p pentimento-scene --features "$features" --lib "$test" -- --exact --ignored --nocapture
    done
fi
