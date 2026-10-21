#!/bin/sh
set -eu
export DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
export CARGO_PROFILE_DEV_OPT_LEVEL=2
export DEADPAN_REQUIRE_PHYSICAL_MAC=1
scratch=/tmp/deadpan-scoped-mute-2026-10-20
cargo nextest run -p deadpan-app -p deadpan-core --locked --test-threads 1 -E 'test(commands_reference_is_current) or test(unobserved_transition_cannot_be_hidden_by_later_mark_or_cut_proof) or test(completion_waits_for_native_receipt_and_retains_operational_success_on_refresh_failure)' > "$scratch/reference-and-leaks.log" 2>&1
cargo nextest run -p deadpan-app --features ui-harness --locked --no-fail-fast -E 'test(scoped) or test(macros) or test(semantic) or test(gain) or test(mute) or test(completion) or test(registry)' > "$scratch/ui.log" 2>&1
cargo build -p deadpan-app --release --features ui-harness --locked --bin deadpan-app > "$scratch/release.log" 2>&1
sh "$scratch/replay.sh"
