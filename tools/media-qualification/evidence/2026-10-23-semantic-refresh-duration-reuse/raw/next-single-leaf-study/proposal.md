# One-leaf replay reuse feasibility

Read-only study, 2026-10-23. Reported main implementation baseline ae5eedd0, with concurrent tested semantic-refresh changes pending qualification/commit. No repository writes, Cargo, Git, runtime or profile work occurred here. Source hashes are recorded separately.

## Verdict

A bounded private prototype is feasible, conditional on preserving the exact replay insertion point, execution modes and leaf-entry TLS reset described below. Existing APIs do not currently carry a semantic result into Store replay; this is a small cross-crate capability addition, not a one-function optimization. It requires Core, Store and CLI changes plus tests. It does not require app command special cases or a generic caching system.

Do not claim a saving from the old roughly49ms inclusive leaf measurements. Those predate layout and semantic-refresh reuse. Current leaf time, equality cost, retention cost and peak memory are UNMEASURED. If root does not want the cross-crate API now, first measure current semantic leaf, replay leaf and their surrounding work using the existing isolated native producer; that is a defensible alternative.

## Narrow eligibility

Retain at most one result per SemanticPlan, created only by Core's own successful application:
- exactly one ResolvedStep::Edit, whose atomic command is Command::InsertTime;
- no register inputs/writes or Yank/Cut/Paste steps; no isolation collection;
- entry and leaf-result derived_clocks both empty;
- ordinary local command work AND compact timing representation at creation and use.
A plan that gains another resolved step drops this candidate; it does not retain a list or recreate it for a later leaf. Motion-only instructions may coexist because they do not change the authored input value. Do not admit InsertAiTime, occurrence edits, RestoreSnapshot, boundary replacement wrappers or any other command in the first experiment.

Eligibility can conservatively miss an optimization. A miss MUST follow the existing application path, not become a new user-visible error.

## Exact immutable binding

Use one opaque Core value, tentatively OneLeafReplay. Its fields are private and it has no public constructor, Deserialize, mutable accessor or Clone implementation. Bind:
- a complete owned immutable entry ProjectDocument value;
- the complete ordinary leaf CommandRequest, including project/expected/new revisions, Hold recipe, identities, timing and exact command payload;
- the complete successful next ProjectDocument at the LEAF revision.

No revision-only key, digest-only match, node-count shortcut or mutable SemanticPlan.document may substitute for full equality. Public SemanticPlan.request/document fields can be modified by callers, so the cached witnesses must be distinct private immutable values. Editing those public fields cannot update or authorize the cache.

Core's private creator should own the call to apply_with_result and seal only its successful output. Do not expose a crate-external method accepting arbitrary before/request/after or a caller-supplied validation proof. A private helper can return the ordinary edit and next value alongside the opaque candidate; keep its constructor private to that helper/module. The leaf EditTransaction need not be retained: replay currently discards it and recomputes the final aggregate transaction independently. Retaining its large forward/inverse bindings would add avoidable memory.

Expose only a read-only optional cache borrow from SemanticPlan. Keep the value in a private Option field. This does make external literal construction of SemanticPlan unavailable, but the exact search found its sole construction in Core planner.rs; the host wrapper already constructs it through Core.

A readonly optional borrow can flow through HostSemanticPlan::commit_compound_prepared without changing its &self receiver or the app callback. It may be used by preview and commit independently, because every use reruns Store authority. No global/thread-local cache, pointer lifetime trick, mutable prepared token or one-shot host side-effect is needed.

## Replay insertion point and first-error order

Place lookup ONLY where replay_compound_captured currently chooses apply_with_result (compound.rs:327), after:
1. outer command::check_revision;
2. Compound kind and transaction.validate;
3. leaf revisions differing from entry/outer;
4. initial document byte size and identity ledger construction;
5. exact step/value/capture/paste handling (eligible Edit has none);
6. Ledger::reserve(current,leaf.command);
7. pre-leaf exact wire size and cumulative staged byte charge.

After this prefix, require the exact one-Edit shape, eligibility modes, complete current == cached entry and leaf == cached request. Perform the ordinary leaf-entry exact-edit reset followed by check_revision, then obtain the sealed next clone and perform the ordinary post-reduction exact-edit consumption/sealed_preserved check described below. On any mismatch, run the existing apply path.

