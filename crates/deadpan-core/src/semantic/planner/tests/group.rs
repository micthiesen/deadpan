use super::*;

fn group(selector: SemanticSelector) -> SemanticInstruction {
    SemanticInstruction::Group {
        selector,
        label: "the long answer".into(),
    }
}
fn entry(selected: &str, cursor: i64) -> SemanticContext {
    SemanticContext {
        selected_child: Some(node(selected)),
        ..context("root", cursor)
    }
}
fn inverse(document: &ProjectDocument, plan: &SemanticPlan) {
    let replay =
        crate::replay_compound::<EditError>(document, plan.request.as_ref().unwrap(), |_| Ok(()))
            .unwrap();
    assert_eq!(replay.document, plan.document);
    assert_eq!(
        replay.edit.inverse.apply(&replay.document).unwrap(),
        *document
    );
    assert!(plan.register_writes.is_empty());
}

#[test]
fn selected_group_uses_exact_child_including_empty_independently_of_cursor() {
    let document = tree(
        &["a", "empty", "b"],
        vec![
            ("a", hold(4)),
            ("empty", BeatNode::sequence("Empty", vec![])),
            ("b", hold(3)),
        ],
    );
    for selected in ["a", "empty"] {
        let plan = plan(
            &document,
            entry(selected, 6),
            vec![group(SemanticSelector::SelectedBeat)],
            &BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(
            plan.context.cursor,
            ProjectFrame(if selected == "a" { 0 } else { 4 })
        );
        assert_eq!(plan.context.selected_child, Some(node("group-0")));
        assert_eq!(
            plan.document.nodes()[&node(selected)],
            document.nodes()[&node(selected)]
        );
        assert!(
            matches!(&plan.document.nodes()[&node("group-0")].kind, NodeKind::Sequence { children } if children == &vec![node(selected)])
        );
        assert_eq!(
            plan.trace[0].resolved_selection,
            Some(SliceCaptureSelection::Child {
                node: node(selected)
            })
        );
        inverse(&document, &plan);
    }
}

#[test]
fn visual_and_motion_group_resolve_staged_ranges_and_clear_selection() {
    let document = fixture(12);
    for (anchor, head, extending) in [(2, 9, true), (9, 2, true), (9, 2, false)] {
        let entry = SemanticContext {
            visual_selection: Some(SemanticVisualSelection {
                anchor: ProjectFrame(anchor),
                head: ProjectFrame(head),
                extending,
            }),
            ..context("root", head)
        };
        let planned = plan(
            &document,
            entry,
            vec![
                group(SemanticSelector::VisualSelection),
                SemanticInstruction::Ungroup,
                SemanticInstruction::MoveScope { end: true },
                group(SemanticSelector::Motion {
                    motion: SemanticMotion::Frames {
                        forward: false,
                        count: NonZeroU32::new(3).unwrap(),
                    },
                }),
            ],
            &BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(planned.trace[0].resolved_range, Some(range(2, 9)));
        assert_eq!(
            planned.trace[1].resolved_selection,
            Some(SliceCaptureSelection::Child {
                node: node("group-0")
            })
        );
        assert_eq!(planned.trace[3].resolved_range, Some(range(9, 12)));
        assert_eq!(planned.context.cursor, ProjectFrame(9));
        assert_eq!(planned.context.selected_child, Some(node("group-2")));
        assert!(planned.context.visual_selection.is_none());
        assert_eq!(
            planned.document.duration().unwrap(),
            document.duration().unwrap()
        );
        inverse(&document, &planned);
    }
}

#[test]
fn ungroup_selects_promoted_or_following_literal_empty_sibling() {
    let document = tree(
        &["a", "group", "following", "b"],
        vec![
            ("a", hold(3)),
            (
                "group",
                BeatNode::sequence("Group", vec![node("inside-empty"), node("inside")]),
            ),
            ("inside-empty", BeatNode::sequence("First", vec![])),
            ("inside", hold(2)),
            ("following", BeatNode::sequence("Next", vec![])),
            ("b", hold(4)),
        ],
    );
    let promoted = plan(
        &document,
        entry("group", 8),
        vec![SemanticInstruction::Ungroup],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(promoted.context.selected_child, Some(node("inside-empty")));
    assert_eq!(promoted.context.cursor, ProjectFrame(3));
    inverse(&document, &promoted);
    let removed = plan(
        &document,
        entry("following", 0),
        vec![SemanticInstruction::Ungroup],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(removed.context.selected_child, Some(node("b")));
    assert_eq!(removed.context.cursor, ProjectFrame(5));
    let empty = tree(
        &["left", "right"],
        vec![
            ("left", BeatNode::sequence("Left", vec![])),
            ("right", BeatNode::sequence("Right", vec![])),
        ],
    );
    let removed = plan(
        &empty,
        entry("left", 0),
        vec![SemanticInstruction::Ungroup],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(removed.context.selected_child, Some(node("right")));
    let removed = plan(
        &empty,
        entry("right", 0),
        vec![SemanticInstruction::Ungroup],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(removed.context.selected_child, Some(node("left")));
    let sole = tree(
        &["only"],
        vec![("only", BeatNode::sequence("Only", vec![]))],
    );
    let removed = plan(
        &sole,
        entry("only", 0),
        vec![SemanticInstruction::Ungroup],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(removed.context.selected_child, None);
    inverse(&sole, &removed);
}

#[test]
fn counted_frozen_calls_group_and_ungroup_in_one_transaction() {
    let document = fixture(8);
    let bank = BTreeMap::from([(
        name('a'),
        macro_value(vec![
            group(SemanticSelector::SelectedBeat),
            SemanticInstruction::Ungroup,
        ]),
    )]);
    let planned = plan(&document, entry("held", 7), vec![call('a', 3)], &bank).unwrap();
    assert_eq!(planned.trace.len(), 7);
    assert_eq!(planned.document.nodes(), document.nodes());
    assert_eq!(planned.context.selected_child, Some(node("held")));
    assert_eq!(planned.document.revision_id(), &revision("outer"));
    assert_eq!(planned.trace[3].before_revision, revision("leaf-1"));
    let Command::Compound { transaction } = &planned.request.as_ref().unwrap().command else {
        panic!()
    };
    assert_eq!(transaction.steps().len(), 6);
    inverse(&document, &planned);
    let original = bank.clone();
    assert!(
        plan(
            &document,
            entry("held", 7),
            vec![call('a', 3), SemanticInstruction::Ungroup],
            &bank
        )
        .is_err()
    );
    assert_eq!(bank, original);
    assert_eq!(document.nodes().len(), 2);
}

#[test]
fn absent_wrong_kind_visual_and_stale_targets_refuse_without_fallback() {
    let document = tree(
        &["a", "group"],
        vec![
            ("a", hold(4)),
            ("group", BeatNode::sequence("Group", vec![])),
        ],
    );
    for mut target in [
        context("root", 1),
        entry("a", 1),
        entry("root", 1),
        entry("missing", 1),
    ] {
        assert!(
            plan(
                &document,
                target.clone(),
                vec![SemanticInstruction::Ungroup],
                &BTreeMap::new()
            )
            .is_err()
        );
        target.visual_selection = Some(SemanticVisualSelection {
            anchor: ProjectFrame(1),
            head: ProjectFrame(1),
            extending: false,
        });
        assert!(
            plan(
                &document,
                target,
                vec![group(SemanticSelector::VisualSelection)],
                &BTreeMap::new()
            )
            .is_err()
        );
    }
    for end in [1, 3] {
        let target = SemanticContext {
            visual_selection: Some(SemanticVisualSelection {
                anchor: ProjectFrame(1),
                head: ProjectFrame(end),
                extending: false,
            }),
            ..entry("group", 1)
        };
        assert!(
            plan(
                &document,
                target,
                vec![SemanticInstruction::Ungroup],
                &BTreeMap::new()
            )
            .unwrap_err()
            .message
            .contains("Visual")
        );
    }
    let nested = tree(
        &["prefix", "scope"],
        vec![
            ("prefix", hold(5)),
            ("scope", BeatNode::sequence("Scope", vec![node("body")])),
            ("body", hold(4)),
        ],
    );
    let result = plan(
        &nested,
        SemanticContext {
            parent: node("scope"),
            selected_child: Some(node("body")),
            ..context("root", 8)
        },
        vec![group(SemanticSelector::SelectedBeat)],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(result.context.parent, node("scope"));
    assert_eq!(result.context.cursor, ProjectFrame(5));
    assert_eq!(result.trace[0].before_scope, range(5, 9));
    inverse(&nested, &result);
}

#[test]
fn group_allocations_are_exact_fresh_and_late_failure_is_atomic() {
    let document = fixture(8);
    let body = program(vec![
        group(SemanticSelector::SelectedBeat),
        group(SemanticSelector::SelectedBeat),
    ]);
    let bank = BTreeMap::new();
    for mode in 0..4 {
        let error = plan_semantic(
            &document,
            &entry("held", 0),
            &body,
            SemanticRegisterBank {
                entries: &bank,
                version: 0,
            },
            revision("outer"),
            |request| {
                let SemanticAllocationRequest::Group { step_index, .. } = request else {
                    panic!()
                };
                Ok(match mode {
                    0 => SemanticAllocation::Ungroup {
                        new_revision: revision("wrong"),
                    },
                    _ => SemanticAllocation::Group {
                        new_revision: revision(if mode == 1 {
                            "outer"
                        } else if step_index == 0 {
                            "first"
                        } else {
                            "second"
                        }),
                        identities: GroupSelectionIdentities {
                            group: node(if mode == 2 { "held" } else { "same-group" }),
                            split: SplitIdentities { nodes: vec![] },
                        },
                    },
                })
            },
            no_original,
        )
        .unwrap_err();
        assert!(matches!(
            error.code,
            EditErrorCode::InvalidCommand | EditErrorCode::IdentityConflict
        ));
    }
    assert_eq!(document, fixture(8));
}

#[test]
fn group_wire_labels_and_counted_step_budget_are_bounded() {
    let valid = serde_json::json!({"instructions":[{"type":"group","selector":{"type":"selected_beat"},"label":"the \\\"answer\\\" 答え"},{"type":"ungroup"}]});
    let decoded: SemanticProgram = serde_json::from_value(valid).unwrap();
    assert_eq!(decoded.instructions().len(), 2);
    for invalid in [
        serde_json::json!({"instructions":[{"type":"ungroup","extra":true}]}),
        serde_json::json!({"instructions":[{"type":"group","selector":{"type":"selected_beat"},"label":"x".repeat(1025)}]}),
        serde_json::json!({"instructions":[{"type":"group","selector":{"type":"selected_beat"},"label":"bad\u{0000}name"}]}),
    ] {
        assert!(serde_json::from_value::<SemanticProgram>(invalid).is_err());
    }
    let document = fixture(8);
    let bank = BTreeMap::from([(
        name('a'),
        macro_value(vec![
            group(SemanticSelector::SelectedBeat),
            SemanticInstruction::Ungroup,
        ]),
    )]);
    let error = plan_semantic(
        &document,
        &entry("held", 0),
        &program(vec![call('a', 513)]),
        SemanticRegisterBank {
            entries: &bank,
            version: 0,
        },
        revision("outer"),
        |_| panic!("step budget must reject before allocation"),
        no_original,
    )
    .unwrap_err();
    assert_eq!(error.code, EditErrorCode::LimitExceeded);
    assert!(error.message.contains("1024"));
}
