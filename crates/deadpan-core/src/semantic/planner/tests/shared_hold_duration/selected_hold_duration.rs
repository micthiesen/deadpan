//! Several selected bodies retain distinct authoring and physical clocks.
use super::*;
use crate::SemanticSelectedHoldDurationContext;

fn selected_inner(primary: u32, outer: Option<u32>) -> SemanticContext {
    let one = |inner: u32| {
        let mut target = target();
        if let Some(outer) = outer {
            target.repeats[0].branch = RepeatEditBranch::Play {
                iteration: iteration("outer", outer),
            };
        }
        target.repeats[1].branch = RepeatEditBranch::Play {
            iteration: iteration("inner", inner),
        };
        if inner == 1 {
            target.node = node("inner-held");
        }
        target
    };
    SemanticContext {
        selected_child: Some(node("outer")),
        selected_hold_duration: Some(Box::new(SemanticSelectedHoldDurationContext {
            root: node("outer"),
            target: one(primary),
            also: vec![one(1 - primary)],
            presentation: Some(instance(
                if primary == 0 { "held" } else { "inner-held" },
                outer.unwrap_or(1),
                Some(primary),
            )),
        })),
        ..context("root", 20)
    }
}

fn selected_outer(all_play: bool, primary: u32) -> SemanticContext {
    let one = |outer: u32| {
        let mut target = target();
        target.repeats[0].branch = RepeatEditBranch::Play {
            iteration: iteration("outer", outer),
        };
        if all_play {
            target.repeats[1].branch = RepeatEditBranch::Play {
                iteration: iteration("inner", 0),
            };
        }
        target
    };
    SemanticContext {
        selected_child: Some(node("outer")),
        selected_hold_duration: Some(Box::new(SemanticSelectedHoldDurationContext {
            root: node("outer"),
            target: one(primary),
            also: vec![one(1 - primary)],
            presentation: Some(instance("held", primary, Some(0))),
        })),
        ..context("root", 20)
    }
}

fn observed(
    before: &ProjectDocument,
    context: &SemanticContext,
    instructions: Vec<SemanticInstruction>,
    bank: &BTreeMap<RegisterName, Arc<RegisterValue>>,
) -> (SemanticPlan, Vec<SemanticAllocationRequest>) {
    let mut allocations = Vec::new();
    let planned = plan_semantic(
        before,
        context,
        &program(instructions),
        SemanticRegisterBank {
            entries: bank,
            version: 0,
        },
        revision("selected-final"),
        |request| {
            allocations.push(request.clone());
            allocate(request)
        },
        no_original,
    )
    .unwrap();
    (planned, allocations)
}

fn check_proof(before: &ProjectDocument, initial: &SemanticContext, planned: &SemanticPlan) {
    let old = initial.selected_hold_duration.as_ref().unwrap();
    let new = planned.context.selected_hold_duration.as_ref().unwrap();
    let proof = crate::derive_scoped_isolation(
        before,
        planned.request.as_ref().unwrap(),
        &planned.document,
    )
    .unwrap();
    assert_eq!(proof.map_forward(&old.target).unwrap(), new.target);
    for (old, new) in old.also.iter().zip(&new.also) {
        assert_eq!(proof.map_forward(old).unwrap(), *new);
        assert_eq!(proof.map_backward(new).unwrap(), *old);
    }
    assert_eq!(
        old.presentation
            .as_ref()
            .map(|p| proof.map_instance_forward(p).unwrap()),
        new.presentation
    );
    let visible = planned
        .document
        .selected_hold_duration_visibility(&initial.parent, new)
        .unwrap();
    assert_eq!(
        planned.context.cursor,
        visible.map_or(initial.cursor, |v| v.frames.start())
    );
    assert_eq!(planned.context.parent, initial.parent);
    assert_eq!(planned.context.selected_child, initial.selected_child);
    reversible(before, planned);
}

