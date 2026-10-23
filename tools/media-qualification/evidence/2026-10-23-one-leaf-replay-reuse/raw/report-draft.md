# One-leaf replay reuse production qualification (2026-10-23)

**Draft; qualification is incomplete.** The all-Core rerun passed 1,543 tests across 71 binaries with zero skips and no `LEAK` annotations. A check of all CLI test compilation contexts also passed. The restarted broad/UI, strict, media, native, portable, and five-pair timing checks are not complete. Post-qualification is waiting on all eight qualification steps. No release app hash, emitted-movie result, timing result, or final evidence inventory is available yet.

## Change and scope

The production optimization reuses the already-proved pure-leaf validation/application result while replaying one eligible ordinary InsertTime leaf. The reused result avoids repeating that pure leaf work; the normal compound traversal and authoritative state checks still run. The optimization does not claim that broad document validation is rerun. Core remains responsible for applying the command. Store still checks current authority and admits/publishes through its immediate transaction callback. Charging, wire/revision checks, TLS/ledger behavior, final net-state/history accounting, and Store current-state authority remain in their existing path. No schema changes are reported: Core 58 and SQLite 87.

The production implementation is the same 14-file Core/Store overlay reviewed in the private prototype. Production qualification adds three test-only path changes: two files containing the positive cached-versus-uncached CLI media test and its shared fixture, plus one existing Store unit-test reference call updated for the new optional argument. The source path set remains 2,053 Rust files against the 2,051-file baseline `3a32c923358c0cb7ca53f821bfb6db3e01ebb466`; six pinned inputs remain unchanged. This qualification has not yet produced a final source commit.

The positive media test compares cached and uncached commits, exact canonical PCM, durable history, and verified emitted files. It has not yet completed in this qualification. The capability, report schema, export count, and claims must be taken from the final release receipt, not assumed from the earlier private prototype.

## Failures and test-only corrections

Two broad compile failures are retained as separate history. The first was E0061 at `crates/deadpan-store/src/compound/tests.rs:953`: an existing internal Store reference call lacked the new optional fifth argument. The test-only correction adds `None`; its independent review verifies one exact call-site replacement, unchanged assertions, and unchanged production files. The restarted broad suite must pass independently.

The second failure occurred because the positive CLI test was placed in a shared recipe included by other integration targets. Preview-export-only root references did not compile in those contexts. The test was refactored across two existing CLI test files: the shared file now provides a fixture helper, and the test lives in the target-local export module. The retained assertion map records all 62 original assertion macros and their multiplicities as preserved. The final refactor review and full restarted qualification are required before reporting it passed.

## Verification status

| Check | Current evidence |
|---|---|
| All-Core tests | 1,543 passed across 71 binaries; zero skipped; no `LEAK` annotations |
| CLI test compilation contexts | `cargo check --locked -p deadpan-cli --tests --features synthetic-worker` passed in 155.114 seconds; this was type checking, not test execution |
| Broader tests | Pending restarted qualification |
| UI-feature tests | Pending restarted qualification |
| Strict Core checks and formatting | Pending |
| UI Clippy and formatting | Pending |
| Debug/release neutral and aligned media | Pending |
| Positive cached/uncached real-media export, PCM and history comparison | Pending |
| Six native replay workflows and Kestrel checks | Pending |
| Two portable package checks | Pending |
| Final alternating quiet timing pairs | Pending |

## Timing and performance claims

The final production timing comparison is pending. When complete, report five alternating fresh control/candidate pairs with their actual release app identities, source/input guards, fixture identities, seven pause checks and one Kestrel check per trial. Keep medians diagnostic: no percentile or product-target pass follows from five pairs. The private prototype's 18.44% result and separate sampled-memory observations are separate evidence and must not be attributed to this production build.

The 100 ms fallback and 50 ms ordinary-edit targets remain open. Do not claim a latency target, memory ceiling, exact memory peak, physical-display timing, source-decoding performance, independent DSP, listening results, or ordinary Move PCM equivalence.

## Evidence package

**Pending.** The planned package is `tools/media-qualification/evidence/2026-10-23-one-leaf-replay-reuse/`. It must include actual bounded receipts, both compile-failure histories and their corrections, exact `.rs.txt` snapshots, source/input/binary identities, retained movies and independent verification receipts, both one-leaf portable package tarballs and checksum manifests, six native reports, and all ten timing trial reports/maps. The final report should state actual payload count, bytes, inventory SHA-256, and independent reread result only after root authorizes and verifies packaging.
