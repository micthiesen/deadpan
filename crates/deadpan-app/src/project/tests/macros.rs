//! Actor-level macro preparation, durable copies and failure receipts.

use std::num::NonZeroU32;

use deadpan_core::{
    FrameCut, RegisterName, SemanticContext, SemanticInstruction, SemanticProgram,
    SemanticSelector, SliceCaptureSelection,
};

use super::*;
use crate::project::macros::{Id, Operation, Outcome, Receipt};
use crate::project::registers::Value;
use crate::project::slice::{CaptureRequest, Captured, CopyId};

fn setup(path: &Path) -> (Harness, ProjectUpdate) {
    drop(seed_holds(path, &["a", "b", "c"]));
    let harness = Harness::new();
    let opened = command(&harness.service, ProjectRequest::Open(path.into()));
    assert!(opened.error.is_none(), "{:?}", opened.error);
    (harness, opened)
}

fn id(update: &ProjectUpdate, request: u64) -> Id {
    let workspace = update.workspace.as_ref().unwrap();
    Id {
        session: workspace.session,
        project: workspace.document.project_id().clone(),
        revision: workspace.document.revision_id().clone(),
        bank_version: update.registers.as_ref().unwrap().version,
        request,
    }
}

fn program(instructions: Vec<SemanticInstruction>) -> Arc<SemanticProgram> {
    Arc::new(SemanticProgram::new(instructions).unwrap())
}

fn cut(count: u32, register: char) -> SemanticInstruction {
    SemanticInstruction::CutFrames {
        operation: FrameCut::new(count).unwrap(),
        register: RegisterName::new(register).unwrap(),
    }
}

fn movement(count: u32) -> SemanticInstruction {
    SemanticInstruction::MoveFrames {
        forward: true,
        count: NonZeroU32::new(count).unwrap(),
    }
}

fn save(
    update: &ProjectUpdate,
    request: u64,
    register: char,
    program: Arc<SemanticProgram>,
) -> Operation {
    Operation::Save {
        id: id(update, request),
        register,
        program,
    }
}

fn run(update: &ProjectUpdate, request: u64, register: char, count: u32, cursor: i64) -> Operation {
    Operation::Run {
        id: id(update, request),
        register,
        count,
        scope: SequenceScope::default(),
        context: SemanticContext {
            parent: update.workspace.as_ref().unwrap().document.root().clone(),
            cursor: ProjectFrame(cursor),
            selected_child: None,
            visual_selection: None,
        },
    }
}

fn send(service: &ProjectService, operation: Operation) -> ProjectUpdate {
    let update = command(service, ProjectRequest::Macro(operation));
    assert!(
        update.macros.as_ref().unwrap().result.is_ok(),
        "{:?}",
        update.macros
    );
    update
}

fn receipt(update: &ProjectUpdate) -> &Receipt {
    update.macros.as_ref().unwrap().result.as_ref().unwrap()
}

fn copied(update: &ProjectUpdate, register: char) -> &Arc<Captured> {
    let Value::Edited(copied) = &update.registers.as_ref().unwrap().entries[&register] else {
        panic!("expected copied Edit")
    };
    copied
}

