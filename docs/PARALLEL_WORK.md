# Parallel implementation workflow

Owner request, 2026-10-26: use the parallel skill to accelerate full-spec work
without reducing final quality or taking shortcuts. If it works, preserve the
workflow through compaction. This file records the current trial and its joins;
it does not establish that parallel execution is faster before verification.

Read /Users/michael/.agents/skills/parallel/SKILL.md when continuing the wave.

For compiler scheduling, use [Fast local Rust feedback](BUILD_FEEDBACK.md).
Routine pure-logic loops keep incremental compilation enabled and avoid the
qualification optimization override. Maintain the actual selected target;
the earlier hardcoded `./target` hygiene missed the external 103 GiB target.
Prefer Blaze for the tested repeated `deadpan-core --lib` body-edit loop.
Plan, integration tests and app/media stay on Cargo; the native JIT paths
panic. The primary schedules both engines, and workers request checks rather
than starting competing compiler jobs. Preserve distinct engine targets and
source attribution. Reuse the daemon during active Core iteration and stop it
when that work ends; see the guide for maintaining its separate cache.

## Retained operating pattern

Use up to three disjoint workers plus the primary, only while independent work
exists. The primary owns architecture, shared interfaces, Cargo scheduling,
integration, evidence, commits and progress. Pair independent implementation
with review across lanes. Do not replace failed acceptance checks with review
judgment, or keep workers busy merely to fill slots.

After compaction, read this file and the newest qualification record, inspect
the actual diff, and reconcile agent/process status before dispatching work.
Quote the owner's authorization and give each worker exact files, invariants,
acceptance checks, scratch path and explicit external-action limits. A compacted
agent list, PATCH READY message or historical freeze is not current execution.

The later wave split native transcript/VAD and shot integration, exact Audio
input indexing, and Core Retime boundary proofs. The primary kept native focus,
layout and replay changes, Store joins and evidence. Reviews crossed those
owners. Native word identity and whole-analysis shot limits both needed fixes
after the implementation looked ready. Keep these acceptance joins.

## First-wave ownership (historical)

There are four harness execution slots. Use three independent workers and keep
the primary session on integration, cross-cutting decisions and additional
independent verification. Do not manufacture more lanes than independent work.

| Owner | Responsibility | Write ownership |
| --- | --- | --- |
| constant_gain_program | Compile complete constant route gains at existing DSP boundaries; retain dynamic fallback | Audio authored_gain.rs and its tests, shared_components.rs and new shared_components modules |
| split_pcm_provider | Canonicalize complete proved sound families within one Graph | Audio shared_eq.rs, new shared_eq/permission_families modules; Graph test initializer only in original_inputs_tests.rs |
| incremental_analysis | Actual cursor-priority incremental analysis, first transcript/VAD integration, then remaining services | Analysis incremental modules and lib.rs export; CLI transcription incremental modules and minimal transcription.rs wiring; app preview transcript/activity wiring |
| Primary session | Integrate, review evidence, run shared builds, maintain progress; independently qualify Store Split history | Store test sound_clocks/general_splits.rs and its module declaration; unassigned shared interfaces; docs, artifacts, Git |

These names are conversation agents, not services. Recheck actual agent status
and tool handles before treating any worker or process as live after compaction.
A recorded hold is not an active hold. Explicitly reconcile ownership changes.

All agents share this checkout. Preserve concurrent changes, including the
foreign fuzz/Cargo.lock and .claude/ entries. No agent-managed worktrees,
worker commits/pushes, whole-tree formatters, new dependencies or shared config
changes. Format only owned files. Main owns commits and full-tree checks.

## Join rules

1. Give each worker relevant normative scope, exact files, invariants,
   acceptance checks, scratch location and external-action limits.
2. Agree interface changes before editing another owner's file. A worker may
   implement private modules directly within assigned ownership.
3. On PATCH READY, freeze that lane's sources for its integrated build. Release
   the freeze explicitly for a fix or next milestone. For executable checks,
   copy the successful binary before another Cargo build can replace it. Pin
   that copy, its source-guarded build receipt and actual runtime resources.
   Then unrelated source may advance during execution. Do not attribute later
   source edits to an older executable.
4. Main schedules Cargo against the shared target. Workers do not start
   competing builds. Do not create extra full target trees merely for parallelism.
5. Freeze only dependencies a check reads. Store depends on Analysis but not
   Audio or the CLI/app: pause Analysis briefly for Store checks while that
   worker continues independent CLI/app implementation.
6. Capture source guards for the stable lane and transitive dependencies.
   Concurrent unrelated writes are excluded explicitly. Final compilation and
   lint require the complete relevant source set stable; runtime checks retain
   the immutable executable identity and its exact build-source attribution.
7. Review the integrated patch independently, fix findings, then run checks
   spanning the join. Audio includes exact WAV PCM, all-owner and selected/nested
   plays, unchanged work/memory limits and strict lint. Lane-local passes are
   provisional, not product completion.
8. Main reconciles reports, owns native/media qualification, and updates
   Requirements, handoff and progress artifact. Record remaining failures;
   award completion only against the fixed acceptance ledger.

No lane may raise budgets, omit effects, weaken PCM comparisons, bypass source
or revision admission, invent provenance, or label partial analysis complete
to obtain a green result. Browser interactions still require the browser role.

## Evidence so far

- Diagnostics completed two current failure traces, restored source byte-for-byte,
  retained a copied executable and left no process running. Raw Original memo
  hits are 65.5% selected and 70.8% root at failure snapshots; repeated complete
  gain programs remain the largest common work consumer. Exact traces:
  /tmp/deadpan-general-split-sounds-2026-10-25/worker-preparation-work-analysis-thirtysecond.json.
