//! Owned gaps keep explicit branch authority and current-position liveness.
use super::*;
use crate::{SemanticMixedHoldDurationContext, SemanticSelectedHoldDurationContext};

fn gaps(plays: u32) -> ProjectDocument {
    let mut d = ProjectDocument::new(
        ProjectId::new("owned-gap-duration").unwrap(),
        revision("gap-base"),
        crate::basis::default_basis(),
        node("root"),
    )
    .unwrap();
    d.nodes.extend([
        (
            node("root"),
            BeatNode::sequence("Root", vec![node("lead"), node("outer"), node("tail")]),
        ),
        (node("lead"), hold(1)),
        (node("tail"), hold(1)),
        (node("outer"), repeated("outer", "inner", 3)),
        (node("inner"), repeated("inner", "body", plays)),
        (node("body"), hold(2)),
        (node("outer-owned"), hold(7)),
    ]);
    let NodeKind::Repeat { gap, .. } = &mut d.nodes.get_mut(&node("inner")).unwrap().kind else {
        panic!()
    };
    *gap = Some(match &hold(1).kind {
        NodeKind::Hold { recipe } => recipe.clone(),
        _ => panic!(),
    });
    d.overrides.insert(
        node("outer"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: iteration("outer", 2),
            root: node("outer-owned"),
        }])
        .unwrap(),
    );
    let entries = (0..plays)
        .map(|i| {
            let id = node(&format!("gap-{i}"));
            d.nodes.insert(id.clone(), hold(3 + i64::from(i)));
            PlayOverride {
                iteration: iteration("inner", i),
                root: id,
            }
        })
        .collect::<Vec<_>>();
    d.gap_overrides
        .insert(node("inner"), PlayOverrides::try_from(entries).unwrap());
    d.validate().unwrap();
    assert_eq!(
        d.duration().unwrap().frames(),
        if plays == 3 { 35 } else { 49 }
    );
    d
}

fn gap_target(outer: Option<u32>, gap: u32) -> ScopedNodeTarget {
    ScopedNodeTarget {
        node: node(&format!("gap-{gap}")),
        repeats: vec![
            RepeatEditStep {
                repeat: node("outer"),
                branch: outer.map_or(RepeatEditBranch::Default, |i| RepeatEditBranch::Play {
                    iteration: iteration("outer", i),
                }),
            },
            RepeatEditStep {
                repeat: node("inner"),
                branch: RepeatEditBranch::Play {
                    iteration: iteration("inner", gap),
                },
            },
        ],
    }
}

fn mixed_gap(gap: u32, live: bool) -> SemanticContext {
    SemanticContext {
        selected_child: Some(node("outer")),
        mixed_hold_duration: Some(Box::new(SemanticMixedHoldDurationContext {
            root: node("outer"),
            target: gap_target(None, gap),
            presentation: live.then(|| instance(&format!("gap-{gap}"), 1, Some(gap))),
        })),
        ..context("root", 20)
    }
}

fn selected_gaps(outer: Option<u32>, order: &[u32], live: bool) -> SemanticContext {
    SemanticContext {
        selected_child: Some(node("outer")),
        selected_hold_duration: Some(Box::new(SemanticSelectedHoldDurationContext {
            root: node("outer"),
            target: gap_target(outer, order[0]),
            also: order[1..].iter().map(|&i| gap_target(outer, i)).collect(),
            presentation: live.then(|| {
                instance(
                    &format!("gap-{}", order[0]),
                    outer.unwrap_or(1),
                    Some(order[0]),
                )
            }),
        })),
        ..context("root", 20)
    }
}

