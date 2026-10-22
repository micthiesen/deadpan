#!/bin/sh
set -eu
export DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
export CARGO_PROFILE_DEV_OPT_LEVEL=2
scratch=/tmp/deadpan-next-shared-fixed-retime-duration-2026-10-22
cargo fmt --all > "$scratch/verified-format.log" 2>&1
python "$scratch/freeze.py" focused-fourth-runtime-source > "$scratch/focused-fourth-freeze.log"
sh "$scratch/focused.sh"
sh "$scratch/verify.sh"
