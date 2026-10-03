use super::*;

fn grouping(selector: SemanticSelector, label: &str) -> SemanticInstruction {
    SemanticInstruction::Group {
        selector,
        label: label.into(),
    }
}

fn selected(update: &ProjectUpdate) -> NodeId {
    update
        .saved_macro
        .as_ref()
        .unwrap()
        .committed()
        .unwrap()
        .selected_node
        .clone()
        .unwrap()
}

fn group_intent(update: &ProjectUpdate, selector: SemanticSelector, label: &str) {
    let edit = snapshot(update).edit.as_ref().unwrap();
    assert_eq!(
        edit.operation,
        RepeatableEdit::Group {
            selector,
            label: label.into()
        }
    );
    assert_eq!(edit.register, None);
    assert!(!edit.uses_register());
}

#[test]
fn group_dot_retains_label_and_exact_child_then_visual_override_becomes_intent() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("group-dot.deadpan");
    let (harness, opened) = setup(&path);
    let opened = command(
        &harness.service,
        ProjectRequest::CaptureEditSlice(capture(
            opened.workspace.as_ref().unwrap(),
            100,
            Some('k'),
            1,
            3,
        )),
    );
    let bank = ProjectStore::open(&path, AccessMode::ReadOnly)
        .unwrap()
        .registers()
        .unwrap();
    let label = "  A shared phrase 🎵  ";
    let first = execute(
        &harness.service,
        apply(
            &opened,
            1,
            context(&opened, 25, Some("b")),
            grouping(SemanticSelector::SelectedBeat, label),
            None,
        ),
    );
    group_intent(&first, SemanticSelector::SelectedBeat, label);
    let first_group = selected(&first);
    let beat = &first.workspace.as_ref().unwrap().document.nodes()[&first_group];
    assert_eq!(beat.label, label);
    assert!(matches!(&beat.kind, NodeKind::Sequence { children } if children == &[node("b")]));
    assert_eq!(
        first
            .saved_macro
            .as_ref()
            .unwrap()
            .committed()
            .unwrap()
            .cursor,
        Some(ProjectFrame(10))
    );
    let version = snapshot(&first).version;
    for (ticket, candidate, expected) in [
        (
            2,
            grouping(SemanticSelector::SelectedBeat, "Changed label"),
            version,
        ),
        (3, grouping(motion(2), label), version),
        (
            4,
            grouping(SemanticSelector::SelectedBeat, label),
            version - 1,
        ),
    ] {
        refused(
            &harness.service,
            &path,
            apply(
                &first,
                ticket,
                context(&first, 0, Some("c")),
                candidate,
                Some(expected),
            ),
            &first,
        );
    }
    let second = execute(
        &harness.service,
        repeat(&first, 5, context(&first, 0, Some("c")), 'z'),
    );
    group_intent(&second, SemanticSelector::SelectedBeat, label);
    let second_group = selected(&second);
    assert!(
        matches!(&second.workspace.as_ref().unwrap().document.nodes()[&second_group].kind,
        NodeKind::Sequence { children } if children == &[node("c")])
    );
    let mut visual = context(&second, 2, Some("a"));
    visual.visual_selection = Some(SemanticVisualSelection {
        anchor: ProjectFrame(7),
        head: ProjectFrame(2),
        extending: true,
    });
    let third = execute(&harness.service, repeat(&second, 6, visual, 'a'));
    group_intent(&third, SemanticSelector::VisualSelection, label);
    assert!(matches!(
        &third.saved_macro.as_ref().unwrap().outcome,
        crate::project::macros::Outcome::Applied {
            cursor: ProjectFrame(2),
            visual_selection: None,
            ..
        }
    ));
    assert_eq!(
        third
            .workspace
            .as_ref()
            .unwrap()
            .document
            .duration()
            .unwrap()
            .frames(),
        30
    );
    assert!(Arc::ptr_eq(
        copied(opened.registers.as_ref().unwrap(), 'k'),
        copied(third.registers.as_ref().unwrap(), 'k')
    ));
    assert_eq!(
        ProjectStore::open(&path, AccessMode::ReadOnly)
            .unwrap()
            .registers()
            .unwrap(),
        bank
    );
    let absent = SemanticContext {
        selected_child: Some(selected(&third)),
        ..context(&third, 2, None)
    };
    refused(
        &harness.service,
        &path,
        repeat(&third, 7, absent.clone(), 'z'),
        &third,
    );
    let empty = SemanticContext {
        visual_selection: Some(SemanticVisualSelection {
            anchor: ProjectFrame(2),
            head: ProjectFrame(2),
            extending: false,
        }),
        ..absent
    };
    refused(
        &harness.service,
        &path,
        repeat(&third, 8, empty, 'z'),
        &third,
    );
    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: third
                .workspace
                .as_ref()
                .unwrap()
                .document
                .revision_id()
                .clone(),
        },
    );
    same_document(
        &undone.workspace.as_ref().unwrap().document,
        &second.workspace.as_ref().unwrap().document,
    );
    group_intent(&undone, SemanticSelector::VisualSelection, label);
    let redone = command(
        &harness.service,
        ProjectRequest::Redo {
            expected_revision: undone
                .workspace
                .as_ref()
                .unwrap()
                .document
                .revision_id()
                .clone(),
        },
    );
    same_document(
        &redone.workspace.as_ref().unwrap().document,
        &third.workspace.as_ref().unwrap().document,
    );
    same_document(&saved(&path), &third.workspace.as_ref().unwrap().document);
}

