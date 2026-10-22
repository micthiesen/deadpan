#!/bin/sh
set -eu
export DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
export CARGO_PROFILE_DEV_OPT_LEVEL=2
export DEADPAN_REQUIRE_PHYSICAL_MAC=1
export DEADPAN_REQUIRE_SYNTHETIC_WORKER=1
cargo nextest run --locked -p deadpan-core -p deadpan-plan -p deadpan-audio -p deadpan-store -p deadpan-app -p deadpan-cli --no-fail-fast -E 'test(mixed_duration) | test(mixed_default_play_generated_duration) | test(mixed_hold_duration)' > /tmp/deadpan-mixed-hold-duration-next-2026-10-21/focused.log 2>&1