fn rows(path: &Path) -> (i64, i64, i64) {
    rusqlite::Connection::open(path.join("project.sqlite")).unwrap().query_row(
        "SELECT (SELECT count(*) FROM revisions),(SELECT count(*) FROM history),(SELECT count(*) FROM transaction_steps)",
        [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
    ).unwrap()
}

fn capture(update: &ProjectUpdate, request: u64, register: char) -> ProjectRequest {
    let workspace = update.workspace.as_ref().unwrap();
    ProjectRequest::CaptureEditSlice(CaptureRequest {
        id: CopyId {
            session: workspace.session,
            project: workspace.document.project_id().clone(),
            source_revision: workspace.document.revision_id().clone(),
            request,
            persisted_version: None,
        },
        register: Some(register),
        scope: SequenceScope::default(),
        parent: workspace.document.root().clone(),
        selection: SliceCaptureSelection::Range {
            range: FrameRange::new(ProjectFrame(1), ProjectFrame(4)).unwrap(),
        },
    })
}

#[test]
fn oriented_visual_copy_keeps_redo_and_replacement_failure_retains_exact_selection_receipt() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("visual-macro-receipt.deadpan");
    let (harness, opened) = setup(&path);
    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: opened
                .workspace
                .as_ref()
                .unwrap()
                .document
                .revision_id()
                .clone(),
        },
    );
    let before = undone.workspace.as_ref().unwrap().document.clone();
    let before_rows = rows(&path);
    let selection = deadpan_core::SemanticVisualSelection {
        anchor: ProjectFrame(17),
        head: ProjectFrame(4),
        extending: false,
    };
    let copied_update = send(
        &harness.service,
        Operation::Apply {
            id: id(&undone, 1),
            instruction: SemanticInstruction::Yank {
                selector: SemanticSelector::VisualSelection,
                register: RegisterName::new('b').unwrap(),
            },
            scope: SequenceScope::default(),
            context: SemanticContext {
                parent: node("root"),
                cursor: ProjectFrame(12),
                selected_child: Some(node("b")),
                visual_selection: Some(selection.clone()),
            },
        },
    );
    assert!(receipt(&copied_update).committed().is_none());
    let Outcome::Applied {
        cursor,
        selected,
        visual_selection,
        ..
    } = &receipt(&copied_update).outcome
    else {
        panic!("range yank receipt")
    };
    assert_eq!(*cursor, ProjectFrame(12));
    assert_eq!(selected.as_ref(), Some(&node("b")));
    assert_eq!(visual_selection.as_ref(), Some(&selection));
    let copy = copied(&copied_update, 'b').clone();
    assert_eq!(copy.child_label(), None);
    assert_eq!(
        copy.slice().selection(),
        &SliceCaptureSelection::Range {
            range: FrameRange::new(ProjectFrame(4), ProjectFrame(17)).unwrap(),
        }
    );
    assert_eq!(copy.id().source_revision, *before.revision_id());
    copy.slice().validate_capture(&before).unwrap();
    assert_eq!(rows(&path), before_rows);
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly).unwrap();
    assert_eq!(reader.snapshot().unwrap(), *before);
    assert!(reader.history_availability().unwrap().1);
    drop(reader);
    command(&harness.service, ProjectRequest::Close);
    let reopened = command(&harness.service, ProjectRequest::Open(path.clone()));
    assert_eq!(copied(&reopened, 'b').slice(), copy.slice());
    assert_eq!(copied(&reopened, 'b').child_label(), None);
    let redone = command(
        &harness.service,
        ProjectRequest::Redo {
            expected_revision: before.revision_id().clone(),
        },
    );
    let before_replace = redone.workspace.as_ref().unwrap().document.clone();
    let before_rows = rows(&path);
    harness
        .service
        .shared
        .workspace_refresh_failure
        .store(true, Ordering::Release);
    let operation = Operation::Apply {
        id: id(&redone, 2),
        instruction: SemanticInstruction::ReplaceSelection {
            register: RegisterName::new('b').unwrap(),
        },
        scope: SequenceScope::default(),
        context: SemanticContext {
            parent: node("root"),
            cursor: ProjectFrame(25),
            selected_child: Some(node("c")),
            visual_selection: Some(deadpan_core::SemanticVisualSelection {
                anchor: ProjectFrame(19),
                head: ProjectFrame(17),
                extending: false,
            }),
        },
    };
    let replaced = send(&harness.service, operation.clone());
    let Outcome::Applied {
        cursor,
        visual_selection,
        committed,
        refresh_error,
        ..
    } = &receipt(&replaced).outcome
    else {
        panic!("range replacement receipt")
    };
    assert_eq!(*cursor, ProjectFrame(17));
    assert!(visual_selection.is_none());
    assert!(refresh_error.as_ref().unwrap().contains("Reopen"));
    let committed = committed.as_ref().unwrap();
    assert_eq!(committed.cursor, Some(ProjectFrame(17)));
    assert!(committed.range_selection.is_none());
    assert_eq!(
        receipt(&replaced).bank_version,
        receipt(&copied_update).bank_version
    );
    assert_eq!(
        replaced.workspace.as_ref().unwrap().document,
        before_replace
    );
    assert_eq!(
        rows(&path),
        (before_rows.0 + 1, before_rows.1 + 1, before_rows.2 + 1)
    );
    assert_eq!(copied(&replaced, 'b').slice(), copy.slice());
    let duplicate = send(&harness.service, operation);
    assert_eq!(receipt(&duplicate).committed(), Some(committed.as_ref()));
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly).unwrap();
    let after = reader.snapshot().unwrap();
    assert_eq!(after.duration().unwrap().frames(), 41);
    assert!(
        after
            .nodes()
            .contains_key(committed.selected_node.as_ref().unwrap())
    );
    reader.validate().unwrap();
    drop(reader);
    command(&harness.service, ProjectRequest::Close);
    let reopened = command(&harness.service, ProjectRequest::Open(path));
    assert_eq!(
        reopened.workspace.as_ref().unwrap().document.as_ref(),
        &after
    );
    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: committed.revision.clone(),
        },
    );
    let mut expected = serde_json::to_value(before_replace.as_ref()).unwrap();
    expected["revision_id"] =
        serde_json::to_value(undone.workspace.as_ref().unwrap().document.revision_id()).unwrap();
    assert_eq!(
        serde_json::to_value(undone.workspace.as_ref().unwrap().document.as_ref()).unwrap(),
        expected
    );
    assert_eq!(copied(&undone, 'b').slice(), copy.slice());
}