#[test]
fn ungroup_dot_keeps_empty_promoted_child_and_refuses_visual_or_missing_targets() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("ungroup-dot.deadpan");
    let mut store = seed_holds(&path, &["a", "b"]);
    for (name, index) in [("empty-first", 0), ("empty-group", 2)] {
        seed_command(
            &mut store,
            Command::Insert {
                parent: node("root"),
                index,
                subtree: Subtree {
                    root: node(name),
                    nodes: std::collections::BTreeMap::from([(
                        node(name),
                        BeatNode::sequence("Empty", vec![]),
                    )]),
                    overrides: Default::default(),
                    gap_overrides: Default::default(),
                },
            },
            &format!("seed-{name}"),
        );
    }
    seed_command(
        &mut store,
        Command::Group {
            parent: node("root"),
            start: 0,
            end: 2,
            id: node("group"),
            label: "Group".into(),
        },
        "seed-group",
    );
    drop(store);
    let harness = Harness::new();
    let opened = command(&harness.service, ProjectRequest::Open(path.clone()));
    let rows = counts(&path);
    let first = execute(
        &harness.service,
        apply(
            &opened,
            1,
            context(&opened, 19, Some("group")),
            SemanticInstruction::Ungroup,
            None,
        ),
    );
    assert_eq!(selected(&first), node("empty-first"));
    assert_eq!(
        first
            .saved_macro
            .as_ref()
            .unwrap()
            .committed()
            .unwrap()
            .cursor,
        Some(ProjectFrame(0))
    );
    let edit = snapshot(&first).edit.as_ref().unwrap();
    assert_eq!(edit.operation, RepeatableEdit::Ungroup);
    assert!(!edit.uses_register());
    for (ticket, chosen) in [(2, None), (3, Some("a")), (4, Some("group"))] {
        refused(
            &harness.service,
            &path,
            repeat(&first, ticket, context(&first, 10, chosen), 'z'),
            &first,
        );
    }
    for (ticket, head) in [(5, 10), (6, 12)] {
        let mut visual = context(&first, 10, Some("empty-group"));
        visual.visual_selection = Some(SemanticVisualSelection {
            anchor: ProjectFrame(10),
            head: ProjectFrame(head),
            extending: false,
        });
        assert_eq!(
            edit.instruction(&visual, RegisterName::new('z').unwrap()),
            SemanticInstruction::Ungroup
        );
        refused(
            &harness.service,
            &path,
            repeat(&first, ticket, visual, 'z'),
            &first,
        );
    }
    let second = execute(
        &harness.service,
        repeat(&first, 7, context(&first, 0, Some("empty-group")), 'z'),
    );
    assert_eq!(selected(&second), node("b"));
    assert_eq!(
        second
            .saved_macro
            .as_ref()
            .unwrap()
            .committed()
            .unwrap()
            .cursor,
        Some(ProjectFrame(10))
    );
    assert_eq!(counts(&path), (rows.0 + 2, rows.1 + 2));
    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: second
                .workspace
                .as_ref()
                .unwrap()
                .document
                .revision_id()
                .clone(),
        },
    );
    same_document(
        &undone.workspace.as_ref().unwrap().document,
        &first.workspace.as_ref().unwrap().document,
    );
    let redone = command(
        &harness.service,
        ProjectRequest::Redo {
            expected_revision: undone
                .workspace
                .as_ref()
                .unwrap()
                .document
                .revision_id()
                .clone(),
        },
    );
    same_document(
        &redone.workspace.as_ref().unwrap().document,
        &second.workspace.as_ref().unwrap().document,
    );
}

