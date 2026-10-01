# Source Slip bounded review

Review scope: applied Source Slip implementation in the shared checkout on 2026-10-01. Read-only review; no checkout edits and no Cargo/native/replay commands run by this reviewer.

## Reviewed

- `crates/deadpan-core/src/source_slip.rs`, `src/command.rs` SlipSource application, legacy adapter rejection, and sound-clock classification.
- `crates/deadpan-core/tests/source_slip.rs`.
- `crates/deadpan-store/src/lib.rs` guarded snapshot/command preparation, `src/source_registration/slip.rs`, and `tests/source_registration/slip.rs`.
- `crates/deadpan-cli/src/live_project.rs` and `tests/source_registration/slip.rs`.
- `crates/deadpan-plan/tests/selected_video_window/slip.rs`.
- `docs/SOURCE_SLIP.md`.

The audio PCM witness `crates/deadpan-audio/src/bound_reads/source_origin/gain/slip.rs` was excluded after the parent reported two failing exact comparisons and assigned their diagnosis to the backend agent. No conclusion is made about those tests.

## Findings

No actionable correctness findings in the reviewed core, store, CLI, or plan scope. Core's exact limits are `video_support.start - effective.start` and `video_support.end - effective.end`; whole-frame limits use ceil/floor, and the full mapping starts move by negative applied delta. The Source duration, spans, window, offsets, owner clocks, and partition allocation stay fixed. Store preview pins a read transaction, validates the request header and stored receipt even for applied-zero, and does not reserve/write; commit preparation rechecks the same admission. CLI adds the report only for Slip dry runs and keeps an applied-zero edit null.

## Independent picture oracle checked

The plan test uses signed origins -10010 and 13013 ticks, VFR starts at offsets `[0, 1400, 4004, 4600, 6200, 7007, 8200]`, terminal offset 12012, and exactly 2002 source ticks per 30000/1001 project frame. For direct Source requests -100, -1, +1, +100, its expected indexed identities/PTS are respectively `(0,0)/(1,1400)`, `(1,1400)/(3,4600)`, `(5,7007)/(6,8200)`, and `(6,8200)/(6,8200)` at the two sampled output positions. A +1 then -1 returns to selection end 7007: frame 5 exists in the full index there, but half-open selected context correctly holds frame 4 at PTS 6200. The fractional source-anchor checks independently expect 4101/1001 before and 3100/1001 after +1; PTS 4600 becomes OutsideMapping.

The parent reported core 47, plan 11, media 33, and store 44 focused tests passing. These are parent-run results, not tests run by this reviewer.