#[test]
fn bank_only_yank_refresh_failure_retains_the_selected_child_copy_and_redo() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("macro-yank-refresh.deadpan");
    let (harness, opened) = setup(&path);
    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: opened
                .workspace
                .as_ref()
                .unwrap()
                .document
                .revision_id()
                .clone(),
        },
    );
    let before = undone.workspace.as_ref().unwrap().document.clone();
    let before_rows = rows(&path);
    let operation = Operation::Apply {
        id: id(&undone, 1),
        instruction: SemanticInstruction::YankBeat {
            register: RegisterName::new('b').unwrap(),
        },
        scope: SequenceScope::default(),
        context: SemanticContext {
            parent: node("root"),
            cursor: ProjectFrame(17),
            selected_child: Some(node("a")),
            visual_selection: None,
        },
    };
    harness
        .service
        .shared
        .workspace_refresh_failure
        .store(true, Ordering::Release);
    let copied_update = send(&harness.service, operation.clone());
    let saved = receipt(&copied_update);
    assert!(saved.committed().is_none());
    assert_eq!(saved.bank_version, 1);
    let Outcome::Applied {
        cursor,
        selected,
        refresh_error,
        ..
    } = &saved.outcome
    else {
        panic!("direct yank receipt")
    };
    assert_eq!(*cursor, ProjectFrame(17));
    assert_eq!(selected.as_ref(), Some(&node("a")));
    assert!(refresh_error.as_ref().unwrap().contains("Reopen"));
    let copy = copied(&copied_update, 'b').clone();
    assert_eq!(copy.child_label(), Some("a"));
    assert_eq!(copy.source_path(), &["Your edit".to_owned()]);
    assert_eq!(
        copy.slice().selection(),
        &SliceCaptureSelection::Child { node: node("a") }
    );
    assert_eq!(
        copy.slice().range(),
        FrameRange::new(ProjectFrame(0), ProjectFrame(10)).unwrap()
    );
    assert_eq!(&copy.id().source_revision, before.revision_id());
    assert_eq!(copy.id().persisted_version, Some(saved.bank_version));
    assert!(Arc::ptr_eq(&copy, copied(&copied_update, '"')));
    assert_eq!(rows(&path), before_rows);
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly).unwrap();
    assert_eq!(&reader.snapshot().unwrap(), before.as_ref());
    assert!(reader.history_availability().unwrap().1);
    drop(reader);
    let duplicate = send(&harness.service, operation);
    assert_eq!(receipt(&duplicate).bank_version, saved.bank_version);
    assert_eq!(rows(&path), before_rows);
    command(&harness.service, ProjectRequest::Close);
    let reopened = command(&harness.service, ProjectRequest::Open(path));
    assert_eq!(copied(&reopened, 'b').slice(), copy.slice());
    assert_eq!(copied(&reopened, 'b').child_label(), Some("a"));
    let redone = command(
        &harness.service,
        ProjectRequest::Redo {
            expected_revision: before.revision_id().clone(),
        },
    );
    assert!(redone.error.is_none(), "{:?}", redone.error);
    assert_eq!(
        redone.workspace.as_ref().unwrap().plan.duration().frames(),
        30
    );
    assert_eq!(copied(&redone, 'b').slice(), copy.slice());
}