- Both audio implementation patches reached PATCH READY independently.
  Integrated compilation passed after an explicit module-path correction.
  Audio library checks pass 187 tests after replacing an invalid 24-layer test
  fixture with the unchanged legal MAX_GAIN_LAYERS limit (16); existing stage
  checks pass 201 tests. The separate full Split matrix remains three passes
  and four work-limit failures. Exact limits/effects/PCM comparisons are intact.
  Independent reviews of both patches found no concrete semantic issue.
  Receipts and immutable test binaries: /tmp/deadpan-parallel-audio-2026-10-26.
- Analysis supplied a compile-safe pure scheduler/stitching milestone and
  paused only that crate while continuing CLI/app wiring.
- The primary's first Store run compiled but failed two new tests. One fixture
  inserted a root BeatSound before an unsupported Repeat preparation; another
  asserted one Split proof representation for every primitive. Correct those
  assumptions to test normative Split behavior and retain the original failure.
  This is not a passing durability result. Receipt:
  /tmp/deadpan-parallel-store-2026-10-26/store-general-split-first.json.
- The second Store run also rejected proof-field assumptions and a fixture
  transformation inside an already retained processing scope. Fixtures now
  construct the primitive/Repeat first, then capture its prior clock; selected
  isolation is checked through the public exact proof and full history replay,
  rather than requiring one internal journal representation. The third and
  fourth runs pass the selected-play test, covering one and two enclosing
  Repeats through SQL rollback, full history replay, reopen, Undo/Redo and
  portable copies. The primitive-owner matrix still fails while preparing its
  Retime: WrapRetime reports "sound clock processing subtree changed". Keep
  this broader lifecycle gap open; do not claim complete all-owner durability.
- Independent analysis review found fractional-centisecond terminal word loss
  and VAD inference preceding memory admission. Both are fixed with regressions;
  helper-file admission and partial-rail ordering fixes also passed re-review.
  Combined checks pass six pure Analysis tests, four CLI tests and three new
  native partial-rail tests plus one existing decimal-entry regression. CLI/app
  all-target compilation and strict seven-crate all-target lint pass. The empty
  app library's zero tests do not count as native verification. Original compile
  and lint failures remain in the receipts.
- Actual Whisper base.en and Silero v6.2.0 cancellation/resume/cache verification
  passes in 48.03 seconds. Tail-first cancellation retains one completed range;
  resume uses its cache and runs three remaining admissions. All four ranges
  then load with zero inference admissions and identical transcript/activity,
  without an authored revision change. Source, fixture, weights, helper and
  example executable guards pass. Initial full audio decode remains; the warm
  scan takes 9,904 ms, so no latency target or speedup is claimed. The initial
  invalid AAC seam fixture and truthful import refusal remain recorded.
  See [audio-analysis qualification](qualification/incremental-audio-analysis-2026-10-26.md)
  and /tmp/deadpan-parallel-integration-2026-10-26. Synthetic speech proves the
  scheduler/cache behavior only; native interaction and accuracy remain open.

## Current continuation

The [later range integration record](qualification/incremental-ranges-2026-10-26.md)
supersedes the first wave's unwired-shot and pending-native state. Native shot
ranges now use exact Media range scans and private caching; the sequential CLI
scanner remains. The real BLAKE3-addressed Whisper/Silero witness passes with
zero inference admissions on four cached ranges. Initial full audio decoding
remains and no latency or quality target is established.

Current ownership is frozen implementation plus primary verification; recheck
the actual latest receipts before treating final checks as complete. The next
sound join still requires the complete sound-bearing WrapRetime/SetRetime
processing lifecycle. Core boundary proofs do not authorize command admission.
Four Audio work-limit failures remain despite the exact input index. Preserve
all limits, timing, source authority, individual support, effects and PCM
comparisons. Plan-owned immutable clock asset inventories are a proposed next
optimization, not implemented evidence. Keep tightly coupled Retime processing
program decisions with one owner, and use other lanes for independent services
or review. Main retains every full-spec requirement and final acceptance.

## Retention decision

Retain this pattern: three disjoint implementation owners, primary-owned shared
builds and integration, independent review, short dependency-scoped freezes,
and explicit acceptance joins. It has delivered independently reviewed patches,
passing combined regressions/strict lint and a real-worker analysis witness
while Store history qualification progressed independently. Review caught real
bugs before acceptance. No serial control run exists, so do not claim a measured
elapsed-time or token saving. Keep tightly coupled sound-clock/Retime decisions
with one owner; parallelize separate analysis services or independent reviews.
If coordination becomes the bottleneck, reduce or re-slice the wave. Do not
keep agents busy merely to fill slots. Update this file and its handoff link
when ownership, evidence, live state or the next join changes.

## Constant-block continuation, 2026-10-26

The resumed product wave used one read-only ownership explorer, one bounded
Audio production worker and one independent reviewer. The primary owned tests,
all Cargo scheduling, isolated six-file validation, the full inherited Split
matrix, diagnostics, source/binary pins, documentation and scoped Git delivery.
No additional implementation lane was needed for this tightly coupled work.

The [constant-block record](qualification/constant-sound-blocks-2026-10-26.md)
retains 193 current library passes, an independent overlay's 179 library/201
stage passes and strict four-crate all-target lint. The complete real-WAV Split
matrix improves to four pass/three work-limit failures; no full lifecycle or
performance claim follows. Source-only independent review covered the complete
six-file optimization. The main session fixed two range-loop lint findings
without changing arithmetic order. Other Core/Plan/Audio/Store work and foreign
files remain uncommitted. Use the smaller wave while the remaining work shares
sound-query ownership and preparation budgets.
