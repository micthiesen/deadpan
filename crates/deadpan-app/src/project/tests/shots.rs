//! Shot analysis is an annotation saved outside history and carried with the
//! workspace across edits and reopening.

use deadpan_analysis::{SIGNATURE_VERSION, ShotAnalysis};

use super::*;

/// `pictures` quiet pictures with a cut at picture 60.
fn analysis(pictures: usize) -> ShotAnalysis {
    ShotAnalysis::new(
        (0..pictures)
            .map(|picture| match picture {
                0 => [0, 0, 0],
                60 => [90, 200, 90],
                61 => [2, 1, 90],
                _ => [2, 1, 2],
            })
            .collect(),
    )
    .unwrap()
}

fn save(
    service: &ProjectService,
    session: u64,
    attempt: u64,
    key: &deadpan_store::ShotAnalysisKey,
    analysis: ShotAnalysis,
) -> ProjectUpdate {
    let _ = service.take_update();
    service
        .submit(ProjectRequest::SaveShotAnalysis {
            expected_session: session,
            attempt,
            key: key.clone(),
            analysis: Arc::new(analysis),
        })
        .unwrap();
    wait(service, |update| {
        update
            .shot_save
            .as_ref()
            .is_some_and(|save| save.attempt == attempt)
    })
}

#[test]
fn saved_shots_are_published_carried_across_edits_and_reloaded() {
    let scratch = tempfile::tempdir().unwrap();
    let service = ProjectService::start(
        Arc::new(|| {}),
        Some(ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap()),
    )
    .unwrap();
    service
        .submit(ProjectRequest::CreateFromSource {
            path: fixture("cfr-bframes.mp4"),
        })
        .unwrap();
    let initialized = wait(&service, |update| {
        update.import.as_ref().is_some_and(|status| {
            matches!(status.stage, ImportStage::Complete | ImportStage::Failed)
        })
    });
    let before = initialized.workspace.unwrap();
    assert!(before.shot_analysis.is_none());
    let Some(SingleSourceState::Ready { asset, node, .. }) = &before.single_source else {
        panic!("original not initialized")
    };
    let content = before.sources[asset]
        .receipt
        .original()
        .content()
        .to_string();
    let key = deadpan_store::ShotAnalysisKey {
        content,
        video_stream: 0,
        signature_version: SIGNATURE_VERSION.into(),
    };

    // Stale sessions, other media and a wrong picture count never save.
    let stale = save(&service, before.session + 1, 1, &key, analysis(120));
    assert!(stale.shot_save.unwrap().error.is_some());
    let foreign = deadpan_store::ShotAnalysisKey {
        content: "blake3:other".into(),
        ..key.clone()
    };
    let other = save(&service, before.session, 2, &foreign, analysis(120));
    assert!(other.shot_save.unwrap().error.is_some());
    let short = save(&service, before.session, 3, &key, analysis(119));
    assert!(short.shot_save.unwrap().error.is_some());

    let saved = save(&service, before.session, 4, &key, analysis(120));
    assert_eq!(saved.shot_save.as_ref().unwrap().error, None);
    let workspace = saved.workspace.unwrap();
    let stored = workspace.shot_analysis.as_ref().unwrap();
    assert_eq!((&stored.key, &stored.analysis), (&key, &analysis(120)));
    assert_eq!(stored.analysis.boundaries(), [60]);
    assert_eq!(
        workspace.document.revision_id(),
        before.document.revision_id()
    );
    assert!(!workspace.can_undo);

    let repeated = edited(
        &service,
        &workspace,
        ProjectEdit::WrapRepeat {
            node: node.clone(),
            plays: 2,
        },
    )
    .workspace
    .unwrap();
    assert!(Arc::ptr_eq(
        repeated.shot_analysis.as_ref().unwrap(),
        workspace.shot_analysis.as_ref().unwrap()
    ));

    let path = repeated.path.clone();
    let reopened = command(&service, ProjectRequest::Open(path));
    assert!(reopened.error.is_none(), "{:?}", reopened.error);
    let reopened = reopened.workspace.unwrap();
    assert_eq!(
        reopened.shot_analysis.as_ref().unwrap().analysis,
        analysis(120)
    );
}

/// A background analysis save must never take the user-command slot. With
/// the writer's mailbox held, the save and a user edit are both admitted in
/// the same busy window; both then complete in order, and neither replaces
/// the other's feedback.
#[test]
fn analysis_save_in_flight_never_refuses_a_user_command() {
    let scratch = tempfile::tempdir().unwrap();
    let service = ProjectService::start(
        Arc::new(|| {}),
        Some(ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap()),
    )
    .unwrap();
    service
        .submit(ProjectRequest::CreateFromSource {
            path: fixture("cfr-bframes.mp4"),
        })
        .unwrap();
    let initialized = wait(&service, |update| {
        update.import.as_ref().is_some_and(|status| {
            matches!(status.stage, ImportStage::Complete | ImportStage::Failed)
        })
    });
    let before = initialized.workspace.unwrap();
    let Some(SingleSourceState::Ready { asset, node, .. }) = &before.single_source else {
        panic!("original not initialized")
    };
    let key = deadpan_store::ShotAnalysisKey {
        content: before.sources[asset]
            .receipt
            .original()
            .content()
            .to_string(),
        video_stream: 0,
        signature_version: SIGNATURE_VERSION.into(),
    };
    let save = |attempt| ProjectRequest::SaveShotAnalysis {
        expected_session: before.session,
        attempt,
        key: key.clone(),
        analysis: Arc::new(analysis(120)),
    };

    service.hold_requests_for_check(true);
    service.submit(save(7)).unwrap();
    assert!(service.annotation_busy() && !service.is_busy());
    // A second background save waits for its own lane, without touching the
    // user-command slot.
    assert_eq!(
        service.submit(save(8)).unwrap_err(),
        "An analysis save is already queued"
    );
    assert!(!service.is_busy());
    service
        .submit(edit_request(
            &before,
            ProjectEdit::WrapRepeat {
                node: node.clone(),
                plays: 2,
            },
        ))
        .unwrap();
    assert!(service.is_busy() && service.annotation_busy());
    // The user-command slot itself stays single.
    assert_eq!(
        service
            .submit(ProjectRequest::Undo {
                expected_revision: before.document.revision_id().clone(),
            })
            .unwrap_err(),
        "Project command is busy"
    );
    let _ = service.take_update();
    service.hold_requests_for_check(false);

    let done = wait(&service, |update| {
        update
            .committed
            .as_ref()
            .is_some_and(|commit| &commit.revision != before.document.revision_id())
            && update
                .shot_save
                .as_ref()
                .is_some_and(|save| save.attempt == 7)
    });
    assert!(done.error.is_none(), "{:?}", done.error);
    assert_eq!(done.shot_save.as_ref().unwrap().error, None);
    let workspace = done.workspace.unwrap();
    assert_ne!(
        workspace.document.revision_id(),
        before.document.revision_id()
    );
    assert_eq!(
        &done.committed.unwrap().revision,
        workspace.document.revision_id()
    );
    // The edit ran after the save, so its workspace carries the analysis.
    assert_eq!(
        workspace.shot_analysis.as_ref().unwrap().analysis,
        analysis(120)
    );
    assert!(!service.is_busy() && !service.annotation_busy());
}
