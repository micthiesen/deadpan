#!/bin/sh
set -eu
export DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
export CARGO_PROFILE_DEV_OPT_LEVEL=2
export DEADPAN_REQUIRE_PHYSICAL_MAC=1
export DEADPAN_REQUIRE_SYNTHETIC_WORKER=1
export DEADPAN_PREVIEW_EXPORT_KEEP=/tmp/deadpan-next-shared-fixed-retime-duration-2026-10-22/real-media
export DEADPAN_SHARED_FIXED_RETIME_DURATION_RESULTS=/tmp/deadpan-next-shared-fixed-retime-duration-2026-10-22/real-media-results.json
cargo nextest run --locked -p deadpan-cli --test preview_export -E 'test(=shared_fixed_retime_hold_duration_preview_and_encoded_output_keep_fixed_windows)' --no-fail-fast