Nothing after the application is skipped: exact post-leaf size, cumulative work charge, Ledger::observe, host visit, current replacement, outer revision assignment, final size/work charge and command::net_transaction all stay in their original order. Final aggregate validation and inverse/history behavior are not replaced by a stored leaf edit. An isolation collector always uses the existing apply_with_isolation branch.

The matching cache was created only after the same pure leaf succeeded for those exact values in the same implementation/mode. Independent leaf-local count/depth/arithmetic/binding-work ceilings therefore have the same successful decision. Their local Work objects are not transaction-wide accumulators. Every transaction-wide mutable byte/fuel/identity/capture charge remains executed. If a future leaf adds a cumulative external budget or configurable limit, that becomes a new cache dependency and must disable this reuse until modeled.

Cache equality is non-authoritative and infallible. It must not be evaluated before the existing failure prefix merely to save work; doing so could change the first refusal or expose a huge unvalidated request to new work. Exact value comparisons are bounded by the already validated Compound/document/request structure.

## Hidden Core execution state

Exact document/request alone is NOT enough:
- command_work::REFERENCE changes local/refusal-equivalence execution. Require local() at creation and use. with_reference_command_work bypasses the cache.
- audio_binding_lifecycle::REFERENCE_REPRESENTATION deliberately keeps complete timing tables/reanchor steps and can change stored bytes. Require compact_representation() at both boundaries, and bypass reuse inside with_reference_timing_representation even when local() is true.
- derived_clock::SEALED changes allowed integral queries. Empty entry AND result derived clocks removes that dependence for the initial scope. Leave all derived-clock leaves uncached, including an insertion that creates them from an initially unclocked document.
- command_work::SCOPED_TIMING is set and restored internally by the InsertTime branch based on beat-sound capture. Its effective value is derived from the exact document/command, not an extra caller parameter. The normal apply helper restores its ambient value; a cache hit must not alter it.
- document validation SCOPES are address-specific immutable proofs. They can change how an identical successful check is computed but not its value/refusal for admitted identical inputs; the cache must not accept one of those raw pointers or external proof objects as its key.
- insert_time::retimed::EXACT_EDIT is destructive TLS. apply_with_durations_captured clears it before leaf check_revision (command.rs:1174), and ordinary successful postprocessing consumes it again (1781). A hit that simply returns next would fail to preserve that side effect. At the leaf boundary, clear it exactly as ordinary entry does, then call leaf check_revision. After obtaining the cached next clone, execute the ordinary exit expression: if !take_exact_edit() { derived_clock::sealed_preserved(current, &next)?; }. Both exact-edit consumers therefore run; sealed_preserved is its existing O(1) success for empty before/after derived clocks (derived_clock.rs:215). No exact flag is authored between them, and final state is false. This explicit entry/exit behavior is required, not an optional cleanup. Do not just clone next and return it.
- provisional capture counters and reused-owner counters are test/diagnostic observations, not authored authority. A hit intentionally executes fewer reducers; private profile expectations must reflect that. Do not fake those counts to resemble an application.

No Core I/O, time-dependent policy, host media lookup or random identity generation occurs in apply_with_result. All identities and recipes come from the request. Provider/media/analysis qualification remains in the host path, not in this cache.

## Store authority and plumbing

Store compound::prepare (386) must still prepare/read the bank, compare bank version and frozen inputs, and ensure every outer/leaf revision is unused. execute must still validate frozen register values and capture reachability, collect capture states and count entry bytes. Its visitor (478 onwards) must still call validate_transition for the exact before/after/leaf, validate Original paste where relevant, serialize captured checkpoints, charge processed/checkpoint bytes and record each step.

The cache cannot replace prepare_command/read_command_snapshot, boundary_transition::prepare, transcript/corrections guard checks, prepare_runtime_bank, command_plan request/edit serialization, historical decode/inverse checks, generation relevance, SQL writes, durability or readback. A cache hit is merely one pure operation inside execute.

Minimum safe threading:
- retain existing replay_compound and replay_compound_captured wrappers using no cache;
- add one narrow Core replay entry point taking Option<&OneLeafReplay> and the same visitor, sharing the same inner body;
- add an optional cache borrow through the existing Store preparation helpers down to compound::execute; existing public methods delegate with None;
- add one narrowly named Store prepared-commit entry point that accepts the opaque borrow and optional transcript guard, then delegates to the SAME Immediate-transaction body. Do not duplicate that body or bypass read_command_snapshot/base-command/boundary transition preparation;
- HostSemanticPlan calls that method with its private Core candidate. Existing ordinary serialized callers/history readers have no cache and retain the old path.

