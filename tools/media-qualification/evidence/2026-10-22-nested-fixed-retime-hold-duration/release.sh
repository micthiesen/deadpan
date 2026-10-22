#!/bin/sh
set -eu
export DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
export CARGO_PROFILE_DEV_OPT_LEVEL=2
scratch=/tmp/deadpan-nested-fixed-retime-duration-2026-10-22
python "$scratch/freeze.py" final-source > "$scratch/final-freeze.log"
cargo build --locked -p deadpan-app --release --features ui-harness --bin deadpan-app > "$scratch/release-app.log" 2>&1
sh "$scratch/native.sh"
sh "$scratch/encoded.sh"
