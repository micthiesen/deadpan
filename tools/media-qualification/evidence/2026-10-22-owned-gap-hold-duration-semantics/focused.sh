#!/bin/sh
set -eu
export DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
export CARGO_PROFILE_DEV_OPT_LEVEL=2
export DEADPAN_REQUIRE_PHYSICAL_MAC=1
export DEADPAN_REQUIRE_SYNTHETIC_WORKER=1
scratch=/tmp/deadpan-owned-gap-hold-duration-next-2026-10-21
filter=$(cat "$scratch/focused-filter.txt")
cargo nextest list --locked -p deadpan-core -p deadpan-plan -p deadpan-audio -p deadpan-store -p deadpan-app -p deadpan-cli -E "$filter" --message-format json > "$scratch/focused-list.json" 2> "$scratch/focused-build.log"
cargo nextest run --locked -p deadpan-core -p deadpan-plan -p deadpan-audio -p deadpan-store -p deadpan-app -p deadpan-cli --no-fail-fast -E "$filter" > "$scratch/focused.log" 2>&1
