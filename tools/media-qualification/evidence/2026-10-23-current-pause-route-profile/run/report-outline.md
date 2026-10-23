# Current first-pause route profile (draft outline)

Status: **planned diagnostic; results pending**. No current route measurements or candidate improvement are asserted here. The existing production qualification evidence remains a separate package and is not merged with this diagnostic.

## Question and measurement boundary

- Question: where time falls on the existing successful native one-leaf pause route.
- Timed interval: service-thread dispatch of the selected native Macro request through completed invalidation/reconciliation, `store_update`, and wake publication.
- Explicit exclusions: input/queue delay before dispatch, UI work before dispatch, later rendering/GPU work, decoder work after wake, and physical key-to-display latency.

## Diagnostic and phase accounting

The temporary opt-in emits a fixed 13-row, ordered, exclusive phase block: `native_admission`, `semantic_plan`, `native_continuation`, `store_entry_misc`, `store_prepare`, `host_runtime_bank`, `store_write`, `sqlite_commit`, `store_cache_publication`, `native_postcommit`, `workspace_refresh`, `native_finish`, `native_publication`.

Each row is expected once. The phase durations must sum exactly to the block's outer elapsed time; markers must show commit, refresh, and publication. Parent time is paused while child phases run, so rows are disjoint. Clock/helper overhead remains in the measured buckets and is not calibrated away. Semantic planning and workspace refresh remain coarse phases. This first pass does not establish attribution below those boundaries.

## Qualification identity and experiment

- Baseline source/input/executed-app identity: **PENDING terminal production qualification manifests**.
- Private diagnostic overlay identity and exact diff: **PENDING**.
- Control build identity: **PENDING exact qualified release build; plan currently names `0cb` as the next control, verify its full SHA from the terminal manifest**.
- Fixture and guard outcomes: **PENDING**.
- Trial design: at least three rotated, fresh-package trials each for uninstrumented control, overlay with profiling disabled, and overlay with profiling enabled. Report exact order, command receipts, source/input/app hashes, compiler-at-build-entry, hardware, load and power observations.
- Validate state parity and authored input equality before comparing durations. Exclude only the exact SQLite coordination sidecars authorized by the fixture guard; report WAL state explicitly.

## Results

**PENDING.** Include per-trial parsed phase durations and outer interval, then per-phase and outer distributions for each lane. Keep each lane separate. Do not pool runs or infer command identity from timestamps. Recompute all summary values from retained trial data. Explain that three or more diagnostic repetitions are not p95 evidence and do not close a latency target.

## Setup history and limitations

The initial private-copy setup missed pinned historical source paths because a broad evidence exclusion also filtered them. The mapping failure and its correction are recorded in `setup-history.json`; this was a setup/packaging issue, not a product-code or test failure. Preserve the initial attempt and corrected path list.

This diagnostic does not change the main checkout or establish a mainline patch, performance improvement, requirement completion, or gate closure. It makes no p95, 100 ms/50 ms target, memory, source-decode, physical-display, or PCM-performance claim. If owned-memory sampling is later performed, retain it as a separate matched lane and label whole-process sampled values as lower bounds, not exact peaks or a memory ceiling.

## Evidence

- New diagnostic evidence package: **PENDING**, with its final inventory path, file count, byte count, and SHA-256 recorded after independent reread.
- Existing production evidence package: separate, already retained at **995 files**; do not merge its inventory or claims into this report.
- Source/parser review and synthetic parser tests: **PENDING final exact identities and receipts**.