This crosses at least Core planner/compound/export, Store compound/lib and CLI macros. Review that whole boundary as one bounded change. App perform_macro should need no command fast path.

## Memory and lifetime

SharedMap currently backs document.nodes/audio_lineage and AudioBindingState.bindings/gap_bindings; clone shares Arc storage until mutation (shared_map.rs). FrozenAudioLayout is also immutable Arc storage. Thus retaining private before/after snapshots does not inherently deep-copy those large maps. The scalar leaf/outer revision difference can share all those maps. Cache comparison uses value equality, not pointer identity.

Other document collections still clone by value (assets, marks, overrides, targets, sounds, timings map keys and journals), and cache retention keeps pre-edit map generations alive longer. One result is a cardinality bound, not proof of unchanged peak RSS. Retain only one entry/request/next triple, no transaction patch, no bytes, no history snapshot list. Entry/result already passed exact document byte caps and structural limits, so retention is bounded by existing per-document limits, but its memory constant and COW peak are unmeasured.

The candidate belongs only to the returned successful SemanticPlan and drops with it. It must not enter a persisted document/history value, a global registry, a retry queue beyond the existing single pending plan, or the register bank. If exact cache retention or hit cloning exceeds the current memory target, reject this optimization rather than weaken bounds or add a generic cache eviction system.

## Smallest refuting test set

Use a standalone Core integration target, e.g. semantic_leaf_replay.rs, with existing test-support oracle; it avoids the large Core unit-test binary for the first probe. No public mutable test hooks are needed.

1. Eligible first interior InsertTime: plan once, assert opaque candidate exists, replay with it and replay with None against the same entry/request; compare complete outcome fields, exact before/after/leaf visitor values and final bytes. Apply forward/inverse aggregate patches and require exact restoration.
2. Same project/revision, different authored VALUE: change a Hold label/duration, mark or asset qualification via normal JSON ingress without changing revision. Cached replay must match uncached output/refusal and preserve that changed field, proving exact value matching rather than ID/pointer trust.
3. Change Hold duration/picture context, split pool, timing identity, leaf revision or outer revision/transaction shape. Compare exact errors, complete results and visitor call counts against uncached replay. Stale/wrong-project outer checks, identity-pool collision and transaction validation must still win before lookup.
4. Normal-created cache used inside with_reference_command_work and with_reference_timing_representation must bypass it and match that mode's uncached representation. Build a fixture where compact/uncompacted timing representations actually differ. Cache creation in those modes returns None.
5. Two leaves, Yank/Paste/Cut, occurrence/isolation, AI pause and input/result derived clocks are ineligible. Replay remains unchanged. Mutating public SemanticPlan fields cannot mutate the cached witnesses.
6. Host visitor rejects an otherwise valid cached result: exact host error and no returned outcome; the cached result cannot bypass the visitor.
7. Existing exact-edit stale-flag refusal/cleanup scenario must match subsequent ordinary execution after a cache hit. Prefer a real failed retimed leaf witness if available; do not add a public setter solely for a test.
8. Budget boundaries: initial/pre/post/final Core staged byte charges must still fail on their original sides, not be recomputed from a stored cache count. Compare cached/uncached visitor reachability and exact first errors. Existing captured programs must remain on the noncache path with checkpoint bytes unchanged.

Then targeted Store tests must prove head/bank/frozen input/allocation/corrections mismatch and media/analysis validation still refuse with no document/history/bank write; callback preparation failure and SQL failure remain atomic. Compare committed request/edit/history bytes and undo/redo/portable-copy behavior with uncached execution. Actual integration must include a fresh real-media pause, not just pure Holds.

## Smallest diagnostic measurement that could reject it

Use the CURRENT semantic-refresh-qualified source for a separate private copy, distinct target and executed binary hash. Retain the same native first-pause producer and matched control/off/on overhead check.

Record three disjoint operation groups in buffered private instrumentation: semantic leaf application; replay cache equality/lookup; replay actual leaf application (zero count on a hit, one on a miss). Keep existing outer service/publication/refresh success markers and final aggregate timing. A hit must retain all Core size/host visitor/final validation counts. Do not infer current costs from old Core24 values or subtract overlapping medians.