#[test]
fn selected_hold_duration_zero_net_change_and_last_primary_use_final_boundary() {
    let before = fixture(true);
    for primary in [0, 1] {
        let initial = selected_inner(primary, None);
        let (planned, allocations) =
            observed(&before, &initial, vec![duration(3)], &BTreeMap::new());
        assert_eq!(planned.document.duration().unwrap().frames(), 45);
        assert_eq!(
            allocations,
            vec![
                SemanticAllocationRequest::Isolation {
                    step_index: 0,
                    nodes: if primary == 0 { 4 } else { 0 },
                    marks: 0
                },
                SemanticAllocationRequest::Isolation {
                    step_index: 1,
                    nodes: if primary == 0 { 0 } else { 4 },
                    marks: 0
                },
            ]
        );
        assert_eq!(
            planned.context.cursor,
            ProjectFrame(if primary == 0 { 19 } else { 26 })
        );
        // When the last Hold is primary it first moves25->24; the later edit
        // of the earlier Hold moves it again to26. Its first boundary is stale.
        let final_authority = planned.context.selected_hold_duration.as_ref().unwrap();
        for target in std::iter::once(&final_authority.target).chain(&final_authority.also) {
            assert!(
                matches!(&planned.document.nodes()[&target.node].kind, NodeKind::Hold { recipe } if recipe.duration.frames() == 3)
            );
            assert_eq!(target.repeats[0].branch, RepeatEditBranch::Default);
        }
        assert_eq!(
            planned.document.nodes()[&node("held")],
            before.nodes()[&node("held")]
        );
        assert_eq!(
            planned.document.nodes()[&node("outer-held")],
            before.nodes()[&node("outer-held")]
        );
        assert_eq!(
            planned.document.nodes()[&node("tail")],
            before.nodes()[&node("tail")]
        );
        check_proof(&before, &initial, &planned);
    }
}

#[test]
fn selected_hold_duration_shared_node_ids_are_distinct_selected_families() {
    let before = fixture(false);
    for all_play in [false, true] {
        let initial = selected_outer(all_play, 1);
        let old = initial.selected_hold_duration.as_ref().unwrap();
        assert_eq!(old.target.node, old.also[0].node);
        let (planned, allocations) =
            observed(&before, &initial, vec![duration(3)], &BTreeMap::new());
        assert_eq!(allocations.len(), 2);
        let count = if all_play { 13 } else { 9 };
        for (index, allocation) in allocations.iter().enumerate() {
            assert_eq!(
                *allocation,
                SemanticAllocationRequest::Isolation {
                    step_index: index,
                    nodes: count,
                    marks: 0
                }
            );
        }
        assert_eq!(planned.document.duration().unwrap().frames(), 47);
        assert_eq!(planned.context.cursor, ProjectFrame(20));
        let new = planned.context.selected_hold_duration.as_ref().unwrap();
        assert_ne!(new.target.node, new.also[0].node);
        assert_eq!(
            planned.document.nodes()[&node("held")],
            before.nodes()[&node("held")]
        );
        check_proof(&before, &initial, &planned);
    }
}

#[test]
fn selected_hold_duration_common_outer_clone_and_counted_calls_requery_exact_pools() {
    let before = fixture(true);
    let bank = BTreeMap::from([
        (name('a'), macro_value(vec![call('b', 1)])),
        (name('b'), macro_value(vec![duration(3), duration(1)])),
    ]);
    for outer in [None, Some(1)] {
        let initial = selected_inner(0, outer);
        let (planned, allocations) = observed(&before, &initial, vec![call('a', 2)], &bank);
        assert_eq!(allocations.len(), 8);
        assert_eq!(
            allocations[0],
            SemanticAllocationRequest::Isolation {
                step_index: 0,
                nodes: if outer.is_some() { 13 } else { 4 },
                marks: 0
            }
        );
        for (index, allocation) in allocations.iter().enumerate().skip(1) {
            assert_eq!(
                *allocation,
                SemanticAllocationRequest::Isolation {
                    step_index: index,
                    nodes: 0,
                    marks: 0
                }
            );
        }
        assert_eq!(
            planned.document.duration().unwrap().frames(),
            if outer.is_some() { 41 } else { 37 }
        );
        check_proof(&before, &initial, &planned);
    }
}

