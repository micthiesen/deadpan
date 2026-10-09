//! Exercise the real writer with refresh failure after durable publication.

use deadpan_core::AudioSample;

use super::*;

pub(super) fn counts(path: &Path) -> (i64, i64) {
    rusqlite::Connection::open(path.join("project.sqlite"))
        .unwrap()
        .query_row(
            "SELECT (SELECT count(*) FROM revisions), (SELECT count(*) FROM history)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap()
}

fn fail_refresh(service: &ProjectService) {
    service
        .shared
        .workspace_refresh_failure
        .store(true, Ordering::Release);
}

fn warning(message: &str, saved: &str) {
    assert!(message.contains(saved), "{message}");
    assert!(message.contains("Injected failure"), "{message}");
    assert!(message.contains("Reopen this project"), "{message}");
}

fn persisted(before: &Workspace, update: &ProjectUpdate) -> ProjectDocument {
    let receipt = update.committed.as_ref().expect("saved edit receipt");
    // A failed refresh must retain the old view. Its mismatched revision keeps
    // this receipt from consuming the visible selection in the native router.
    assert_eq!(
        *update.workspace.as_ref().unwrap().document,
        *before.document
    );
    assert_ne!(&receipt.revision, before.document.revision_id());
    let reader = ProjectStore::open(&before.path, AccessMode::ReadOnly).unwrap();
    let saved = reader.snapshot().unwrap();
    assert_eq!(saved.revision_id(), &receipt.revision);
    saved
}

fn reopen(service: &ProjectService, path: &Path) -> Arc<Workspace> {
    command(service, ProjectRequest::Close);
    let opened = command(service, ProjectRequest::Open(path.into()));
    assert!(opened.error.is_none(), "{:?}", opened.error);
    assert!(opened.committed.is_none());
    opened.workspace.unwrap()
}

fn assert_undo(service: &ProjectService, before: &Workspace, saved: &Workspace) {
    let undone = command(
        service,
        ProjectRequest::Undo {
            expected_revision: saved.document.revision_id().clone(),
        },
    );
    assert!(undone.error.is_none(), "{:?}", undone.error);
    let actual = undone.workspace.unwrap();
    assert_eq!(actual.document.nodes(), before.document.nodes());
    assert_eq!(actual.document.assets(), before.document.assets());
    assert_eq!(actual.document.sounds(), before.document.sounds());
    assert_ne!(actual.document.revision_id(), before.document.revision_id());
    assert!(actual.can_redo);
}

#[test]
fn sound_place_and_delete_retain_exact_saved_receipts_after_refresh_failure() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::with_library(Some(
        ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap(),
    ));
    let before = sound::catalog(&harness);
    let request = || {
        sound::request(
            &before,
            ProjectSoundEdit::Place {
                asset: sound::catalog_asset(&before),
                at: AudioSample(137),
            },
        )
    };
    let rows = counts(&before.path);
    fail_refresh(&harness.service);
    let update = command(&harness.service, request());
    warning(update.error.as_ref().unwrap(), "Sound placed and saved");
    let saved = persisted(&before, &update);
    let receipt = update.committed.unwrap();
    assert!(receipt.preserve_cursor);
    assert!(receipt.cursor.is_none());
    assert!(receipt.selected_node.is_none());
    let sound = receipt.sound.unwrap().selected.unwrap();
    assert_eq!(saved.sounds()[&sound].offset, AudioSample(137));
    assert_eq!(counts(&before.path), (rows.0 + 1, rows.1 + 1));
    let retry = command(&harness.service, request());
    assert!(retry.error.is_some());
    assert!(retry.committed.is_none());
    assert_eq!(counts(&before.path), (rows.0 + 1, rows.1 + 1));
    let placed = reopen(&harness.service, &before.path);
    assert_eq!(*placed.document, saved);

    let rows = counts(&before.path);
    fail_refresh(&harness.service);
    let removed = command(
        &harness.service,
        sound::request(&placed, ProjectSoundEdit::Delete { id: sound.clone() }),
    );
    warning(removed.error.as_ref().unwrap(), "Sound removed and saved");
    assert!(!persisted(&placed, &removed).sounds().contains_key(&sound));
    assert_eq!(removed.committed.unwrap().sound.unwrap().selected, None);
    assert_eq!(counts(&before.path), (rows.0 + 1, rows.1 + 1));
    let deleted = reopen(&harness.service, &before.path);
    assert_undo(&harness.service, &placed, &deleted);
}

fn insertion_failure(cold: bool) {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::new();
    create(&harness.service, &scratch.path().join("insert.deadpan"));
    let mut before = harness.imported("cfr-bframes.mp4");
    if cold {
        before = reopen(&harness.service, &before.path);
    }
    let asset = before.sources.keys().next().unwrap().clone();
    let rows = counts(&before.path);
    fail_refresh(&harness.service);
    let mut update = insert(&harness.service, &before, &asset);
    if cold {
        assert!(update.committed.is_none());
        assert_eq!(counts(&before.path), rows);
        harness.finish(harness.job());
        update = wait(&harness.service, |update| {
            update
                .import
                .as_ref()
                .is_some_and(|status| status.stage == ImportStage::Failed)
        });
        let status = update.import.as_ref().unwrap();
        assert_eq!(status.asset.as_ref(), Some(&asset));
        warning(status.error.as_ref().unwrap(), "Source inserted and saved");
    } else {
        warning(update.error.as_ref().unwrap(), "Source inserted and saved");
        assert!(harness.jobs.try_recv().is_err());
    }
    let saved = persisted(&before, &update);
    let receipt = update.committed.unwrap();
    assert!(!receipt.preserve_cursor);
    assert_eq!(receipt.scope, SequenceScope::default());
    assert!(receipt.sound.is_none());
    assert!(matches!(
        saved.nodes()[&receipt.selected_node.unwrap()].kind,
        NodeKind::Source { .. }
    ));
    assert_eq!(saved.duration().unwrap().frames(), 120);
    assert_eq!(counts(&before.path), (rows.0 + 1, rows.1 + 1));
    let retry = insert(&harness.service, &before, &asset);
    assert!(retry.error.is_some());
    assert!(retry.committed.is_none());
    assert_eq!(counts(&before.path), (rows.0 + 1, rows.1 + 1));
    let reopened = reopen(&harness.service, &before.path);
    assert_eq!(*reopened.document, saved);
    assert_undo(&harness.service, &before, &reopened);
}

