#!/bin/sh
set -eu
export DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
export CARGO_PROFILE_DEV_OPT_LEVEL=2
export DEADPAN_REQUIRE_PHYSICAL_MAC=1
export DEADPAN_REQUIRE_SYNTHETIC_WORKER=1
export DEADPAN_PREVIEW_EXPORT_KEEP=1
export DEADPAN_NESTED_FIXED_RETIME_DURATION_RESULTS=/tmp/deadpan-nested-fixed-retime-duration-2026-10-22/real-media-results.json
scratch=/tmp/deadpan-nested-fixed-retime-duration-2026-10-22
filter=$(cat "$scratch/focused-second-filter.txt")
cargo nextest list --locked -p deadpan-core -p deadpan-plan -p deadpan-audio -p deadpan-store -p deadpan-app -p deadpan-cli -E "$filter" --message-format json > "$scratch/focused-second-list.json" 2> "$scratch/focused-second-build.log"
python "$scratch/focused-second_identity.py" > "$scratch/focused-second-identity.log"
cargo nextest run --locked -p deadpan-core -p deadpan-plan -p deadpan-audio -p deadpan-store -p deadpan-app -p deadpan-cli -E "$filter" --no-fail-fast > "$scratch/focused-second.log" 2>&1
