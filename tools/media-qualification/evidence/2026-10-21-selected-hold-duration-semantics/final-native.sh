#!/bin/sh
set -eu
export DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
scratch=/tmp/deadpan-selected-hold-duration-next-2026-10-21
cargo build --locked -p deadpan-app --release --features ui-harness --bin deadpan-app > "$scratch/final-release-app.log" 2>&1
sh "$scratch/replay.sh"
