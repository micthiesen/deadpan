#!/bin/sh
set -eu
export DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
export CARGO_PROFILE_DEV_OPT_LEVEL=2
export DEADPAN_REQUIRE_PHYSICAL_MAC=1
export DEADPAN_REQUIRE_SYNTHETIC_WORKER=1
scratch=/tmp/deadpan-this-play-fixed-retime-duration-2026-10-22

cargo fmt --all > "$scratch/baseline-format.log" 2>&1
cargo check --locked -p deadpan-app --bin deadpan-app --tests > "$scratch/baseline-check.log" 2>&1
rg --files -g '*.rs' | sort > "$scratch/baseline-code-paths.txt"
python "$scratch/freeze_baseline.py" > "$scratch/baseline-freeze.log"
cargo nextest list --locked -p deadpan-app --bin deadpan-app -E 'test(native_this_play_fixed_retime_hold_duration_offers_entry_after_public_isolation_control)' --message-format json > "$scratch/baseline-list.json" 2> "$scratch/baseline-build.log"
python "$scratch/baseline_identity.py" > "$scratch/baseline-identity.log"
cargo nextest run --locked -p deadpan-app --bin deadpan-app --no-fail-fast -E 'test(native_this_play_fixed_retime_hold_duration_offers_entry_after_public_isolation_control)' > "$scratch/baseline.log" 2>&1
