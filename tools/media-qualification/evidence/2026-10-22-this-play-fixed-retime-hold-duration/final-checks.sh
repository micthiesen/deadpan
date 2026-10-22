#!/bin/sh
set -eu
export DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
export CARGO_PROFILE_DEV_OPT_LEVEL=2
export DEADPAN_REQUIRE_PHYSICAL_MAC=1
export DEADPAN_REQUIRE_SYNTHETIC_WORKER=1
scratch=/tmp/deadpan-this-play-fixed-retime-duration-2026-10-22
cargo fmt --all > "$scratch/final-format.log" 2>&1
python "$scratch/freeze.py" ui-final-runtime-source > "$scratch/ui-final-freeze.log"
cargo xtask check --since 5cd480c6e6a96db4922f6e4c366ea4eb81e7afd9 --no-test > "$scratch/lint.log" 2>&1
cargo clippy --locked -p deadpan-app --features ui-harness --all-targets -- -D warnings > "$scratch/ui-lint.log" 2>&1
filter='test(semantic) | test(macros) | test(scoped) | test(hold_duration) | test(retime) | test(completion) | test(registry) | test(owned_gap)'
cargo nextest run --locked -p deadpan-app --no-fail-fast -E "$filter" > "$scratch/app-final.log" 2>&1
cargo nextest run --locked -p deadpan-app --features ui-harness --no-fail-fast -E "$filter" > "$scratch/ui.log" 2>&1
cargo build --locked -p deadpan-app --features ui-harness --bin deadpan-app > "$scratch/dev-native-build.log" 2>&1
target/debug/deadpan-app --ui-check --scenario this-play-fixed-duration --mode visual --output "$scratch/debug-visual" --kestrel-source /Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift > "$scratch/debug-visual.log" 2>&1