#[test]
fn counted_yank_paste_prepares_intermediate_runtime_copy_and_full_undo_redo() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("macro-yank-paste.deadpan");
    let (harness, opened) = setup(&path);
    let saved = send(
        &harness.service,
        save(
            &opened,
            1,
            'a',
            program(vec![
                SemanticInstruction::Yank {
                    selector: SemanticSelector::SelectedBeat,
                    register: RegisterName::new('b').unwrap(),
                },
                SemanticInstruction::Paste {
                    register: RegisterName::new('b').unwrap(),
                    before: true,
                },
            ]),
        ),
    );
    let before = saved.workspace.as_ref().unwrap().document.clone();
    let before_rows = rows(&path);
    let mut operation = run(&saved, 2, 'a', 2, 24);
    let Operation::Run { context, .. } = &mut operation else {
        unreachable!()
    };
    context.selected_child = Some(node("a"));
    let executed = send(&harness.service, operation);
    let after = executed.workspace.as_ref().unwrap().document.clone();
    assert_eq!(after.duration().unwrap().frames(), 50);
    assert_eq!(
        rows(&path),
        (before_rows.0 + 1, before_rows.1 + 1, before_rows.2 + 2)
    );
    let copy = copied(&executed, 'b').clone();
    assert_eq!(copy.child_label(), Some("Copied contents"));
    assert_eq!(
        copy.bounds(),
        FrameRange::new(ProjectFrame(0), ProjectFrame(40)).unwrap()
    );
    assert_ne!(&copy.id().source_revision, before.revision_id());
    assert_eq!(
        copy.id().persisted_version,
        Some(receipt(&executed).bank_version)
    );
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly).unwrap();
    let captured = reader
        .capture_snapshot_at(&copy.id().source_revision)
        .unwrap();
    copy.slice().validate_capture(&captured).unwrap();
    let SliceCaptureSelection::Child { node: copied_root } = copy.slice().selection() else {
        panic!("second yank must capture the first paste's selected wrapper")
    };
    assert_eq!(
        copy.child_label(),
        Some(captured.nodes()[copied_root].label.as_str())
    );
    let deadpan_core::NodeKind::Sequence {
        children: copied_children,
    } = &captured.nodes()[copied_root].kind
    else {
        panic!("pasted wrapper")
    };
    assert_eq!(copied_children.len(), 1);
    assert_ne!(copied_children[0], node("a"));
    assert_eq!(
        captured.nodes()[&copied_children[0]],
        before.nodes()[&node("a")]
    );
    drop(reader);
    let committed = receipt(&executed).committed().unwrap();
    assert_eq!(committed.cursor, Some(ProjectFrame(0)));
    let selected = committed.selected_node.as_ref().unwrap();
    assert_ne!(selected, copied_root);
    let deadpan_core::NodeKind::Sequence { children } = &after.nodes()[after.root()].kind else {
        unreachable!()
    };
    assert_eq!(
        children,
        &[
            selected.clone(),
            copied_root.clone(),
            node("a"),
            node("b"),
            node("c")
        ]
    );
    let deadpan_core::NodeKind::Sequence {
        children: pasted_children,
    } = &after.nodes()[selected].kind
    else {
        panic!("second paste wrapper")
    };
    assert_eq!(pasted_children.len(), 1);
    assert_ne!(&pasted_children[0], copied_root);
    assert_eq!(after.nodes()[&pasted_children[0]].label, "Copied contents");
    let deadpan_core::NodeKind::Sequence {
        children: nested_children,
    } = &after.nodes()[&pasted_children[0]].kind
    else {
        panic!("copied first paste wrapper")
    };
    assert_eq!(nested_children.len(), 1);
    assert_eq!(
        after.nodes()[&nested_children[0]],
        before.nodes()[&node("a")]
    );
    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: committed.revision.clone(),
        },
    );
    let mut expected = serde_json::to_value(before.as_ref()).unwrap();
    expected["revision_id"] =
        serde_json::to_value(undone.workspace.as_ref().unwrap().document.revision_id()).unwrap();
    assert_eq!(
        serde_json::to_value(undone.workspace.as_ref().unwrap().document.as_ref()).unwrap(),
        expected
    );
    command(&harness.service, ProjectRequest::Close);
    let reopened = command(&harness.service, ProjectRequest::Open(path));
    assert_eq!(copied(&reopened, 'b').slice(), copy.slice());
    assert_eq!(copied(&reopened, 'b').child_label(), copy.child_label());
    let redone = command(
        &harness.service,
        ProjectRequest::Redo {
            expected_revision: reopened
                .workspace
                .as_ref()
                .unwrap()
                .document
                .revision_id()
                .clone(),
        },
    );
    let mut expected = serde_json::to_value(after.as_ref()).unwrap();
    expected["revision_id"] =
        serde_json::to_value(redone.workspace.as_ref().unwrap().document.revision_id()).unwrap();
    assert_eq!(
        serde_json::to_value(redone.workspace.as_ref().unwrap().document.as_ref()).unwrap(),
        expected
    );
    assert_eq!(copied(&redone, 'b').slice(), copy.slice());
}

