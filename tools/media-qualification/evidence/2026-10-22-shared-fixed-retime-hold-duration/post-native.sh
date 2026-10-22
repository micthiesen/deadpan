#!/bin/sh
set -eu
export DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
export CARGO_PROFILE_DEV_OPT_LEVEL=2
export DEADPAN_REQUIRE_PHYSICAL_MAC=1
export DEADPAN_REQUIRE_SYNTHETIC_WORKER=1
scratch=/tmp/deadpan-next-shared-fixed-retime-duration-2026-10-22
cargo fmt --all > "$scratch/final-format.log" 2>&1
python "$scratch/freeze.py" > "$scratch/final-freeze.log"
cargo xtask check --since ce1568ea7e5e52e259cd3af984087ef55fb2ba53 --no-test > "$scratch/final-lint.log" 2>&1
cargo clippy --locked -p deadpan-app --features ui-harness --all-targets -- -D warnings > "$scratch/final-ui-lint.log" 2>&1
cargo nextest run --locked -p deadpan-app --features ui-harness --no-fail-fast -E 'test(semantic) | test(macros) | test(scoped) | test(hold_duration) | test(retime) | test(completion) | test(registry) | test(owned_gap)' > "$scratch/final-ui.log" 2>&1
cargo build --locked -p deadpan-app --release --features ui-harness --bin deadpan-app > "$scratch/release-app.log" 2>&1
sh "$scratch/encoded.sh"
sh "$scratch/native.sh"
