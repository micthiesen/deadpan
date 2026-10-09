//! Service boundaries for persistent copies, independent of timeline history.

use super::*;
use crate::project::registers::{Bank, OriginalRequest, Value};
use crate::project::slice::{CaptureRequest, CopyId};

mod compound;

fn id(workspace: &Workspace, request: u64) -> CopyId {
    CopyId {
        session: workspace.session,
        project: workspace.document.project_id().clone(),
        source_revision: workspace.document.revision_id().clone(),
        request,
        persisted_version: None,
    }
}

fn capture(
    workspace: &Workspace,
    request: u64,
    register: char,
    start: i64,
    end: i64,
) -> CaptureRequest {
    CaptureRequest {
        id: id(workspace, request),
        register: Some(register),
        scope: SequenceScope::default(),
        parent: workspace.document.root().clone(),
        selection: deadpan_core::SliceCaptureSelection::Range {
            range: FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap(),
        },
    }
}

fn copied(bank: &Bank, register: char) -> &Arc<crate::project::slice::Captured> {
    let Value::Edited(copied) = &bank.entries[&register] else {
        panic!("edited register")
    };
    copied
}

fn counts(path: &Path) -> (i64, i64) {
    rusqlite::Connection::open(path.join("project.sqlite"))
        .unwrap()
        .query_row(
            "SELECT (SELECT count(*) FROM revisions), (SELECT count(*) FROM history)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap()
}

#[test]
fn original_and_edited_names_reopen_without_history_and_restamp_shared_aliases() {
    let scratch = tempfile::tempdir().unwrap();
    let service = ProjectService::start(
        Arc::new(|| {}),
        Some(ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap()),
    )
    .unwrap();
    service
        .submit(ProjectRequest::CreateFromSource {
            ownership: OriginalOwnership::Managed,
            path: fixture("cfr-bframes.mp4"),
        })
        .unwrap();
    let initialized = wait(&service, |update| {
        update.import.as_ref().is_some_and(|status| {
            matches!(status.stage, ImportStage::Complete | ImportStage::Failed)
        })
    });
    assert!(initialized.error.is_none(), "{:?}", initialized.error);
    let before = initialized.workspace.unwrap();
    let initial_counts = counts(&before.path);
    let source = before
        .sources
        .values()
        .find(|source| source.video_index.is_some())
        .unwrap();
    let original = OriginalRequest {
        id: id(&before, 1),
        register: Some('o'),
        asset: source.asset.clone(),
        qualification: source.receipt.id().clone(),
        ordinals: 5..13,
    };
    let saved = command(&service, ProjectRequest::CaptureOriginal(original.clone()));
    assert!(saved.captured_original.unwrap().result.is_ok());
    let original_bank = saved.registers.unwrap();
    assert_eq!(original_bank.version, 1);
    let saved = command(
        &service,
        ProjectRequest::CaptureEditSlice(capture(&before, 2, 'e', 7, 18)),
    );
    assert!(Arc::ptr_eq(saved.workspace.as_ref().unwrap(), &before));
    let bank = saved.registers.unwrap();
    let immediate = saved.captured_slice.unwrap().result.unwrap();
    assert!(Arc::ptr_eq(copied(&bank, 'e'), &immediate));
    assert!(Arc::ptr_eq(copied(&bank, 'e'), copied(&bank, '"')));
    assert_eq!(bank.version, 2);
    assert_eq!(counts(&before.path), initial_counts);

    let closed = command(&service, ProjectRequest::Close);
    assert!(closed.registers.is_none());
    assert!(closed.captured_original.is_none());
    let reopened = command(&service, ProjectRequest::Open(before.path.clone()));
    let after = reopened.workspace.unwrap();
    let restored = reopened.registers.unwrap();
    assert_eq!(restored.version, bank.version);
    assert_eq!(restored.session, after.session);
    assert_ne!(restored.session, bank.session);
    assert_eq!(*after.document, *before.document);
    assert_eq!(counts(&before.path), initial_counts);
    assert!(Arc::ptr_eq(copied(&restored, 'e'), copied(&restored, '"')));
    assert_eq!(copied(&restored, 'e').slice(), immediate.slice());
    assert_eq!(
        copied(&restored, 'e').id().persisted_version,
        Some(bank.version)
    );
    assert_eq!(copied(&restored, 'e').id().session, after.session);
    assert_ne!(copied(&restored, 'e').id(), immediate.id());
    assert!(
        matches!(&restored.entries[&'o'], Value::Original { asset, qualification, ordinals } if asset == &original.asset && qualification == &original.qualification && ordinals == &(5..13))
    );

    let stale = command(&service, ProjectRequest::CaptureOriginal(original));
    assert!(stale.captured_original.unwrap().result.is_err());
    assert!(Arc::ptr_eq(stale.registers.as_ref().unwrap(), &restored));
    let placed = command(
        &service,
        ProjectRequest::PasteEditedSlice(crate::project::slice::Paste {
            expected_session: after.session,
            expected_revision: after.document.revision_id().clone(),
            copied: copied(&restored, 'e').clone(),
            scope: SequenceScope::default(),
            parent: after.document.root().clone(),
            destination: crate::project::splice::Destination::Slot(1),
        }),
    );
    assert!(placed.error.is_none(), "{:?}", placed.error);
    assert_eq!(
        placed.workspace.unwrap().plan.duration().frames(),
        after.plan.duration().frames() + 11
    );
}

#[test]
fn named_cut_and_undo_reopen_the_historical_copy_and_saved_receipt_survives_new_writes() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("cut.deadpan");
    drop(seed_holds(&path, &["a", "b", "c"]));
    let harness = Harness::new();
    let before = command(&harness.service, ProjectRequest::Open(path.clone()))
        .workspace
        .unwrap();
    let initial_counts = counts(&path);
    let saved = command(
        &harness.service,
        ProjectRequest::CutEditSlice(capture(&before, 1, 'c', 3, 16)),
    );
    let receipt = saved.cut_slice.unwrap().result.unwrap();
    let after = saved.workspace.unwrap();
    let bank = saved.registers.unwrap();
    assert!(Arc::ptr_eq(copied(&bank, 'c'), &receipt.copied));
    assert!(Arc::ptr_eq(copied(&bank, 'c'), copied(&bank, '"')));
    assert_eq!(counts(&path), (initial_counts.0 + 1, initial_counts.1 + 1));
    let newer = command(
        &harness.service,
        ProjectRequest::CaptureEditSlice(capture(&after, 2, 'n', 0, 5)),
    );
    assert!(newer.captured_slice.unwrap().result.is_ok());
    assert_eq!(newer.saved_cut.unwrap().committed, receipt.committed);
    let bank = newer.registers.unwrap();
    assert_eq!(copied(&bank, 'c').slice(), receipt.copied.slice());
    assert!(!Arc::ptr_eq(copied(&bank, 'c'), copied(&bank, '"')));
    let undone = command(
        &harness.service,
        ProjectRequest::Undo {
            expected_revision: after.document.revision_id().clone(),
        },
    );
    assert_eq!(undone.workspace.unwrap().plan.duration().frames(), 30);
    assert!(Arc::ptr_eq(undone.registers.as_ref().unwrap(), &bank));
    command(&harness.service, ProjectRequest::Close);
    let reopened = command(&harness.service, ProjectRequest::Open(path));
    let restored = reopened.registers.unwrap();
    assert_eq!(copied(&restored, 'c').slice(), receipt.copied.slice());
    assert_eq!(copied(&restored, 'c').bounds(), receipt.copied.bounds());
    assert_eq!(copied(&restored, 'c').source_path(), &["Your edit"]);
    assert!(Arc::ptr_eq(copied(&restored, 'n'), copied(&restored, '"')));
    assert_eq!(restored.version, bank.version);
}