#[test]
fn empty_selected_beat_yanks_and_pastes_at_its_sibling_slot_without_cursor_inference() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("macro-empty-paste.deadpan");
    let mut store = seed_holds(&path, &["a", "b"]);
    seed_command(
        &mut store,
        Command::Insert {
            parent: node("root"),
            index: 1,
            subtree: Subtree {
                root: node("empty"),
                nodes: BTreeMap::from([(node("empty"), BeatNode::sequence("Empty beat", vec![]))]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
        "with-empty",
    );
    drop(store);
    let harness = Harness::new();
    let opened = command(&harness.service, ProjectRequest::Open(path.clone()));
    let copied_update = send(
        &harness.service,
        Operation::Apply {
            id: id(&opened, 1),
            instruction: SemanticInstruction::Yank {
                selector: SemanticSelector::SelectedBeat,
                register: RegisterName::new('c').unwrap(),
            },
            scope: SequenceScope::default(),
            context: SemanticContext {
                parent: node("root"),
                cursor: ProjectFrame(3),
                selected_child: Some(node("empty")),
                visual_selection: None,
            },
        },
    );
    assert_eq!(
        copied(&copied_update, 'c').child_label(),
        Some("Empty beat")
    );
    let pasted = send(
        &harness.service,
        Operation::Apply {
            id: id(&copied_update, 2),
            instruction: SemanticInstruction::Paste {
                register: RegisterName::new('c').unwrap(),
                before: false,
            },
            scope: SequenceScope::default(),
            context: SemanticContext {
                parent: node("root"),
                cursor: ProjectFrame(3),
                selected_child: Some(node("b")),
                visual_selection: None,
            },
        },
    );
    let committed = receipt(&pasted).committed().unwrap();
    assert_eq!(committed.cursor, Some(ProjectFrame(20)));
    let selected = committed.selected_node.as_ref().unwrap();
    let document = &pasted.workspace.as_ref().unwrap().document;
    assert_eq!(document.duration().unwrap().frames(), 20);
    let deadpan_core::NodeKind::Sequence { children } = &document.nodes()[&node("root")].kind
    else {
        unreachable!()
    };
    assert_eq!(
        children,
        &[node("a"), node("empty"), node("b"), selected.clone()]
    );
    assert_eq!(document.nodes()[selected].label, "Copied contents");
    let deadpan_core::NodeKind::Sequence { children: imported } = &document.nodes()[selected].kind
    else {
        panic!("selected imported wrapper")
    };
    assert_eq!(imported.len(), 1);
    assert_ne!(imported[0], node("empty"));
    assert_eq!(
        document.nodes()[&imported[0]],
        document.nodes()[&node("empty")]
    );
    assert_eq!(
        document.nodes().len(),
        opened.workspace.as_ref().unwrap().document.nodes().len() + 2
    );
    assert_eq!(copied(&pasted, 'c').slice().duration().frames(), 0);
    assert_eq!(
        copied(&pasted, 'c').slice().selection(),
        &SliceCaptureSelection::Child {
            node: node("empty")
        }
    );
    assert_eq!(
        copied(&pasted, 'c').slice(),
        copied(&copied_update, 'c').slice()
    );
    ProjectStore::open(&path, AccessMode::ReadOnly)
        .unwrap()
        .validate()
        .unwrap();
}

#[test]
fn typed_selected_cuts_keep_empty_child_and_staged_labels_through_one_undo() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("typed-selected-cuts.deadpan");
    let mut store = seed_holds(&path, &["a", "b"]);
    seed_command(
        &mut store,
        Command::Insert {
            parent: node("root"),
            index: 1,
            subtree: Subtree {
                root: node("empty"),
                nodes: BTreeMap::from([(node("empty"), BeatNode::sequence("Empty beat", vec![]))]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
        "with-empty",
    );
    drop(store);
    let harness = Harness::new();
    let opened = command(&harness.service, ProjectRequest::Open(path.clone()));
    let saved = send(
        &harness.service,
        save(
            &opened,
            1,
            'm',
            program(vec![
                SemanticInstruction::Cut {
                    selector: SemanticSelector::SelectedBeat,
                    register: RegisterName::new('c').unwrap(),
                },
                SemanticInstruction::Yank {
                    selector: SemanticSelector::SelectedBeat,
                    register: RegisterName::new('d').unwrap(),
                },
                SemanticInstruction::Cut {
                    selector: SemanticSelector::SelectedBeat,
                    register: RegisterName::new('e').unwrap(),
                },
            ]),
        ),
    );
    let before = saved.workspace.as_ref().unwrap().document.clone();
    let before_rows = rows(&path);
    let mut operation = run(&saved, 2, 'm', 1, 3);
    let Operation::Run { context, .. } = &mut operation else {
        unreachable!()
    };
    context.selected_child = Some(node("empty"));
    let executed = send(&harness.service, operation);
    let after = executed.workspace.as_ref().unwrap().document.clone();
    assert_eq!(after.duration().unwrap().frames(), 10);
    assert!(!after.nodes().contains_key(&node("empty")));
    assert!(!after.nodes().contains_key(&node("b")));
    assert_eq!(
        rows(&path),
        (before_rows.0 + 1, before_rows.1 + 1, before_rows.2 + 2)
    );
    let empty = copied(&executed, 'c').clone();
    assert_eq!(empty.child_label(), Some("Empty beat"));
    assert_eq!(empty.slice().duration().frames(), 0);
    assert_eq!(
        empty.slice().selection(),
        &SliceCaptureSelection::Child {
            node: node("empty")
        }
    );
    assert_eq!(&empty.id().source_revision, before.revision_id());
    empty.slice().validate_capture(&before).unwrap();
    let staged = copied(&executed, 'd').clone();
    assert_eq!(staged.child_label(), Some("b"));
    assert_eq!(
        staged.slice().selection(),
        &SliceCaptureSelection::Child { node: node("b") }
    );
    assert_ne!(&staged.id().source_revision, before.revision_id());
    assert_ne!(&staged.id().source_revision, after.revision_id());
    assert_eq!(
        staged.id().persisted_version,
        Some(receipt(&executed).bank_version)
    );
    assert_eq!(
        copied(&executed, 'e').slice().selection(),
        staged.slice().selection()
    );
    assert_eq!(
        copied(&executed, 'e').slice().range(),
        staged.slice().range()
    );
    assert_eq!(
        copied(&executed, 'e').id().source_revision,
        staged.id().source_revision
    );
    assert_eq!(copied(&executed, 'e').child_label(), Some("b"));
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly).unwrap();
    let historical = reader
        .capture_snapshot_at(&staged.id().source_revision)
        .unwrap();
    assert!(!historical.nodes().contains_key(&node("empty")));
    staged.slice().validate_capture(&historical).unwrap();
    copied(&executed, 'e')
        .slice()
        .validate_capture(&historical)
        .unwrap();
    drop(reader);
    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: after.revision_id().clone(),
        },
    );
    let mut expected = serde_json::to_value(before.as_ref()).unwrap();
    expected["revision_id"] =
        serde_json::to_value(undone.workspace.as_ref().unwrap().document.revision_id()).unwrap();
    assert_eq!(
        serde_json::to_value(undone.workspace.as_ref().unwrap().document.as_ref()).unwrap(),
        expected
    );
    command(&harness.service, ProjectRequest::Close);
    let reopened = command(&harness.service, ProjectRequest::Open(path.clone()));
    assert_eq!(copied(&reopened, 'c').slice(), empty.slice());
    assert_eq!(copied(&reopened, 'c').child_label(), Some("Empty beat"));
    assert_eq!(copied(&reopened, 'd').slice(), staged.slice());
    assert_eq!(copied(&reopened, 'd').child_label(), Some("b"));
    let redone = command(
        &harness.service,
        ProjectRequest::Redo {
            expected_revision: reopened
                .workspace
                .as_ref()
                .unwrap()
                .document
                .revision_id()
                .clone(),
        },
    );
    let mut expected = serde_json::to_value(after.as_ref()).unwrap();
    expected["revision_id"] =
        serde_json::to_value(redone.workspace.as_ref().unwrap().document.revision_id()).unwrap();
    assert_eq!(
        serde_json::to_value(redone.workspace.as_ref().unwrap().document.as_ref()).unwrap(),
        expected
    );
    ProjectStore::open(&path, AccessMode::ReadOnly)
        .unwrap()
        .validate()
        .unwrap();
}

#[test]
fn recorded_program_runs_counted_cuts_as_one_undo_and_restores_intermediate_copy_after_reopen() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("macro-actor.deadpan");
    let (harness, opened) = setup(&path);
    let copied_before = command(&harness.service, capture(&opened, 1, 'c'));
    let body = program(vec![movement(1), cut(2, 'b')]);
    let saved = send(&harness.service, save(&copied_before, 1, 'a', body.clone()));
    assert!(Arc::ptr_eq(
        copied(&copied_before, '"'),
        copied(&saved, '"')
    ));
    assert!(Arc::ptr_eq(
        copied(&copied_before, 'c'),
        copied(&saved, 'c')
    ));
    let initial_rows = rows(&path);
    let before = saved.workspace.as_ref().unwrap().clone();
    let executed = send(&harness.service, run(&saved, 2, 'a', 2, 3));
    assert_eq!(
        executed
            .workspace
            .as_ref()
            .unwrap()
            .plan
            .duration()
            .frames(),
        26
    );
    assert_eq!(
        rows(&path),
        (initial_rows.0 + 1, initial_rows.1 + 1, initial_rows.2 + 2)
    );
    let committed = receipt(&executed).committed().unwrap();
    assert_eq!(committed.cursor, Some(ProjectFrame(5)));
    let latest = copied(&executed, 'b').clone();
    assert!(Arc::ptr_eq(&latest, copied(&executed, '"')));
    assert!(Arc::ptr_eq(
        copied(&copied_before, 'c'),
        copied(&executed, 'c')
    ));
    assert_eq!(
        latest.bounds(),
        FrameRange::new(ProjectFrame(0), ProjectFrame(28)).unwrap()
    );
    assert_ne!(&latest.id().source_revision, before.document.revision_id());
    assert_eq!(
        latest.id().persisted_version,
        Some(executed.registers.as_ref().unwrap().version)
    );
    assert_eq!(latest.source_path(), &["Your edit".to_string()]);
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly).unwrap();
    latest
        .slice()
        .validate_capture(
            &reader
                .capture_snapshot_at(&latest.id().source_revision)
                .unwrap(),
        )
        .unwrap();
    drop(reader);
    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: committed.revision.clone(),
        },
    );
    let undone_workspace = undone.workspace.as_ref().unwrap();
    let mut expected_undo = serde_json::to_value(before.document.as_ref()).unwrap();
    expected_undo["revision_id"] =
        serde_json::to_value(undone_workspace.document.revision_id()).unwrap();
    assert_eq!(
        serde_json::to_value(undone_workspace.document.as_ref()).unwrap(),
        expected_undo
    );
    assert_eq!(copied(&undone, 'b').slice(), latest.slice());
    let closed = command(&harness.service, ProjectRequest::Close);
    assert!(closed.saved_macro.is_none() && closed.macros.is_none());
    let reopened = command(&harness.service, ProjectRequest::Open(path));
    assert!(reopened.saved_macro.is_none() && reopened.macros.is_none());
    let Value::Macro(restored) = &reopened.registers.as_ref().unwrap().entries[&'a'] else {
        panic!("macro")
    };
    assert_eq!(restored, &body);
    assert_eq!(copied(&reopened, 'b').slice(), latest.slice());
    assert_eq!(copied(&reopened, 'b').bounds(), latest.bounds());
}

