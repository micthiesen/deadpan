#!/bin/sh
set +e
export CARGO_PROFILE_DEV_OPT_LEVEL=2
scratch=/tmp/deadpan-scoped-mute-2026-10-20
cargo nextest run --locked -p deadpan-core --lib -E 'test(scoped_mute_true_isolates_one_nested_play_and_preserves_gain_curves_order_with_exact_pools)' > "$scratch/old-witness.log" 2>&1
code=$?
printf '%s\n' "$code" > "$scratch/old-witness.exit"
exit "$code"
