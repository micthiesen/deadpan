use super::*;

fn setter(plays: u32) -> SemanticInstruction {
    SemanticInstruction::SetRepeatPlays {
        plays: NonZeroU32::new(plays).unwrap(),
    }
}

fn setup_repeats(path: &Path) -> (Harness, ProjectUpdate) {
    let mut store = seed_holds(path, &["a", "b", "c"]);
    for (child, wrapper, plays, gap) in [
        ("a", "repeat-a", 2, Some(hold(2))),
        ("b", "repeat-b", 3, None),
    ] {
        seed_command(
            &mut store,
            Command::WrapRepeat {
                node: node(child),
                id: node(wrapper),
                plays,
                gap,
                anchor_policy: Default::default(),
            },
            &format!("seed-{wrapper}"),
        );
    }
    drop(store);
    let harness = Harness::new();
    let opened = command(&harness.service, ProjectRequest::Open(path.into()));
    assert!(opened.error.is_none(), "{:?}", opened.error);
    (harness, opened)
}

fn count_intent(update: &ProjectUpdate, plays: u32) {
    let edit = snapshot(update).edit.as_ref().unwrap();
    assert_eq!(
        edit,
        &LastEdit {
            operation: RepeatableEdit::SetRepeatPlays {
                plays: NonZeroU32::new(plays).unwrap(),
            },
            register: None,
        }
    );
    assert!(!edit.uses_register());
}

fn plays(update: &ProjectUpdate, name: &str) -> u32 {
    let NodeKind::Repeat { iterations, .. } =
        &update.workspace.as_ref().unwrap().document.nodes()[&node(name)].kind
    else {
        panic!("expected Repeat")
    };
    iterations.len()
}

#[test]
fn count_dot_targets_new_selected_repeat_preserves_copy_and_authors_same_count() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("repeat-count-dot.deadpan");
    let (harness, opened) = setup_repeats(&path);
    let mut copied_child = capture(opened.workspace.as_ref().unwrap(), 100, Some('k'), 0, 1);
    copied_child.selection = SliceCaptureSelection::Child { node: node("c") };
    let opened = command(
        &harness.service,
        ProjectRequest::CaptureEditSlice(copied_child),
    );
    assert!(opened.error.is_none(), "{:?}", opened.error);
    let bank = ProjectStore::open(&path, AccessMode::ReadOnly)
        .unwrap()
        .registers()
        .unwrap();
    let initial_rows = counts(&path);
    // Cursor is in another beat: the explicit selected Repeat owns this setter.
    let first = execute(
        &harness.service,
        apply(
            &opened,
            1,
            context(&opened, 55, Some("repeat-a")),
            setter(4),
            None,
        ),
    );
    count_intent(&first, 4);
    assert_eq!(plays(&first, "repeat-a"), 4);
    assert_eq!(plays(&first, "repeat-b"), 3);
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
    let second = execute(
        &harness.service,
        repeat(&first, 2, context(&first, 0, Some("repeat-b")), 'z'),
    );
    count_intent(&second, 4);
    assert_eq!(plays(&second, "repeat-a"), 4);
    assert_eq!(plays(&second, "repeat-b"), 4);
    assert_eq!(
        second
            .saved_macro
            .as_ref()
            .unwrap()
            .committed()
            .unwrap()
            .cursor,
        Some(ProjectFrame(46))
    );
    assert!(Arc::ptr_eq(
        copied(opened.registers.as_ref().unwrap(), 'k'),
        copied(second.registers.as_ref().unwrap(), 'k')
    ));
    assert_eq!(
        ProjectStore::open(&path, AccessMode::ReadOnly)
            .unwrap()
            .registers()
            .unwrap(),
        bank
    );
    let third = execute(
        &harness.service,
        repeat(&second, 3, context(&second, 95, Some("repeat-b")), '"'),
    );
    count_intent(&third, 4);
    assert_ne!(
        third.workspace.as_ref().unwrap().document.revision_id(),
        second.workspace.as_ref().unwrap().document.revision_id()
    );
    assert_eq!(
        third.workspace.as_ref().unwrap().document.nodes(),
        second.workspace.as_ref().unwrap().document.nodes()
    );
    assert!(snapshot(&third).version > snapshot(&second).version);
    assert_eq!(counts(&path), (initial_rows.0 + 3, initial_rows.1 + 3));
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
    count_intent(&undone, 4);
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
    command(&harness.service, ProjectRequest::Close);
    let reopened = command(&harness.service, ProjectRequest::Open(path.clone()));
    assert_eq!(
        reopened.workspace.as_ref().unwrap().document,
        redone.workspace.as_ref().unwrap().document
    );
    assert_eq!(
        ProjectStore::open(&path, AccessMode::ReadOnly)
            .unwrap()
            .registers()
            .unwrap(),
        bank
    );
    assert!(
        snapshot(&reopened).edit.is_none(),
        "session-local dot proof is not restored by reopen"
    );
}

