# Preferred next action: one focused validation/preflight microprofile

Read-only analysis of the exact current fine source. No source writes, Cargo/compiler/runtime, Git or Main changes occurred. The only new file is this note.

I recommend a bounded microprofile before another optimization. The measured semantic_leaf_apply43.467459ms contains input/reducer/binding/diff/result-validation work; semantic_plan33.948583ms is already EXCLUSIVE RESIDUAL, including several different preflight/provider/index passes. Neither measurement identifies which proof reuse would save time or preserve the earliest refusal. Do not guess a new API from them.

The capability snapshot is0.004750ms, or4.75microseconds. Dismiss capability-clone optimization. The named semantic/replay/Store size groups are13.410083/13.060417/11.608125ms for4/4/3sites (reported sum of their medians38.078625ms), not all serialization. History codec/readback22.490084ms is separately measured serialization+decode+Eq work; it is not part of the two regions investigated here.

## Grounded current call map

| Exact code | Remaining work and proof relevance |
| --- | --- |
| Core semantic/planner.rs plan_semantic_with_provider:663-675 | program.validate(), document.validate(), revision check, validate_context and Original context precede the first measured document wire. Without a matching scope, document.validate calls recipe_durations_shared, which fully validates with context limits. |
| planner.rs scope_bounds:1593 and insert_time/source_splice.rs source_splice_boundary:124 | Scope admission calls source_splice_boundary twice, for start/end. Each calls document.durations(), then constructs AnchorIndex::from_durations. A retained proof avoids complete validation, but durations() still clones its BTreeMap and the index still traverses parents/children. with_sealed is an exact-clock interpretation guard, not a validation proof. |
| planner.rs child_ends:1664 and planner initialization:676-730 | Another document.durations() supplies child ends. Allocation-ID collection, child_indices, occupied_nodes, marks/revision sets and current=document.clone also occur here. occupied_nodes visits both authored nodes and every retained timing layout's node/lineage keys. These are plausible scaling traversals, not measured individual costs. |
| planner/pause.rs insert_hold_with_intent:282-328 | Ordinary pause_insertion_target resolves the actual placement before provider/allocation; occurrence/retimed intent has distinct validation. occurrence/target.rs resolve:229-245 calls document.durations(). Provider(self) follows target/scope admission and precedes charge_step and allocation. |
| CLI macros.rs plan_program_with_transcript_guard:~540 | The pause-provider callback compiles RenderPlan for the staged document, then resolves the actual provider and qualified picture index. RenderPlan::compile in deadpan-plan/src/plan.rs:698 begins with recipe_durations_shared and constructs indexes over nodes. This belongs to the semantic residual, not semantic_leaf_apply. It may repeat input validation when the staged clone has no matching scope. Do not attribute the whole callback to validation; plan construction/media authority are separate work. |
| planner/pause.rs commit_leaf and planner/selection.rs refresh_children:456 | After the pure result and its measured post-wire admission, refresh_children obtains current.durations_shared once and borrows that same current value under with_proved_durations. The three subsequent queries reuse it, but obtaining it can be a fresh complete result validation. Two AnchorIndex constructions, map copies and child-index building still execute. This exact refresh reuse is already implemented; do not propose it again. |
| semantic/program.rs validate:822/943; compound.rs AtomicCommand::new:27/46 and ResolvedTransaction::validate:157/170 | Typed command/program limits also serialize their actual bounded payloads. These codecs are not included by the named4+4+3document-size rows. The one instruction/ordinary leaf payload differs from a full10000node document; no cost is assigned to these checks without timing. |

## What the measured pure leaf actually contains

Core command.rs apply_with_result:1092 invokes apply_with_durations(..., previous=None). apply_with_durations_captured:1164 keeps exact-clock sealing, EXACT_EDIT reset, revision, public picture/gain/sound checks, occurrence/retime preparation and sound/allowance capture before reduction. It does not simply start by calling document.validate.

For ordinary InsertTime:
- command.rs:~1434 invokes insert_time::apply_known within the existing scoped-timing-capture guard; a rejected provisional projection can trigger its existing complete-capture retry.
- insert_time.rs:51-127 validates Hold/timing/identities and obtains document.durations() at89, then resolves boundaries/retimes/Split budget.
- Its binding capture, scoped layout, Split and rescope work follows. split_captured:395 reuses retained durations for the exact borrowed working copy only when that proof already exists; otherwise Split validates normally.
- apply_known returns structural durations when proved. command.rs:1478 records KnownValidation::Structure.
- command.rs:1759-1818 restores captured sound state, checks sealed clocks, prunes/compacts, locks basis and invokes net_transaction_known.
- net_transaction_known:1873 first obtains before duration (retained exact pointer proof or structural_duration), builds diffs for every carried map and AudioBindingPatch, then validates the result. KnownValidation::Structure reuses result structural durations but still checks the remaining invariants/bindings. previous=None means no supplied previous BindingProof for validate_after_with. It builds changed IDs and the inverse afterward.

These are plausible repeated traversals and validation opportunities, but43.467459ms does not distinguish input duration validation from binding projection, Split, post-processing, diff or result validation. The separately measured replay_net_aggregate7.575750ms uses a different KnownValidation/previous-proof context; it cannot be subtracted from the pure-leaf median to infer a reducer cost.

## Existing proof mechanisms and their limits

