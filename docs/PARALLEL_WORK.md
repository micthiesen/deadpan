# Parallel implementation workflow

Owner request, 2026-10-26: use the parallel skill to accelerate full-spec work
without reducing final quality or taking shortcuts. If it works, preserve the
workflow through compaction. This file records the current trial and its joins;
it does not establish that parallel execution is faster before verification.

Read /Users/michael/.agents/skills/parallel/SKILL.md when continuing the wave.

## Current wave

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
3. On PATCH READY, freeze that lane's sources for its integrated build and
   verification. Release the freeze explicitly for a fix or next milestone.
4. Main schedules Cargo against the shared target. Workers do not start
   competing builds. Do not create extra full target trees merely for parallelism.
5. Freeze only dependencies a check reads. Store depends on Analysis but not
   Audio or the CLI/app: pause Analysis briefly for Store checks while that
   worker continues independent CLI/app implementation.
6. Capture source guards for the stable lane and transitive dependencies.
   Concurrent unrelated writes are excluded explicitly. Final combined
   qualification requires the complete relevant source set stable and records
   the executed binary identity.
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

## Compaction continuation

The current integration checks are terminal. Recheck agents and tool handles
after compaction; recorded freezes are not live holds. Keep runtime source
stable while the primary retains evidence and commits the independent audio
analysis milestone. Actual native interaction/coexistence, quality, long-media
and packaging qualification remain. An unwired CLI shot scheduler draft exists
under shots/incremental; exclude it from the audio milestone. Agree Media/app
range interfaces and exact sequential-picture equivalence before linking it.
The Store primitive matrix requires sound-bearing WrapRetime lifecycle work
before its Retime fixture can reach Split; the established refusal must not be
bypassed. Four Audio work-limit failures remain. Main retains every full-spec
requirement and final acceptance decision.

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
