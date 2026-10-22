#!/bin/sh
set -eu
export DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
export CARGO_PROFILE_DEV_OPT_LEVEL=2
export DEADPAN_REQUIRE_PHYSICAL_MAC=1
export DEADPAN_REQUIRE_SYNTHETIC_WORKER=1
scratch=/tmp/deadpan-selected-hold-duration-next-2026-10-21
export DEADPAN_PREVIEW_EXPORT_KEEP=1
export DEADPAN_SELECTED_DURATION_RESULTS="$scratch/encoded-result.json"
cargo nextest list --release --locked -p deadpan-cli --test preview_export -E 'test(selected_hold_duration_semantics_preview_and_encoded_output_keep_each_source_clock)' --message-format json > "$scratch/encoded-list.json" 2> "$scratch/encoded-build.log"
python "$scratch/encoded_identity.py" pre
cargo nextest run --release --locked -p deadpan-cli --test preview_export --no-fail-fast -E 'test(selected_hold_duration_semantics_preview_and_encoded_output_keep_each_source_clock)' > "$scratch/encoded.log" 2>&1
python "$scratch/encoded_identity.py" post > "$scratch/encoded-summary.log"
