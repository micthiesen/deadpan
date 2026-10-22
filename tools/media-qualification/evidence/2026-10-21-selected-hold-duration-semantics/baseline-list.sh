#!/bin/sh
set -eu
export DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
export CARGO_PROFILE_DEV_OPT_LEVEL=2
cargo nextest list --locked -p deadpan-app --bin deadpan-app --message-format json > /tmp/deadpan-selected-hold-duration-next-2026-10-21/baseline-list.json
