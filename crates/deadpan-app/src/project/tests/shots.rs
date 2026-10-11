//! Shot analysis is an annotation saved outside history and carried with the
//! workspace across edits and reopening.

use deadpan_analysis::{SIGNATURE_VERSION, ShotAnalysis};

use super::*;

/// `pictures` quiet pictures with a cut at picture 60.
fn analysis(pictures: usize) -> ShotAnalysis {
    ShotAnalysis::from_changes(
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
    let analysis = Arc::new(analysis);
    // A checkpoint may still occupy the one-request annotation lane.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while service
        .submit(ProjectRequest::SaveShotAnalysis {
            expected_session: session,
            attempt,
            key: key.clone(),
            analysis: Arc::clone(&analysis),
        })
        .is_err()
    {
        assert!(
            std::time::Instant::now() < deadline,
            "annotation lane stayed busy"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
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
            ownership: OriginalOwnership::Managed,
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
        interpretation: None,
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

    // Scan checkpoints are saved best effort for the session's Original
    // only, outside history, and the finished analysis removes them.
    let progress = |session, key: &deadpan_store::ShotAnalysisKey, start, next| {
        let progress = deadpan_analysis::ShotProgressTail::new(
            120,
            start,
            analysis(120).measures()[start..next].to_vec(),
        )
        .unwrap();
        // The annotation lane admits one request at a time.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while service
            .submit(ProjectRequest::SaveShotProgress {
                expected_session: session,
                key: key.clone(),
                progress: Arc::new(progress.clone()),
            })
            .is_err()
        {
            assert!(
                std::time::Instant::now() < deadline,
                "annotation lane stayed busy"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    };
    progress(before.session + 1, &key, 0, 10);
    progress(before.session, &foreign, 0, 20);
    progress(before.session, &key, 0, 30);
    // Later checkpoints append only their changed measures.
    progress(before.session, &key, 25, 40);
    // A tail that does not join the saved progress is dropped.
    progress(before.session, &key, 50, 60);
    // A later annotation request completes after the checkpoints.
    let refused = save(&service, before.session, 5, &key, analysis(119));
    assert!(refused.shot_save.unwrap().error.is_some());
    let stored_progress = || {
        deadpan_store::ProjectStore::open(&before.path, deadpan_store::AccessMode::ReadOnly)
            .unwrap()
            .shot_scan_progress(&key, 120)
            .unwrap()
    };
    assert_eq!(
        stored_progress().map(|progress| progress.measures().to_vec()),
        Some(analysis(120).measures()[..40].to_vec())
    );

    let saved = save(&service, before.session, 4, &key, analysis(120));
    assert_eq!(saved.shot_save.as_ref().unwrap().error, None);
    assert!(stored_progress().is_none());
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

    // A picture correction invalidates the visible analysis and any late
    // background save from the previous interpretation, without moving time.
    let choice = deadpan_core::SourceInterpretation {
        range: Some(deadpan_core::SourceColorRange::Full),
        chroma_location: Some(deadpan_core::SourceChromaLocation::Center),
    };
    let change = |session, revision| ProjectRequest::InterpretSource {
        expected_session: session,
        expected_revision: revision,
        asset: asset.clone(),
        interpretation: Some(choice),
    };
    let stale = command(
        &service,
        change(
            repeated.session + 1,
            repeated.document.revision_id().clone(),
        ),
    );
    assert!(stale.error.is_some());
    let stale = command(
        &service,
        change(repeated.session, before.document.revision_id().clone()),
    );
    assert!(stale.error.is_some());
    let changed = command(
        &service,
        change(repeated.session, repeated.document.revision_id().clone()),
    );
    assert!(changed.error.is_none(), "{:?}", changed.error);
    assert!(
        changed.committed.is_none(),
        "source correction cannot retarget a selection"
    );
    let saved_interpretation = changed.source_interpretation_saved.unwrap();
    assert_eq!(&saved_interpretation.base, repeated.document.revision_id());
    let changed = changed.workspace.unwrap();
    assert!(changed.shot_analysis.is_none());
    assert_eq!(changed.document.nodes(), repeated.document.nodes());
    assert_eq!(changed.document.assets(), repeated.document.assets());
    assert!(Arc::ptr_eq(
        &changed.sources[asset],
        &repeated.sources[asset]
    ));
    assert!(
        save(&service, changed.session, 10, &key, analysis(120))
            .shot_save
            .unwrap()
            .error
            .is_some()
    );
    let corrected_key = deadpan_store::ShotAnalysisKey {
        interpretation: Some(choice),
        ..key.clone()
    };
    assert!(
        save(&service, changed.session, 11, &corrected_key, analysis(120))
            .shot_save
            .unwrap()
            .error
            .is_none()
    );
    let restored = command(
        &service,
        ProjectRequest::Undo {
            expected_revision: changed.document.revision_id().clone(),
        },
    );
    assert!(restored.error.is_none(), "{:?}", restored.error);
    let restored = restored.workspace.unwrap();
    assert_eq!(restored.shot_analysis.as_ref().unwrap().key, key);
    assert!(restored.document.source_interpretations().is_empty());
    let redone = command(
        &service,
        ProjectRequest::Redo {
            expected_revision: restored.document.revision_id().clone(),
        },
    );
    assert!(redone.error.is_none(), "{:?}", redone.error);
    assert_eq!(
        redone
            .workspace
            .as_ref()
            .unwrap()
            .shot_analysis
            .as_ref()
            .unwrap()
            .key,
        corrected_key
    );
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
            ownership: OriginalOwnership::Managed,
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
        interpretation: None,
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
