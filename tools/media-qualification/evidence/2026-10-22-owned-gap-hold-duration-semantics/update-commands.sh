#!/bin/sh
set -eu
export DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
export CARGO_PROFILE_DEV_OPT_LEVEL=2
export DEADPAN_REQUIRE_PHYSICAL_MAC=1
export DEADPAN_REQUIRE_SYNTHETIC_WORKER=1
scratch=/tmp/deadpan-owned-gap-hold-duration-next-2026-10-21

export DEADPAN_UPDATE_COMMANDS_MD=1
cargo nextest run --locked -p deadpan-app -E 'test(commands_reference_is_current)' > "$scratch/commands-update.log" 2>&1
