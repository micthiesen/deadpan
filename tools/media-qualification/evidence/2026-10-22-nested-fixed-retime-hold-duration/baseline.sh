#!/bin/sh
set -eu
export DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
export CARGO_PROFILE_DEV_OPT_LEVEL=2
s=/tmp/deadpan-nested-fixed-retime-duration-2026-10-22
cargo fmt --all > "$s/baseline-format.log" 2>&1
cargo check --locked -p deadpan-app --all-targets > "$s/baseline-check.log" 2>&1
python "$s/freeze_baseline.py" > "$s/baseline-freeze.log"
cargo nextest list --locked -p deadpan-app -E 'test(native_this_play_nested_fixed_retime_hold_duration_offers_entry_after_public_control)' --message-format json > "$s/baseline-list.json" 2> "$s/baseline-build.log"
python "$s/baseline_identity.py" > "$s/baseline-identity.log"
cargo nextest run --locked -p deadpan-app -E 'test(native_this_play_nested_fixed_retime_hold_duration_offers_entry_after_public_control)' --no-fail-fast > "$s/baseline.log" 2>&1