#[test]
fn group_refresh_failure_retains_proof_and_exact_retry_does_not_replace_newer_intent() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("group-refresh.deadpan");
    let (harness, opened) = setup(&path);
    fail_refresh(&harness.service);
    let operation = apply(
        &opened,
        1,
        context(&opened, 20, Some("a")),
        grouping(SemanticSelector::SelectedBeat, "Saved group"),
        None,
    );
    let first = execute(&harness.service, operation.clone());
    group_intent(&first, SemanticSelector::SelectedBeat, "Saved group");
    let receipt = first.saved_macro.as_ref().unwrap();
    assert!(matches!(
        &receipt.outcome,
        crate::project::macros::Outcome::Applied {
            refresh_error: Some(_),
            ..
        }
    ));
    assert_eq!(
        snapshot(&first).head.as_ref(),
        Some(&receipt.committed().unwrap().revision)
    );
    assert!(
        snapshot(&first)
            .edit_for(first.workspace.as_ref().unwrap())
            .is_err()
    );
    let rows = counts(&path);
    let duplicate = execute(&harness.service, operation.clone());
    assert_eq!(counts(&path), rows);
    assert_eq!(
        duplicate.saved_macro.as_ref().unwrap().committed(),
        receipt.committed()
    );
    assert!(matches!(
        &duplicate.saved_macro.as_ref().unwrap().outcome,
        crate::project::macros::Outcome::Applied {
            refresh_error: Some(_),
            ..
        }
    ));
    assert_eq!(snapshot(&duplicate), snapshot(&first));
    refused(
        &harness.service,
        &path,
        repeat(&first, 2, context(&first, 10, Some("b")), 'z'),
        &first,
    );
    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: receipt.committed().unwrap().revision.clone(),
        },
    );
    same_document(
        &undone.workspace.as_ref().unwrap().document,
        &opened.workspace.as_ref().unwrap().document,
    );
    let changed = cut(
        &harness.service,
        frame_cut(
            undone.workspace.as_ref().unwrap(),
            3,
            Some('a'),
            20,
            1,
            None,
        ),
    );
    let rows = counts(&path);
    let duplicate = execute(&harness.service, operation.clone());
    assert_eq!(counts(&path), rows);
    assert_eq!(snapshot(&duplicate), snapshot(&changed));
    let mut altered = operation;
    let MacroOperation::Apply { instruction, .. } = &mut altered else {
        unreachable!()
    };
    *instruction = grouping(SemanticSelector::SelectedBeat, "Different");
    refused(&harness.service, &path, altered, &changed);
    let saved_macro = execute(
        &harness.service,
        MacroOperation::Save {
            id: id(&changed, 4),
            register: 'q',
            program: Arc::new(
                SemanticProgram::new(vec![grouping(SemanticSelector::SelectedBeat, "Counted")])
                    .unwrap(),
            ),
        },
    );
    assert_eq!(snapshot(&saved_macro), snapshot(&changed));
    let rows = counts(&path);
    let bank = ProjectStore::open(&path, AccessMode::ReadOnly)
        .unwrap()
        .registers()
        .unwrap();
    let ran = execute(
        &harness.service,
        MacroOperation::Run {
            id: id(&saved_macro, 5),
            register: 'q',
            count: 2,
            scope: SequenceScope::default(),
            context: context(&saved_macro, 10, Some("b")),
        },
    );
    assert!(snapshot(&ran).edit.is_none());
    assert_eq!(counts(&path), (rows.0 + 1, rows.1 + 1));
    assert_eq!(
        ProjectStore::open(&path, AccessMode::ReadOnly)
            .unwrap()
            .registers()
            .unwrap(),
        bank
    );
    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: ran
                .workspace
                .as_ref()
                .unwrap()
                .document
                .revision_id()
                .clone(),
        },
    );
    same_document(
        &undone.workspace.as_ref().unwrap().document,
        &changed.workspace.as_ref().unwrap().document,
    );
}