- document.rs ValidatedDocument::scope:2412 retains durations plus binding proof for its immutable Arc.
- document.rs validated::durations:2601 matches the exact ProjectDocument ADDRESS. Equal project/revision IDs or authored-value equality do not transfer that proof to another object.
- The App check_macro_context returns store.snapshot() as a ProjectDocument; CLI receives its plain borrow and Planner creates current=document.clone. An outer scope around another Arc cannot automatically prove this staged clone.
- apply_validated:1071 already enters the retained scope and supplies previous to net_transaction_known; the latter filters previous by exact document pointer before reusing binding owners.
- with_proved_durations:2481 retains only durations for the exact unchanged borrow. Its debug path recomputes complete validation. It is not permission to reuse stale structure after InsertTime.
- validated_in_scope/validate_bindings_in_scope:2426 onward reuse unchanged owners only through a computed binding patch and an actual retained BindingProof.
- A successful pure leaf computes complete result validation, but apply_with_result returns a raw ProjectDocument and drops that proof. Retaining a result proof might be useful if result revalidation is measured as material; that is a possible follow-up, not an API recommendation now.

If measurement justifies proof reuse, prefer the existing exact-borrow/local-scope mechanisms before inventing any cache. Separate input-only preflight reuse from result proof reuse; their invalidation and binding authority differ.

## The one recommended microprofile

Extend only these two measured buckets, retaining all existing exclusive rows and4/4/3/0counts. Add nine fixed child categories, each count1 for the qualified ordinary empty-derived one-leaf target:

1. semantic_head_validate: exactly document.validate in plan_semantic_with_provider.
2. semantic_context_admission: exactly validate_context(document,context), including both scope boundaries and their AnchorIndexes.
3. semantic_planner_setup: after initial measured document wire through existing revision/occupied-ID/child/index collection and Planner construction, before planner.execute. Preserve all initializer evaluation/drop order; do not extract field expressions.
4. semantic_pause_target: the existing target if/match expression in insert_hold_with_intent.
5. semantic_pause_provider: the existing provider(self) call, including the real CLI RenderPlan/provider/qualified-index work.
6. semantic_children_refresh: exactly refresh_children after commit_leaf installs current. This refines the remaining semantic residual, outside pure apply and post-wire.
7. leaf_insert_known: the existing ordinary InsertTime reduction region around scoped apply_known and its unchanged fallback selection; one region even if the fallback repeats work.
8. leaf_input_durations: nested inside7 around the original document.durations call in apply_known. No generic per-node/duration-query hooks.
9. leaf_net_construction: the original final net_transaction_known call site in apply_with_durations_captured (not the general net helper, so replay aggregate remains separately measured).

Existing semantic_plan and semantic_leaf_apply become exclusive residuals when these children run. Do not subtract old medians to infer new children. These9fixed events need no per-node timers or data logging. Scope is seven private existing paths: helper+App emitter, Core planner.rs, planner/pause.rs, planner/selection.rs, command.rs, insert_time.rs. No CLI/media adapter, Store/history, public optimization API, VM bypass or format change is required. Retain the same gate/success/publication/conservation protocol and calculate the fixed trace bound before keeping4096bytes.

This split answers whether head/preflight validation, actual provider work, refresh, input durations or net construction is material. If leaf_insert_known or leaf_net_construction still dominates internally, do not call it validation cost; a subsequent bounded decision would need its remaining specific work identified.

## Invariants, counterexamples and oracle

No optimization is authorized by this note. Any later candidate must preserve:
- program/fuel/depth/identity limits; exact64MiB and cumulative wire charges; original serializer success requirements;
- document-before-context/refusal order, target-before-provider-before-allocation ordering, host errors and unchanged inputs;
- actual nested Sequence/zero-duration group/mark behavior, outside siblings and every selection/context/trace/register field;
- local/reference modes, derived-clock sealing/fallback, EXACT_EDIT entry/exit, provisional-wire admission and scope restoration on errors;
- every current Store bank/head/allocation/media/analysis/correction/history/inverse/transaction/publication authority.

Counterexamples: same revision with altered duration/node/framing/binding table; plain clone versus the scoped Arc address; multiple pauses changing structure; new timing tables or restored sounds invalidating Complete validation; derived/fractional retimes with integral-looking roots; late provider/allocation refusal; invalid caller-owned values that must fail before a later context/limit check. Never attach input durations to a changed result or move validation earlier merely to make a proof convenient.

Use existing semantic_refresh_wide_bound_document_replaces_the_proof_after_each_pause, nested_scope_keeps_zero_groups_marks_and_outside_siblings, late_provider_and_allocator_failures_keep_order_and_inputs, nested_follow_speed_pauses_keep_the_derived_clock_fallback, plus local_command_work_equals_the_reference_on_random_edit_sequences. Validated tests reused_proofs_equal_complete_validation_through_edits and reused_proofs_still_reject_every_changed_owner_and_table are the complete-validation oracle. Keep one_leaf_replay_* exact document/request/edit/visitor/inverse/first-error tests and the actual qualified media Some/None publication/export test.

For this microprofile itself, Root should require exact state/receipt/history equality against qualified control with gate off/on, fixed counts and zero-row behavior, checked conservation and quiet rotated trials. Existing fine evidence stays immutable. This note claims no validation subcost, optimization gain or overhead.
