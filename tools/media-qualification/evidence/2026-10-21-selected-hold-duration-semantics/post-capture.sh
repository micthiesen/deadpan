#!/bin/sh
set -eu
export DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
export CARGO_PROFILE_DEV_OPT_LEVEL=2
export DEADPAN_REQUIRE_PHYSICAL_MAC=1
export DEADPAN_REQUIRE_SYNTHETIC_WORKER=1
scratch=/tmp/deadpan-selected-hold-duration-next-2026-10-21
cargo fmt --all > "$scratch/post-capture-format.log" 2>&1
python "$scratch/freeze.py" final-source > "$scratch/post-capture-freeze.log"
filter='test(semantic) | test(macros) | test(scoped) | test(hold_duration) | test(retime) | test(completion) | test(registry)'
cargo nextest run --locked -p deadpan-core -p deadpan-plan -p deadpan-audio -p deadpan-store -p deadpan-app -p deadpan-cli --no-fail-fast -E "package(deadpan-app) & ($filter)" > "$scratch/post-capture-app.log" 2>&1
cargo nextest run --locked -p deadpan-app --features ui-harness --no-fail-fast -E "$filter" > "$scratch/post-capture-ui.log" 2>&1
cargo xtask check --since f384a858 --no-test > "$scratch/post-capture-lint.log" 2>&1
cargo clippy --locked -p deadpan-app --features ui-harness --all-targets -- -D warnings > "$scratch/post-capture-ui-lint.log" 2>&1
cargo build --locked -p deadpan-app --release --features ui-harness --bin deadpan-app > "$scratch/post-capture-release-app.log" 2>&1
