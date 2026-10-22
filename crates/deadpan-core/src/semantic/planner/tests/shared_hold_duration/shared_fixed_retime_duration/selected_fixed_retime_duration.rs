//! Selected bodies preserve disjoint literal families inside fixed windows.
use super::*;
use crate::SemanticSelectedHoldDurationContext;

fn selected_fixture(pitch: PitchPolicy, crop: i64) -> ProjectDocument {
    let mut d = fixed(pitch, crop, 6);
    d.nodes.insert(node("inner"), repeated("inner", "stage", 2));
    let NodeKind::Repeat { child, .. } = &mut d.nodes.get_mut(&node("outer")).unwrap().kind else {
        panic!()
    };
    *child = node("inner");
    d.validate().unwrap();
    assert_eq!(d.duration().unwrap().frames(), 42);
    d
}

// The selected axis has two distinct explicit Plays. The other level keeps
// either literal Default authority or its separately captured second Play.
fn selected_context(inner_axis: bool, other_default: bool, primary: u32) -> SemanticContext {
    let one = |position| {
        let mut value = target();
        value.repeats[usize::from(inner_axis)].branch = RepeatEditBranch::Play {
            iteration: iteration(if inner_axis { "inner" } else { "outer" }, position),
        };
        if !other_default {
            value.repeats[usize::from(!inner_axis)].branch = RepeatEditBranch::Play {
                iteration: iteration(if inner_axis { "outer" } else { "inner" }, 1),
            };
        }
        value
    };
    SemanticContext {
        selected_child: Some(node("outer")),
        selected_hold_duration: Some(Box::new(SemanticSelectedHoldDurationContext {
            root: node("outer"),
            target: one(primary),
            also: vec![one(1 - primary)],
            presentation: Some(instance(
                "held",
                if inner_axis { 1 } else { primary },
                Some(if inner_axis { primary } else { 1 }),
            )),
        })),
        ..context("root", 20)
    }
}

fn plan_selected(
    d: &ProjectDocument,
    c: &SemanticContext,
    instructions: Vec<SemanticInstruction>,
) -> Result<(SemanticPlan, Vec<SemanticAllocationRequest>), EditError> {
    let mut requests = Vec::new();
    let planned = plan_semantic(
        d,
        c,
        &program(instructions),
        SemanticRegisterBank {
            entries: &BTreeMap::new(),
            version: 0,
        },
        revision(&format!("{}-selected-fixed", d.revision_id())),
        |request| {
            requests.push(request.clone());
            allocate_selected(d, request)
        },
        no_original,
    )?;
    Ok((planned, requests))
}

fn allocate_selected(
    d: &ProjectDocument,
    request: SemanticAllocationRequest,
) -> Result<SemanticAllocation, EditError> {
    let SemanticAllocationRequest::Isolation {
        step_index,
        nodes,
        marks,
    } = request
    else {
        panic!("duration requires isolation")
    };
    Ok(SemanticAllocation::Isolation {
        new_revision: revision(&format!("{}-selected-leaf-{step_index}", d.revision_id())),
        identities: crate::OccurrenceIdentities {
            nodes: (0..nodes)
                .map(|index| {
                    node(&format!(
                        "{}-selected-node-{step_index}-{index}",
                        d.revision_id()
                    ))
                })
                .collect(),
            marks: (0..marks)
                .map(|index| {
                    MarkId::new(format!(
                        "{}-selected-mark-{step_index}-{index}",
                        d.revision_id()
                    ))
                    .unwrap()
                })
                .collect(),
        },
    })
}

fn visibility(d: &ProjectDocument, c: &SemanticContext) -> HoldDurationVisibility {
    d.selected_hold_duration_visibility(&c.parent, c.selected_hold_duration.as_ref().unwrap())
        .unwrap()
        .unwrap()
}