#[test]
fn selected_hold_duration_skips_validated_noop_first_or_last_but_refuses_all_noop() {
    let before = fixture(false);
    for requested in [2, 4] {
        let initial = selected_inner(0, None);
        let (planned, allocations) = observed(
            &before,
            &initial,
            vec![duration(requested)],
            &BTreeMap::new(),
        );
        assert_eq!(
            allocations,
            vec![SemanticAllocationRequest::Isolation {
                step_index: 0,
                nodes: if requested == 2 { 0 } else { 4 },
                marks: 0
            }]
        );
        assert_eq!(
            planned.document.duration().unwrap().frames(),
            if requested == 2 { 41 } else { 49 }
        );
        check_proof(&before, &initial, &planned);
    }
    let mut same = before;
    let NodeKind::Hold { recipe } = &mut same.nodes.get_mut(&node("held")).unwrap().kind else {
        panic!()
    };
    recipe.duration = FrameDuration::new(4).unwrap();
    same.validate().unwrap();
    let error = plan_semantic(
        &same,
        &selected_inner(0, None),
        &program(vec![duration(4)]),
        SemanticRegisterBank {
            entries: &BTreeMap::new(),
            version: 0,
        },
        revision("no-op-final"),
        |_| panic!("all-noop set allocated"),
        no_original,
    )
    .unwrap_err();
    assert_eq!(error.code, EditErrorCode::InvalidCommand);
    assert!(
        error.message.contains("already have that duration"),
        "{error:?}"
    );
}

