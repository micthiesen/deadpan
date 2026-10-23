# Exact equality and signed-zero audit

Read-only source audit, 2026-10-23. No code execution, Cargo, runtime or Git. This conclusion applies to the pinned current Core model and the proposed eligible one-InsertTime cache, not arbitrary future types or original JSON spellings.

## Conclusion

For successfully serializable, nonprovisional current authored ProjectDocument values and typed eligible InsertTime CommandRequest values, their current PartialEq equality implies identical compact bytes from the SAME current serde_json serializer. No legal retained IEEE signed-zero counterexample was found: these authored models contain no f32/f64 fields.

The tentative no-bytes cache is therefore defensible for this pinned scope. No pointer-only map witnesses, digest authority or extra serialized cache buffers are needed now. This is a source proof plus a required regression obligation, not a test result. The cache memory qualification described in the main proposal still remains owed.

## Floating-field evidence

An exact-token search of all Core Rust sources found the entire f32/f64 inventory:
- audio_gain.rs:498-502 computes DSP drive_factor/shape using f64. The authored recipe is SoftSaturation { drive: GainDb }; GainDb stores i32 millidecibels (78-118), never a floating value.
- semantic/gags.rs:166,204 converts ExactRatio/gain to floating decimal for display formatting. The result is not a stored authored numeric field.
- compound/wire.rs:97 temporarily accepts a generic JSON f64 during bounded preflight; typed CommandRequest/AtomicCommand decoding follows. That generic Value is not stored in ProjectDocument or CommandRequest.
- edit_slice/wire.rs:60 and audio_reference/preflight.rs:204 reject floating scalars.
- remaining occurrences are tests, including SharedMap's intentional NaN equality test.

FramingPose stores three ExactRatio values, not floats (framing.rs:55-59). CapturedCanvas layers store Option<FramingPose>, dimensions u32 and a fit enum (picture_context.rs:33-42). GainDb stores i32 and serializes through that integer type. EQ cutoff is u16; treatment gains remain GainDb. Source/frame/sample geometry and clocks are integral values or exact ratios.

ExactRatio (exact.rs:9-38,53-64) stores private i128 numerator/denominator, derives fieldwise Eq, and deserializes from decimal STRINGS through parse::<i128>() and Self::new. Self::new requires positive denominator and divides both values by gcd. Input numerator "-0" (or "+0") becomes integer zero, reduces to denominator1, and serialization uses i128::to_string(), emitting numerator "0". There is no negative-zero sign bit to retain. A literal JSON -0.0 in place of an ExactRatio object or its string numerator is not a legal typed ratio value. Integer gain/geometry fields likewise have no signed-zero bit; if any ingress spelling is accepted it can only become integer zero and its canonical integer wire.

Labels or other ordinary strings containing "-0.0" remain text; differing strings compare unequal and cannot be treated as interchangeable numeric values.

## PartialEq and serializer audit

ProjectDocument derives PartialEq/Eq and Serialize (document.rs:595). It includes all authored fields, including presentation/basis, schema/project/revision/root, nodes, assets, source interpretations, marks, sound data, overrides, lineage, retained audio bindings and targets. CommandRequest derives the same traits and includes all request identity and command fields (command.rs:634).

SharedMap::PartialEq compares the inner BTreeMap by CONTENT, not Arc identity (shared_map.rs:91-95). Its Serialize delegates to that same ordered map (105-108). Identifiers derive textual equality and serialize into their exact String text. BTreeMap order is deterministic; derived record and enum serializers use fixed declaration order. Option/Vec distinctions and enum variants remain part of Eq even when semantically similar. Equal values therefore cannot differ in emitted defaults or array order.

AudioBindingState's custom serializer (audio_binding.rs:2025-2061) serializes timings in BTreeMap order, owner maps by content, and conditionally emits gap/sound fields solely from those same compared maps. GeneratedContentId/ObjectRef custom serialization uses the compared digest/length plus a compile-time algorithm constant. CapturedEditSet's equality includes its serialized wire plus derived data; it does not omit a wire field and is outside the eligible command anyway.