fn check_selected(d: &ProjectDocument, c: &SemanticContext, p: &SemanticPlan, length: i64) {
    assert_eq!(p.document.duration(), d.duration());
    let before = c.selected_hold_duration.as_ref().unwrap();
    let after = p.context.selected_hold_duration.as_ref().unwrap();
    let proof =
        crate::derive_scoped_isolation(d, p.request.as_ref().unwrap(), &p.document).unwrap();
    assert_eq!(proof.map_forward(&before.target).unwrap(), after.target);
    assert_eq!(proof.map_forward(&before.also[0]).unwrap(), after.also[0]);
    assert_eq!(proof.map_backward(&after.target).unwrap(), before.target);
    assert_eq!(proof.map_backward(&after.also[0]).unwrap(), before.also[0]);
    assert_eq!(
        after.presentation,
        before
            .presentation
            .as_ref()
            .map(|v| proof.map_instance_forward(v).unwrap())
    );
    assert_ne!(after.target.node, after.also[0].node);
    let chosen = [&before.target, &before.also[0]];
    let mut changed = 0;
    for outer in 0..3 {
        for inner in 0..2 {
            let old = instance("held", outer, Some(inner));
            let mapped = proof.map_instance_forward(&old).unwrap();
            let affected = chosen.iter().any(|t| t.matches_instance(d, &old).unwrap());
            changed += usize::from(affected);
            let NodeKind::Hold { recipe } = &p.document.nodes()[&mapped.node].kind else {
                panic!()
            };
            assert_eq!(recipe.duration.frames(), if affected { length } else { 2 });
            let stage_instance = proof
                .map_instance_forward(&instance("stage", outer, Some(inner)))
                .unwrap();
            let mut expected = d.nodes()[&node("stage")].clone();
            let NodeKind::Retime { child, .. } = &mut expected.kind else {
                panic!()
            };
            *child = proof
                .map_instance_forward(&instance("body", outer, Some(inner)))
                .unwrap()
                .node;
            assert_eq!(p.document.nodes()[&stage_instance.node], expected);
        }
    }
    assert!((2..=6).contains(&changed));
    for (id, old) in d.nodes() {
        assert_eq!(&p.document.nodes()[id], old);
    }
    assert_eq!(
        p.context.cursor,
        visibility(&p.document, &p.context).frames.start()
    );
    assert_eq!(p.context.parent, c.parent);
    assert_eq!(p.context.selected_child, c.selected_child);
    reversible(d, p);
}

#[test]
fn selected_fixed_retime_duration_both_axes_literal_families_pitches_crops_and_owned_requery() {
    for inner_axis in [false, true] {
        for other_default in [false, true] {
            for primary in [0, 1] {
                for pitch in [
                    PitchPolicy::FollowSpeed,
                    PitchPolicy::Preserve,
                    PitchPolicy::Shift { semitones: 3 },
                ] {
                    for crop in [0, 3] {
                        let d = selected_fixture(pitch, crop);
                        let mut c = selected_context(inner_axis, other_default, primary);
                        c.cursor = visibility(&d, &c).frames.start();
                        let old = visibility(&d, &c);
                        let (p, allocations) = plan_selected(&d, &c, vec![duration(4)]).unwrap();
                        let expected = match (inner_axis, other_default) {
                            (true, true) => [5, 5],
                            (true, false) => [11, 5],
                            (false, true) => [6, 6],
                            (false, false) => [11, 11],
                        };
                        assert_eq!(
                            allocations,
                            expected
                                .into_iter()
                                .enumerate()
                                .map(|(step_index, nodes)| SemanticAllocationRequest::Isolation {
                                    step_index,
                                    nodes,
                                    marks: 0
                                })
                                .collect::<Vec<_>>()
                        );
                        assert_eq!(
                            p.document.nodes().len(),
                            d.nodes().len() + expected.into_iter().sum::<usize>()
                        );
                        assert_eq!(
                            visibility(&p.document, &p.context).exact.start,
                            old.exact.start
                        );
                        check_selected(&d, &c, &p, 4);
                        let shrink = if crop == 0 { 1 } else { 3 };
                        let (next, allocations) =
                            plan_selected(&p.document, &p.context, vec![duration(shrink)]).unwrap();
                        assert_eq!(
                            allocations,
                            [
                                SemanticAllocationRequest::Isolation {
                                    step_index: 0,
                                    nodes: 0,
                                    marks: 0
                                },
                                SemanticAllocationRequest::Isolation {
                                    step_index: 1,
                                    nodes: 0,
                                    marks: 0
                                }
                            ]
                        );
                        assert_eq!(next.document.nodes().len(), p.document.nodes().len());
                        assert_eq!(
                            next.context.selected_hold_duration,
                            p.context.selected_hold_duration
                        );
                        // Existing owned definitions change; original untouched
                        // template assertions belong to the fresh isolation.
                        assert_eq!(next.document.duration(), d.duration());
                        reversible(&p.document, &next);
                    }
                }
            }
        }
    }
}

