# Cursor-priority audio analysis, 2026-10-26

This is a partial DP-10.7 implementation and local qualification. It does not
close DP-10, delivery gates, accuracy or performance qualification.
The runtime remains an uncommitted development candidate. The checks exercised
the current shared Core/Plan/Audio tree, with exact source inventories and
unchanged guards; no pristine-HEAD comparison was performed.

## Implemented behavior

Native automatic transcription and detection-only speech activity now process
disjoint 24-second source ranges with 2.4 seconds of context on either side.
The maximum model input is 28.8 seconds. Ownership/context boundaries share
the exact 16 kHz centisecond, VAD and energy grids. Source audio retains its
measured origin/rate and the existing exact-phase preparation.

The latest visible Original or Edit picture PTS replaces the next-range
priority through one atomic slot. A running bounded range finishes before
retargeting. Coverage completes once per range. The partial rail shows measured
completed/total seconds and covered words; full search and editorial word/pause
objects require complete Workspace analysis. Corrections run off the UI and
partial display checks their version/error authority.

Real VAD and Whisper share one inference admission for each uncached range.
The slot releases after that range completes, so queued local refinement can
run before the next background range. Cached ranges invoke neither worker.
Complete publication retains existing session, Original and save guards and
does not change authored revisions.

Overlapping recognizer observations use bounded one-to-one lexical/timing
matching; disjoint repeated words remain separate. A word beginning before
the exact terminal sample can retain its rounded centisecond endpoint; a start
at/past measured source end grants no coverage. Detector and energy hops are
stitched only with complete actual global coverage, never invented silence.

The private AnalysisRanges cache uses descriptor-relative atomic publication,
0700 directories, 0600 records, checksums and bounded regular-file reads.
Keys include Original content/stream, origin/rate/extent, model and VAD hashes,
language, protocol, helper bytes, implementation and exact owned/context ranges.
Publication bounds are 8 MiB per record, 512 MiB total and 8,192 entries.
Damaged/unavailable caches rebuild and report failures without discarding
valid analysis. Opening AudioSession still decodes the full source stream.
This versioning belongs to the new range cache. Complete SQLite results retain
their existing TranscriptKey/SpeechActivityKey fields and save/read behavior;
historical annotations have not acquired new host-algorithm provenance.

## Review and automated checks

Independent review found and fixed terminal-word loss and VAD inference before
admission. Additional review fixed blocking FIFO helper admission and a partial
rail lookup that assumed word ends were monotonic. Source corruption/limits,
admission refusal, cancellation and terminal coverage have regressions.

- Current pure Analysis tests: six pass.
- CLI cache, coordinator and helper-admission tests: four pass.
- Native app binary filter: three new partial-rail tests and one existing
  decimal-entry test pass. The default app library is empty; its zero tests
  are not native verification.
- CLI/app all-target checks pass after retained SHA-format and example-type
  compile corrections.
- Strict all-target lint passes for Core, Plan, Audio, Analysis, CLI, app and
  Store with warnings denied. Its first two runs found only test construction
  lints, corrected without changing runtime behavior. The passing receipt is
  parallel-strict-third.json (9.214 seconds, unchanged source guard).

All recorded runs have unchanged relevant source guards. The receipt directory
is /tmp/deadpan-parallel-integration-2026-10-26. General Split audio work has its
own unrelated four preparation-limit failures; these are not hidden by this lane.

## Actual local-worker witness

The example qualify_incremental_analysis creates a fresh single-Original
project and runs the actual retained native helper with installed Whisper base.en
and approved pinned Silero v6.2.0. The fixture repeats repository Samantha
synthetic speech for scheduler/cache verification; it is not an accuracy corpus.

The first looped MP4 failed strict import before any model ran: repetition-seam
AAC frames declared 1,964/1,965 samples but contained only 1,024. The corrected
fixture resets video/audio timestamps before encoding. Independent inspection
finds contiguous audio frames with no duration exceeding physical samples.
Both fixtures, exact commands and the original typed failure are retained.
No decoder/admission limit was weakened.

The successful run took 48.03 seconds and records:

- First priority is tail range 1,152,000..1,153,151; cancellation follows its
  completed publication after 1,689 ms.
- Resume reads that range from cache, retargets to zero, then completes ranges
  0..384,000, 384,000..768,000 and 768,000..1,152,000.
- Three uncached inference admissions finish the resumed scan.
- The next run reads all four ranges from cache with zero worker admissions,
  reproduces transcript/activity exactly, and leaves authored revision unchanged.
- Standalone VAD reuses the same exact completed activity.
- The all-cached run still takes 9,904 ms; initial full decode and range
  preparation remain. No latency target or speedup is claimed.

The successful receipt is incremental-real-worker-second.json. Source, example,
helper, weights and fixture have unchanged guards before/after execution.
The [retained evidence](../../tools/model-qualification/evidence/2026-10-26-incremental-audio/inventory.json)
contains 40 hashed payloads, including original failed runs, exact runner/source
inventories, fixture commands and independent audio-frame metadata. Inventory
SHA-256: 8cf9a77eb0f5086c23501ddc2f93c9eb274633afb1ae3bb635b7e05e8ebaae6f.
An independent read-only audit verified the inventory, every compressed/expanded
hash and size, receipt guards, failure attribution and successful assertions.
Executed example SHA-256:
6304aad0d4423a4dccaa43efb1188542dc61b043ba6cd5ce9193642c7c1de48c.
Retained helper SHA-256:
bc4c6bc4ca54f8939a49de70434000811e2ef565f78ae46c22019929b6a8f4d3.
Whisper SHA-256:
a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002.
Silero SHA-256:
2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987.
Corrected fixture SHA-256:
647b6335f0182cda896b9218ca2e2b5582d29ce46120d813d2f03021bbe2ec9c.

The helper is the retained, previously qualified executable, not a claimed new
build from this development tree. Later lint-only test edits do not change the
runtime used by this witness. Models, source media and executables are not
committed as qualification assets.

## Remaining scope

Actual native keyboard/layout/partial-button and model-slot coexistence checks
remain. The CLI transcribe command still uses its existing whole-input path;
the new range API is used by the native app. Shot scheduling exists only as an
unwired draft, and the remaining tracking/refinement service integration is
unchanged. Full quality-corpus, long-media, performance and packaging acceptance
for this new integration remain. Keep all broader requirements in scope.
