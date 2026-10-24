# Local document wire-count reuse

Read-only investigation of Main d6a6e23, Rust source map f973b941a3f71b70cd0826cf6ab3f5ceb68395e5407690bea79f8e5a49e53a53. No implementation, build, benchmark, test execution, Git action or Main write. The existing compact-json-size qualification was read first.

## Decision

Both relationships support exact local count reuse:

1. A successfully serialized document and its untouched literal clone have identical actual serialized bytes.
2. A successfully serialized document and that same owned value after replacing only revision_id differ by exactly the serialized old/new revision tokens.

The proof must be ownership and dataflow. ProjectDocument equality, revision equality, a caller-supplied number, or an assumption that all equal Rust values have equal JSON lengths is insufficient.

Recommend one private **compound-only prototype first**, covering those two boundaries in exactly one ordinary InsertTime step. Keep semantic and Store changes out of that first patch. Compound has one local current assignment and an immutable visitor boundary; semantic has seven authored replacement sites across five files. The semantic opportunities below are defensible, but adding them requires separate current/count maintenance or equally narrow one-step gating.

No performance or memory saving is predicted. Supplied phase medians are inclusive labels, not extra source functions or evidence that this proposed change improves them.

## Actual serializer and prior work

- compound/wire.rs:13-35: Count::write uses checked_add and admits only totals <= maximum. size invokes serde_json::to_writer, retaining no output buffer, and maps every serialization/writer refusal to LimitExceeded, "resolved transaction serialized byte limit".
- document.rs:596-629: ProjectDocument derives Clone and Serialize over fixed fields. revision_id is one ordinary field. Empty-field omissions depend on actual tables, not mode or revision.
- document.rs:913-946: to_compact_json and compact_json_size use the same compact serializer with different bounded sinks. Both prioritize an exceeded-byte error. They neither validate nor cache a document count.
- docs/qualification/compact-json-size-2026-10-23.md: exact counting in Store is already qualified. It avoids retaining complete output buffers, but still traverses each time. Do not redo that implementation or weaken actual checkpoint/history JSON production.

Store compact_json_size calls must keep their own DocumentError mapping, serialization and authority/work boundaries. Equal nominal 64 MiB limits do not authorize replacing them with a Core count.

## Literal clone proof, not equality

ProjectDocument::clone preserves actual authored representations and immutable shared tables, including hidden serialization-relevant state. SharedMap serializes its Arc-owned BTreeMap (shared_map.rs:105). AudioBindingState reads fixed tables and sorted timing records (audio_binding.rs:2025-2058). FrozenAudioLayout reads immutable inner data and rejects a provisional complete marker (audio_reference.rs:292-301).

The latter is a decisive existing counterexample to equality reuse. audio_reference.rs:3589, one_leaf_replay_equal_provisional_value_cannot_bypass_wire_admission, constructs documents that compare equal although the normal layout serializes and the provisional layout refuses. FrozenAudioLayout::PartialEq intentionally omits that marker. Whole-document Eq and "same revision" do not prove prior serialization success applies.

Generic Serialize values 0.0 and -0.0 can also compare equal with different JSON lengths. Current authored coordinates/gain are exact typed data, but generic wire::size must not become an Eq cache. ExactRatio's decimal-string wire normalizes on construction (exact.rs:9-36); raw JSON "-0" spelling is not a retained byte witness after parsing.

Literal clones retain every escaped/UTF-8 label, default/empty-field decision and actual representation. There is no deserialization or reconstruction.

The inspected ProjectDocument serializer graph does not read command-work/timing-representation TLS. Reference modes change representations produced by commands; they do not alter serialization of an existing immutable representation. GeneratedContentId/ObjectRef are pure wrappers (generated.rs:113,178). Explicit audio-context edge serialization is immutable (audio_reference/explicit_edges.rs:16,56). Frozen layout index and wire_bytes OnceLock fields are serde-skipped (audio_reference.rs:243-269). Populating a skipped cache in an immutable callback cannot change document JSON.

A future serializer that consumes state, reads TLS or mutable external state would invalidate this proof. This is not a generic Serialize guarantee.

## Compound dataflow and error order

compound.rs:294-469 currently preserves this order:

1. Outer revision check, transaction validation and leaf/base/outer revision separation.
2. Initial document wire::size at line317, initializing cumulative bytes.
3. Ledger construction and document.clone into current.
4. Per edit, Ledger.reserve, pre-leaf wire::size at line368, cumulative charge.
5. Actual leaf reuse/application, post-leaf next wire::size at line397, cumulative charge, Ledger.observe.
6. Read-only visitor, optional register publication, current = next.
7. Replace current.revision_id with outer revision; final wire::size at line426 and cumulative charge.
8. Construct exact net edit.

(a) For the first ordinary Edit step, current is a literal clone between successful initial and pre-leaf counts. Ledger::new/reserve only read the document and mutate their own ID sets (compound/identities.rs:14-60). No visitor ran before that first edit. Reusing the actual initial count does not move reserve, pre-leaf admission or its charge.

(b) For a one-step InsertTime result, the actual post-leaf count succeeds before Ledger.observe and visit. Both receive shared document references. current then becomes that same next value; its only later authored change before final size is top-level revision_id. Existing one-leaf net reuse already relies on this restriction. A visitor may refuse, panic or run nested commands/scoped helpers, but cannot safely mutate authored state through the shared reference. Refusal must still precede final arithmetic and final charge.

Exact final count:

    post_bytes - serialized_token_bytes(old_revision) + serialized_token_bytes(outer_revision)