#[test]
fn selected_fixed_retime_duration_partial_noop_still_maps_primary_and_counted_calls() {
    let d = selected_fixture(PitchPolicy::FollowSpeed, 0);
    let c = selected_context(true, false, 1);
    let (first, _) = plan_selected(&d, &c, vec![duration(4)]).unwrap();
    let mut partial = first.document.clone();
    let a = first.context.selected_hold_duration.as_ref().unwrap();
    let NodeKind::Hold { recipe } = &mut partial.nodes.get_mut(&a.also[0].node).unwrap().kind
    else {
        panic!()
    };
    recipe.duration = FrameDuration::new(2).unwrap();
    partial.validate().unwrap();
    let (second, allocations) = plan_selected(&partial, &first.context, vec![duration(4)]).unwrap();
    assert_eq!(
        allocations,
        [SemanticAllocationRequest::Isolation {
            step_index: 0,
            nodes: 0,
            marks: 0
        }]
    );
    assert_eq!(
        second.context.selected_hold_duration,
        first.context.selected_hold_duration
    );
    reversible(&partial, &second);
    let (counted, allocations) =
        plan_selected(&d, &c, vec![duration(4), duration(3), duration(4)]).unwrap();
    assert_eq!(allocations.len(), 6);
    assert!(allocations[2..].iter().all(|r| matches!(
        r,
        SemanticAllocationRequest::Isolation {
            nodes: 0,
            marks: 0,
            ..
        }
    )));
    check_selected(&d, &c, &counted, 4);
    let error = plan_selected(&first.document, &first.context, vec![duration(4)]).unwrap_err();
    assert!(error.message.contains("already have that duration"));
}

#[test]
fn selected_fixed_retime_duration_hidden_or_unsampled_counterpart_refuses_before_allocating() {
    for output in [1, 6] {
        let mut d = selected_fixture(PitchPolicy::FollowSpeed, 0);
        // Only the second selected outer branch gets a different fixed crop.
        d.nodes.extend([
            (
                node("bad-stage"),
                stage(
                    "bad-body",
                    output,
                    if output == 1 { 0 } else { 5 },
                    8,
                    PitchPolicy::FollowSpeed,
                ),
            ),
            (
                node("bad-body"),
                BeatNode::sequence(
                    "Counterpart",
                    vec![node("bad-before"), node("bad-held"), node("bad-speech")],
                ),
            ),
            (node("bad-before"), hold(2)),
            (node("bad-held"), hold(2)),
            (node("bad-speech"), hold(6)),
        ]);
        d.overrides.insert(
            node("inner"),
            PlayOverrides::try_from(vec![PlayOverride {
                iteration: iteration("inner", 1),
                root: node("bad-stage"),
            }])
            .unwrap(),
        );
        d.validate().unwrap();
        let mut c = selected_context(true, false, 0);
        c.selected_hold_duration.as_mut().unwrap().also[0].node = node("bad-held");
        let error = plan_semantic(
            &d,
            &c,
            &program(vec![duration(2)]),
            SemanticRegisterBank {
                entries: &BTreeMap::new(),
                version: 0,
            },
            revision("hidden-selected"),
            |_| panic!("invalid no-op counterpart allocated"),
            no_original,
        )
        .unwrap_err();
        assert_eq!(error.code, EditErrorCode::SelectionUnavailable);
        assert!(
            error.message.contains("hidden") || error.message.contains("sampled"),
            "{error:?}"
        );
    }
}

