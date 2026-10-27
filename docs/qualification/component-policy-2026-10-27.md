# Shared Components structural decisions, 2026-10-27

Constant and dynamic Components execution now share their permission, first-clock,
owner-order, EQ-classification and Original explicit-Tail exclusion helpers. Their
numerical loops, callback orders, work/check calls and allocations stay in place.
There is no production demand visitor, policy bit carrier, resource proof or
runtime batching in this milestone. General sound-bearing Split remains open.

## Exact boundary

Helpers accept immutable route/owner layout and an active-route boolean lookup.
Runtime supplies that lookup from the existing permitted flags; tests also supply
external metadata flags before route PCM allocation. EQ classification reads the
treatment recipe's non-unity property without evaluating a numeric transfer.
Permission masks use voice IDs, not route indices. Clock selection keeps the
first active member and its complete affine/origin/Repeat clock.

Constant execution retains owner/stage/sample/route order for EQ and
owner/route/sample/effect order for wet returns. Dynamic execution retains
sample/owner/stage/route/effect order. Constant's permission and clock walks remain
separate; dynamic wet keeps its single combined walk. The same EQ predicate also
controls Program checkpoint flushing. No numerical sum or gain operation moves.

The existing sorted owner-reference vector and clock clones remain. There is no
new persistent policy storage or reduced residency charge. Empty PCM capacity in
a test is not a proof of zero metadata allocation or a recursive construction
peak. These helpers neither validate media nor grant their own source authority.

Four new tests cover 256 metadata/callback bridge cases and separate literal
expectations. The bridge shares the extracted helpers, so it is not an independent
correctness oracle for those helpers. It compares full clock identity, affine
phase/slope, Repeat escalation, temporal stage, sample, complete mask and selected
voice against each real numerical engine. It covers gain order, mute, sends,
skip/stop, inactive samples, final targets and aliases. Literal cases check
callback counts, distinct voice IDs, first active clocks from different Repeat
plays, dry versus wet Tail masks, and the existing voice-limit refusal. Existing
independent numerical, bitwise-output and work/error fixtures remain unchanged.

## Verification

Same qualified macOS/M5 Max/Rust/FFmpeg/ORT environment, locked dependencies,
opt-level 2, debug assertions and overflow checks as the preceding storage
milestone. Main guards 1,400 inputs; the isolated published runtime guards 1,172.
Both change exactly three Components files and add the same two files. All other
inputs match their previous frozen baseline, and all five task files match.

| Check | Result |
| --- | --- |
| Main three-package build | Pass, 84.08 seconds. |
| Audio library | 217 pass, zero ignored. |
| Complete Audio definition target | 171 pass, zero ignored, 269.52 seconds. |
| Real-WAV wet-owner stages | Ten pass, zero ignored. |
| Real-WAV gain stages | 19 pass, zero ignored. |
| Strict check | `cargo xtask check --no-test`: formatting and strict all-target Core/Plan/Audio/Store lint pass. |
| Isolated five-file build | Pass, 84.80 seconds. |
| Isolated Audio library | 200 pass, zero ignored. |
| Isolated historical birth/processing PCM | Eight pass, zero ignored, 9.84 seconds. |

Independent review found no supported defect. No fixture or lint correction was
needed. All receipts are terminal, with guarded sources and executed binaries
unchanged before/after each run. These durations are verification records, not a
throughput comparison or new native performance qualification.

The full 203 existing stages and seven unfinished Split cases are not rerun for
this extraction. The [preceding storage milestone](wet-residency-2026-10-27.md)
passed all 203 and retained four Split passes/three work failures. That remains
historical evidence, not a new complete-stage or Split result for this source.
No UI, native ABI, packaging or whole-product gate is rerun. No requirement or
gate closes; runtime graph reads still request one output.

## Remaining work and evidence

A production policy carrier/visitor still needs exact Original/sound/routed
suppression and exhaustion facts, admitted metadata storage, shared callback
ordering, and fully billed reconstruction/visits. A cold resource fold must then
count future required outputs/tiles and recursive scratch before extra histories
allocate. Live and historical intrinsic processing need an exact eligibility
gate; authored pitch alone is insufficient. Never use fake PCM, a copied mixer,
warm peaks, work waivers or failed-batch retry as a substitute.

Raw records remain in `/tmp/deadpan-component-policy-2026-10-27`. The
[inventory](../../tools/media-qualification/evidence/2026-10-27-component-policy/inventory.json)
retains the exact patch, changed-file before snapshots, scratch audit, independent
review, source/binary pins, main/isolated lineage, commands, output and research.
All 58 gzip payloads verify stored and expanded hashes: 1,519,629 stored bytes and
6,252,240 expanded bytes. Inventory SHA-256:
`9d717633adabeebe95bda37c0714a55f7f25aa46961e63d7737d8dd97ee095ed`.
