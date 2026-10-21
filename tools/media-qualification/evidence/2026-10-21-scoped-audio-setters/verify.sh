#!/bin/sh
set -eu
export DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
export CARGO_PROFILE_DEV_OPT_LEVEL=2
export DEADPAN_REQUIRE_PHYSICAL_MAC=1
export DEADPAN_REQUIRE_SYNTHETIC_WORKER=1
scratch=/tmp/deadpan-scoped-audio-setters-2026-10-21
filter='test(scoped) or test(macros) or test(semantic) or test(gain) or test(mute) or test(saturat) or test(balance) or test(equaliz) or test(completion) or test(registry)'
cargo nextest run -p deadpan-app -p deadpan-core --locked --no-fail-fast -E "$filter" > "$scratch/normal.log" 2>&1
cargo nextest run -p deadpan-app --features ui-harness --locked --no-fail-fast -E "$filter" > "$scratch/ui.log" 2>&1
cargo build -p deadpan-app --release --features ui-harness --locked --bin deadpan-app > "$scratch/release-app.log" 2>&1
DEADPAN_PREVIEW_EXPORT_KEEP=1 DEADPAN_SCOPED_AUDIO_RESULTS="$scratch/encoded-result.json" cargo nextest run --release -p deadpan-cli --features synthetic-worker --test preview_export --locked --success-output immediate -E 'test(scoped_audio_chain_preview_and_complete_encoded_output_isolate_one_nested_play)' > "$scratch/encoded.log" 2>&1
sh "$scratch/replay.sh"
