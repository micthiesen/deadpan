#!/bin/sh
set -eu
export DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
export CARGO_PROFILE_DEV_OPT_LEVEL=2
export DEADPAN_REQUIRE_PHYSICAL_MAC=1
export DEADPAN_REQUIRE_SYNTHETIC_WORKER=1
scratch=/tmp/deadpan-native-fixed-retime-hold-2026-10-21

cargo nextest run --locked -p deadpan-app --bin deadpan-app -E 'test(commands_reference_is_current)' > "$scratch/commands-reference.log" 2>&1
filter='test(semantic) or test(macros) or test(scoped) or test(hold_duration) or test(retime) or test(completion) or test(registry)'
cargo nextest list --locked -p deadpan-core -p deadpan-app -p deadpan-cli -p deadpan-store -p deadpan-audio -E "$filter" --message-format json > "$scratch/normal-list.json" 2> "$scratch/normal-build.log"
cargo nextest run --locked -p deadpan-core -p deadpan-app -p deadpan-cli -p deadpan-store -p deadpan-audio --no-fail-fast -E "$filter" > "$scratch/normal.log" 2>&1
cargo nextest run --locked -p deadpan-app --features ui-harness --no-fail-fast -E "$filter" > "$scratch/ui.log" 2>&1
cargo xtask check --since ed33550d --no-test > "$scratch/lint.log" 2>&1
cargo clippy -p deadpan-app --features ui-harness --locked --all-targets -- -D warnings > "$scratch/ui-lint.log" 2>&1
cargo build -p deadpan-app --release --features ui-harness --locked --bin deadpan-app > "$scratch/release-app.log" 2>&1