The only authored-value custom equality with ignored fields is FrozenAudioLayout (audio_reference.rs:303-314):
- every serialized field is compared: root, rate, nodes, overrides, gap overrides, lineage and derived clocks;
- ignored index and wire_bytes are derived caches and serde-skipped;
- ignored CompleteCharge is PROVISIONAL admission metadata and affects SERIALIZATION SUCCESS: Serialize rejects a provisional table (292-300).

Thus an unconditional theorem over arbitrary private provisional intermediates would be wrong: equal compared layout fields can have different serialization availability. Cache entry/result eligibility must be nonprovisional and successfully pass the existing canonical serializations. This is already naturally ordered: semantic entry and post-leaf wire::size use serde_json::to_writer, and replay initial/pre-leaf wire::size must succeed before lookup (compound/wire.rs:30-34). Retain those checks in full. An explicit private nonprovisional eligibility guard is prudent; do not move lookup before the required size traversal or suppress its error.

For the admitted normal layouts that can reach the cache, equal FrozenAudioLayout fields produce identical serialized bytes. Their Arc pointer equality shortcut is safe because all authored fields are private and immutable once admitted.

## What is and is not meant by wire equality

The theorem concerns current canonical compact serialization of the typed values. Original input whitespace, key order, legacy explicit defaults or decimal-string spelling can differ before typed admission; Deadpan already converts those into one typed current model. A cache must not claim to preserve arbitrary raw ingress text. Standalone historical audio-context codecs can deliberately choose a different wrapper/vocabulary; the proposed cache never substitutes one codec for another. Both the existing cumulative size traversal and the committed current document use the same current typed serializer.

If a future authored field introduces IEEE floating storage, a Value/RawValue payload, serializer-dependent hidden state or equality that ignores serialized fields, this proof must be redone. Ordinary floating PartialEq would be UNSOUND for a cache if +0.0 and -0.0 serialize differently. Do not rely on Eq's name alone as future proof.

## Required refuting regression

Add the following to the proposed standalone cache integration target:
1. Build two same-project/revision documents with framing center_x supplied as ExactRatio numerator "0"/denominator "1" and "-0"/"7", using normal JSON ingress. Assert the typed docs compare equal AND their complete compact JSON bytes are identical before invoking any cache.
2. Create the cache from the first and replay it against the second. Require cached/uncached exact result bytes, visitor before/after bytes, aggregate edit bytes and inverse restoration to match. Repeat the alias spelling in the request's captured framing/Hold picture context, not only the entry document.
3. Try literal IEEE -0.0/+0.0 where typed ratio fields are required; assert typed ingress refusal. For integer gain/geometry ingress test accepted spellings normalize to identical zero bytes, or assert the existing precise refusal if floats are rejected. Do not weaken serde to admit floats for this test.
4. Change an authored string label from "-0.0" to "+0.0" without changing project/revision; docs must compare unequal, cache must fall back, and untouched label bytes must be preserved.
5. For a representative framed/effected/marked/bound-audio document and typed InsertTime request, clone/reconstruct equal values through current serde and assert Eq => identical current compact serialization. Include explicit/default forms and complete retained timings.
6. Private unit coverage: construct a provisional slice and its normal materialized counterpart with equal compared fields where practical. Prove provisional serialization is refused and it cannot become a cache candidate; lookup must not suppress the original preflight serialization error. No public mutation hook is needed.

These tests refute accidental future reliance on IEEE equality or ignored wire state without adding a production byte witness. No such tests were run in this study.

## If a byte witness becomes necessary later

