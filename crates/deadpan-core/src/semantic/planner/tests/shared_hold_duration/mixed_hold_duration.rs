//! Mixed authority must resize the authored family, not one sampled occurrence.
use super::*;
use crate::{
    CommandRequest, OccurrenceIdentities, PreparedScopedHoldDuration,
    SemanticMixedHoldDurationContext, prepare_scoped_hold_duration,
};

fn mixed(outer_play: bool, outer: u32) -> SemanticContext {
    let mut target = target();
    let index = if outer_play { 0 } else { 1 };
    target.repeats[index].branch = RepeatEditBranch::Play {
        iteration: iteration(
            if outer_play { "outer" } else { "inner" },
            if outer_play { outer } else { 0 },
        ),
    };
    SemanticContext {
        selected_child: Some(node("outer")),
        mixed_hold_duration: Some(Box::new(SemanticMixedHoldDurationContext {
            root: node("outer"),
            target,
            presentation: Some(instance("held", outer, Some(0))),
        })),
        ..context("root", 20)
    }
}

fn request(before: &ProjectDocument, target: ScopedNodeTarget, frames: i64) -> CommandRequest {
    let duration = FrameDuration::new(frames).unwrap();
    let requirements = before
        .scoped_hold_duration_requirements(&target, duration)
        .unwrap();
    CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: revision("mixed-edit"),
        command: Command::SetScopedHoldDuration {
            target,
            duration,
            identities: OccurrenceIdentities {
                nodes: (0..requirements.nodes)
                    .map(|n| node(&format!("mixed-node-{n}")))
                    .collect(),
                marks: (0..requirements.marks)
                    .map(|n| crate::MarkId::new(format!("mixed-mark-{n}")).unwrap())
                    .collect(),
            },
        },
    }
}

fn prepare(
    before: &ProjectDocument,
    context: &SemanticContext,
    frames: i64,
) -> (CommandRequest, PreparedScopedHoldDuration) {
    let authority = context.mixed_hold_duration.as_ref().unwrap();
    let request = request(before, authority.target.clone(), frames);
    let prepared =
        prepare_scoped_hold_duration(before, &request, authority.presentation.as_ref()).unwrap();
    (request, prepared)
}

fn check_inverse(
    before: &ProjectDocument,
    request: &CommandRequest,
    prepared: &PreparedScopedHoldDuration,
) {
    assert_eq!(
        prepared
            .transaction
            .inverse
            .apply(&prepared.document)
            .unwrap(),
        *before
    );
    assert_eq!(
        ProjectDocument::from_json(&prepared.document.to_json().unwrap()).unwrap(),
        prepared.document
    );
    assert_eq!(
        serde_json::from_str::<CommandRequest>(&serde_json::to_string(request).unwrap()).unwrap(),
        *request
    );
    let (transaction, after) = crate::apply_with_result(before, request).unwrap();
    assert_eq!(after, prepared.document);
    assert_eq!(transaction, prepared.transaction);
}

#[test]
fn mixed_hold_duration_branch_orders_preserve_sparse_overrides_and_exact_proof() {
    let before = fixture(false);
    for outer_play in [false, true] {
        for outer in [0, 1] {
            for frames in [1, 3] {
                let context = mixed(outer_play, outer);
                let authority = context.mixed_hold_duration.as_ref().unwrap();
                let needs = before
                    .scoped_hold_duration_requirements(
                        &authority.target,
                        FrameDuration::new(frames).unwrap(),
                    )
                    .unwrap();
                assert_eq!(
                    (needs.nodes, needs.marks),
                    (if outer_play { 9 } else { 4 }, 0)
                );
                let (request, prepared) = prepare(&before, &context, frames);
                let factor = if outer_play { 1 } else { 2 };
                assert_eq!(
                    prepared.document.duration().unwrap().frames(),
                    45 + factor * (frames - 2)
                );
                assert_eq!(
                    prepared.document.nodes()[&node("held")],
                    before.nodes()[&node("held")]
                );
                for untouched in ["inner-held", "outer-held", "outer-owned"] {
                    assert_eq!(
                        prepared.document.nodes()[&node(untouched)],
                        before.nodes()[&node(untouched)]
                    );
                }
                assert_eq!(
                    prepared.target.repeats[if outer_play { 1 } else { 0 }].branch,
                    RepeatEditBranch::Default
                );
                let proof = prepared
                    .isolation()
                    .validate(&before, &request, &prepared.document)
                    .unwrap();
                assert_eq!(
                    proof.map_forward(&authority.target).unwrap(),
                    prepared.target
                );
                assert_eq!(
                    proof.map_backward(&prepared.target).unwrap(),
                    authority.target
                );
                assert_eq!(
                    proof
                        .map_instance_forward(authority.presentation.as_ref().unwrap())
                        .unwrap(),
                    *prepared.presentation.as_ref().unwrap()
                );
                for representative in [0, 1] {
                    let original = instance("held", representative, Some(0));
                    let mapped = proof.map_instance_forward(&original).unwrap();
                    assert_eq!(
                        mapped.node != original.node,
                        !outer_play || representative == outer
                    );
                }
                let expected_start = 5
                    + 14 * i64::from(outer)
                    + if !outer_play {
                        i64::from(outer) * (frames - 2)
                    } else {
                        0
                    };
                assert_eq!(
                    prepared.boundary.as_ref().unwrap().frame,
                    ProjectFrame(expected_start)
                );
                assert!(
                    prepared
                        .map_target(&prepared.document, &authority.target)
                        .is_err()
                );
                check_inverse(&before, &request, &prepared);
            }
        }
    }
}

