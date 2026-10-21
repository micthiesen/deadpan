#!/bin/sh
set -u
export DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
export CARGO_PROFILE_DEV_OPT_LEVEL=2
cargo nextest run --locked -p deadpan-core --lib -E 'test(scoped_gain_step_isolates_one_nested_play_with_exact_pools_and_one_inverse)' > /tmp/deadpan-scoped-gain-2026-10-20/old-witness.log 2>&1
witness_status=$?
printf '%s\n' "$witness_status" > /tmp/deadpan-scoped-gain-2026-10-20/old-witness.exit
exit "$witness_status"
