# Non-separate SharedMix suppression ownership, 2026-10-31

The non-separate SharedMix producer now retains its actual permission grid
through destination admission and disposal, then returns an owned AudioSample
suppression mask. The existing source reservation remains live inside
read_shared_program. WideBlock carries the opaque owner into read_shared_input;
it no longer unwraps this producer into an ordinary mask.

This closes one derived native mask producer. Separate Components, projected
PreserveTape/Plan policy, exhausted vectors, PCM/dependency storage and whole
preparation remain outside this claim. No DP requirement or Gate acceptance
changes. General sound-bearing Split remains disabled.

## Production behavior

SharedMix::finish moves its exact final Vec<Permissions> into a pending carrier
after the existing numeric work. It does not copy or reconstruct permissions.
The caller keeps the existing source reservation through equalizer callbacks,
the depth check, projection and physical disposal, inside the old result
closure and before its active_frames release tail.

Projection checks each actual bounded permission entry and bills an additional
Scan. It counts precisely the empty entries, admits that many AudioSample range
slots and the future Box, then independently scans and emits the old absolute
singleton intervals in order. It finishes and merges the same funded vector.
The source grid is explicitly dropped after the bounded result closure on both
success and error. Empty output performs its scan without creating destination
storage or initializing a destination account.

The source reservation remains the existing frames*12 plus mix.resident_frames
after unchanged prediction retirement. For this non-separate path, the latter
includes (buses+1)*frames*(2+WORDS), including the fixed permission bitsets.
These are observations of existing funding, not a new dedicated source lease.
No old reservation multiplier, canonical query weight, production cap or refund
changes. Cached WideBlocks still contain empty masks; their existing PCM-only
reservation formulas remain unchanged.

The new private WideBlock has the same shallow size/alignment as a reconstructed
instance with the exact four old field types. This is a source-layout test,
not an old-binary experiment or a claim that all preparation allocations are
now accounted.

## Immutable before and complete outputs

The 33rd oracle was frozen before this implementation from the independently
qualified published Root checkpoint 6458e9b989172147e0844fcb251c89ce8c479aaf:
35,440,251 canonical bytes, SHA-256
596d738a53f6ef4cbf7870d74b3025504f6289f735ea91f4a38ed45c6661f44a.
The E-only replay retains Point policy and Root ownership. Older literal replay
composition independently reproduces all four preceding complete bodies.

The final current body is 35,593,716 canonical bytes, SHA-256
9d6f3c67d9d611f46e75360006845a1b9419b30168a983db1ca9e4cf79c8be0a.
Three runs per build agree exactly across Main and the isolated shipping copy.
All 744 complete native outputs, including 264 successes and 480 errors, remain
exact against the Root before body. All 42 output fields across the ten internal
and four transfer cases remain exact, including PCM bits, masks/capacities,
dependencies, depth and exhausted data. No invalid flag or error was normalized.

Six account fields change literally. The two 514-frame aggregate cases retain
522 frames after local work and controller drop, formerly 3. The held-public-mask
aggregate retains 529, formerly 10. Each now retains the additional 519-frame
payload of its returned 257-range mask. Actual mask-drop observations remain
separate from the observer's retained account shell.

The first pre-observer captures are retained separately: 35,440,551 bytes,
SHA-256 3af013356e4b941d89973a015db7cfe7c36b6ff3a5ebeb4be106af0c60b184cf.
They establish the core implementation comparison, not producer-local receipts.

## Native proof and observation

Three actual checked public graph tests use the real WAV/provider and
read_shared_input. The alternating 514-frame fixture reaches real 256/256/2 producer
chunks, compares the complete result with E-only legacy and retains the returned
owner beyond provider/controller/work disposal. It checks actual source and
destination identity during admission, physical source disposal before the
reservation tail, final capacity and actual payload release.

A Source-only 64-frame request reaches zero emission and creates no destination storage
or reservation. A measured work-prefix request admits the real destination and
emits a partial result before a subsequent actual Scan refuses. Its seed includes
the measured reserve's work in both lanes, including native account and host
costs. It verifies the typed Limit, source/request cleanup, no returned block and
the initialized account shell after unwind. No source ledger, admission or
failure was fabricated. These are not forced cancellation, residency refusal
or numeric-callback failure tests for this producer.