#[test]
fn save_preserves_redo_and_motion_only_execution_never_writes_history_or_registers() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("macro-motion.deadpan");
    let (harness, opened) = setup(&path);
    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: opened
                .workspace
                .as_ref()
                .unwrap()
                .document
                .revision_id()
                .clone(),
        },
    );
    let initial_rows = rows(&path);
    let saved = send(
        &harness.service,
        save(&undone, 1, 'm', program(vec![movement(3)])),
    );
    assert_eq!(rows(&path), initial_rows);
    let bank = saved.registers.as_ref().unwrap().clone();
    let executed = send(&harness.service, run(&saved, 2, 'm', 2, 4));
    assert!(receipt(&executed).committed().is_none());
    let Outcome::Executed { cursor, .. } = receipt(&executed).outcome else {
        panic!("executed")
    };
    assert_eq!(cursor, ProjectFrame(10));
    assert!(Arc::ptr_eq(&bank, executed.registers.as_ref().unwrap()));
    assert_eq!(rows(&path), initial_rows);
    let redone = command(
        &harness.service,
        ProjectRequest::Redo {
            expected_revision: executed
                .workspace
                .as_ref()
                .unwrap()
                .document
                .revision_id()
                .clone(),
        },
    );
    assert!(redone.error.is_none(), "{:?}", redone.error);
    assert_eq!(redone.workspace.unwrap().plan.duration().frames(), 30);
}