fn observed(
    d: &ProjectDocument,
    c: &SemanticContext,
    instructions: Vec<SemanticInstruction>,
) -> (SemanticPlan, Vec<(usize, usize)>) {
    let mut pools = Vec::new();
    let result = plan_semantic(
        d,
        c,
        &program(instructions),
        SemanticRegisterBank {
            entries: &BTreeMap::new(),
            version: 0,
        },
        revision("gap-final"),
        |r| {
            let SemanticAllocationRequest::Isolation { nodes, marks, .. } = &r else {
                panic!("{r:?}")
            };
            pools.push((*nodes, *marks));
            allocate(r)
        },
        no_original,
    )
    .unwrap();
    (result, pools)
}

fn proof(d: &ProjectDocument, c: &SemanticContext, p: &SemanticPlan) {
    let mapped =
        crate::derive_scoped_isolation(d, p.request.as_ref().unwrap(), &p.document).unwrap();
    if let Some(old) = &c.mixed_hold_duration {
        let new = p.context.mixed_hold_duration.as_ref().unwrap();
        assert_eq!(mapped.map_forward(&old.target).unwrap(), new.target);
        assert_eq!(
            old.presentation
                .as_ref()
                .map(|i| mapped.map_instance_forward(i).unwrap()),
            new.presentation
        );
    } else {
        let old = c.selected_hold_duration.as_ref().unwrap();
        let new = p.context.selected_hold_duration.as_ref().unwrap();
        for (old, new) in std::iter::once(&old.target)
            .chain(&old.also)
            .zip(std::iter::once(&new.target).chain(&new.also))
        {
            assert_eq!(mapped.map_forward(old).unwrap(), *new);
            assert_eq!(mapped.map_backward(new).unwrap(), *old);
        }
        assert_eq!(
            old.presentation
                .as_ref()
                .map(|i| mapped.map_instance_forward(i).unwrap()),
            new.presentation
        );
    }
    assert_eq!(p.context.parent, c.parent);
    assert_eq!(p.context.selected_child, c.selected_child);
    reversible(d, p);
}

#[test]
fn owned_gap_hold_duration_mixed_default_then_play_preserves_other_branches() {
    let d = gaps(3);
    for value in [1, 4] {
        let c = mixed_gap(0, true);
        let (p, pools) = observed(&d, &c, vec![duration(value)]);
        assert_eq!(pools, [(0, 0)]);
        assert_eq!(
            p.document.duration().unwrap().frames(),
            35 + 2 * (i64::from(value) - 3)
        );
        assert_eq!(p.context.cursor, ProjectFrame(16 + i64::from(value) - 3));
        for id in ["body", "gap-1", "gap-2", "outer-owned"] {
            assert_eq!(p.document.nodes()[&node(id)], d.nodes()[&node(id)]);
        }
        assert_eq!(p.document.gap_overrides(), d.gap_overrides());
        let NodeKind::Repeat { gap: a, .. } = &d.nodes()[&node("inner")].kind else {
            panic!()
        };
        let NodeKind::Repeat { gap: b, .. } = &p.document.nodes()[&node("inner")].kind else {
            panic!()
        };
        assert_eq!(a, b);
        proof(&d, &c, &p);
    }
}