Every helper request asserts sound_context_frames==0 immediately after return.
Success, zero and partial observations reject unfinished/current rows and both
truncation flags. The partial case requires the actual account observer after
accepted destination admission.

Producer receipts are test-only and default off. They record actual Vec
length/capacity, live caller reservation/active frames, actual reserve boundaries,
real work counters in both lanes, source disposal and destination custody.
Pointer integers remain private; only equality/distinctness results serialize.
The existing spend hook performs its original debit exactly once. The closed
projection has no recursive SharedMix callback. These producer intervals remain
independent of whole-invocation work and inclusive query intervals.

The 477 completed query intervals still reconcile in both lanes. One real
resident-quota partial remains partial; there are no foreign, open, dropped,
misnested or overflow intervals. Completed inclusive rows are never summed into
a global fee or used to reprice canonical queries.

Independent capture review confirms 758 enabled, finished, untruncated producer
snapshots. Nine rows contain actual positive producers, three chunks in each of
the two aggregate cases and the held-public-mask case. Every row retains its
source reservation at real destination admission and after source disposal,
before the outer tail. Actual destination capacity, owned pointer custody and
source/destination distinction hold. Per-row work reconciles with native phase
units and actual reserve work, avoiding double counting of Account work.
The remaining snapshots are valid empty traces. Zero and partial-failure paths
are separately executed assertion tests, not serialized positive-row coverage.
The review does not establish destination-admission refusal.

## Verification and retained failures

All 21 Audio targets pass in both variants: 1,125 Main and 1,119 isolated tests,
23 ignored manual capture entrypoints. Main's seven pre-existing unfinished
general Split tests remain filtered. Final Audio libraries pass 544/538.
Every final test binary was rebuilt, pinned and executed; no stale test run
substitutes for changed code.

Main cargo xtask check --no-test, isolated Audio all-target Clippy with warnings
denied, both formatting and hygiene checks pass. Audio rustdoc passes with zero
executable examples. The native builds use optimized dev level 2, debug assertions
and overflow checks, with incremental compilation disabled. Qualified FFmpeg
8.0.3 and ORT 1.31.0 prefixes and actual compiler-artifact identities are retained.

An initial strict check rejected an unnecessary clippy::ptr_arg expectation.
Vec-specific capacity access produces no lint; the expectation was removed.
The reserve observer was also made to return before borrowing work or allocating
JSON unless an actual producer row is active. Independent review found three
missing cleanup assertions; these were added before the final build/matrix.
These corrections and their earlier passing/failing runs are retained without
claiming that early tests executed later assertions.

Production-candidate and integrated observer reviews both conclude GO. The final
source audit preserves all 33 immutable payloads, Main's 2,909 and the isolated
copy's 2,581 other initial named inputs. Main's inherited shared_eq.rs difference
is preserved by rebasing the exact isolated patch onto unchanged Main regions.
The isolated copy remains the shipping authority.

Evidence: [compressed inventory](../../tools/media-qualification/evidence/2026-10-31-shared-mix-mask-ownership/inventory.json).
Inventory SHA-256: 55eb0e15e12dce7e94201673fc44aaa02740e9d350e67b0eef4353a9fefcb085.
Its 481 records retain 3,114,737,519 expanded bytes in 309,782,282 stored bytes.
They include compiler/run/source guards, all capture bodies and immutable
before proof, native assertion sources/runs, independent reviews and shipping
source. Full Main snapshots, broad Main maps and pinned executable copies are
excluded. Every archived input was rechecked against its current file after
compression. The Components contract/review is explicitly prospective evidence.

The remaining producer recommendation is single-output Components. Unlike this
producer, it has no final permission Vec to move; any flags scratch must be
separately admitted and record the actual post-finite, post-PCM suppression
decisions. Projected PreserveTape policy is a distinct later scope.
