# Narrow fixed-window This-play Hold duration

Core implementation paths:

- crates/deadpan-core/src/command.rs
- crates/deadpan-core/src/scoped_edit.rs
- crates/deadpan-core/src/scoped_edit/hold_duration.rs
- crates/deadpan-core/src/semantic/planner/hold_duration.rs
- crates/deadpan-core/src/semantic/planner/retime.rs
- crates/deadpan-core/src/semantic/planner/tests/scoped_hold_duration.rs
- crates/deadpan-core/src/semantic/planner/tests/scoped_hold_duration/fixed_retime_duration.rs
- crates/deadpan-core/src/semantic/planner/tests/shared_hold_duration/shared_fixed_retime_duration.rs

No app, audio, store, CLI, documentation, Cargo.lock or other foreign path was
changed by this worker in the implementation phase. The earlier app witness is
preserved. No Cargo, formatter or Git command was run in this phase.

Runtime APIs:

ProjectDocument::fixed_occurrence_hold_duration_visibility(
    &self, parent: &NodeId, root: &NodeId, instance: &InstancePath
) -> Result<HoldDurationVisibility, EditError>

PreparedOccurrenceHoldDuration::fixed_visibility()
    -> Option<HoldDurationVisibility>

PreparedOccurrenceHoldDuration.boundary -> Option<ResolvedBoundary>

The boundary is still exactly from AnchorIndex when it resolves. None is
accepted only for fixed-window OutsideMapping where the intrinsic start is
cropped away but sampled visibility exists. No boundary is fabricated.
Legacy ordinary paths require Some. Native continuation and trace use
fixed_visibility.frames for the new fixed path.

fixed_occurrence_preflight is the named eligibility helper. Ordinary paths
without any Retime return None and retain their existing behavior. A path
containing a Retime requires exactly one Repeat and exactly one unclocked
purpose Edit Retime below that Repeat, otherwise Sequence ancestors. It proves
that the target follows the effective stable body child, not a gap, and invokes
the shared geometry projector before isolation. Native query additionally
checks the captured parent/root and original occurrence-wrapper sound policy.

Command predispatch follows original picture/gain/sound validators. It uses
isolate_temporal and exact identity pools, maps the real stable presentation,
then recursively applies ordinary SetHoldDuration to the already isolated tree.
The fixed local-suffix clock reducer captures the actual pre-edit physical
layout. It never invokes occurrence_owners_after or its integral root-suffix
capture. After reduction the mapped candidate must still have sampled visibility
and fixed project duration. One net transaction preserves original intent and
the actual isolation proof.

Shared geometry now checks the effective stable body child. Literal Default
authoring policy remains independently enforced by the shared query.

Nine focused tests added:

- fixed_occurrence_duration_fresh_pools_affine_visibility_all_pitches_and_inverse
- fixed_occurrence_duration_owned_and_staged_setters_allocate_zero_nodes
- fixed_occurrence_duration_crop_and_intrinsic_boundary_remain_distinct
- fixed_occurrence_duration_exact_pools_stale_and_wrong_root_refuse
- fixed_occurrence_duration_unsupported_ancestry_keeps_atomic_refusals
- fixed_occurrence_duration_hidden_unsampled_and_staged_disappearance_rollback
- fixed_occurrence_duration_shrink_cannot_rewrite_retained_input_end
- fixed_occurrence_duration_derived_clock_refuses_before_isolation
- fixed_occurrence_duration_preserves_source_recipe_lattice_after_isolation

The last test uses structurally qualified SourceAudio coordinates at NTSC and
44.1kHz, checks retained numeric origin/rate/sample-grid/support against the
pre-edit capture, and checks the new live affine origin separately. It does not
decode PCM or claim the fixture contains measured nonzero samples. Audio/store
workers own those independent evidence layers.

The prior shared-fixed test's formerly valid This-play refusal now uses its
existing nested Repeat fixture, which remains deliberately unsupported.

Root owns compilation, formatting, tests and execution evidence. All code and
tests were frozen for that first compile after this note.
