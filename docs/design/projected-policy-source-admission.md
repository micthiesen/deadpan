# Provider-aware Signal children and Tape/native admission

Original decision: qualify the private explicit-grid Signal-policy child kernel before the separate Tape factory, retaining ordinary readers and native routing through those prerequisites. Point's grid-specific entry is not a PreserveTape factory. The child, Tape Plan layer and selected routed native source-policy handoff are now qualified independently.

The [provider child checkpoint](../qualification/provider-policy-child-kernel-2026-10-31.md) qualifies the private child for checked public operands, including reachable repeated and definition metadata. The separate [Tape factory checkpoint](../qualification/tape-policy-factory-2026-10-31.md) now qualifies stored-grid traversal, child normalization, combined inventory and the independent exact final Signal Vec, with sealed Tape provenance through the existing custody protocol. Scope is source authority. The original decision and pins below remain historical; the Tape contract records its Plan boundary and the now-qualified selected native lifetime contract. The preceding [destination checkpoint](../qualification/projected-policy-destination-2026-10-31.md) funds the native destination copy. The [native source checkpoint](../qualification/native-tape-source-admission-2026-11-01.md) now qualifies actual query/inventory lifetime and exact final Signal Vec transfer for routed PreserveTape. Other preparation policies, graph/PCM/dependency construction, whole preparation and product Gates remain outside that unit.

## Standalone Signal prerequisite for intrinsic preflight

The [Signal factory checkpoint](../qualification/signal-policy-factory-2026-11-01.md)
qualifies public PointCeil policy and after-Preserve modes, whole-public empty
work, provider error ordering, once-forwarded work, final Vec custody and literal
resource/capacity differences. Point/Tape retain their sealed origins.
Its 39 frozen payloads and Plan protocol results do not establish native
preflight admission. Remaining stage_projection policies need actual ordinary
capture and native lifetime/account evidence; input policy must remain live
through subsequent graph query/recursive preflight in existing order.
Mix remains separate.

## Historical ownership gap before native source admission

The pre-J routed.rs:471 prepares and retains the actual PreparedBlock Arc, samples PCM, then calls suppress_signal with PreserveTape(projection.output_policy()). stages.rs:3128 constructs an ordinary AudioPolicyQuery, manually debits its work, preflights every content, checks control, validates and zeros the mask, and creates a fresh merged ordinary Vec. G then admits a destination copy with the real mask context. The policy query and its inventory have already dropped when G begins. G therefore proves ownership of that destination, not admission of the source policy inventory, query headers/spans, recursive work buffers, or original final Vec.

PreparedBlock retains PCM/dependencies/depth, not the policy query. Its warm memo can remain live while source policy is recomputed. Tape/Signal graphs and projection operands are preexisting borrowed inputs. OwnedHeap visitation measures those already constructed operands; it is not evidence their construction was admitted.

## Original Point-only constraints

At the original source pins, audio_point_policy_query.rs:210 called signal_policy at PointCeil with endpoints=true, while audio_policy_query.rs:529 used only structural_child_query and omitted independent Source output Hold carriers. The provider-aware child checkpoint closes that provider limitation; the Tape checkpoint separately closes traversal and aggregate storage.

Tape policy is different. audio_tape.rs:400 preserves the Tape's own grid boundary rule; after Preserve, direct run policy uses endpoints=false. A policy_override always recursively calls policy_after_preserve, even from ordinary tape.policy. Source voice input_signal() uses apply_holds=false; output_signal() uses apply_holds=true (audio_source_voice.rs:88-104). Projection scope validation checks the real owner/plan/definition/repeat relationship, not a Structural-only provider restriction. Rejecting accepted Source voices or silently substituting the root/Point signal would change the contract.

The qualified Tape factory reuses AdmittedSignalPolicyQuery and into_suppression: their exact typed Signal Vec, issuer and independent final lease do not depend on PointCeil. Construction remains private and retains all guards. No general lease extractor or caller-fabricated provenance is exposed. Point entry behavior/error strings stay intact; Tape carries its own sealed provenance.

