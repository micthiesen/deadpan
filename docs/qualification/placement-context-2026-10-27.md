# Joined sound placement and context clock, 2026-10-27

Certified-Split afterbirth history obtains placement and its exact context clock
together. Placement already projects the owner, opaque provider and higher
ancestors. The joined query captures two scalar ratios from the same checked
projection, removing the caller's duplicate projection. A context below the
provider that placement did not visit receives its original projection once,
after the private placement chain and projection paths have dropped.

The old public placement API retains its original visits, allocations, arithmetic
order and work through a compile-time no-capture branch. The joined query charges
one scalar extraction plus every actual fallback projection against its supplied
aggregate budget. Context membership still checks the exact node and complete
stable Repeat prefix. Ordinary afterbirth paths retain the old API. No cache,
limit, media authority, retained path or plan-owning Arc is added.

Five oracle tests compare the joined result with separate checked placement and
projection calls. They cover owner/provider/higher reuse, below-provider fallback,
literal old-API work, Preserve and Shift, derived 9/14 and FollowSpeed 5/7 clocks,
16 Repeat segments with body/gap overrides, foreign branches, and exact/one-short
budgets. Reused queries remove actual work; fallback queries charge their work.

## Verification

Apple M5 Max, macOS 26.5.2, Rust 1.97.1, locked dependencies and qualified FFmpeg.
Audio uses optimization level 2 with debug assertions and overflow checks.

| Check | Result |
| --- | --- |
| Current Core library | 800 pass; zero ignored. |
| Current Plan | 534 pass across 28 executables; zero ignored. |
| Current Audio library/definition | 198/171 pass; zero ignored. |
| Current existing real-WAV stages | 203 pass; seven unfinished Split tests run separately; zero ignored. |
| Isolated four-file patch | Builds; 775 Core, 521 Plan, 184 Audio library and eight historical birth/processing tests pass; zero ignored. |
| Final focused tests | Five joined-placement tests pass after the mechanical test cleanup. |
| Final strict check | `cargo xtask check --no-test` passes formatting and strict all-target Core/Plan/Audio/Store lint. |
| Unchanged real-WAV Split matrix | Four pass, three work-limit failures; 405.33 seconds. |

Initial strict lint found one collapsible nested `if` in a new test. The test-only
cleanup is retained as its own patch. All production inputs match the broad
runs; final focused tests and strict lint pass after that cleanup. The broad
suites were not repeated for the syntax change. Independent source review finds
no concrete defect.

The incomplete reads are Source/Sequence `cut-three`, limited `0+511`, and
selected/nested selected `prefix-two`, limited `0+4096`. All-owner and nested
selected reach `plan work per read`; selected reaches `frozen projection work
exhausted`. Processed root-sound refinement still passes its complete PCM
comparison. The matrix overlaps an isolated build, so its wall time is not a
calibrated performance measurement.

Current runs guard 1,393 source inputs; isolated runs guard 1,165. Source and
executed binary hashes remain stable. Raw records and pinned binaries are in
`/tmp/deadpan-placement-context-2026-10-27`. The
[evidence inventory](../../tools/media-qualification/evidence/2026-10-27-placement-context/inventory.json)
retains the exact four-file patch, complete candidate reconstruction, commands,
outputs, initial lint failure, cleanup, source/binary identities and review.
All 249 payloads verify stored and expanded hashes: 10,054,329 stored bytes and
40,025,945 expanded bytes. Inventory SHA-256:
`67d2d9b9dad13ae1c317b7d3be91134f5596d618ba69bd60ab2884f97e20a0d9`.

No requirement, criterion or delivery gate closes from this milestone. The full
product goal stays active, with general sound-bearing Split still unqualified.