#[test]
fn stale_or_changed_identity_wrong_type_scope_and_late_failure_publish_no_partial_edits() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("macro-refused.deadpan");
    let (harness, opened) = setup(&path);
    let initial_rows = rows(&path);
    let operation = save(
        &opened,
        1,
        'a',
        program(vec![
            cut(1, 'b'),
            SemanticInstruction::Call {
                register: RegisterName::new('z').unwrap(),
                count: NonZeroU32::new(1).unwrap(),
            },
        ]),
    );
    let saved = send(&harness.service, operation.clone());
    let duplicate = send(&harness.service, operation.clone());
    assert_eq!(
        receipt(&saved).bank_version,
        receipt(&duplicate).bank_version
    );
    let mut collision = operation;
    let Operation::Save { program: body, .. } = &mut collision else {
        unreachable!()
    };
    *body = program(vec![movement(4)]);
    let mut stale_session = run(&saved, 3, 'a', 1, 0);
    let Operation::Run { id, .. } = &mut stale_session else {
        unreachable!()
    };
    id.session += 1;
    let mut wrong_scope = run(&saved, 4, 'a', 1, 0);
    let Operation::Run { context, .. } = &mut wrong_scope else {
        unreachable!()
    };
    context.parent = node("a");
    for operation in [
        collision,
        run(&opened, 2, 'a', 1, 0),
        stale_session,
        wrong_scope,
        run(&saved, 5, 'a', 1, 0),
    ] {
        let failed = command(&harness.service, ProjectRequest::Macro(operation));
        assert!(failed.macros.as_ref().unwrap().result.is_err());
        assert_eq!(
            failed.registers.as_ref().unwrap().version,
            saved.registers.as_ref().unwrap().version
        );
        assert_eq!(
            failed.workspace.as_ref().unwrap().document,
            opened.workspace.as_ref().unwrap().document
        );
        assert_eq!(failed.saved_macro.as_ref().unwrap().id, receipt(&saved).id);
        assert_eq!(rows(&path), initial_rows);
    }
    let copy = command(&harness.service, capture(&saved, 2, 'c'));
    let failed = command(
        &harness.service,
        ProjectRequest::Macro(run(&copy, 6, 'c', 1, 0)),
    );
    assert!(failed.macros.unwrap().result.unwrap_err().contains("macro"));
    assert_eq!(rows(&path), initial_rows);
}