## Original child implementation contract

Add an internal child operation accepting the actual borrowed AudioSignal, exact Signal range, explicit boundary rule and existing stop flags, plus the caller's real QueryBudget. It must return the existing guarded Signal query carrier and share the caller's issuer and accepted canonical/additional accounting. Reuse the Structural branch unchanged. Implement the other real provider branches from query_inner in order, rather than calling ordinary query_inner and charging its finished allocations afterward.

This is not a public generic query API or permission expansion. Its first consumer is the private admitted policy engine. Each callback/allocation must use the same established QueryBudget collection/header/content helpers; every nested owning field actually built must be guarded. The graph/recipe Arc supplied by the caller remains borrowed input, explicitly outside result-construction admission.

Preserve provider validation priority from audio_signal.rs:1069:
1. Structural delegates its existing path.
2. Other providers validate limits, reject RequiresOwnedInput, then compute the requested-rule grid and support boundaries and validate the range.
3. Projected constructs its current span/header query and reports literal work1, including an empty query. Ordinary policy then rejects a non-Leaf current span. Do not move that rejection ahead of the query's validation or actual work.
4. Source and Silence each spend their existing canonical1 before their actual span construction; span-limit checks precede each iteration. Preserve clipped and allocated samples, headers, lookup, source selection, and all nested values. An empty Source/Silence query does not acquire a nonempty provider iteration fee.

Next extend the private admitted signal_policy with Source output Hold handling. The ordinary path first completely resolves/observes current query, appends its ranges and inventory, then obtains the Structural Hold carrier query, observes its literal work/lookup, then charges1 per emitted Hold rule. Only explicit current silent Holds contribute; carrier Original content, endpoints, retained bindings and Hold issuer identities do not become this independent voice's inventory. Preserve this sequence and all errors.

Avoid using the existing structural_hold_carrier clone and calling it admitted: that clone can allocate owning fields before admission. A sealed borrowed provider-selection operation may query the same actual signal's structural carrier semantics without an owning clone, provided it reproduces the current checked query and exact rule semantics. If the implementation instead constructs the actual intermediate Hold rules, their issuer/header allocations also require genuine admission. Choosing a more direct policy-only extraction requires explicit ordinary-vs-admitted tests of complete query validation and canonical error/work priority; it must not start emitting Holds before a later provider query error would have occurred.

This child prerequisite stops here. It does not claim that Tape's run traversal, aggregate inventory, final Vec, override recursion, or native source policy has become admitted. There is no runtime integration until the actual child/provider proof is complete.

## Reviewed implementation constraints

Preserve empty provider behavior. Empty Source/Silence queries have canonical query work zero but still construct their real headers. Projected reports canonical query work one even when empty; an empty Projected query has no non-Leaf span, so policy must not reject it categorically. A nonempty Projected policy rejection remains late, after ordinary query validation and construction.

After finishing the current query and inventory, determine whether a Source output carrier exists before requesting its child work limit. Source input (`apply_holds=false`) must retain an otherwise valid exact-budget success. Source output still evaluates its carrier for an empty request, as ordinary code does.

Complete the carrier query before emitted-Hold fees or ranges. A sealed borrowed Source-output bridge may use the actual structural fields with false/false stop flags, avoiding an allocating provider clone. Restrict that bridge to `Source { apply_holds: true }`; keep the public Point/Structural entry unchanged.

## Qualified Tape factory and follow-on native contract

