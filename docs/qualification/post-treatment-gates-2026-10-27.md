# Post-treatment sound gates, 2026-10-27

Preparation omits the second gate query when the canonical event-return predicate
is false. The first full gate query, source admission and retained-input preflight
remain. The decision costs one work unit; a required query retains its complete
reported work. Owner processing and Tail Sends do not turn an ordinary event
into a processed event return. No cache, dependency, permission or limit changes.

The investigation also exposed an existing root-sound PCM bug. Direct
post-treatment gates suppressed already-applied input edges, but routed root
gates reused compiled Automatic island tags. The authored fade was therefore
applied again to processed dry output. A real-PCM regression fails at sample 37:
`-0.000013255305` instead of `-0.0025450184`.

The corrected plan carries complete and post-treatment edge policies through
immutable boundaries, retaining exact semantic coordinates and physical sample
labels. Authored endpoints need no additional post fade; a new cut or Hold keeps
its policy. Exact coincidence deduplicates the already-applied endpoint fade.
Both phases preserve genuine Hard choices. Projection happens only at final
span emission, with a work charge. This adds no second island vector, layout
traversal, persisted format or media-admission shortcut.

An initial Option-neutral design was rejected before commit. Its focused routed
test passed, but a SilentHold wholly before an offset-37 event added the same
second fade at exact coincidence. The new 48,000-sample invariant keeps Original
audio absent, event timing fixed and project duration unchanged. It fails that
candidate and passes the corrected paired-policy version. Independent review
corrected its earlier recommendation after this executable evidence.

## Verification

Apple M5 Max, macOS 26.5.2, Rust 1.97.1, locked dependencies and qualified FFmpeg.
Audio uses optimization level 2 with debug and overflow checks. Independent final
source review finds no concrete defect.

| Check | Result |
| --- | --- |
| Final current Plan | 534 pass across 28 executables; zero ignored. |
| Final current Audio library | 198 pass; zero ignored. |
| Final current event-return PCM suite | Eight pass, including both new regressions; zero ignored. |
| Final current root-route/Hold waveform | One pass; zero ignored. |
| Final isolated nine-file patch | Builds; 521 Plan, 184 Audio library and eight event-return PCM tests pass; zero ignored. |
| Strict final check | `cargo xtask check --no-test` passes formatting and strict all-target Core/Plan/Audio/Store lint. |
| Final unchanged real-WAV Split matrix | Four pass, three work-limit failures; 410.27 seconds. |

The preceding full candidate run passes 533 Plan, 198 Audio library, all 171
Audio definition and all 202 then-existing stage tests, with the seven unfinished
Split cases run separately. Its matrix remains four pass/three work-limit
failures. Those are intermediate results, not acceptance of the rejected
coincidence rule. After the small policy correction, final checks above rerun
Plan, Audio library and the affected event-return/root-waveform paths; the full
definition and other stage suites were not repeated.

Three Plan regressions cover insertion/deletion, current Holds, real Hard
choices, exact coincidence and odd RoundEven transport at 32,000 fps.
They check support, retained labels, envelope progress and exact/one-short work
budgets. The two new PCM regressions compare complete output and boundary reads.
Gate preparation tests inspect actual root/current/routed prepared spans, compare
required query work with an explicit query, prove laziness for unused queries,
and retain exact, one-short and zero-budget refusals. An initial test requested
576 frames through the existing 256-frame routed-reader guard; the fixture was
corrected without changing that production limit. All original failures remain.

Current runs guard 1,392 source inputs; isolated runs guard 1,164. Source and
executed binary hashes remain stable. Raw records and pinned executables are in
`/tmp/deadpan-root-post-gates-run-2026-10-27`, with the separate coincidence
probe in `/tmp/deadpan-coincident-edge-probe-2026-10-27`.
The final matrix executable has SHA-256
`eeff17f682fbd0d197d91a32156452f87f916e813e9f8f8674699b9ee5b8339d`.
The [evidence inventory](../../tools/media-qualification/evidence/2026-10-27-post-treatment-gates/inventory.json)
retains the exact nine-file patch, full candidate reconstruction, original and
rejected-design failures, commands, outputs, identities and independent review.
All 497 payloads verify stored and expanded hashes: 20,429,695 stored bytes and
81,251,287 expanded bytes. Inventory SHA-256:
`8ba35886247ce3a7dbcb9d95de65e8027bfcfbd3cfe17085b931e4a919dc2ca2`.

No requirement, criterion or delivery gate closes from this milestone.

The final incomplete reads remain Source/Sequence `cut-three`, limited `0+511`,
and selected/nested selected `prefix-two`, limited `0+4096`. All-owner and selected
reach `plan work per read`; nested selected reaches `audio binding work exhausted`.
The earlier candidate stopped selected at `sound processing history work`.
Processed root-sound refinement still passes its complete authored/faded/limited
comparison. The final matrix overlaps isolated tests and strict lint, so its wall
time is not a calibrated performance result.
