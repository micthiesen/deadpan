#!/bin/sh
set -eu
export DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
export CARGO_PROFILE_DEV_OPT_LEVEL=2
scratch=/tmp/deadpan-next-shared-fixed-retime-duration-2026-10-22
cargo build --locked -p deadpan-app --features ui-harness --bin deadpan-app > "$scratch/dev-native-build.log" 2>&1
target/debug/deadpan-app --ui-check --scenario fixed-retime-duration --mode visual --output "$scratch/debug-visual" --kestrel-source /Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift > "$scratch/debug-visual.log" 2>&1
