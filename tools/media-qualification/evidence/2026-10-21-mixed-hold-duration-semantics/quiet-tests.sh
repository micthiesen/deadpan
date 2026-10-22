#!/bin/sh
set -eu
export DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix
export CARGO_PROFILE_DEV_OPT_LEVEL=2
export DEADPAN_REQUIRE_PHYSICAL_MAC=1
export DEADPAN_REQUIRE_SYNTHETIC_WORKER=1
python /tmp/deadpan-mixed-hold-duration-next-2026-10-21/quiet_audio.py
cargo nextest run --locked -p deadpan-app --bin deadpan-app --features ui-harness --test-threads 1 -E 'test(ordinary_duration_availability_visits_only_selected_ancestry_beside_a_million_plays) | test(exact_mark_transition_preserves_current_nested_navigation_and_repeated_receipt_is_noop) | test(terminal_cursor_uses_a_visible_sample_instead_of_unsampled_first_plays)' > /tmp/deadpan-mixed-hold-duration-next-2026-10-21/quiet-tests.log 2>&1
