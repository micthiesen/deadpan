#!/bin/sh
set -eu
export DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
export CARGO_PROFILE_DEV_OPT_LEVEL=2
export DEADPAN_REQUIRE_PHYSICAL_MAC=1
export DEADPAN_REQUIRE_SYNTHETIC_WORKER=1
scratch=/tmp/deadpan-mixed-hold-duration-next-2026-10-21
cargo clippy -p deadpan-app --features ui-harness --locked --all-targets -- -D warnings > "$scratch/final-ui-lint.log" 2>&1
sh "$scratch/replay.sh"
python "$scratch/summarize_replay.py" > "$scratch/replay-summary.log"
