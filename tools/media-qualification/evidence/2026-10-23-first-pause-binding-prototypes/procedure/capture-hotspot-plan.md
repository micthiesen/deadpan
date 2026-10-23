# Next first-pause profile: scoped frozen-layout complete-wire pass

Read-only plan, 2026-10-22. No code/build/runtime was changed or run.

## Evidence and scope

The retained pre-compact profile reports first-pause `Capture for insertion` median 18.793 ms, including initial physical-owner lattice capture and layout work (`docs/qualification/first-pause-profile-2026-10-23.md:58-68`). The uninstrumented compact-edge prototype's first-pause median is 111.362 ms versus 127.513 ms baseline, still above the 100 ms bound; its phase attribution is explicitly not applicable (`.../first-pause-profile-2026-10-23.md:105-120`). Thus the prior measurement establishes the parent phase as a plausible area to remeasure, not that capture still dominates after compaction.

## Narrow candidate to isolate

Instrument the complete-wire-size/count pass inside `FrozenAudioLayout::capture_scoped` (`crates/deadpan-core/src/audio_reference.rs:652-690`), with specific attention to `WireCount::node` at line 676. This pass deliberately walks every current document node even when the retained provisional layout is a small slice, because it validates every frozen node and proves the complete layout's JSON byte bound before slicing. It also creates `HashMap` entries for node durations/parents and counts the exact wire form. Do not omit this full-lattice work or its exact bound.

One safe hypothesis to test after measuring this phase is reducing temporary allocation in the byte counter: `WireCount::node` currently calls `frozen.duration.frames().to_string().len()` once per node (`audio_reference.rs:2788-2790`). A digit-count helper could return the same decimal width without allocating. This is only a candidate; the current profile does not isolate it, and no measured dominance is claimed. The counter's exact match to serialization (`audio_reference.rs:2753-2757`, test at 3080) remains mandatory.

## Non-overlapping timer anchors

Use monotonic `Instant` timers, one elapsed value per bracket, and print only after the measured call. Keep the existing outer `capture_for_insertion_scoped` timer for comparison. Add nested, disjoint timers in `capture_scoped`:

1. Precharge: immediately around `Self::precharge(document)` at `audio_reference.rs:640`; its work is the full structural edge/run walk (`575-614`).
2. Duration acquisition: `644-647` (usually retained durations, otherwise `document.durations()`).
3. Complete scan: `654-690`, from allocation/initialization of `info` and `WireCount` through `wire.layout` and byte-limit decision. If a second diagnostic resolution is warranted, time only `wire.node(...)` at line 676 into one accumulator; this requires one extra clock read per visited node and must be reported as high-overhead attribution, never compared as ordinary phase time without an overhead control. Keep the primary coarse complete-scan measurement as the low-overhead evidence.
4. Required-ancestor closure: `691-704`.
5. Provisional slice construction: `705-789`, including `retained_overrides` and the `admit_sized(..., false)` call.
6. Inside `admit_sized`, separately bracket only `layout.build_index()` (`audio_reference.rs:914`) if needed; the caller timer otherwise groups it with slice construction.
7. Existing debug-only complete-capture/equivalence oracles (`797-819`) must be a separate timer or excluded from the release comparison; do not remove them to improve the number.

At the call site, keep `capture.walk` distinct from layout capture (`audio_binding_lifecycle.rs:291-296,347-375`) so the retained `Capture for insertion` phase can be reconciled as placement-entry charging + reducer lattice capture + scoped/full frozen-layout capture. `capture_with_placements` first calls `document.validate()` (`263-296`); report whether its retained-duration fast path applies rather than counting it as duration recomputation. The operation may fall back to complete `FrozenAudioLayout::capture` at `audio_binding_lifecycle.rs:370-375`, which must get its own timer/call label; do not combine that fallback with the scoped-slice result.

## Falsification and limits

Use the same fresh 10,000-Hold package/profile path, source and executed-binary identities, load guard, and baseline/candidate pairing root already uses. First establish whether the complete-scan timer remains material after edge omission. If it is not a meaningful share of the current capture phase, reject this candidate without pursuing the digit-count change. If it is material, compare a future scratch-only implementation of nonallocating decimal-width counting against the current exact `to_string().len()` counter; require unchanged `WireCount` equality against actual compact serialization, existing scoped-capture-vs-full-capture equivalence, exact layout decode/round-trip, and malformed/limit refusal behavior. Do not relax `precharge`, node validation, full wire bound, requested-alias checks, inverse restoration, replay, or durability.

Timer instrumentation is not free. The primary phase brackets add only a bounded number of clock reads and delayed stderr writes, but they perturb short phase latencies. The per-node timer is materially more intrusive and should be treated as diagnostic only, with a separately recorded instrumented-vs-uninstrumented paired delta. Five pairs remain short diagnostics, not p95 qualification and not DP-24 PASS evidence.
