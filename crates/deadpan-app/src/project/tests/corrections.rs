//! Transcript corrections are saved outside history with their own Undo,
//! published as the workspace's corrected words and kept across reopening.

use deadpan_analysis::{AnalysedAudio, CorrectedTranscript, Corrections, Transcript, Word};
use deadpan_store::CorrectionChange;

use super::*;

fn transcript() -> Transcript {
    let word = |text: &str, start_cs, end_cs| Word {
        text: text.into(),
        start_cs,
        end_cs,
        probability: 0.9,
        segment: 0,
    };
    Transcript::new(
        AnalysedAudio {
            origin: 0,
            sample_rate: 48_000,
            duration_cs: 300,
        },
        vec![
            word("to", 10, 40),
            word("day", 45, 90),
            word("now", 120, 160),
        ],
    )
    .unwrap()
}

fn change(
    service: &ProjectService,
    workspace: &Workspace,
    attempt: u64,
    expected_version: u64,
    change: CorrectionChange,
) -> ProjectUpdate {
    let _ = service.take_update();
    service
        .submit(ProjectRequest::ChangeCorrections(
            crate::project::CorrectionRequest {
                expected_session: workspace.session,
                attempt,
                key: workspace.corrections.as_ref().unwrap().key.clone(),
                expected_version,
                change,
                transcript: workspace.transcript.clone(),
                activity: workspace.speech_activity.clone(),
            },
        ))
        .unwrap();
    wait(service, |update| {
        update
            .correction_save
            .as_ref()
            .is_some_and(|save| save.attempt == attempt)
    })
}

fn texts(workspace: &Workspace) -> Vec<String> {
    workspace
        .transcript
        .as_ref()
        .unwrap()
        .transcript
        .words()
        .iter()
        .map(|word| word.text.clone())
        .collect()
}

#[test]
fn corrections_are_published_undone_without_edit_history_and_reloaded() {
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
    let Some(SingleSourceState::Ready { asset, .. }) = &before.single_source else {
        panic!("original not initialized")
    };
    let receipt = &before.sources[asset].receipt;
    let key = deadpan_store::TranscriptKey {
        content: receipt.original().content().to_string(),
        audio_stream: receipt.snapshot().audio().unwrap().stream().stream_index,
        model_sha256: "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002".into(),
        language: "en".into(),
        engine: "whisper.cpp 1.8.3".into(),
    };
    let _ = service.take_update();
    service
        .submit(ProjectRequest::SaveTranscript {
            expected_session: before.session,
            attempt: 1,
            key,
            transcript: Arc::new(transcript()),
        })
        .unwrap();
    let saved = wait(&service, |update| {
        update
            .transcript_save
            .as_ref()
            .is_some_and(|save| save.attempt == 1)
    });
    let workspace = saved.workspace.unwrap();
    assert_eq!(workspace.corrections.as_ref().unwrap().version(), 0);

    let joined = Corrections::empty(Corrections::clock_of(&transcript()))
        .merge_words(&CorrectedTranscript::recognized(transcript()), 0)
        .unwrap();
    // A stale version is refused and changes nothing.
    let stale = change(
        &service,
        &workspace,
        1,
        5,
        CorrectionChange::Apply {
            corrections: joined.clone(),
            label: "join".into(),
        },
    );
    assert!(stale.correction_save.unwrap().error.is_some());

    let applied = change(
        &service,
        &workspace,
        2,
        0,
        CorrectionChange::Apply {
            corrections: joined,
            label: "join".into(),
        },
    );
    assert_eq!(applied.correction_save.as_ref().unwrap().error, None);
    let corrected = applied.workspace.unwrap();
    assert_eq!(texts(&corrected), ["today", "now"]);
    assert!(corrected.transcript.as_ref().unwrap().corrected(0));
    assert_eq!(
        *corrected.transcript.as_ref().unwrap().proposal,
        transcript()
    );
    assert_eq!(
        corrected.document.revision_id(),
        before.document.revision_id()
    );
    assert!(!corrected.can_undo, "corrections are not edits");

    let undone = change(&service, &corrected, 3, 1, CorrectionChange::Undo)
        .workspace
        .unwrap();
    assert_eq!(texts(&undone), ["to", "day", "now"]);
    let redone = change(&service, &undone, 4, 2, CorrectionChange::Redo)
        .workspace
        .unwrap();
    assert_eq!(texts(&redone), ["today", "now"]);

    let reopened = command(&service, ProjectRequest::Open(redone.path.clone()));
    assert!(reopened.error.is_none(), "{:?}", reopened.error);
    let reopened = reopened.workspace.unwrap();
    assert_eq!(texts(&reopened), ["today", "now"]);
    let stored = reopened.corrections.as_ref().unwrap();
    assert_eq!(stored.version(), 3);
    assert_eq!(
        stored.stored.as_ref().unwrap().undo.as_deref(),
        Some("join")
    );

    // A change computed against a replaced transcript is refused.
    let mut stale_basis = reopened.annotated();
    stale_basis.transcript = Some(Arc::new(crate::project::OriginalTranscript::new(
        deadpan_cli::speech::StoredWords {
            key: reopened.transcript.as_ref().unwrap().key.clone(),
            proposal: transcript(),
            corrected: CorrectedTranscript::recognized(transcript()),
            corrections_error: None,
        },
    )));
    let refused = change(&service, &stale_basis, 5, 3, CorrectionChange::Undo);
    assert!(
        refused
            .correction_save
            .unwrap()
            .error
            .is_some_and(|error| error.contains("changed while correcting"))
    );
}