#[test]
fn owned_gap_hold_duration_mixed_play_gap_then_default_body_is_owned_not_cloned() {
    let mut d = gaps(3);
    // Replace one existing owned gap leaf with an ordinary nested Repeat.
    d.nodes.get_mut(&node("gap-0")).unwrap().kind = NodeKind::Repeat {
        child: node("nested-held"),
        iterations: IterationOrder::new(repeat_allocation("nested"), 2).unwrap(),
        gap: None,
        escalation: None,
    };
    d.nodes.insert(node("nested-held"), hold(3));
    d.validate().unwrap();
    assert_eq!(d.duration().unwrap().frames(), 41);
    let c = SemanticContext {
        selected_child: Some(node("outer")),
        mixed_hold_duration: Some(Box::new(SemanticMixedHoldDurationContext {
            root: node("outer"),
            target: ScopedNodeTarget {
                node: node("nested-held"),
                repeats: vec![
                    RepeatEditStep {
                        repeat: node("outer"),
                        branch: RepeatEditBranch::Default,
                    },
                    RepeatEditStep {
                        repeat: node("inner"),
                        branch: RepeatEditBranch::Play {
                            iteration: iteration("inner", 0),
                        },
                    },
                    RepeatEditStep {
                        repeat: node("gap-0"),
                        branch: RepeatEditBranch::Default,
                    },
                ],
            },
            presentation: Some(InstancePath {
                node: node("nested-held"),
                repeats: vec![
                    crate::RepeatInstance {
                        node: node("outer"),
                        iteration: iteration("outer", 1),
                    },
                    crate::RepeatInstance {
                        node: node("inner"),
                        iteration: iteration("inner", 0),
                    },
                    crate::RepeatInstance {
                        node: node("gap-0"),
                        iteration: iteration("nested", 0),
                    },
                ],
            }),
        })),
        ..context("root", 20)
    };
    let (p, pools) = observed(&d, &c, vec![duration(4)]);
    assert_eq!(pools, [(0, 0)]);
    assert_eq!(p.document.duration().unwrap().frames(), 45);
    proof(&d, &c, &p);

    // Actual outer Play / inner Default direction: a root Repeat's own gap
    // contains a repeated body. That gap edge is independently owned.
    let mut only = d.clone();
    let NodeKind::Repeat { child, .. } = &mut only.nodes.get_mut(&node("outer")).unwrap().kind
    else {
        panic!()
    };
    *child = node("outer-body");
    only.nodes.insert(node("outer-body"), hold(2));
    only.overrides.clear();
    only.gap_overrides.remove(&node("inner"));
    for id in ["inner", "body", "gap-1", "gap-2", "outer-owned"] {
        only.nodes.remove(&node(id));
    }
    only.gap_overrides.insert(
        node("outer"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: iteration("outer", 0),
            root: node("gap-0"),
        }])
        .unwrap(),
    );
    only.validate().unwrap();
    assert_eq!(only.duration().unwrap().frames(), 14);
    let mut c = c;
    let authority = c.mixed_hold_duration.as_mut().unwrap();
    authority.target.repeats = vec![
        RepeatEditStep {
            repeat: node("outer"),
            branch: RepeatEditBranch::Play {
                iteration: iteration("outer", 0),
            },
        },
        RepeatEditStep {
            repeat: node("gap-0"),
            branch: RepeatEditBranch::Default,
        },
    ];
    authority.presentation = Some(InstancePath {
        node: node("nested-held"),
        repeats: vec![
            crate::RepeatInstance {
                node: node("outer"),
                iteration: iteration("outer", 0),
            },
            crate::RepeatInstance {
                node: node("gap-0"),
                iteration: iteration("nested", 0),
            },
        ],
    });
    c.cursor = ProjectFrame(3);
    let (p, pools) = observed(&only, &c, vec![duration(4)]);
    assert_eq!(pools, [(0, 0)]);
    assert_eq!(p.document.duration().unwrap().frames(), 16);
    assert_eq!(p.context.cursor, ProjectFrame(3));
    proof(&only, &c, &p);
}

