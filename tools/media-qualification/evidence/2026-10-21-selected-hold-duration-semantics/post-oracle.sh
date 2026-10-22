#!/bin/sh
set -eu
export DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
scratch=/tmp/deadpan-selected-hold-duration-next-2026-10-21
cargo fmt --all > "$scratch/post-oracle-format.log" 2>&1
python "$scratch/freeze.py" final-source > "$scratch/post-oracle-freeze.log"
cargo clippy --locked -p deadpan-app --features ui-harness --all-targets -- -D warnings > "$scratch/post-oracle-lint.log" 2>&1
cargo build --locked -p deadpan-app --release --features ui-harness --bin deadpan-app > "$scratch/post-oracle-release.log" 2>&1
sh "$scratch/replay.sh"