#[test]
fn count_repeat_rejects_visual_wrong_target_and_stale_intent_without_writes() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("repeat-count-refusal.deadpan");
    let (harness, opened) = setup_repeats(&path);
    let first = execute(
        &harness.service,
        apply(
            &opened,
            1,
            context(&opened, 0, Some("repeat-a")),
            setter(4),
            None,
        ),
    );
    let version = snapshot(&first).version;
    for (ticket, selected, requested, expected_version) in [
        (2, None, 4, version),
        (3, Some("c"), 4, version),
        (4, Some("a"), 4, version),
        (5, Some("repeat-b"), 5, version),
        (6, Some("repeat-b"), 4, version - 1),
    ] {
        refused(
            &harness.service,
            &path,
            apply(
                &first,
                ticket,
                context(&first, 0, selected),
                setter(requested),
                Some(expected_version),
            ),
            &first,
        );
    }
    for (ticket, head) in [(7, 0), (8, 3)] {
        let mut visual = context(&first, 0, Some("repeat-a"));
        visual.visual_selection = Some(SemanticVisualSelection::Time {
            anchor: ProjectFrame(0),
            head: ProjectFrame(head),
            extending: false,
        });
        let last = snapshot(&first).edit.as_ref().unwrap();
        assert_eq!(
            last.instruction(&visual, RegisterName::new('z').unwrap()),
            setter(4)
        );
        refused(
            &harness.service,
            &path,
            repeat(&first, ticket, visual, 'z'),
            &first,
        );
    }
    let stale = repeat(&first, 9, context(&first, 0, Some("repeat-b")), 'z');
    let saved_macro = execute(
        &harness.service,
        MacroOperation::Save {
            id: id(&first, 10),
            register: 'q',
            program: Arc::new(SemanticProgram::new(vec![setter(4)]).unwrap()),
        },
    );
    assert_eq!(snapshot(&saved_macro), snapshot(&first));
    refused(&harness.service, &path, stale, &saved_macro);
    let rows = counts(&path);
    let bank = ProjectStore::open(&path, AccessMode::ReadOnly)
        .unwrap()
        .registers()
        .unwrap();
    let ran = execute(
        &harness.service,
        MacroOperation::Run {
            id: id(&saved_macro, 11),
            register: 'q',
            count: 2,
            scope: SequenceScope::default(),
            context: context(&saved_macro, 0, Some("repeat-b")),
        },
    );
    assert_eq!(plays(&ran, "repeat-b"), 4);
    assert!(
        snapshot(&ran).edit.is_none(),
        "named programs do not install direct-edit proof"
    );
    assert_eq!(counts(&path), (rows.0 + 1, rows.1 + 1));
    assert_eq!(
        ProjectStore::open(&path, AccessMode::ReadOnly)
            .unwrap()
            .registers()
            .unwrap(),
        bank
    );
}

#[test]
fn saved_count_refresh_failure_keeps_receipt_and_exact_retry_cannot_replace_newer_proof() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("repeat-count-refresh.deadpan");
    let (harness, opened) = setup_repeats(&path);
    fail_refresh(&harness.service);
    let operation = apply(
        &opened,
        1,
        context(&opened, 55, Some("repeat-a")),
        setter(4),
        None,
    );
    let first = execute(&harness.service, operation.clone());
    count_intent(&first, 4);
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
    let retried = duplicate.saved_macro.as_ref().unwrap();
    assert_eq!(retried.id, receipt.id);
    assert_eq!(retried.bank_version, receipt.bank_version);
    assert_eq!(retried.committed(), receipt.committed());
    match (&retried.outcome, &receipt.outcome) {
        (
            crate::project::macros::Outcome::Applied {
                scope,
                cursor,
                selected,
                visual_selection,
                refresh_error,
                ..
            },
            crate::project::macros::Outcome::Applied {
                scope: expected_scope,
                cursor: expected_cursor,
                selected: expected_selected,
                visual_selection: expected_visual,
                refresh_error: expected_refresh,
                ..
            },
        ) => assert_eq!(
            (scope, cursor, selected, visual_selection, refresh_error),
            (
                expected_scope,
                expected_cursor,
                expected_selected,
                expected_visual,
                expected_refresh
            ),
        ),
        _ => panic!("Retry must preserve the exact Applied outcome"),
    }
    assert_eq!(snapshot(&duplicate), snapshot(&first));
    assert_eq!(counts(&path), rows);
    refused(
        &harness.service,
        &path,
        repeat(&first, 2, context(&first, 22, Some("repeat-b")), 'z'),
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
            4,
            Some('a'),
            52,
            1,
            None,
        ),
    );
    let rows = counts(&path);
    let duplicate = execute(&harness.service, operation.clone());
    assert_eq!(counts(&path), rows);
    assert_eq!(snapshot(&duplicate), snapshot(&changed));
    assert_eq!(
        duplicate.saved_macro.as_ref().unwrap().committed(),
        receipt.committed()
    );
    let mut altered = operation;
    let MacroOperation::Apply { instruction, .. } = &mut altered else {
        unreachable!()
    };
    *instruction = setter(5);
    refused(&harness.service, &path, altered, &changed);
}