Reconsider the memory choice rather than silently switching to a digest. The minimal exact alternative is one bounded canonical entry witness plus exact request witness. Capture the entry bytes during an already-required semantic entry serialization; during replay's existing required pre-leaf serialization, compare emitted chunks against those bytes while still counting the ENTIRE output and preserving its original limit/error precedence. A mismatch is a flag, never an early serializer error. After that traversal and the original staged charge, a mismatch falls back to the old apply path. Do not stop on the first mismatch, use size/hash as authority, or add lookup before existing refusals.

That would retain up to MAX_DOCUMENT_JSON_BYTES (64MiB) of entry bytes plus the bounded eligible request and one result's existing snapshots. It changes the memory constant and must pass peak-memory measurement or be rejected. No such witness is recommended or drafted for the current integer/rational authored model.

## Complete custom equality inventory and provisional sequence

The exact custom PartialEq search over Core found only:
1. SharedMap<K,V> (shared_map.rs:91): delegates to inner BTreeMap content equality; no wire field omitted.
2. FrozenAudioLayout (audio_reference.rs:303): serialized fields all compared, with index/wire_bytes/CompleteCharge omitted as detailed above.
3. ValidatedDocument (document.rs:2523): delegates to its ProjectDocument value, omitting retained durations and BindingProof. Those are validation metadata with no document wire fields; the proposed cache key is ProjectDocument, not a caller-supplied ValidatedDocument proof.

All other relevant authored equality implementations are derived, including identifiers, ratios, Camera/effect/gain/geometry records, ProjectDocument and eligible CommandRequest. No custom floating approximation/epsilon comparison was found.

Provisional exclusion is required at BOTH candidate creation and lookup:
- Creation: plan_semantic_with_provider executes initial document wire::size at planner.rs:664 before constructing Planner. A provisional timing rejects serialization and cannot reach the first leaf. commit_leaf executes next wire::size and its cumulative charge at pause.rs:1072-1077 before adopting next and returning a successful plan. If an internal opaque candidate were tentatively created immediately after apply, that failing plan must drop it; no cache accessor can expose a candidate from an unsuccessful plan. Explicit private no-provisional guards on both stored entry and stored next make this invariant direct.
- Use: replay_compound_captured runs initial wire::size at compound.rs:264 before cloning current. For the eligible one-Edit path no earlier step can alter it. It then runs Ledger::reserve and pre-leaf current wire::size at314 and its original cumulative charge, before the proposed lookup at327. wire::size uses serde_json::to_writer (compound/wire.rs:30-34), and FrozenAudioLayout::Serialize refuses CompleteCharge.is_some at audio_reference.rs:294-299. Therefore an equal-by-fields but provisional CURRENT value fails the mandatory original traversal before any hit can occur. Keep an explicit no-provisional guard at use as defense against future movement of the lookup; it is an eligibility miss, not a new early error.
- Cached result: seal only the successful, post-serialization, nonprovisional leaf value. Public SemanticPlan.document mutation must not alter that private value. The mandatory replay post-result serialization still executes after a hit.

Add a private test that compares an equal-by-authored-fields provisional/normal pair, confirms the provisional serialization refusal and confirms no candidate exposure/hit can bypass the existing creation/use errors. Compare exact first error and visitor count zero, not only an is_err assertion.

## Preferred next action

Root's selected next step is current-baseline leaf timing plus owned peak-memory diagnostics, before cross-crate cache API work. This audit changes no code and does not authorize an implementation. It establishes that a new byte witness is not necessary for the current eligible typed model, while preserving the option to reject the cache on equality/clone cost or memory results.

If future exactness requires byte witnesses for BOTH entry and leaf command, their existing limits are each64MiB (MAX_DOCUMENT_JSON_BYTES and MAX_COMPOUND_WIRE_BYTES), so the conservative witness ceiling is128MiB plus fixed request identity overhead and retained result snapshots. That is a material memory change requiring measurement, not a free exactness guard. Count/compare through the original mandatory traversals and never turn mismatch into an early serialization error.