#[test]
fn owned_gap_hold_duration_three_selected_gaps_map_marks_and_final_primary() {
    let mut d = gaps(4);
    d.marks.insert(
        crate::MarkId::new("gap-local").unwrap(),
        Mark {
            label: "gap-local".into(),
            owner: node("gap-0"),
            boundary: BoundaryAnchor {
                coordinate: Anchor::Local {
                    node: node("gap-0"),
                    position: ExactRatio::new(5, 2).unwrap(),
                },
                bias: InsertionBias::Right,
            },
            loss_policy: AnchorLossPolicy::KeepUnresolved,
            state: MarkState::Bound,
            fragments: vec![],
        },
    );
    d.validate().unwrap();
    let c = selected_gaps(Some(1), &[2, 0, 1], true);
    let (p, pools) = observed(&d, &c, vec![duration(2)]);
    assert_eq!(pools, [(6, 1), (0, 0), (0, 0)]);
    assert_eq!(p.document.duration().unwrap().frames(), 43);
    assert_eq!(p.context.cursor, ProjectFrame(31));
    let a = p.context.selected_hold_duration.as_ref().unwrap();
    for target in std::iter::once(&a.target).chain(&a.also) {
        assert!(
            matches!(&p.document.nodes()[&target.node].kind, NodeKind::Hold { recipe } if recipe.duration.frames() == 2)
        );
    }
    let copied = crate::MarkId::new("isolate-0-0").unwrap();
    assert!(matches!(
        &p.document.marks()[&copied].state,
        MarkState::Unresolved { .. }
    ));
    assert_eq!(
        p.document.marks()[&crate::MarkId::new("gap-local").unwrap()],
        d.marks()[&crate::MarkId::new("gap-local").unwrap()]
    );
    proof(&d, &c, &p);
}

#[test]
fn owned_gap_hold_duration_dormant_primary_live_and_noop_members_use_actual_proof() {
    let d = gaps(3);
    for outer in [None, Some(1)] {
        let c = selected_gaps(outer, &[2, 0, 1], false);
        let (p, pools) = observed(&d, &c, vec![duration(3)]);
        assert_eq!(
            pools,
            if outer.is_some() {
                vec![(5, 0), (0, 0)]
            } else {
                vec![(0, 0), (0, 0)]
            }
        );
        assert_eq!(p.context.cursor, c.cursor);
        assert_eq!(
            p.document.duration().unwrap().frames(),
            if outer.is_some() { 34 } else { 33 }
        );
        assert!(
            p.context
                .selected_hold_duration
                .as_ref()
                .unwrap()
                .presentation
                .is_none()
        );
        proof(&d, &c, &p);
    }
    let c = mixed_gap(2, false);
    let (p, pools) = observed(&d, &c, vec![duration(2), duration(6)]);
    assert_eq!(pools, [(0, 0), (0, 0)]);
    assert_eq!(p.document.duration(), d.duration());
    assert_eq!(p.context.cursor, c.cursor);
    assert_eq!(p.document.audio_bindings(), d.audio_bindings());
    proof(&d, &c, &p);
}

