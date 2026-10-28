# Reuse the actual owner query for input capture, 2026-10-28 (UTC)

Numerical Root/Domain node preparation now captures from its already checked
one-sample owner query. The sealed candidate owns that query and borrows its
actual parent. Capture consumes a checked actual member index and uses the
stored parent seed/sample. A free owner key, clock, reconstructed Domain or
external query cannot supply capture ancestry.

The legacy owner-input API and candidates share one capture walker.
Input.work keeps total query-plus-capture provenance; a typed receipt separates
the already billed query Q from additional capture C. Absence reports Q and
zero capture without requesting fresh limits. The actual numerical consumer
bills Q before validation/selection and C after capture. It removes a real
second membership query and intermediary clock clone.

First key selection, Retained behavior, sound fallback, native admission,
source checks and output policy remain. Successful retained-probe work stays
charged. The combined 65,536 input-work cap and global remaining budget still
apply; internal zero capture allowance reaches the existing structural-work
failure. No cap, weight, refund, cache, retry or cold activation change.

## Verification

Rust 1.97.1, locked dependencies, qualified FFmpeg/ORT, opt-level 2, debug
assertions and overflow checks. Guards cover 1,429 main and 1,200 isolated
inputs. All eight staged source files equal the isolated final source. Seven
match main; plan.rs preserves its unrelated inherited overlay.

| Check | Result |
| --- | --- |
| Main / isolated Audio library | 286 / 267 pass, none ignored. |
| Main / isolated full Plan | 557 / 544 pass across 29 executables each, none failed, ignored or filtered. |
| Main enabled / full isolated real-WAV stages | 203 / 203 pass; main filters seven unfinished Split cases. |
| Main complete definition / focused historical PCM | 171 / eight pass; historical run filters 163 other cases. |
| Prior numerical consumer with new API/tests | Isolated Audio: 266 pass / one expected failure in the actual tight-budget work-savings test. |
| Required strict gate | cargo xtask check --no-test passes formatting and strict all-target Core/Plan/Audio/Store lint. |
| Independent review | No supported correctness findings. |
| Seven unfinished Split cases | Four pass / three planning-work failures, none ignored; 408.22 seconds. |

Plan tests cover actual Root and Domain capture, hidden negative support,
retained binding/reference grids, concrete Repeat play, foreign plans, actual
member bounds, exact/one-short work, absence and query storage parity.
Audio checks unchanged contexts and derives removed Q from an independent
legacy query oracle: new billed work plus Q equals the qualified old total 28,
with the retained probe's nine units still charged. Restoring the prior
duplicate-query consumer makes the reduced-budget assertion fail. No
old-failing PCM or wall-time speedup claim is made.

The first restored isolated build reused the exact known-negative binary:
copy2 had restored an older source mtime, and Cargo reported fresh artifacts
in 0.16 seconds. It is rejected as positive qualification. Touching the restored
file forces capture-scoped-build2; its Audio hash differs and all 267 tests pass.
Only that rebuilt positive binary supplies isolated results. Source hashes and
successful Cargo JSON alone would not have caught this; the counterexample
test and pinned binary identity do. Development now documents this hazard.
There is no production, compiler or positive runtime failure in this delivery.

All 15 build/check/test receipts are terminal with source and binary guards
unchanged. Main/fresh isolated builds take 53.69/32.19 seconds; main stage and
definition runs 94.46/76.93 seconds; isolated stages 64.97 seconds. These are
verification durations, not performance acceptance. No GUI, ABI, packaging or
release checks are repeated.

The unfinished failures now report: Source/Sequence refinement, cut-three,
limited 0+511: sound clock scope work; nested selected without root sound,
prefix-two, limited 0+4096: source occurrence construction work; one selected
play, prefix-two, limited 0+4096: audio binding work exhausted. Preserve these
changed subphases. Processed-root, partition independence, each depth and
audible witness still pass. The work optimization does not close Split.

## Remaining work and evidence

One live query replaces the former second query during capture; actual query
capacity/heap parity tests cover this storage change. They are not prior
metadata constructor admission. A cold inventory still needs exact descriptor
continuations, complete transient/retained/native bounds and recursive input
coverage. General sound-bearing Split stays disabled; batching is not active.
Coverage stays about 85%, ten Complete requirements and all seven gates open.

The [inventory](../../tools/media-qualification/evidence/2026-10-28-owner-input-capture/inventory.json)
retains 231 gzip payloads: exact source/patches, query design, all 32-target pins,
commands/output, Plan per-target logs, old-consumer negative, rejected stale
binary, fresh positive, source/index equalities and independent review.
Stored/expanded hashes verify: 2,619,939 / 10,915,047 bytes. Inventory SHA-256:
72edf14d01dfc710c10436f71c9b9a3dac52cd3fae8edcea563b20750fbbbcd4.
Raw task work is in /tmp/deadpan-owner-input-capture-2026-10-28.
