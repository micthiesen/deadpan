//! `yy`, `dd` and `p` on a beat in one explicit Repeat play.

use super::*;

fn apply_with_bank(target: &Target, bank: u64, instruction: SemanticInstruction) -> Operation {
    let Operation::Apply {
        mut id,
        instruction,
        repeat_version,
        scope,
        context,
    } = apply(target, instruction)
    else {
        unreachable!("apply builds an Apply")
    };
    id.bank_version = bank;
    Operation::Apply {
        id,
        instruction,
        repeat_version,
        scope,
        context,
    }
}

fn copied_slice(update: &ProjectUpdate, register: char) -> Arc<deadpan_core::CapturedEditSlice> {
    match update.registers.as_ref().unwrap().entries.get(&register) {
        Some(crate::project::registers::Value::Edited(captured)) => captured.slice().clone(),
        other => panic!("an edited copy in {register}: {other:?}"),
    }
}

#[test]
fn copying_in_one_play_and_pasting_in_another_changes_only_that_play_and_undoes() {
    let scratch = tempfile::tempdir().unwrap();
    let (service, opened) = open(&scratch.path().join("copy.deadpan"), false);
    let before = (*opened.document).clone();
    // `yy` on x in play 2 (20..30) copies that occurrence without an edit.
    let copy = command(
        &service,
        ProjectRequest::Macro(apply_with_bank(
            &target(&opened, "x", &[("rep", 1)], 25),
            0,
            SemanticInstruction::Yank {
                selector: SemanticSelector::SelectedBeat,
                register: deadpan_core::RegisterName::unnamed(),
            },
        )),
    );
    saved(&copy);
    let copy_update = copy;
    let copy = &copy_update;
    let workspace = copy.workspace.clone().unwrap();
    assert_eq!(*workspace.document, before);
    let slice = copied_slice(copy, '"');
    assert_eq!(
        slice.range(),
        deadpan_core::FrameRange::new(ProjectFrame(20), ProjectFrame(30)).unwrap()
    );
    assert_eq!(slice.plays().len(), 1);
    assert_eq!(
        *slice.as_ref(),
        deadpan_core::CapturedEditSlice::capture_occurrence(
            &before,
            target(&opened, "x", &[("rep", 1)], 25)
                .presentation
                .as_ref()
                .unwrap(),
            slice.capture_timing().clone(),
        )
        .unwrap()
    );
    let bank = copy.registers.as_ref().unwrap().version;

    // `p` on y in play 3 (50..60) inserts the copy after it, at 60.
    let paste = command(
        &service,
        ProjectRequest::Macro(apply_with_bank(
            &target(&workspace, "y", &[("rep", 2)], 55),
            bank,
            SemanticInstruction::Paste {
                register: deadpan_core::RegisterName::unnamed(),
                before: false,
            },
        )),
    );
    saved(&paste);
    let document = &paste.workspace.as_ref().unwrap().document;
    assert_eq!(document.duration().unwrap().frames(), 80);
    assert_eq!(
        children(document, &node("body")),
        vec![node("x"), node("y")]
    );
    assert_eq!(document.overrides()[&node("rep")].len(), 1);
    let copy = owned(document, &node("rep"), &play("rep", 2));
    let kids = children(document, &copy);
    assert_eq!(kids.len(), 3);
    let committed = paste.committed.as_ref().unwrap();
    let receipt = committed.scoped.as_ref().unwrap();
    assert_eq!(receipt.target.node, kids[2]);
    assert_eq!(committed.cursor, Some(ProjectFrame(60)));
    let restored = undo(&service, document);
    assert_eq!(restored.nodes(), before.nodes());
    assert!(restored.overrides().is_empty());

    // The same copy also pastes at the parent, after z, as an ordinary beat.
    let mut root = target(&workspace, "x", &[("rep", 1)], 65);
    root.revision = restored.revision_id().clone();
    let mut context = scoped_context(&root);
    context.scoped = None;
    context.selected_child = Some(node("z"));
    let Operation::Apply {
        id,
        instruction,
        repeat_version,
        scope,
        ..
    } = apply_with_bank(
        &root,
        bank,
        SemanticInstruction::Paste {
            register: deadpan_core::RegisterName::unnamed(),
            before: false,
        },
    )
    else {
        unreachable!()
    };
    let parent = command(
        &service,
        ProjectRequest::Macro(Operation::Apply {
            id,
            instruction,
            repeat_version,
            scope,
            context,
        }),
    );
    saved(&parent);
    let document = &parent.workspace.as_ref().unwrap().document;
    assert_eq!(document.duration().unwrap().frames(), 80);
    assert_eq!(children(document, &node("root")).len(), 3);

    // The native parent paste (`p` outside the play) places it too.
    let crate::project::registers::Value::Edited(captured) =
        copy_update.registers.as_ref().unwrap().entries[&'"'].clone()
    else {
        unreachable!()
    };
    let workspace = parent.workspace.as_ref().unwrap();
    let native = command(
        &service,
        ProjectRequest::PasteEditedSlice(crate::project::slice::Paste {
            expected_session: workspace.session,
            expected_revision: workspace.document.revision_id().clone(),
            copied: captured,
            scope: SequenceScope::default(),
            parent: node("root"),
            destination: crate::project::splice::Destination::Slot(0),
        }),
    );
    assert!(native.error.is_none(), "{:?}", native.error);
    let document = &native.workspace.as_ref().unwrap().document;
    assert_eq!(document.duration().unwrap().frames(), 90);
}

