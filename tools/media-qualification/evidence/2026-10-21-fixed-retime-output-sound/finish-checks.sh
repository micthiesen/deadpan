#!/bin/sh
set -eu
export DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
export CARGO_PROFILE_DEV_OPT_LEVEL=2
export DEADPAN_REQUIRE_PHYSICAL_MAC=1
export DEADPAN_REQUIRE_SYNTHETIC_WORKER=1
scratch=/tmp/deadpan-fixed-retime-owned-sound-2026-10-21
cargo nextest run --locked -p deadpan-audio --test stages --no-fail-fast -E 'test(fixed_retime_output_sound)' > "$scratch/focused-pcm.log" 2>&1
python "$scratch/pin-binary.py" final-pcm target/debug/deps/stages-4506359765072bfd
cargo nextest run --locked -p deadpan-store --test source_registration --no-fail-fast -E 'test(fixed_retime_output_sound)' > "$scratch/focused-store.log" 2>&1
python "$scratch/pin-binary.py" final-store target/debug/deps/source_registration-9dc8c89f5f7a9079
cargo nextest run --locked -p deadpan-audio -p deadpan-store --no-fail-fast > "$scratch/broad.log" 2>&1
cargo xtask check --since 072f0d82 --no-test > "$scratch/lint-final.log" 2>&1
export DEADPAN_PREVIEW_EXPORT_KEEP=1
export DEADPAN_FIXED_RETIME_OUTPUT_SOUND_RESULTS="$scratch/encoded-result.json"
cargo nextest run --release --locked -p deadpan-cli --test preview_export --no-fail-fast -E 'test(fixed_retime_output_owned_sound_keeps_retained_phase_through_hold_edits_and_complete_export)' > "$scratch/encoded.log" 2>&1