#[test]
fn committed_macro_refresh_failure_retains_receipt_and_copy_and_exact_retry_is_idempotent() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("macro-refresh.deadpan");
    let (harness, opened) = setup(&path);
    let saved = send(
        &harness.service,
        save(&opened, 1, 'a', program(vec![cut(2, 'b')])),
    );
    let initial_rows = rows(&path);
    harness
        .service
        .shared
        .workspace_refresh_failure
        .store(true, Ordering::Release);
    let operation = run(&saved, 2, 'a', 2, 3);
    let executed = send(&harness.service, operation.clone());
    let retained = receipt(&executed).clone();
    let Outcome::Executed {
        refresh_error,
        committed,
        ..
    } = &retained.outcome
    else {
        panic!("executed")
    };
    assert!(refresh_error.as_ref().unwrap().contains("Reopen"));
    assert_eq!(
        executed.workspace.as_ref().unwrap().document.revision_id(),
        saved.workspace.as_ref().unwrap().document.revision_id()
    );
    assert_eq!(
        rows(&path),
        (initial_rows.0 + 1, initial_rows.1 + 1, initial_rows.2 + 2)
    );
    let duplicate = send(&harness.service, operation);
    assert_eq!(receipt(&duplicate).committed(), committed.as_deref());
    let failed = command(
        &harness.service,
        ProjectRequest::Macro(run(&executed, 3, 'a', 1, 0)),
    );
    assert!(
        failed
            .macros
            .as_ref()
            .unwrap()
            .result
            .as_ref()
            .unwrap_err()
            .contains("Reopen")
    );
    assert_eq!(
        failed.saved_macro.as_ref().unwrap().committed(),
        committed.as_deref()
    );
    assert!(Arc::ptr_eq(copied(&executed, 'b'), copied(&failed, 'b')));
    assert_eq!(
        rows(&path),
        (initial_rows.0 + 1, initial_rows.1 + 1, initial_rows.2 + 2)
    );
    command(&harness.service, ProjectRequest::Close);
    let reopened = command(&harness.service, ProjectRequest::Open(path));
    assert_eq!(
        reopened.workspace.as_ref().unwrap().document.revision_id(),
        &committed.as_ref().unwrap().revision
    );
    assert_eq!(
        copied(&reopened, 'b').slice(),
        copied(&executed, 'b').slice()
    );
    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: committed.as_ref().unwrap().revision.clone(),
        },
    );
    assert_eq!(undone.workspace.unwrap().plan.duration().frames(), 30);
}

#[test]
fn nested_macro_copies_keep_their_staged_absolute_bounds_and_original_group_labels() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("nested-macro.deadpan");
    let mut store = seed_holds(&path, &["lead"]);
    seed_command(
        &mut store,
        Command::Insert {
            parent: node("root"),
            index: 1,
            subtree: Subtree {
                root: node("group"),
                nodes: BTreeMap::from([
                    (
                        node("group"),
                        BeatNode::sequence("A named group", vec![node("nested")]),
                    ),
                    (node("nested"), BeatNode::hold("Nested pause", hold(10))),
                ]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
        "nested",
    );
    drop(store);
    let harness = Harness::new();
    let opened = command(&harness.service, ProjectRequest::Open(path));
    let saved = send(
        &harness.service,
        save(&opened, 1, 'a', program(vec![cut(1, 'b')])),
    );
    let scope = SequenceScope::default()
        .descend(saved.workspace.as_ref().unwrap(), &node("group"))
        .unwrap();
    let operation = Operation::Run {
        id: id(&saved, 2),
        register: 'a',
        count: 2,
        scope: scope.clone(),
        context: SemanticContext {
            parent: node("group"),
            cursor: ProjectFrame(12),
            selected_child: None,
            visual_selection: None,
        },
    };
    let executed = send(&harness.service, operation);
    let copied = copied(&executed, 'b');
    assert_eq!(copied.scope(), &scope);
    assert_eq!(
        copied.bounds(),
        FrameRange::new(ProjectFrame(10), ProjectFrame(19)).unwrap()
    );
    assert_eq!(
        copied.source_path(),
        &["Your edit".to_string(), "A named group".to_string()]
    );
    assert_eq!(
        copied.slice().range(),
        FrameRange::new(ProjectFrame(12), ProjectFrame(13)).unwrap()
    );
}
