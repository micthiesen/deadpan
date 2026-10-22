#!/bin/sh
set -eu
export DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
export CARGO_PROFILE_DEV_OPT_LEVEL=2
export DEADPAN_REQUIRE_PHYSICAL_MAC=1
export DEADPAN_REQUIRE_SYNTHETIC_WORKER=1
scratch=/tmp/deadpan-native-fixed-retime-hold-2026-10-21
export DEADPAN_PREVIEW_EXPORT_KEEP=1
export DEADPAN_FRACTIONAL_OWNER_SUPPORT_RESULTS="$scratch/encoded-result.json"
cargo nextest run --release --locked -p deadpan-cli --test preview_export --no-fail-fast -E 'test(fractional_processed_owner_support_keeps_nonzero_shared_master_and_complete_export)' > "$scratch/encoded.log" 2>&1
