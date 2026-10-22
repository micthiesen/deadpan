#!/bin/sh
set -eu
export DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
export CARGO_PROFILE_DEV_OPT_LEVEL=2
export DEADPAN_REQUIRE_PHYSICAL_MAC=1
export DEADPAN_REQUIRE_SYNTHETIC_WORKER=1
scratch=/tmp/deadpan-mixed-hold-duration-next-2026-10-21
cargo fmt --all > "$scratch/final-format.log" 2>&1
python "$scratch/freeze.py" > "$scratch/freeze.log"
filter='test(semantic) or test(macros) or test(scoped) or test(hold_duration) or test(retime) or test(completion) or test(registry)'
cargo nextest list --locked -p deadpan-core -p deadpan-plan -p deadpan-audio -p deadpan-store -p deadpan-app -p deadpan-cli -E "$filter" --message-format json > "$scratch/final-list.json" 2> "$scratch/final-build.log"
cargo nextest run --locked -p deadpan-core -p deadpan-plan -p deadpan-audio -p deadpan-store -p deadpan-app -p deadpan-cli --no-fail-fast -E "$filter" > "$scratch/final-regression.log" 2>&1
cargo nextest run --locked -p deadpan-app --features ui-harness --no-fail-fast -E "$filter" > "$scratch/final-ui.log" 2>&1
sh "$scratch/quiet-tests.sh"
cargo xtask check --since bcbf8493 --no-test > "$scratch/final-lint.log" 2>&1
cargo clippy -p deadpan-app --features ui-harness --locked --all-targets -- -D warnings > "$scratch/final-ui-lint.log" 2>&1
cargo build -p deadpan-app --release --features ui-harness --locked --bin deadpan-app > "$scratch/final-release-app.log" 2>&1