#[test]
fn mixed_hold_duration_preserves_partial_physical_clocks_across_both_branch_orders() {
    for partial in [false, true] {
        let mut before = fixture(true);
        let old = crate::capture_unbound_audio_bindings(
            &before,
            AudioTimingId {
                allocation: revision("mixed-old-clock"),
                ordinal: 0,
            },
        )
        .unwrap();
        if partial {
            before.audio_bindings = old.clone();
            before.audio_bindings.bindings.remove(&node("tail"));
            before.validate().unwrap();
        }
        for outer_play in [false, true] {
            for frames in [1, 3] {
                let (request, prepared) = prepare(&before, &mixed(outer_play, 1), frames);
                for (owner, outer, inner) in [
                    ("held", 0, Some(0)),
                    ("after", 0, Some(0)),
                    ("held", 1, Some(0)),
                    ("after", 1, Some(0)),
                    ("inner-after", 1, Some(1)),
                    ("outer-after", 2, None),
                ] {
                    let original = instance(owner, outer, inner);
                    let mapped = prepared.map_instance(&before, &original).unwrap();
                    let old_clock = old
                        .resolve(&original.node, &original, crate::MAX_DOCUMENT_NODES)
                        .unwrap();
                    let new_clock = prepared
                        .document
                        .audio_bindings()
                        .resolve(&mapped.node, &mapped, crate::MAX_DOCUMENT_NODES)
                        .unwrap();
                    assert_eq!(
                        new_clock.lattice.origin, old_clock.lattice.origin,
                        "{outer_play} {frames} {original:?}"
                    );
                    assert_eq!(
                        new_clock.lattice.sample_boundary(ExactRatio::ZERO).unwrap(),
                        old_clock.lattice.sample_boundary(ExactRatio::ZERO).unwrap()
                    );
                }
                for (owner, binding) in &before.audio_bindings.bindings {
                    assert_eq!(
                        prepared.document.audio_bindings.bindings[owner].lattice,
                        binding.lattice
                    );
                }
                check_inverse(&before, &request, &prepared);
            }
        }
    }
}

#[test]
fn mixed_hold_duration_exact_marks_are_copied_then_resized_not_dropped() {
    let mut before = fixture(false);
    before.marks.insert(
        crate::MarkId::new("mixed-local").unwrap(),
        Mark {
            owner: node("held"),
            label: "Local".into(),
            boundary: BoundaryAnchor {
                coordinate: Anchor::Local {
                    node: node("held"),
                    position: ExactRatio::new(3, 2).unwrap(),
                },
                bias: InsertionBias::Right,
            },
            loss_policy: AnchorLossPolicy::KeepUnresolved,
            state: MarkState::Bound,
            fragments: vec![],
        },
    );
    before.validate().unwrap();
    for outer_play in [false, true] {
        let context = mixed(outer_play, 1);
        let needs = before
            .scoped_hold_duration_requirements(
                &context.mixed_hold_duration.as_ref().unwrap().target,
                FrameDuration::new(1).unwrap(),
            )
            .unwrap();
        assert_eq!(needs.marks, 1);
        let (request, prepared) = prepare(&before, &context, 1);
        assert_eq!(
            prepared.document.marks()[&crate::MarkId::new("mixed-local").unwrap()],
            before.marks()[&crate::MarkId::new("mixed-local").unwrap()]
        );
        let copied = &prepared.document.marks()[&crate::MarkId::new("mixed-mark-0").unwrap()];
        assert_eq!(copied.owner, prepared.target.node);
        assert!(matches!(copied.state, MarkState::Unresolved { .. }));
        check_inverse(&before, &request, &prepared);
    }
}

