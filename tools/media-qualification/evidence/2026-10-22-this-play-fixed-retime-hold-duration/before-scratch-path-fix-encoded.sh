#!/bin/sh
set -eu
export DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
export CARGO_PROFILE_DEV_OPT_LEVEL=2
export DEADPAN_REQUIRE_PHYSICAL_MAC=1
export DEADPAN_REQUIRE_SYNTHETIC_WORKER=1
scratch=/tmp/deadpan-this-play-this-play-fixed-duration-2026-10-22

export DEADPAN_PREVIEW_EXPORT_KEEP=1
export DEADPAN_THIS_PLAY_FIXED_RETIME_DURATION_RESULTS="$scratch/encoded-result.json"
cargo nextest list --release --locked -p deadpan-cli --test preview_export -E 'test(this_play_fixed_retime_hold_duration_preview_and_encoded_output_keep_fixed_windows)' --message-format json > "$scratch/encoded-list.json" 2> "$scratch/encoded-build.log"
python "$scratch/encoded_identity.py" pre > "$scratch/encoded-identity-pre.log"
cargo nextest run --release --locked -p deadpan-cli --test preview_export --no-fail-fast -E 'test(this_play_fixed_retime_hold_duration_preview_and_encoded_output_keep_fixed_windows)' > "$scratch/encoded.log" 2>&1
python "$scratch/encoded_identity.py" post > "$scratch/encoded-summary.log"
