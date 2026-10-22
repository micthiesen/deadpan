#!/bin/sh
set -eu
export DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
export CARGO_PROFILE_DEV_OPT_LEVEL=2
export DEADPAN_REQUIRE_PHYSICAL_MAC=1
export DEADPAN_REQUIRE_SYNTHETIC_WORKER=1
scratch=/tmp/deadpan-native-fixed-retime-hold-2026-10-21
cargo fmt --all -- --check > "$scratch/final-format.log" 2>&1
cargo nextest run -p deadpan-app --features ui-harness --locked --no-fail-fast -E 'test(macros) or test(scoped) or test(camera)' > "$scratch/final-ui-tests.log" 2>&1
cargo clippy -p deadpan-app --features ui-harness --locked --all-targets -- -D warnings > "$scratch/final-ui-lint.log" 2>&1
cargo build -p deadpan-app --release --features ui-harness --locked --bin deadpan-app > "$scratch/final-release-app.log" 2>&1