#[test]
fn cutting_in_one_play_saves_the_register_and_closes_up_that_play() {
    let scratch = tempfile::tempdir().unwrap();
    let (service, opened) = open(&scratch.path().join("cut.deadpan"), false);
    let before = (*opened.document).clone();
    // `"add` on y in play 1 (10..20).
    let cut = command(
        &service,
        ProjectRequest::Macro(apply_with_bank(
            &target(&opened, "y", &[("rep", 0)], 15),
            0,
            SemanticInstruction::Cut {
                selector: SemanticSelector::SelectedBeat,
                register: deadpan_core::RegisterName::new('a').unwrap(),
            },
        )),
    );
    saved(&cut);
    let document = &cut.workspace.as_ref().unwrap().document;
    assert_eq!(document.duration().unwrap().frames(), 60);
    assert_eq!(
        children(document, &node("body")),
        vec![node("x"), node("y")]
    );
    let slice = copied_slice(&cut, 'a');
    assert_eq!(copied_slice(&cut, '"'), slice);
    assert_eq!(
        slice.range(),
        deadpan_core::FrameRange::new(ProjectFrame(10), ProjectFrame(20)).unwrap()
    );
    let committed = cut.committed.as_ref().unwrap();
    assert_eq!(committed.cursor, Some(ProjectFrame(10)));
    // Dot cuts x in play 3 (now 30..40) into the same named register.
    let semantic = cut.semantic.as_ref().unwrap();
    let instruction = SemanticInstruction::Cut {
        selector: SemanticSelector::SelectedBeat,
        register: deadpan_core::RegisterName::new('a').unwrap(),
    };
    let workspace = cut.workspace.as_ref().unwrap();
    let mut operation = apply_with_bank(
        &target(workspace, "x", &[("rep", 2)], 35),
        cut.registers.as_ref().unwrap().version,
        instruction,
    );
    let Operation::Apply { repeat_version, .. } = &mut operation else {
        unreachable!()
    };
    *repeat_version = Some(semantic.version);
    let dotted = command(&service, ProjectRequest::Macro(operation));
    saved(&dotted);
    let dotted_document = &dotted.workspace.as_ref().unwrap().document;
    assert_eq!(dotted_document.duration().unwrap().frames(), 50);
    assert_eq!(
        copied_slice(&dotted, 'a').range(),
        deadpan_core::FrameRange::new(ProjectFrame(30), ProjectFrame(40)).unwrap()
    );
    // Undo restores each play; the copy remains.
    let undone = undo(&service, dotted_document);
    let restored = undo(&service, &undone);
    assert_eq!(restored.nodes(), before.nodes());
}
