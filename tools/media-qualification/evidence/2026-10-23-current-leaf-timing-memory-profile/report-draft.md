# Current semantic leaf timing and owned-memory profile (2026-10-23)

**Status: PENDING Root packaging approval and final evidence validation.** The release build, observer smoke validation, and all 18 timing/memory trial rows are recorded in scratch. Package-level source, receipt, sample, and inventory checks have not run; this draft makes no timing or memory result claim. Do not publish it as a completed qualification.

## Purpose and scope

This is a temporary diagnostic overlay on the qualified production baseline `3a32c923358c0cb7ca53f821bfb6db3e01ebb466`. It adds four bounded profile rows around the native service route and the two disjoint `apply_with_result` calls used by semantic execution and replay. A zero-time success marker confirms refresh completed. The instrumentation does not change the production algorithm, Store behavior, command result, cache, validation, authority, or publication path. The overlay changes six Rust files and adds `crates/deadpan-core/src/leaf_profile.rs`; the frozen Rust path set is 2,052 versus the baseline's 2,051. Six qualified inputs remain fixed. Core 58 / SQLite 87 are unchanged.

The four profile phases are inclusive app-service, semantic leaf apply, replay leaf apply, and successful-refresh marker. Only the first three consume time; the semantic and replay calls are disjoint children of the outer service interval. Their per-trial sum may be subtracted from the same trial's outer interval as a residual for other service work, cleanup, and instrumentation overhead. No median subtraction, overlapping-span addition, or attribution to a finer phase is valid.

Only the private-on candidate run sets `DEADPAN_CURRENT_LEAF_PROFILE=1`. Control, private-off, and the owned-observer memory control/off runs must emit no profile block. The planned study has three rotated fresh-process timing triplets followed by three separate owned-observer memory triplets: nine trials per lane, 18 total. Each replay requires seven pause checks and one Kestrel check on a fresh 10,000-Hold Background/Silence package.

## Build and source identity

The qualified control app SHA-256 is `a9ce60a4d227e8c8107b06602915e6323e0cfabbf12610b67a85eda77bdd4a0f`. The private release app SHA-256 is `14ef183594c33011c7557c8affd0bf516d24a55a9994d42a8d0648deb01e6ede`. The separate memory observer SHA-256 is `d1166aef7e1b30c787bb39fa536c18aee9af9da4f2327120d321432205fc0ed2`. Qualified media/transcription/tracking helpers are reused, not freshly rebuilt. Final source and observer maps and all six inputs must match their exact frozen hashes at packaging time.

Formatting and release app build completed successfully. The first observer link attempt failed; the resumed thin-LTO link succeeded against the retained `deadpan_native_process` rlib. Five resumed observer smoke cases pass for natural exit, setup failure, spawn failure, timeout, and injected sample refusal. Ten parser synthetic tests pass, and eight measurement-driver negative guard cases pass. The first observer-link failure, smoke-harness correction attempts, and resumed smoke records remain separately retained as history.

## Timing comparison

**PENDING.** The final lane must contain three trials each for qualified control, profile-off candidate, and profile-on candidate. Report raw per-run results and recomputed min/median/max only. This sample is diagnostic; it does not establish a percentile or close the 100 ms fallback or 50 ms ordinary-edit target. The profile reports call-path durations after the user-lane operation, but queue wait is unmeasured. Instrumentation overhead is unqualified.

| Median | Control | Profile off | Profile on |
|---|---:|---:|---:|
| Input to commit | **PENDING** | **PENDING** | **PENDING** |
| Input to offscreen Background composition | **PENDING** | **PENDING** | **PENDING** |

Per-phase rows and the same-trial `outer - semantic leaf - replay leaf` residual are **PENDING**. The residual includes other service work, cleanup, and instrumentation overhead; it is not an individual component's cost.

## Owned-memory comparison

**PENDING.** This is a second set of fresh control/off/on triplets with the owned-child observer active. It must remain separate from the unobserved timing lane. The observer samples the main app's `phys_footprint` and resident size over its whole replay lifetime; startup and later app work are included, and brief peaks can be missed. These are sampled lower bounds, not exact peak, per-leaf memory, a memory ceiling, or worker/GPU memory.

| Sampled whole-app maximum | Control | Profile off | Profile on |
|---|---:|---:|---:|
| Physical footprint | **PENDING** | **PENDING** | **PENDING** |
| Resident size | **PENDING** | **PENDING** | **PENDING** |

Final package validation must recompute each receipt's sample count, monotonic sample clocks, maximum gap, query sum, and footprint/RSS maxima from the retained raw samples. Group teardown and sole reap must be confirmed in every memory trial. Do not pool observer-lane latency values into the timing comparison.

## Final gates and evidence

**PENDING validation:** the 18 trial rows and 54 make/run/parse commands; source, binary, observer and six-input guards; authored-path equality except exact SQLite coordination sidecars with empty WAL; six profile-on blocks; all scenario checks; both recomputed summaries; Root's packaging GO; and the final inventory count, bytes, and SHA-256. The evidence package excludes binaries, source archives, targets, generated databases, and fixture packages.

This diagnostic does not qualify instrumentation overhead, ordinary playback, source decode, PCM performance, physical display, exact peak memory, per-leaf memory, or any product latency target.
