#!/bin/sh
set -eu
export DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
export CARGO_PROFILE_DEV_OPT_LEVEL=2
export DEADPAN_REQUIRE_PHYSICAL_MAC=1
export DEADPAN_REQUIRE_SYNTHETIC_WORKER=1
scratch=/tmp/deadpan-fixed-retime-owned-sound-2026-10-21
cargo nextest run --locked -p deadpan-core --lib --no-fail-fast -E 'test(fixed_retime_) or test(sound_owner_clock)' > "$scratch/focused-core.log" 2>&1
cargo nextest run --locked -p deadpan-audio --test stages --no-fail-fast -E 'test(fixed_retime_output_sound)' > "$scratch/focused-pcm.log" 2>&1
cargo nextest run --locked -p deadpan-store --test source_registration --no-fail-fast -E 'test(fixed_retime_output_sound)' > "$scratch/focused-store.log" 2>&1