Prefer counting both actual RevisionId tokens through the existing serializer instead of hand-maintaining escaping. RevisionId currently allows only 1-128 ASCII letters/digits/hyphens/underscores (document.rs:49-111), so token counts are small and include quotes. Field name, delimiters and every other byte stay unchanged.

Use checked subtraction/addition and the same 64 MiB comparison/error at the original final size site. Do not use saturating/signed unchecked deltas, assume equal-length outer IDs, or replace authored leaf allocation IDs. Capture the old header locally before changing it. If arithmetic unexpectedly fails, fall back to actual full sizing rather than inventing a refusal. With real 64 MiB counts and <=130-byte tokens, overflow cannot occur on this Mac.

The previous successful full count proves unchanged fields serialize successfully, including absence of a provisional refusal. Replacing a valid revision token introduces only a possible total-size refusal.

Preserve all four initial/pre/post/final admission outcomes and charged amounts. The optimization deliberately avoids recomputing two successful whole-document serializations; do not claim those traversals still execute. Retain each checked 512 MiB charge at its original position and retain "staged document work byte limit". Final 64 MiB refusal still wins before final cumulative-work refusal.

An exact-64 MiB leaf result plus a longer outer revision still visits once, then refuses final wire size before aggregate/Store work. Initial/post serialization refusal cannot mint a reusable successful count. Earlier identity/visitor failures keep their original precedence.

One eligible step has four <=64 MiB charges and cannot reach 512 MiB alone. Keep those charges. Exercise the real cumulative cap through the existing noneligible multistep 60 MiB fixture, without lowering caps.

## Small private prototype

Keep wire::size, public APIs, schema, OneLeafReplay and Store unchanged.

- Retain the actual initial count separately from cumulative bytes in replay_compound_inner.
- Reuse it at the first pre-leaf size only for the existing exact one-ordinary-InsertTime/no-input shape. This clone proof does not depend on candidate Eq or a candidate being present. Other shapes retain the original count.
- Retain the actual successful post-leaf size locally for that one-step shape. Use it for final header-only size after unchanged visitor/current assignment. Candidate-Some and candidate-None can both use it because both produced and actually counted next here. Never take counts from OneLeafReplay or a semantic plan.
- Count header tokens and decide checked total/limit at the original final size point. A private local helper is acceptable; expose no count/capability API and accept no caller-trusted size.
- All other paths retain full sizing. Reference modes can conservatively retain full sizing in the first trial, although literal-clone/header serialization proof itself is representation-independent.
- Add a debug-only full wire::size oracle and test-only scoped selection witness if needed. No public switch, persistent cache, JSON buffer or document snapshot is needed.

Local usize values and revision headers suffice.

## Semantic applicability and additional obligation

planner.rs:673 counts the entry; around line706 clones it into Planner.current. selection.rs:319 charge_resolution recounts for object-resolution work. content.rs:624 charge_step recounts before a leaf. pause.rs:1069-1088 commit_leaf performs actual leaf and actual post-wire/charge in admission. planner.rs:732-741 changes only the outer revision header before final macro wire/charge.

The same proofs can apply to untouched entry before first authored replacement and actually counted last result before final header replacement. Callbacks receive shared documents. Allocator/provider outputs become commands and cannot directly mutate current.

However document_bytes is cumulative work, not the current document's size. Never cache that total. A generic current count needs its own private field updated with the value, or narrow entry/one-leaf gating.

All seven current replacements follow actual successful post-counts and append a resolved edit step before returning:

- planner.rs:1559 (Cut)
- planner/content.rs:599 (Paste)
- planner/group.rs:260
- planner/pause.rs:1085
- planner/repeat.rs:81,166,196

Before any resolved step, current remains the exact entry clone; steps.is_empty is a conservative guard because even nonediting steps disable it. A generic current/count field must be maintained at all seven sites, including isolation-generated leaves and partial-failure ordering. A narrow pause final shortcut must prove final shape and count belong to that actual result; a stale candidate alone is insufficient.

This maintenance surface is why the recommended first experiment is compound-only. No fundamental serde/visitor obstruction was found for semantic reuse, but selecting a semantic opcode does not prove it produces only one leaf.

## Decisive tests and bounded verification

No tests were executed here. Required checks:

1. New versus reference full-count replay compares whole output/edit wire, exact errors, visitor sequence/count, register writes and cumulative charged amounts. Separately prove shortcut selection.
2. Identity refusal still wins before pre-leaf charge. Initial/post wire refusal stays before visitor0. Visitor refusal has one visit and no final-size reuse. Nested commands/reference-mode helpers restore state.
3. Shorter/equal/128-byte outer IDs: computed sizes equal actual compact bytes with escaped/UTF-8 labels, defaults, nested bindings, captured context and derived/reference representations. Body must remain bit/representation-identical.
4. Existing equal-normal/provisional counterexample still refuses appropriately. A distinct equal value never inherits a size. A generic signed-zero example can document why generic Eq reuse is forbidden; do not add authored float fields.
5. Real 64 MiB initial/post refusals, exact-cap success, longer outer final refusal after visitor1 and shorter outer accepted with exact charge. Noneligible fallback keeps actual serializers.
6. Real 512 MiB multistep fallback retains refusal after three visits. A later semantic proposal must test repeated nonediting resolution charges and multi-leaf/isolation transitions to prevent stale initial/post counts.
7. Existing Some/None authority, actual Store codec/size/readback/inverse, history/undo/redo/reopen and real-media oracles remain.

Only after exact source review and these oracles pass should Root compare a private candidate against the matching uninstrumented control. Keep executed binary/source hashes, independent timing/memory lanes and existing quiet fresh fixtures. No gain or product-gate claim follows from phase labels.
