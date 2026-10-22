#!/bin/sh
set -eu
export DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
export CARGO_PROFILE_DEV_OPT_LEVEL=2
export DEADPAN_REQUIRE_PHYSICAL_MAC=1
export DEADPAN_REQUIRE_SYNTHETIC_WORKER=1
scratch=/tmp/deadpan-this-play-fixed-retime-duration-2026-10-22
cargo fmt --all > "$scratch/format.log" 2>&1
cargo check --locked -p deadpan-core -p deadpan-app -p deadpan-store -p deadpan-audio -p deadpan-cli --all-targets > "$scratch/check-all.log" 2>&1
cargo check --locked -p deadpan-app --all-targets --features ui-harness > "$scratch/check-ui.log" 2>&1