#[test]
fn register_storage_failure_preserves_runtime_bank_timeline_and_cut_receipt() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("failure.deadpan");
    drop(seed_holds(&path, &["a", "b", "c"]));
    let harness = Harness::new();
    let before = command(&harness.service, ProjectRequest::Open(path.clone()))
        .workspace
        .unwrap();
    let saved = command(
        &harness.service,
        ProjectRequest::CutEditSlice(capture(&before, 1, 'a', 0, 3)),
    );
    let receipt = saved.saved_cut.unwrap();
    let current = saved.workspace.unwrap();
    let bank = saved.registers.unwrap();
    let initial_counts = counts(&path);
    let database = rusqlite::Connection::open(path.join("project.sqlite")).unwrap();
    database.execute_batch("CREATE TRIGGER fail_register_write BEFORE INSERT ON registers BEGIN SELECT RAISE(ABORT, 'injected register storage failure'); END;").unwrap();
    let failed = command(
        &harness.service,
        ProjectRequest::CaptureEditSlice(capture(&current, 2, 'b', 5, 10)),
    );
    assert!(
        failed
            .captured_slice
            .unwrap()
            .result
            .unwrap_err()
            .contains("injected register storage failure")
    );
    assert!(Arc::ptr_eq(failed.registers.as_ref().unwrap(), &bank));
    let failed = command(
        &harness.service,
        ProjectRequest::CutEditSlice(capture(&current, 3, 'c', 5, 10)),
    );
    assert!(
        failed
            .cut_slice
            .unwrap()
            .result
            .unwrap_err()
            .contains("injected register storage failure")
    );
    assert!(Arc::ptr_eq(failed.registers.as_ref().unwrap(), &bank));
    assert_eq!(failed.saved_cut.unwrap().committed, receipt.committed);
    assert!(Arc::ptr_eq(failed.workspace.as_ref().unwrap(), &current));
    assert_eq!(counts(&path), initial_counts);
    database
        .execute_batch("DROP TRIGGER fail_register_write")
        .unwrap();
    command(&harness.service, ProjectRequest::Close);
    let reopened = command(&harness.service, ProjectRequest::Open(path));
    let durable = reopened.registers.unwrap();
    assert_eq!(durable.version, bank.version);
    assert_eq!(durable.entries.len(), bank.entries.len());
    assert_eq!(copied(&durable, 'a').slice(), copied(&bank, 'a').slice());
}