The qualified Tape factory reuses the closed guarded range/inventory machinery and final typed Signal carrier. Required literal order from the original audio_tape.rs:408-489:
- limits, sample_count, route-end boundary, full range validation precede traversal;
- each actual run_for binary-search comparison is canonical1; fetch/coverage/nonadvance checks then the existing per-run canonical1 and zero-remaining-work refusal;
- maximum_spans remains per logical provider query, not an aggregate inventory limit; aggregate canonical plus new additional work remains checked, and shared-budget forwarding never re-debits a child's already accepted work;
- direct children use Tape rule and the caller's endpoints mode; overrides recursively use after-Preserve;
- child work folds and checked lookup addition happen before collection extension/next cursor;
- final sorting, empty/reversed suppression filtering and overlap/adjacency merge retain literal values and typed Signal coordinates.

Signal policy recursion currently starts depth0 per provider query. Tape override recursion is a separate traversal with no existing policy depth fee/cap. Do not accidentally reuse one depth counter to introduce a new ordinary rejection or waive existing Signal Bound depth checks. Source reservations must remain live while extending destination inventory/ranges, and during sort/merge/final exact Vec allocation; each temporary's physical storage must drop before its guard. Preserve partial accepted work on refusal. No cap raises, canonical repricing, refunds, fake tickets, or ordinary intermediate policy Vec.

The final Signal Vec needs its own exact-capacity lease, separate from all temporary/inventory guards. During native consumption keep the actual policy inventory and all guards live through the content preflight, control check, full suppression-range validation and PCM zeroing, and through final exclusive handoff preparation/commit. The prepared Arc remains live as today. Work is manually debited only for an unforwarded ordinary result; accepted admitted canonical/additional work must not be debited again. A native adapter change should be separate from the child prerequisite and retain a literal legacy lane. Do not broaden every SignalInput or preflight caller in the same patch.

## Actual before and failure proof required

Freeze ordinary evidence before implementing the provider kernel. Use a bounded set, preserving rejected setups and coverage gaps rather than creating private providers:
- Reuse current checked G projected Source/Hold fixture: positive query615..768 and empty query0..256. Parent observed actual positive len/cap1, empty0, Prepared dependency1 and warm prepared residency768. This is Structural Tape evidence only.
- Build checked AudioSourceVoice through source_voice and compare its public input_signal versus output_signal in a valid Tape/projection. Qualify real source metadata/receipt and selection. Prove independent Source content and explicit current Hold behavior; carrier Original/bindings/endpoints must stay out of voice inventory. If this projection rejects a setup, retain that error and use an actual accepted Plan Tape child test, explicitly separate from native coverage.
- Use genuine public Silence/padding or Projected children only where public construction reaches them. Record actual rejection when unavailable. RequiresOwnedInput and non-Leaf Projected error ordering needs Plan coverage even if native public projection cannot reach it.
- Genuine intrinsic override via public Tape intrinsic construction, plus accepted fractional/negative grid geometry and current nested Bound/retime fixtures. Query full/crop/empty/outside/reversed and endpoint modes. No Point clock substitution or private enum mutation.
- Two or more real runs with a small per-provider span cap and a work boundary proving aggregate work/lookup behavior, plus a late child/provider error after earlier inventory acceptance. Keep exact ordinary canonical work and actual admitted additional work separately.

Capture complete borrowed values of the actual query, headers/spans/content inventory and suppression ranges; exact returned errors, lookup/canonical/additional/forwarded work; issuer and actual guard/live/peak receipts; caller/input state before and after. Existing complete borrowed serializers may be reused. No Debug-only substitute, destructive serialization clone, cap normalization or residual attribution. Drop each ordinary result before constructing its separate admitted lane. Preserve actual source/provider failures and input ownership scope.

For the later Tape+native unit add exact final pointer/len/cap custody proof, Box-only overlap with Plan guards, same-issuer rejection, actual reserve/work/cancel failures and held output after controller/ReadWork drop. Existing C custody primitives are evidence of that seam, not proof that a Tape factory constructs correct contents. Initial work-prefix witnesses must come from real stable requests; do not fill or alter accepted counters at a boundary.

## Explicit gaps