#[test]
fn owned_gap_hold_duration_current_position_not_ordinal_controls_dormancy() {
    let mut d = gaps(3);
    let NodeKind::Repeat { iterations, .. } = &mut d.nodes.get_mut(&node("inner")).unwrap().kind
    else {
        panic!()
    };
    *iterations = iterations.moved(2, 3, 0).unwrap();
    d.validate().unwrap();
    assert_eq!(d.duration().unwrap().frames(), 37);
    let formerly_final = mixed_gap(2, true);
    let visible = d
        .mixed_hold_duration_visibility(
            &formerly_final.parent,
            formerly_final.mixed_hold_duration.as_ref().unwrap(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(visible.frames, range(17, 22));
    let now_final = mixed_gap(1, false);
    assert!(
        d.mixed_hold_duration_visibility(
            &now_final.parent,
            now_final.mixed_hold_duration.as_ref().unwrap()
        )
        .unwrap()
        .is_none()
    );
    let (p, _) = observed(&d, &now_final, vec![duration(2)]);
    assert_eq!(p.document.duration(), d.duration());
    assert_eq!(p.context.cursor, now_final.cursor);
    proof(&d, &now_final, &p);
}

#[test]
fn owned_gap_hold_duration_dormant_all_play_raw_works_but_live_boundary_api_stays_strict() {
    let d = gaps(3);
    let path = instance("gap-2", 1, Some(2));
    let needs = d
        .occurrence_hold_duration_requirements(&path, FrameDuration::new(2).unwrap())
        .unwrap();
    assert_eq!((needs.nodes, needs.marks), (5, 0));
    let request = crate::CommandRequest {
        project_id: d.project_id().clone(),
        expected_revision: d.revision_id().clone(),
        new_revision: revision("dormant-gap-raw"),
        command: Command::EditOccurrence {
            instance: path,
            edit: crate::OccurrenceEdit::SetHoldDuration {
                duration: FrameDuration::new(2).unwrap(),
            },
            identities: crate::OccurrenceIdentities {
                nodes: (0..needs.nodes)
                    .map(|i| node(&format!("dormant-gap-{i}")))
                    .collect(),
                marks: vec![],
            },
        },
    };
    let (tx, after) = crate::apply_with_result(&d, &request).unwrap();
    assert_eq!(tx.duration_delta, 0);
    assert_eq!(after.duration(), d.duration());
    assert!(crate::prepare_occurrence_hold_duration(&d, &request).is_err());
    assert_eq!(tx.inverse.apply(&after).unwrap(), d);
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
}

#[test]
fn owned_gap_hold_duration_million_default_uses_remain_compact() {
    let mut d = gaps(3);
    let NodeKind::Repeat { iterations, .. } = &mut d.nodes.get_mut(&node("outer")).unwrap().kind
    else {
        panic!()
    };
    *iterations = IterationOrder::new(repeat_allocation("outer"), 1_000_000).unwrap();
    d.validate().unwrap();
    let c = mixed_gap(0, true);
    let (p, pools) = observed(&d, &c, vec![duration(4)]);
    assert_eq!(pools, [(0, 0)]);
    assert_eq!(
        p.document.duration().unwrap().frames() - d.duration().unwrap().frames(),
        999_999
    );
    assert_eq!(p.document.nodes().len(), d.nodes().len());
    assert_eq!(p.document.audio_bindings(), d.audio_bindings());
    proof(&d, &c, &p);
}

#[test]
fn owned_gap_hold_duration_forged_missing_and_late_failures_are_atomic() {
    let d = gaps(3);
    let mut forged = selected_gaps(None, &[0, 1], true);
    forged.selected_hold_duration.as_mut().unwrap().also[0].repeats[1].branch =
        RepeatEditBranch::Play {
            iteration: iteration("inner", 0),
        };
    let error = plan_semantic(
        &d,
        &forged,
        &program(vec![duration(2)]),
        SemanticRegisterBank {
            entries: &BTreeMap::new(),
            version: 0,
        },
        revision("forged-gap"),
        |_| panic!("forged gap allocated"),
        no_original,
    )
    .unwrap_err();
    assert_eq!(error.code, EditErrorCode::SelectionUnavailable);
    assert!(error.message.contains("outside its selected Play"));
    let mut missing = selected_gaps(None, &[0, 1], true);
    missing.selected_hold_duration.as_mut().unwrap().also[0].node = node("missing-gap");
    assert!(
        d.selected_hold_duration_visibility(
            &missing.parent,
            missing.selected_hold_duration.as_ref().unwrap()
        )
        .is_err()
    );
    let c = selected_gaps(Some(1), &[0, 1], true);
    let snapshot = d.to_json().unwrap();
    let mut allocations = 0;
    let error = plan_semantic(
        &d,
        &c,
        &program(vec![duration(2), duration(2)]),
        SemanticRegisterBank {
            entries: &BTreeMap::new(),
            version: 0,
        },
        revision("late-gap"),
        |r| {
            allocations += 1;
            allocate(r)
        },
        no_original,
    )
    .unwrap_err();
    assert_eq!(allocations, 2);
    assert!(error.message.contains("already have that duration"));
    assert_eq!(d.to_json().unwrap(), snapshot);
    let mut allocations = 0;
    let error = plan_semantic(
        &d,
        &c,
        &program(vec![duration(2)]),
        SemanticRegisterBank {
            entries: &BTreeMap::new(),
            version: 0,
        },
        revision("gap-pool"),
        |r| {
            allocations += 1;
            let mut value = allocate(r)?;
            if allocations == 2 {
                let SemanticAllocation::Isolation { identities, .. } = &mut value else {
                    panic!()
                };
                identities.nodes.push(node("surplus-gap-pool"));
            }
            Ok(value)
        },
        no_original,
    )
    .unwrap_err();
    assert_eq!(allocations, 2);
    assert!(error.message.contains("exact node and mark identity pools"));
    assert_eq!(d.to_json().unwrap(), snapshot);
}

#[test]
fn owned_gap_hold_duration_materialized_gap_keeps_closed_lattice_and_partial_clocks() {
    let mut d = gaps(3);
    let media = fixture(true);
    d.presentation_basis = media.presentation_basis().clone();
    d.assets = media.assets().clone();
    let NodeKind::Hold { recipe: audible } = &media.nodes()[&node("held")].kind else {
        panic!()
    };
    let mut default_gap = audible.clone();
    default_gap.duration = FrameDuration::new(1).unwrap();
    let NodeKind::Repeat { gap, .. } = &mut d.nodes.get_mut(&node("inner")).unwrap().kind else {
        panic!()
    };
    *gap = Some(default_gap);
    d.nodes.remove(&node("gap-0"));
    d.gap_overrides.insert(
        node("inner"),
        PlayOverrides::try_from(vec![
            PlayOverride {
                iteration: iteration("inner", 1),
                root: node("gap-1"),
            },
            PlayOverride {
                iteration: iteration("inner", 2),
                root: node("gap-2"),
            },
        ])
        .unwrap(),
    );
    d.nodes
        .insert(node("body"), media.nodes()[&node("before")].clone());
    let mut tail = media.nodes()[&node("tail")].clone();
    let NodeKind::Source { source } = &mut tail.kind else {
        panic!()
    };
    source.duration = FrameDuration::new(1).unwrap();
    d.nodes.insert(node("tail"), tail);
    d.validate().unwrap();
    assert_eq!(d.duration().unwrap().frames(), 31);
    let allocation = revision("gap-materialize");
    let (_, mut d) = crate::apply_with_result(
        &d,
        &crate::CommandRequest {
            project_id: d.project_id().clone(),
            expected_revision: d.revision_id().clone(),
            new_revision: allocation.clone(),
            command: Command::IsolateGap {
                node: node("inner"),
                iteration: iteration("inner", 0),
                id: node("gap-0"),
                timing: AudioTimingId {
                    allocation,
                    ordinal: 0,
                },
            },
        },
    )
    .unwrap();
    let closed = d.audio_bindings().bindings()[&node("gap-0")]
        .lattice
        .clone();
    d.audio_bindings = crate::capture_unbound_audio_bindings(
        &d,
        AudioTimingId {
            allocation: revision("gap-partial-clock"),
            ordinal: 0,
        },
    )
    .unwrap();
    d.audio_bindings.bindings.remove(&node("tail"));
    d.validate().unwrap();
    let old = d.audio_bindings().clone();
    let c = mixed_gap(0, true);
    let (p, pools) = observed(&d, &c, vec![duration(3)]);
    assert_eq!(pools, [(0, 0)]);
    assert_eq!(p.document.duration().unwrap().frames(), 35);
    assert_eq!(
        p.document.audio_bindings().bindings()[&node("gap-0")].lattice,
        closed
    );
    for outer in [0, 1] {
        for inner in [0, 1, 2] {
            let at = instance("body", outer, Some(inner));
            let previous = old
                .resolve(&at.node, &at, crate::MAX_DOCUMENT_NODES)
                .unwrap();
            let current = p
                .document
                .audio_bindings()
                .resolve(&at.node, &at, crate::MAX_DOCUMENT_NODES)
                .unwrap();
            assert_eq!(current.lattice.origin, previous.lattice.origin);
            assert_eq!(
                current.lattice.sample_boundary(ExactRatio::ZERO).unwrap(),
                previous.lattice.sample_boundary(ExactRatio::ZERO).unwrap()
            );
        }
    }
    assert!(
        p.document
            .audio_bindings()
            .bindings()
            .contains_key(&node("tail"))
    );
    proof(&d, &c, &p);
}

#[test]
fn owned_gap_hold_duration_counted_named_calls_resolve_staged_owned_recipes() {
    let d = gaps(3);
    let c = selected_gaps(Some(1), &[2, 1, 0], false);
    let bank = BTreeMap::from([
        (name('a'), macro_value(vec![call('b', 1)])),
        (name('b'), macro_value(vec![duration(2), duration(3)])),
    ]);
    let mut pools = vec![];
    let p = plan_semantic(
        &d,
        &c,
        &program(vec![call('a', 2)]),
        SemanticRegisterBank {
            entries: &bank,
            version: 0,
        },
        revision("gap-counted"),
        |r| {
            let SemanticAllocationRequest::Isolation { nodes, marks, .. } = &r else {
                panic!()
            };
            pools.push((*nodes, *marks));
            allocate(r)
        },
        no_original,
    )
    .unwrap();
    assert_eq!(pools.len(), 12);
    assert_eq!(pools[0], (5, 0));
    assert!(pools[1..].iter().all(|pool| *pool == (0, 0)));
    assert_eq!(p.document.duration().unwrap().frames(), 34);
    assert_eq!(p.context.cursor, c.cursor);
    proof(&d, &c, &p);
}

#[test]
fn selected_body_hold_duration_excess_allplay_allocator_pool_refuses_after_valid_leaf() {
    // This authority was already supported before owned-gap admission. Its
    // wrong-pool failure must be independent of the new branch family.
    let d = fixture(false);
    let target = |outer| ScopedNodeTarget {
        node: node("held"),
        repeats: vec![
            RepeatEditStep {
                repeat: node("outer"),
                branch: RepeatEditBranch::Play {
                    iteration: iteration("outer", outer),
                },
            },
            RepeatEditStep {
                repeat: node("inner"),
                branch: RepeatEditBranch::Play {
                    iteration: iteration("inner", 0),
                },
            },
        ],
    };
    let c = SemanticContext {
        selected_child: Some(node("outer")),
        selected_hold_duration: Some(Box::new(SemanticSelectedHoldDurationContext {
            root: node("outer"),
            target: target(0),
            also: vec![target(1)],
            presentation: Some(instance("held", 0, Some(0))),
        })),
        ..context("root", 5)
    };
    assert_eq!(
        d.selected_hold_duration_visibility(&c.parent, c.selected_hold_duration.as_ref().unwrap())
            .unwrap()
            .unwrap()
            .frames,
        range(5, 7)
    );
    let mut allocations = 0;
    let error = plan_semantic(
        &d,
        &c,
        &program(vec![duration(3)]),
        SemanticRegisterBank {
            entries: &BTreeMap::new(),
            version: 0,
        },
        revision("body-surplus"),
        |r| {
            let SemanticAllocationRequest::Isolation { nodes, marks, .. } = &r else {
                panic!()
            };
            assert_eq!((*nodes, *marks), (13, 0));
            allocations += 1;
            let mut value = allocate(r)?;
            if allocations == 2 {
                let SemanticAllocation::Isolation { identities, .. } = &mut value else {
                    panic!()
                };
                identities.nodes.push(node("extra-body-pool"));
            }
            Ok(value)
        },
        no_original,
    )
    .unwrap_err();
    assert_eq!(allocations, 2);
    assert_eq!(error.code, EditErrorCode::InvalidCommand);
    assert!(
        error
            .message
            .contains("selected Hold duration requires its exact node and mark identity pools")
    );
    assert_eq!(d.duration().unwrap().frames(), 45);
}