#[test]
fn mixed_hold_duration_counted_calls_requery_owned_targets_and_map_full_compound() {
    let before = fixture(true);
    let bank = BTreeMap::from([
        (name('a'), macro_value(vec![call('b', 1)])),
        (name('b'), macro_value(vec![duration(3), duration(1)])),
    ]);
    for outer_play in [false, true] {
        let initial = mixed(outer_play, 1);
        let mut allocations = vec![];
        let planned = plan_semantic(
            &before,
            &initial,
            &program(vec![call('a', 2)]),
            SemanticRegisterBank {
                entries: &bank,
                version: 0,
            },
            revision("mixed-final"),
            |request| {
                allocations.push(request.clone());
                allocate(request)
            },
            no_original,
        )
        .unwrap();
        assert_eq!(allocations.len(), 4);
        assert_eq!(
            allocations[0],
            SemanticAllocationRequest::Isolation {
                step_index: 0,
                nodes: if outer_play { 9 } else { 4 },
                marks: 0
            }
        );
        for (i, allocation) in allocations.iter().enumerate().skip(1) {
            assert_eq!(
                *allocation,
                SemanticAllocationRequest::Isolation {
                    step_index: i,
                    nodes: 0,
                    marks: 0
                }
            );
        }
        let old = initial.mixed_hold_duration.as_ref().unwrap();
        let mapped = planned.context.mixed_hold_duration.as_ref().unwrap();
        let proof = crate::derive_scoped_isolation(
            &before,
            planned.request.as_ref().unwrap(),
            &planned.document,
        )
        .unwrap();
        assert_eq!(proof.map_forward(&old.target).unwrap(), mapped.target);
        assert_eq!(
            proof
                .map_instance_forward(old.presentation.as_ref().unwrap())
                .unwrap(),
            *mapped.presentation.as_ref().unwrap()
        );
        assert_eq!(planned.context.parent, initial.parent);
        assert_eq!(planned.context.selected_child, initial.selected_child);
        let visible = planned
            .document
            .mixed_hold_duration_visibility(&initial.parent, mapped)
            .unwrap()
            .unwrap();
        assert_eq!(planned.context.cursor, visible.frames.start());
        assert_eq!(
            planned.document.duration().unwrap().frames(),
            if outer_play { 44 } else { 43 }
        );
        reversible(&before, &planned);
    }
}

