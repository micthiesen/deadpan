#!/bin/sh
set -eu
export DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
export CARGO_PROFILE_DEV_OPT_LEVEL=2
export DEADPAN_REQUIRE_PHYSICAL_MAC=1
scratch=/tmp/deadpan-scoped-gain-2026-10-20
cargo nextest run -p deadpan-app --features ui-harness --locked -E 'test(unobserved_transition_cannot_be_hidden_by_later_mark_or_cut_proof)' > "$scratch/isolated-leak.log" 2>&1
sh "$scratch/replay.sh"
