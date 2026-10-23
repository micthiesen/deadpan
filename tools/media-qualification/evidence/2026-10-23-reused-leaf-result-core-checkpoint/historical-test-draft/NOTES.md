# Reused validated leaf result: focused differential test draft

Scratch only. No repository edits, Cargo, formatting, Git, native UI or timing command was run. The parent owns integration, compilation and verification. Read source-hashes.json before applying pause_tests.patch or appending test_addition.rs.txt. The full proposed test file is proposed_pause_tests.rs.txt; all retained Rust is .rs.txt so rustfmt does not treat it as source.

## Intended behavior

The test belongs in the existing semantic planner pause test module and reuses its actual tree, hold, context, pause, allocator and plan_pauses helpers. It sets a native mark on a 12-frame Hold at local 9, then inserts a two-frame Hold at the following seam through the legacy public command/patch path to establish real retained audio bindings. The tested program inserts a three-frame pause at frame 5 inside the original Hold, then sets the new pause's clip gain to -3 dB. There are exactly two authored leaves, with deterministic allocated identities, no register writes and no external media.

For each actual resolved leaf, the test calculates the old path (apply then forward.apply), compares it with apply_with_result's transaction and returned document, and applies both inverses back to that leaf's entry. It retains each whole before/after document, then compares the authoritative replay visitor's stages exactly. At the final outer revision it compares all document fields, explicit audio-binding and mark tables, inverse restoration, and unchanged input object plus JSON. The mark remains Bound and resolves at exact frame 12 after the insertion. This is semantic/differential evidence; it measures no PCM, wall time, allocation count or threshold. It does not assert which implementation function Planner::commit_leaf calls.

## Existing refusal coverage worth preserving

- semantic/planner/tests.rs::fresh_identity_contract_rejects_revision_capture_and_split_reuse: allocated revision/capture/split identity freshness.
- semantic/planner/tests.rs::fuel_charges_calls_and_clamped_motions_and_rejects_huge_counts_without_allocating: instruction fuel and pre-allocation refusal.
- semantic/planner/tests.rs::cumulative_document_and_capture_budgets_refuse_before_another_leaf_allocation: staged/capture byte exhaustion before another cut allocation. This is generic planner budget coverage, not a separate measurement of the setter's post-result charge.
- semantic/planner/tests.rs::resolved_edit_limit_is_separate_from_instruction_fuel: resolved-step cap.
- semantic/planner/tests/pause.rs::a_pause_inside_a_beat_splits_it_and_milliseconds_round_once: interior split, exact duration rounding and absent host picture provider refusal.
- semantic/planner/tests/pause.rs::ai_pause_refuses_non_silent_or_non_deterministic_host_recipes: direct command admission of invalid AI recipes.
- semantic/planner/tests/pause.rs::the_long_answer_refuses_a_pause_hidden_in_a_nested_group: ordinary Sequence scope refusal.
- semantic/planner/tests/scoped_framing.rs::scoped_zoom_refuses_default_mixed_absent_foreign_and_mismatched_authority_before_allocation: captured scope authority before allocations.
- semantic/planner/tests/scoped_framing.rs::scoped_zoom_requires_exact_unique_node_mark_and_revision_allocations: exact scoped isolation allocations.
- semantic/planner/tests/scoped_framing.rs::scoped_macro_allowlist_applies_inside_calls_and_late_failure_publishes_nothing: late refusal/unchanged entry.

The proposed production experiment should change only the two apply-plus-forward lines in Planner::commit_leaf, retaining the existing wire-size charge and maximum, current assignment, resolved-step append and refresh_children ordering. apply_with_result already documents that its validated result equals applying its complete forward patch; authoritative Compound replay already adopts that result. Root should still run the new differential test and relevant existing suites on final integrated source. No execution has established that this draft compiles or passes.