Measure both large first-pause and a mismatch/fallback case, plus owned-process peak memory during preparation/publication. Reject if exact equality+cloning/retention erases any benefit or violates the memory target. No gain, target, overhead or memory qualification is claimed by this design.

## Validation reused versus independent authority executed

The cache hit intentionally avoids executing the ordinary leaf a second time. Its opaque value reuses the successful pure decisions from that exact original call: picture-context/audio-gain request validation; pure sound grouping and command validation; identity/duration arithmetic within the reducer; binding capture, Split, phase/reanchor work, mark/lineage transforms; pruning/compaction and timed-basis rules; and complete leaf result/patch validation. These are not independently rerun merely because the result is cached. They are reusable only because Core itself applied that exact request to that exact complete immutable entry in the same allowed execution mode and sealed the successful complete result.

Independent current authority still executes on every use:
- Core outer/leaf revision checks, transaction structure, allocation Ledger reserve/observe;
- every initial, pre-leaf, post-leaf, captured and final cumulative document charge;
- explicit ordinary entry and exit exact-edit TLS behavior;
- every host visitor and final aggregate net_transaction validation;
- Store current head/identity, bank/frozen input, historical allocation uniqueness, capture/Original/media/analysis/transcript/corrections checks;
- fallible runtime-bank preparation before durable write;
- request/edit historical decoding, inverse checks, SQL durability, undo/redo and publication.

This distinction must remain explicit in the implementation review. A statement that the cache skips no checks would be false. It reuses one already successful pure call while preserving separately authoritative current checks.

## Refutation details before implementation approval

A Core cache can be stale even when its entry revision string matches. Test a normal public JSON-derived replacement entry with changed authored values but the SAME project/revision. Test a different cache produced by a different request on that same entry. Test swapped caches from different entries whose leaf request identifiers match. On all mismatches the old apply path must decide the result/error and visitor order.

For wrong outer project/revision and invalid transaction/identity pools, assert exact error and zero host visits, not only that an error occurred. For a valid same-value entry reconstructed through JSON at another address, require the same result as uncached replay; a pointer-only scheme would merely miss this case, while an ID-only scheme could accept the unsafe cases above.

Mode switching is bidirectional: normal-created candidate used under either reference oracle must bypass; attempts to create candidates while either reference mode is active must return None. After each attempt, run another ordinary uncached leaf and compare with the reference so TLS reset/restoration cannot silently affect later work.

The exact-edit stale-state test can live as a private unit test nested in insert_time/retimed.rs and directly set its private TLS before calling the public opaque-cache replay API. That needs no new public setter/hook. If an existing failed-retimed-leaf fixture exposes the same state, prefer that real witness. The standalone integration probe still covers all public mismatches/mode behavior first.

No implementation should start until root accepts this full cache contract and the chosen current-baseline timing/memory option. A smaller fallback is to reuse the prior isolated Core24 producer on CURRENT source with semantic_leaf_apply_call, replay_leaf_apply_call and final aggregate timing, plus owned peak memory. This establishes whether the second leaf is still material before adding cross-crate plumbing.

## Adversarial wire-equality follow-up

See wire-equality-conclusion.md and its separately pinned hashes. Current authored numeric fields are integer/rational, including Camera, captured framing, effects, gain and geometry. No retained IEEE signed-zero counterexample exists in the current eligible model. ExactRatio string "-0" normalizes through i128/gcd to canonical "0"/"1"; ordinary f64 occurrences are transient DSP/display/preflight calculations.

Full value Eq is sufficient for identical current compact bytes only for successfully serializable NONPROVISIONAL values using the same serializer. FrozenAudioLayout equality ignores provisional CompleteCharge, which affects serialization success, so retain all required canonical size traversals before matching and explicitly exclude provisional cache witnesses. No extra byte buffers are currently recommended. The follow-up records signed-zero/refusal/string-label tests, a future byte-witness fallback with64MiB entry cost, and a memory-based rejection option.

## Selected next action after wire audit

Current-baseline leaf timing and owned peak-memory diagnostics come FIRST, before cross-crate cache implementation. All proposed cache work remains design-only. The completed wire audit found no stored IEEE signed-zero field in the current eligible typed model, and documents the only custom equality caveat: provisional CompleteCharge affects serialization availability and must be excluded at creation and use. Existing mandatory successful size traversals precede candidate exposure/lookup; their errors must remain authoritative.
