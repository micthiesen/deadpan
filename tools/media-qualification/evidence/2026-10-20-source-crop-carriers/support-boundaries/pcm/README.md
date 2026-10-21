# Selected Source support boundary PCM regression patch

Scratch-only candidate patch against `crates/deadpan-audio/tests/stages/clocked_speed_update/source_crop_pcm.rs`. No repository files changed; no Cargo, Git, or runtime execution was performed.

The candidate keeps the existing fixture interface by making `crop_fixture` a wrapper around `crop_fixture_at(mapping_start, crop_start, crop_end, offset, fresh)`. Both new tests retain the 160-sample natural-rate source, the outer two-frame Holds, the public retained/fresh binding construction, and the exact signed-offset compensation.

`speed_update_selected_crop_is_clipped_to_intrinsic_source_before_raw_allocation` checks `[999/1000, 103/100)` against intrinsic Source support `[0, 1)`. The selected old carrier is nonempty (`3235..3236` retained, `4837..4838` fresh), but the physically meaningful intersection `[999/1000, 1)` has an empty root carrier (`3235..3235` / `4837..4837`). The candidate selected allocation clips from `[20781, 21276)` to `[20781, 20797)`, exactly 16 samples.

`speed_update_selected_crop_outside_intrinsic_source_keeps_empty_raw_domain` checks `[5/2, 2501/1000)` against `[0, 1)`. It preserves the literal nonpositive intersection `[5/2, 1)` and requires `AudioBoundDomain::Empty`; the selected candidate samples `[44784, 44800)` lie beyond the physical Source allocation `[4805, 20797)`.

Both cases run retained/fresh and offsets ±1. They assert the full physical plan allocation, exact digital zeros and suppression, no source-provider calls, alternate request partitions, identical PCM after public update, unchanged Source definition and old timing records, JSON round-trip, input JSON stability, and inverse behavior through the existing public-update helper. Root can apply the unified diff at `support-boundaries.patch` if it wants this additional boundary coverage. The scratch source was rustfmt-formatted only; no runtime result is claimed.