#[test]
fn selected_fixed_retime_duration_late_visibility_and_pool_failures_leave_input_unchanged() {
    let d = selected_fixture(PitchPolicy::FollowSpeed, 3);
    let c = selected_context(true, false, 0);
    let snapshot = d.to_json().unwrap();
    let error = plan_selected(&d, &c, vec![duration(1)]).unwrap_err();
    assert!(
        error.message.contains("hidden") || error.message.contains("sampled"),
        "{error:?}"
    );
    assert_eq!(d.to_json().unwrap(), snapshot);
    for surplus in [false, true] {
        let mut count = 0;
        let error = plan_semantic(
            &d,
            &c,
            &program(vec![duration(4)]),
            SemanticRegisterBank {
                entries: &BTreeMap::new(),
                version: 0,
            },
            revision("bad-final-pool"),
            |request| {
                let mut allocation = allocate_selected(&d, request)?;
                count += 1;
                if count == 2 {
                    let SemanticAllocation::Isolation { identities, .. } = &mut allocation else {
                        panic!()
                    };
                    if surplus {
                        identities.nodes.push(node("surplus"));
                    } else {
                        identities.nodes.pop();
                    }
                }
                Ok(allocation)
            },
            no_original,
        )
        .unwrap_err();
        assert_eq!(count, 2);
        assert!(
            error.message.contains("exact node and mark identity pools"),
            "{error:?}"
        );
        assert_eq!(d.to_json().unwrap(), snapshot);
    }
}

#[test]
fn selected_fixed_retime_duration_dormant_defaults_keep_absent_presentation_and_cursor() {
    let mut d = selected_fixture(PitchPolicy::Preserve, 0);
    d.nodes
        .extend([(node("live-0"), hold(6)), (node("live-1"), hold(6))]);
    d.overrides.insert(
        node("inner"),
        PlayOverrides::try_from(
            (0..2)
                .map(|ordinal| PlayOverride {
                    iteration: iteration("inner", ordinal),
                    root: node(&format!("live-{ordinal}")),
                })
                .collect::<Vec<_>>(),
        )
        .unwrap(),
    );
    d.validate().unwrap();
    let mut c = selected_context(false, true, 1);
    c.selected_hold_duration.as_mut().unwrap().presentation = None;
    assert!(
        d.selected_hold_duration_visibility(&c.parent, c.selected_hold_duration.as_ref().unwrap())
            .unwrap()
            .is_none()
    );
    let (p, requests) = plan_selected(&d, &c, vec![duration(4)]).unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(p.document.duration(), d.duration());
    assert_eq!(p.context.cursor, c.cursor);
    let after = p.context.selected_hold_duration.as_ref().unwrap();
    assert!(after.presentation.is_none());
    assert!(
        p.document
            .selected_hold_duration_visibility(&c.parent, after)
            .unwrap()
            .is_none()
    );
    let proof =
        crate::derive_scoped_isolation(&d, p.request.as_ref().unwrap(), &p.document).unwrap();
    let before = c.selected_hold_duration.as_ref().unwrap();
    for (old, new) in [&before.target, &before.also[0]]
        .into_iter()
        .zip([&after.target, &after.also[0]])
    {
        assert_eq!(proof.map_forward(old).unwrap(), *new);
        assert_eq!(proof.map_backward(new).unwrap(), *old);
        assert!(
            matches!(&p.document.nodes()[&new.node].kind, NodeKind::Hold { recipe } if recipe.duration.frames() == 4)
        );
    }
    for (id, original) in d.nodes() {
        assert_eq!(&p.document.nodes()[id], original);
    }
    reversible(&d, &p);
}
