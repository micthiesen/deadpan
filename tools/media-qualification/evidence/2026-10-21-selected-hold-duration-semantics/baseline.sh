#!/bin/sh
set -eu
export DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
export CARGO_PROFILE_DEV_OPT_LEVEL=2
export DEADPAN_REQUIRE_PHYSICAL_MAC=1
export DEADPAN_REQUIRE_SYNTHETIC_WORKER=1
cargo nextest run --locked -p deadpan-app --bin deadpan-app --no-fail-fast -E 'test(native_selected_hold_duration_offers_entry_after_zero_net_compound_control)' > /tmp/deadpan-selected-hold-duration-next-2026-10-21/baseline.log 2>&1
