#!/bin/sh
set -eu
export DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
export CARGO_PROFILE_DEV_OPT_LEVEL=2
export DEADPAN_REQUIRE_PHYSICAL_MAC=1
scratch=/tmp/deadpan-scoped-gain-2026-10-20
cargo xtask check --since c111e8bf --no-test > "$scratch/check-final.log" 2>&1
cargo nextest run -p deadpan-app --features ui-harness --locked --no-fail-fast -E 'test(scoped) or test(macros) or test(semantic) or test(gain) or test(completion)' > "$scratch/ui.log" 2>&1
cargo build -p deadpan-app -p deadpan-media-worker -p deadpan-transcribe -p deadpan-track --release --features deadpan-app/ui-harness --locked --bins > "$scratch/release.log" 2>&1
