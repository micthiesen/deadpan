#!/bin/sh
set -eu
export DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
export CARGO_PROFILE_DEV_OPT_LEVEL=2
export DEADPAN_REQUIRE_PHYSICAL_MAC=1
export DEADPAN_REQUIRE_SYNTHETIC_WORKER=1
scratch=/tmp/deadpan-fractional-owner-support-2026-10-21
cargo nextest list --locked -p deadpan-plan -p deadpan-audio -p deadpan-playback --message-format json > "$scratch/broad-list.json" 2> "$scratch/broad-build.log"
cargo nextest run --locked -p deadpan-plan -p deadpan-audio -p deadpan-playback --no-fail-fast > "$scratch/broad.log" 2>&1
cargo xtask check --since 25646268 --no-test > "$scratch/lint.log" 2>&1
