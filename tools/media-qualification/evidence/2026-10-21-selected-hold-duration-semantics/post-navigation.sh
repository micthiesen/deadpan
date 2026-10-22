#!/bin/sh
set -eu
export DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
scratch=/tmp/deadpan-selected-hold-duration-next-2026-10-21
cargo fmt --all > "$scratch/post-navigation-format.log" 2>&1
python "$scratch/freeze.py" final-source > "$scratch/post-navigation-freeze.log"
cargo clippy --locked -p deadpan-app --features ui-harness --all-targets -- -D warnings > "$scratch/post-navigation-lint.log" 2>&1