#[test]
fn mixed_hold_duration_dormancy_still_isolates_selected_definition_without_fabricating_play() {
    for outer_play in [false, true] {
        let before = dormant(!outer_play);
        let mut context = mixed(outer_play, 1);
        context.mixed_hold_duration.as_mut().unwrap().presentation = None;
        assert!(
            before
                .mixed_hold_duration_visibility(
                    &context.parent,
                    context.mixed_hold_duration.as_ref().unwrap()
                )
                .unwrap()
                .is_none()
        );
        let initial = context.clone();
        let planned = plan(
            &before,
            context,
            vec![duration(3), duration(1)],
            &BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(
            planned.document.duration().unwrap(),
            before.duration().unwrap()
        );
        assert_eq!(planned.context.cursor, initial.cursor);
        assert!(
            planned
                .context
                .mixed_hold_duration
                .as_ref()
                .unwrap()
                .presentation
                .is_none()
        );
        assert_ne!(
            planned
                .context
                .mixed_hold_duration
                .as_ref()
                .unwrap()
                .target
                .node,
            node("held")
        );
        assert_eq!(
            planned.document.nodes()[&node("held")],
            before.nodes()[&node("held")]
        );
        assert!(
            planned.document.audio_bindings().timings().len()
                <= before.audio_bindings().timings().len() + 2
        );
        reversible(&before, &planned);
    }
}

#[test]
fn mixed_hold_duration_million_play_identity_and_timing_work_stays_compact() {
    let mut before = fixture(true);
    for owner in ["outer", "inner"] {
        let NodeKind::Repeat { iterations, .. } =
            &mut before.nodes.get_mut(&node(owner)).unwrap().kind
        else {
            panic!()
        };
        *iterations = IterationOrder::new(repeat_allocation(owner), 1_000_000).unwrap();
    }
    before.validate().unwrap();
    for outer_play in [false, true] {
        let context = mixed(outer_play, 1);
        let (request, prepared) = prepare(&before, &context, 3);
        assert_eq!(
            prepared.document.duration().unwrap().frames() - before.duration().unwrap().frames(),
            999_999
        );
        assert_eq!(
            prepared.document.nodes().len() - before.nodes().len(),
            if outer_play { 9 } else { 4 }
        );
        assert!(prepared.document.audio_bindings().timings().len() <= 1);
        check_inverse(&before, &request, &prepared);
    }
}

#[test]
fn mixed_hold_duration_noop_and_late_refusals_are_atomic_before_extra_allocation() {
    let before = fixture(false);
    let snapshot = before.clone();
    for (instructions, expected, message) in [
        (vec![duration(2)], 0, "already has that duration"),
        (
            vec![duration(3), duration(3)],
            1,
            "already has that duration",
        ),
        (
            vec![duration(3), motion(true, 1)],
            1,
            "supports only Hold duration",
        ),
    ] {
        let mut allocations = 0;
        let error = plan_semantic(
            &before,
            &mixed(false, 1),
            &program(instructions),
            SemanticRegisterBank {
                entries: &BTreeMap::new(),
                version: 0,
            },
            revision("mixed-refusal"),
            |request| {
                allocations += 1;
                allocate(request)
            },
            no_original,
        )
        .unwrap_err();
        assert_eq!(allocations, expected);
        assert_eq!(error.code, EditErrorCode::InvalidCommand);
        assert!(error.message.contains(message), "{error:?}");
        assert_eq!(before, snapshot);
    }
}

#[test]
fn mixed_hold_duration_rejects_forged_authority_stale_requests_and_wrong_pools() {
    let before = fixture(false);
    let base = mixed(false, 1);
    let mut invalid_contexts = vec![];
    let mut value = base.clone();
    value.mixed_hold_duration.as_mut().unwrap().presentation = None;
    invalid_contexts.push(value);
    let mut value = base.clone();
    value.mixed_hold_duration.as_mut().unwrap().target.repeats[1].branch =
        RepeatEditBranch::Default;
    invalid_contexts.push(value);
    let mut value = base.clone();
    value.mixed_hold_duration.as_mut().unwrap().target.repeats[1].branch = RepeatEditBranch::Play {
        iteration: iteration("inner", 9),
    };
    invalid_contexts.push(value);
    let mut value = base.clone();
    value.visual_selection = Some(SemanticVisualSelection::Time {
        anchor: ProjectFrame(5),
        head: ProjectFrame(6),
        extending: false,
    });
    invalid_contexts.push(value);
    let mut value = base.clone();
    value.shared_hold_duration = entry(Some(1)).shared_hold_duration;
    invalid_contexts.push(value);
    for value in invalid_contexts {
        let error = plan_semantic(
            &before,
            &value,
            &program(vec![duration(3)]),
            SemanticRegisterBank {
                entries: &BTreeMap::new(),
                version: 0,
            },
            revision("mixed-invalid"),
            |_| panic!("invalid authority allocated"),
            no_original,
        )
        .unwrap_err();
        assert!(
            matches!(
                error.code,
                EditErrorCode::InvalidCommand | EditErrorCode::SelectionUnavailable
            ),
            "{error:?}"
        );
        assert_ne!(error.code, EditErrorCode::IdentityConflict);
    }
    let authority = base.mixed_hold_duration.as_ref().unwrap();
    for additional in [false, true] {
        let mut request = request(&before, authority.target.clone(), 3);
        let Command::SetScopedHoldDuration { identities, .. } = &mut request.command else {
            panic!()
        };
        if additional {
            identities.nodes.push(node("mixed-extra"));
        } else {
            identities.nodes.pop();
        }
        let error = crate::apply(&before, &request).unwrap_err();
        assert_eq!(error.code, EditErrorCode::InvalidCommand);
        assert!(error.message.contains("exact node and mark identity pools"));
    }
    let mut stale = request(&before, authority.target.clone(), 3);
    stale.expected_revision = revision("stale");
    assert_eq!(
        crate::apply(&before, &stale).unwrap_err().code,
        EditErrorCode::RevisionConflict
    );
    let mut duplicate = request(&before, authority.target.clone(), 3);
    let Command::SetScopedHoldDuration { identities, .. } = &mut duplicate.command else {
        panic!()
    };
    identities.nodes[0] = node("held");
    assert_eq!(
        crate::apply(&before, &duplicate).unwrap_err().code,
        EditErrorCode::IdentityConflict
    );
}