#[test]
fn unreadable_corrections_refuse_word_operators_until_discarded() {
    let words = deadpan_cli::speech::StoredWords {
        key: deadpan_store::TranscriptKey {
            content: "blake3:x".into(),
            audio_stream: 1,
            model_sha256: "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002".into(),
            language: "en".into(),
            engine: "test".into(),
        },
        proposal: transcript(),
        corrected: CorrectedTranscript {
            skipped: 2,
            ..CorrectedTranscript::recognized(transcript())
        },
        corrections_error: None,
    };
    let skipped = crate::project::OriginalTranscript::new(words.clone());
    assert!(
        skipped
            .problem()
            .is_some_and(|problem| problem.contains("2 stored corrections"))
    );
    assert_eq!(skipped.current().skipped, 2);
    let unreadable = crate::project::OriginalTranscript::new(deadpan_cli::speech::StoredWords {
        corrections_error: Some("bad rule".into()),
        ..words
    });
    assert!(
        unreadable
            .problem()
            .is_some_and(|problem| problem.contains("unreadable"))
    );
}

/// A session that only saves analyses (no authored edit) is still backed
/// up: backups follow every commit, operational ones included.
#[test]
fn a_transcript_only_session_is_backed_up() {
    let scratch = tempfile::tempdir().unwrap();
    let service = ProjectService::start(
        Arc::new(|| {}),
        Some(ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap()),
    )
    .unwrap();
    service.set_backup_interval_for_check(std::time::Duration::from_millis(50));
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
    let path = before.path.clone();
    // Let the first automatic backup settle on the initialized project.
    let first = wait(&service, |update| {
        update.backups.latest.is_some() && update.backups.running.is_none()
    });
    let first_id = first.backups.latest.unwrap().0.id;
    let count = |path: &std::path::Path| {
        deadpan_store::backups::list_backups(path)
            .unwrap()
            .into_iter()
            .filter(|backup| backup.id != first_id)
            .count()
    };
    // Nothing new while nothing changes.
    std::thread::sleep(std::time::Duration::from_millis(250));
    let quiet = count(&path);
    let Some(SingleSourceState::Ready { asset, .. }) = &before.single_source else {
        panic!("original not initialized")
    };
    let receipt = &before.sources[asset].receipt;
    let key = deadpan_store::TranscriptKey {
        content: receipt.original().content().to_string(),
        audio_stream: receipt.snapshot().audio().unwrap().stream().stream_index,
        model_sha256: "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002".into(),
        language: "en".into(),
        engine: "whisper.cpp 1.8.3".into(),
    };
    service
        .submit(ProjectRequest::SaveTranscript {
            expected_session: before.session,
            attempt: 1,
            key,
            transcript: Arc::new(transcript()),
        })
        .unwrap();
    let saved = wait(&service, |update| {
        update
            .transcript_save
            .as_ref()
            .is_some_and(|save| save.attempt == 1)
    });
    assert!(saved.transcript_save.unwrap().error.is_none());
    let revision = saved.workspace.unwrap().document.revision_id().clone();
    assert_eq!(&revision, before.document.revision_id(), "no authored edit");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while count(&path) <= quiet {
        assert!(
            std::time::Instant::now() < deadline,
            "the transcript was never backed up"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}