#[test]
fn selected_hold_duration_keeps_partial_physical_clocks_and_mark_copy_loss() {
    let mut before = fixture(true);
    before.marks.insert(
        crate::MarkId::new("selected-local").unwrap(),
        Mark {
            owner: node("held"),
            label: "Retained".into(),
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
    let old = crate::capture_unbound_audio_bindings(
        &before,
        AudioTimingId {
            allocation: revision("selected-old-clock"),
            ordinal: 0,
        },
    )
    .unwrap();
    before.audio_bindings = old.clone();
    before.audio_bindings.bindings.remove(&node("tail"));
    before.validate().unwrap();
    for primary in [0, 1] {
        let initial = selected_inner(primary, None);
        let (planned, allocations) =
            observed(&before, &initial, vec![duration(1)], &BTreeMap::new());
        let clone_index = if primary == 0 { 0 } else { 1 };
        assert_eq!(
            allocations[clone_index],
            SemanticAllocationRequest::Isolation {
                step_index: clone_index,
                nodes: 4,
                marks: 1
            }
        );
        let mark = crate::MarkId::new(format!("isolate-{clone_index}-0")).unwrap();
        assert!(matches!(
            planned.document.marks()[&mark].state,
            MarkState::Unresolved { .. }
        ));
        assert_eq!(
            planned.document.marks()[&crate::MarkId::new("selected-local").unwrap()],
            before.marks()[&crate::MarkId::new("selected-local").unwrap()]
        );
        let proof = crate::derive_scoped_isolation(
            &before,
            planned.request.as_ref().unwrap(),
            &planned.document,
        )
        .unwrap();
        for (owner, outer, inner) in [
            ("held", 0, Some(0)),
            ("after", 1, Some(0)),
            ("inner-held", 0, Some(1)),
            ("inner-after", 1, Some(1)),
            ("outer-after", 2, None),
        ] {
            let original = instance(owner, outer, inner);
            let mapped = proof.map_instance_forward(&original).unwrap();
            let old = old
                .resolve(&original.node, &original, crate::MAX_DOCUMENT_NODES)
                .unwrap();
            let new = planned
                .document
                .audio_bindings()
                .resolve(&mapped.node, &mapped, crate::MAX_DOCUMENT_NODES)
                .unwrap();
            assert_eq!(new.lattice.origin, old.lattice.origin, "{original:?}");
            assert_eq!(
                new.lattice.sample_boundary(ExactRatio::ZERO).unwrap(),
                old.lattice.sample_boundary(ExactRatio::ZERO).unwrap()
            );
        }
        assert_eq!(planned.document.duration().unwrap().frames(), 37);
        check_proof(&before, &initial, &planned);
    }
}

#[test]
fn selected_hold_duration_dormant_and_million_play_sets_stay_compact() {
    let before = dormant(false);
    let mut initial = selected_outer(false, 1);
    initial
        .selected_hold_duration
        .as_mut()
        .unwrap()
        .presentation = None;
    assert!(
        before
            .selected_hold_duration_visibility(
                &initial.parent,
                initial.selected_hold_duration.as_ref().unwrap()
            )
            .unwrap()
            .is_none()
    );
    let (planned, _) = observed(&before, &initial, vec![duration(3)], &BTreeMap::new());
    assert_eq!(
        planned.document.duration().unwrap(),
        before.duration().unwrap()
    );
    assert_eq!(planned.context.cursor, initial.cursor);
    assert_eq!(
        planned.document.nodes()[&node("held")],
        before.nodes()[&node("held")]
    );
    check_proof(&before, &initial, &planned);
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
    let initial = selected_outer(false, 1);
    let (planned, allocations) = observed(&before, &initial, vec![duration(3)], &BTreeMap::new());
    assert_eq!(allocations.len(), 2);
    assert_eq!(planned.document.nodes().len() - before.nodes().len(), 18);
    assert_eq!(
        planned.document.duration().unwrap().frames() - before.duration().unwrap().frames(),
        1_999_998
    );
    assert!(planned.document.audio_bindings().timings().len() <= 2);
    check_proof(&before, &initial, &planned);
}

#[test]
fn selected_hold_duration_late_noop_instruction_and_pool_failures_are_atomic() {
    let before = fixture(false);
    let snapshot = before.clone();
    for (instructions, message) in [
        (vec![duration(3), duration(3)], "already have that duration"),
        (
            vec![duration(3), motion(true, 1)],
            "supports only Hold duration",
        ),
    ] {
        let mut allocations = 0;
        let error = plan_semantic(
            &before,
            &selected_inner(0, None),
            &program(instructions),
            SemanticRegisterBank {
                entries: &BTreeMap::new(),
                version: 0,
            },
            revision("selected-refusal"),
            |request| {
                allocations += 1;
                allocate(request)
            },
            no_original,
        )
        .unwrap_err();
        assert_eq!(allocations, 2);
        assert_eq!(error.code, EditErrorCode::InvalidCommand);
        assert!(error.message.contains(message), "{error:?}");
        assert_eq!(before, snapshot);
    }
    let mut allocations = 0;
    let error = plan_semantic(
        &before,
        &selected_inner(0, None),
        &program(vec![duration(3)]),
        SemanticRegisterBank {
            entries: &BTreeMap::new(),
            version: 0,
        },
        revision("selected-wrong-pool"),
        |request| {
            allocations += 1;
            let mut allocation = allocate(request)?;
            if allocations == 2 {
                let SemanticAllocation::Isolation { identities, .. } = &mut allocation else {
                    panic!()
                };
                identities.nodes.push(node("surplus-last"));
            }
            Ok(allocation)
        },
        no_original,
    )
    .unwrap_err();
    assert_eq!(allocations, 2);
    assert_eq!(error.code, EditErrorCode::InvalidCommand);
    assert!(
        error.message.contains("exact node and mark identity pools"),
        "{error:?}"
    );
    assert_eq!(before, snapshot);
}

#[test]
fn selected_hold_duration_rejects_duplicate_overlap_forged_and_noop_unsupported_targets_before_allocation()
 {
    let before = fixture(false);
    let base = selected_inner(0, None);
    let mut cases = Vec::new();
    let mut value = base.clone();
    let selected = value.selected_hold_duration.as_mut().unwrap();
    selected.also[0] = selected.target.clone();
    cases.push(value);
    let mut value = base.clone();
    value.selected_hold_duration.as_mut().unwrap().also[0].repeats[0].branch =
        RepeatEditBranch::Play {
            iteration: iteration("outer", 0),
        };
    cases.push(value);
    let mut value = base.clone();
    value.selected_hold_duration.as_mut().unwrap().also[0].repeats[1].branch =
        RepeatEditBranch::Play {
            iteration: iteration("inner", 8),
        };
    cases.push(value);
    let mut value = base.clone();
    value.selected_hold_duration.as_mut().unwrap().also[0].node = node("inner-owned");
    cases.push(value);
    let mut value = base.clone();
    value.selected_hold_duration.as_mut().unwrap().presentation = None;
    cases.push(value);
    let mut value = base.clone();
    value.shared_hold_duration = entry(Some(1)).shared_hold_duration;
    cases.push(value);
    let mut value = base.clone();
    value.visual_selection = Some(SemanticVisualSelection::Time {
        anchor: ProjectFrame(19),
        head: ProjectFrame(20),
        extending: false,
    });
    cases.push(value);
    for value in cases {
        let error = plan_semantic(
            &before,
            &value,
            &program(vec![duration(4)]),
            SemanticRegisterBank {
                entries: &BTreeMap::new(),
                version: 0,
            },
            revision("selected-forged"),
            |_| panic!("invalid set allocated"),
            no_original,
        )
        .unwrap_err();
        assert_eq!(error.code, EditErrorCode::SelectionUnavailable, "{error:?}");
        assert_ne!(error.code, EditErrorCode::IdentityConflict);
    }
    let mut retimed = before;
    // Fixed Edit windows are eligible. A Partition still cannot authorize a
    // selected duration change, even when that member would be a no-op.
    let mut rate = BeatNode::sequence("Unsupported", vec![]);
    rate.kind = NodeKind::Retime {
        child: node("inner-held"),
        duration: FrameDuration::new(4).unwrap(),
        mapping: range(0, 4),
        pitch: crate::PitchPolicy::FollowSpeed,
        purpose: crate::RetimePurpose::Partition,
    };
    retimed.nodes.insert(node("selected-rate"), rate);
    let NodeKind::Sequence { children } =
        &mut retimed.nodes.get_mut(&node("inner-owned")).unwrap().kind
    else {
        panic!()
    };
    children[1] = node("selected-rate");
    retimed.validate().unwrap();
    let error = plan_semantic(
        &retimed,
        &base,
        &program(vec![duration(4)]),
        SemanticRegisterBank {
            entries: &BTreeMap::new(),
            version: 0,
        },
        revision("selected-retime"),
        |_| panic!("unsupported no-op member allocated"),
        no_original,
    )
    .unwrap_err();
    assert_eq!(error.code, EditErrorCode::SelectionUnavailable);
    assert!(error.message.contains("unclocked Edit Retime"), "{error:?}");
}

#[test]
fn selected_hold_duration_three_plays_retain_all_stable_targets_through_common_clone() {
    let mut before = fixture(true);
    let NodeKind::Repeat { iterations, .. } =
        &mut before.nodes.get_mut(&node("inner")).unwrap().kind
    else {
        panic!()
    };
    // Author the initial three-play order with the public compact order API.
    // Retain its first two identities and append a distinct third identity;
    // SetRepeatPlays does not admit this directly nested Repeat parent.
    *iterations = iterations
        .resized(3, revision("selected-three-plays"))
        .unwrap();
    before.validate().unwrap();
    assert_eq!(before.duration().unwrap().frames(), 57);
    let NodeKind::Repeat { iterations, .. } = &before.nodes()[&node("inner")].kind else {
        panic!()
    };
    assert_eq!(iterations.at(0), Some(iteration("inner", 0)));
    assert_eq!(iterations.at(1), Some(iteration("inner", 1)));
    let retained = iterations.clone();
    let one = |position| ScopedNodeTarget {
        node: node(if position == 1 { "inner-held" } else { "held" }),
        repeats: vec![
            RepeatEditStep {
                repeat: node("outer"),
                branch: RepeatEditBranch::Play {
                    iteration: iteration("outer", 1),
                },
            },
            RepeatEditStep {
                repeat: node("inner"),
                branch: RepeatEditBranch::Play {
                    iteration: retained.at(position).unwrap(),
                },
            },
        ],
    };
    // Last play is primary, then the earlier default and owned counterpart.
    // The first two targets deliberately share the same original NodeId.
    let primary = one(2);
    let initial = SemanticContext {
        selected_child: Some(node("outer")),
        selected_hold_duration: Some(Box::new(SemanticSelectedHoldDurationContext {
            root: node("outer"),
            target: primary.clone(),
            also: vec![one(0), one(1)],
            presentation: Some(crate::InstancePath {
                node: primary.node.clone(),
                repeats: primary
                    .repeats
                    .iter()
                    .map(|step| {
                        let RepeatEditBranch::Play { iteration } = &step.branch else {
                            panic!()
                        };
                        crate::RepeatInstance {
                            node: step.repeat.clone(),
                            iteration: iteration.clone(),
                        }
                    })
                    .collect(),
            }),
        })),
        ..context("root", 40)
    };
    let selected = initial.selected_hold_duration.as_ref().unwrap();
    assert_eq!(selected.target.node, selected.also[0].node);
    assert_eq!(
        before
            .selected_hold_duration_visibility(&initial.parent, selected)
            .unwrap()
            .unwrap()
            .frames,
        range(39, 41)
    );
    let (planned, allocations) = observed(&before, &initial, vec![duration(3)], &BTreeMap::new());
    assert_eq!(
        allocations,
        [13, 4, 0]
            .into_iter()
            .enumerate()
            .map(|(step_index, nodes)| SemanticAllocationRequest::Isolation {
                step_index,
                nodes,
                marks: 0
            })
            .collect::<Vec<_>>()
    );
    assert_eq!(planned.document.duration().unwrap().frames(), 58);
    // Original selected inner lengths2,4,2 become3,3,3. Earlier changes
    // move the final primary39->40->39 after its own initial resize.
    assert_eq!(planned.context.cursor, ProjectFrame(39));
    let final_selection = planned.context.selected_hold_duration.as_ref().unwrap();
    let selected = std::iter::once(&final_selection.target)
        .chain(&final_selection.also)
        .collect::<Vec<_>>();
    assert_eq!(
        selected
            .iter()
            .map(|target| &target.node)
            .collect::<BTreeSet<_>>()
            .len(),
        3
    );
    for (target, position) in selected.iter().zip([2, 0, 1]) {
        assert!(
            matches!(&planned.document.nodes()[&target.node].kind, NodeKind::Hold { recipe } if recipe.duration.frames() == 3)
        );
        assert_eq!(
            target.repeats[1].branch,
            RepeatEditBranch::Play {
                iteration: retained.at(position).unwrap()
            }
        );
    }
    let mapped_inner = &final_selection.target.repeats[1].repeat;
    let NodeKind::Repeat { iterations, .. } = &planned.document.nodes()[mapped_inner].kind else {
        panic!()
    };
    assert_eq!(*iterations, retained);
    assert_eq!(
        planned.document.nodes()[&node("inner")],
        before.nodes()[&node("inner")]
    );
    assert_eq!(
        planned.document.nodes()[&node("held")],
        before.nodes()[&node("held")]
    );
    assert_eq!(planned.document.nodes().len() - before.nodes().len(), 17);
    check_proof(&before, &initial, &planned);
}

#[test]
fn selected_hold_duration_exact_target_bound_and_wire_bound_refuse_1025() {
    let mut before = fixture(false);
    let NodeKind::Repeat { iterations, .. } =
        &mut before.nodes.get_mut(&node("outer")).unwrap().kind
    else {
        panic!()
    };
    // One independent override at position2 is excluded from the selection.
    *iterations = IterationOrder::new(repeat_allocation("outer"), 1026).unwrap();
    before.validate().unwrap();
    let mut initial = selected_outer(false, 0);
    let authority = initial.selected_hold_duration.as_mut().unwrap();
    let one = |position| {
        let mut target = authority.target.clone();
        target.repeats[0].branch = RepeatEditBranch::Play {
            iteration: iteration("outer", position),
        };
        target
    };
    authority.also = (1..1025)
        .filter(|position| *position != 2)
        .map(one)
        .collect();
    assert_eq!(authority.also.len() + 1, crate::MAX_SCOPED_TARGETS);
    assert_eq!(
        before
            .selected_hold_duration_visibility(&initial.parent, authority)
            .unwrap()
            .unwrap()
            .frames,
        range(5, 7)
    );
    let wire = serde_json::to_value(&**authority).unwrap();
    assert_eq!(
        serde_json::from_value::<SemanticSelectedHoldDurationContext>(wire).unwrap(),
        **authority
    );
    let mut extra = authority.target.clone();
    extra.repeats[0].branch = RepeatEditBranch::Play {
        iteration: iteration("outer", 1025),
    };
    authority.also.push(extra);
    let error = before
        .selected_hold_duration_visibility(&initial.parent, authority)
        .unwrap_err();
    assert_eq!(error.code, EditErrorCode::SelectionUnavailable);
    assert!(error.message.contains("2..=1024"), "{error:?}");
    assert!(
        serde_json::from_value::<SemanticSelectedHoldDurationContext>(
            serde_json::to_value(&**authority).unwrap()
        )
        .is_err()
    );
    authority.also.clear();
    assert!(
        before
            .selected_hold_duration_visibility(&initial.parent, authority)
            .unwrap_err()
            .message
            .contains("2..=1024")
    );
    assert!(
        serde_json::from_value::<SemanticSelectedHoldDurationContext>(
            serde_json::to_value(&**authority).unwrap()
        )
        .is_err()
    );
}