#[test]
fn warm_original_insertion_retains_saved_receipt_after_refresh_failure() {
    insertion_failure(false);
}

#[test]
fn worker_original_insertion_retains_saved_receipt_after_refresh_failure() {
    insertion_failure(true);
}

#[test]
fn initialized_original_retains_saved_baseline_after_refresh_failure() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::with_library(Some(
        ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap(),
    ));
    let before = command(
        &harness.service,
        ProjectRequest::CreateFromSource {
            ownership: OriginalOwnership::Managed,
            path: fixture("cfr-bframes.mp4"),
        },
    )
    .workspace
    .unwrap();
    harness.finish(harness.job());
    let qualification = harness.job();
    fail_refresh(&harness.service);
    harness.finish(qualification);
    let failed = wait(&harness.service, |update| {
        update
            .import
            .as_ref()
            .is_some_and(|status| status.stage == ImportStage::Failed)
    });
    warning(
        failed.import.as_ref().unwrap().error.as_ref().unwrap(),
        "Original saved",
    );
    let saved = persisted(&before, &failed);
    let receipt = failed.committed.unwrap();
    let asset = failed.import.unwrap().asset.unwrap();
    assert_eq!(saved.duration().unwrap().frames(), 120);
    let rows = counts(&before.path);
    assert_eq!(rows, (2, 1));
    let rejected = command(
        &harness.service,
        ProjectRequest::InitializeSource {
            ownership: OriginalOwnership::Managed,
            expected_session: before.session,
            expected_revision: before.document.revision_id().clone(),
            path: fixture("cfr-bframes.mp4"),
        },
    );
    assert!(rejected.committed.is_none());
    // This stale request may prepare, but can never initialize a second baseline.
    if rejected.error.is_none() {
        harness.finish(harness.job());
        harness.finish(harness.job());
        let rejected = wait(&harness.service, |update| {
            update
                .import
                .as_ref()
                .is_some_and(|status| status.stage == ImportStage::Failed)
        });
        assert!(rejected.committed.is_none());
        assert!(
            !rejected
                .import
                .unwrap()
                .error
                .unwrap()
                .contains("Original saved")
        );
    }
    assert_eq!(counts(&before.path), rows);
    let reopened = reopen(&harness.service, &before.path);
    assert_eq!(*reopened.document, saved);
    assert!(!reopened.can_undo);
    assert!(!reopened.can_redo);
    let Some(SingleSourceState::Ready {
        asset: actual_asset,
        node,
        ..
    }) = &reopened.single_source
    else {
        panic!("saved Original baseline is missing");
    };
    assert_eq!(actual_asset, &asset);
    assert_eq!(receipt.selected_node.as_ref(), Some(node));
}

#[test]
fn catalog_registration_reports_saved_asset_without_inventing_an_edit_receipt() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::new();
    let before = create(&harness.service, &scratch.path().join("catalog.deadpan"));
    import(&harness.service, "cfr-bframes.mp4");
    harness.finish(harness.job());
    let qualification = harness.job();
    fail_refresh(&harness.service);
    harness.finish(qualification);
    let failed = wait(&harness.service, |update| {
        update
            .import
            .as_ref()
            .is_some_and(|status| status.stage == ImportStage::Failed)
    });
    let status = failed.import.unwrap();
    warning(status.error.as_ref().unwrap(), "Source registered");
    assert!(failed.committed.is_none());
    assert_eq!(*failed.workspace.unwrap().document, *before.document);
    let reopened = reopen(&harness.service, &before.path);
    assert!(reopened.sources.contains_key(&status.asset.unwrap()));
    assert_eq!(reopened.document.duration().unwrap().frames(), 0);
}

#[test]
fn qualification_failure_has_no_saved_receipt_or_history_change() {
    let scratch = tempfile::tempdir().unwrap();
    let harness = Harness::new();
    create(&harness.service, &scratch.path().join("refused.deadpan"));
    let before = harness.imported("cfr-bframes.mp4");
    let before = reopen(&harness.service, &before.path);
    let rows = counts(&before.path);
    let asset = before.sources.keys().next().unwrap();
    insert(&harness.service, &before, asset);
    let job = harness.job();
    harness
        .replies
        .send(worker::Reply {
            id: job.id,
            result: Err("injected qualification refusal".into()),
        })
        .unwrap();
    let failed = wait(&harness.service, |update| {
        update
            .import
            .as_ref()
            .is_some_and(|status| status.stage == ImportStage::Failed)
    });
    assert!(failed.committed.is_none());
    assert_eq!(
        failed.import.unwrap().error.as_deref(),
        Some("injected qualification refusal")
    );
    assert_eq!(*failed.workspace.unwrap().document, *before.document);
    assert_eq!(counts(&before.path), rows);
}