#[test]
fn historical_scope_and_empty_child_labels_are_derived_after_reopen() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("scope.deadpan");
    let mut store = seed_holds(&path, &["a", "b"]);
    seed_command(
        &mut store,
        Command::Group {
            parent: node("root"),
            start: 0,
            end: 2,
            id: node("group"),
            label: "My group".into(),
        },
        "grouped",
    );
    seed_command(
        &mut store,
        Command::Insert {
            parent: node("group"),
            index: 1,
            subtree: Subtree {
                root: node("empty"),
                nodes: BTreeMap::from([(
                    node("empty"),
                    BeatNode::sequence("An empty child", vec![]),
                )]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
        "empty-child",
    );
    drop(store);
    let harness = Harness::new();
    let before = command(&harness.service, ProjectRequest::Open(path.clone()))
        .workspace
        .unwrap();
    let scope = SequenceScope::default()
        .descend(&before, &node("group"))
        .unwrap();
    let request = CaptureRequest {
        id: id(&before, 1),
        register: Some('e'),
        scope: scope.clone(),
        parent: node("group"),
        selection: deadpan_core::SliceCaptureSelection::Child {
            node: node("empty"),
        },
    };
    let saved = command(&harness.service, ProjectRequest::CutEditSlice(request));
    assert!(saved.cut_slice.unwrap().result.is_ok());
    command(&harness.service, ProjectRequest::Close);
    let reopened = command(&harness.service, ProjectRequest::Open(path));
    let bank = reopened.registers.unwrap();
    let restored = copied(&bank, 'e');
    assert_eq!(restored.scope(), &scope);
    assert_eq!(restored.source_path(), &["Your edit", "My group"]);
    assert_eq!(restored.child_label(), Some("An empty child"));
    assert_eq!(restored.slice().duration(), FrameDuration::ZERO);
    assert_eq!(
        restored.bounds(),
        FrameRange::new(ProjectFrame(0), ProjectFrame(20)).unwrap()
    );
}