The Tape checkpoint closes the outer Plan factory prerequisite and changes no native acceptance. Native output-policy integration still leaves ordinary stage_projection input/output content preflight chunks, Tape/projection graph construction and remaps, other suppress_signal callers, PCM/dependency allocations and provider-owned storage outside its claim. The existing preparation phase and provider observation priority remain literal. No ordinary multi-output or Gate scope is added.

The original source-pinned design, navigation, exact pins and independent review remain in the destination checkpoint's evidence archive. That original design phase performed no implementation, builds, native runs or fixture regeneration. Subsequent child and Tape executions are recorded in their separate qualification reports above.

## Source pins

| Scope path | Bytes | SHA-256 |
|---|---:|---|
| crates/deadpan-plan/src/audio_tape.rs | 25405 | 8a2359dcf69be1eeb1961697b722032d8b36d3ab009adb3c2c3fe1ed8363540e |
| crates/deadpan-plan/src/audio_policy.rs | 19748 | 20bbf032dc337d536e4474ffe6e27b92903420b22ac8ebf83ffdd72a4ed38f5d |
| crates/deadpan-plan/src/audio_policy_query.rs | 29864 | ae9363b8043665a45336c2e577b60d1d963c92a2ef2cd297f6adced3109a5d06 |
| crates/deadpan-plan/src/audio_point_policy_query.rs | 12193 | 97f652b0d444fbeaca5adfdb018e7808277479737872a77929bb2fc620c14384 |
| crates/deadpan-plan/src/audio_signal.rs | 45296 | 7df9b5ac48ce2e31583df57cd3a560a616a8a0d54e38beeccee3be821133b335 |
| crates/deadpan-plan/src/audio_signal_processing_query.rs | 16722 | f15670ddc0e798ce146807bc1dde78adb89374ca782b60e6ff81fa13fdce4368 |
| crates/deadpan-plan/src/audio_source_voice.rs | 12294 | 1fbc5f2d0384103357eecf701b150691db8e64063113fa99ccd97cb07e0833a5 |
| crates/deadpan-plan/src/audio_hold_policy.rs | 7467 | 26e6605813ae97f8c836a5cb339808d4012372a06610ab66331fc16ad5fbb4bd |
| crates/deadpan-plan/src/audio_projection.rs | 14899 | eba65cf5e76c3e62ee0f48d0ca137bc0f0892ef18050db264e7d4018322dcbbf |
| crates/deadpan-audio/src/routed.rs | 22285 | 4d69e36d86973816c6dae24435f33a8de8f8e2dc29bf498f6d2d49cceb9e504f |
| crates/deadpan-audio/src/stages.rs | 167903 | 4c7d47c425596f6a60c6431de15dd9c7536fcb7dd8fd14e252cb2619923b1ad9 |
| crates/deadpan-audio/src/signal_input.rs | 2656 | e1b9782f8730a19ce2fc46e393033f642d666d6109466b045146d747b8a08085 |
| crates/deadpan-audio/src/stage_projection.rs | 12320 | a5f08ab39c0f3b24538f69d0ce8560bc00544ca262fb6340bd92298a9a03f2c6 |
| crates/deadpan-audio/src/sound_context_admission/policy_mask_handoff.rs | 5369 | a0e4f6e4bc77038a05e5f67d5ac3962f8d99899d3c7d946517dd37a8bbddc822 |
| crates/deadpan-audio/src/point_suppression/policy_handoff.rs | 3819 | f336809ca102625d4033a0fa2bbfc05a45f51031b2003d7a4b8524586d3de821 |
| crates/deadpan-audio/src/routed_projected_mask_tests.rs | 16934 | 7fdce74f575b9d9fde4884bb4b52f1a0e156fb253566266611e85b64bce7dc4a |
| crates/deadpan-audio/src/projected_mask_before_trace.rs | 3760 | a088ef82214b1c3595fc911840e8c8153b7f16202643832357dc8adb59ffc328 |
