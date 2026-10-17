//! Native sleep invalidates late recognition saves without replacing prior analysis.

use deadpan_analysis::{
    ActivityAudio, AnalysedAudio, CorrectedTranscript, Corrections, SpeechActivity, Transcript,
    Word,
};
use deadpan_store::{CorrectionChange, SpeechActivityKey, TranscriptKey};

use super::*;

fn transcript(first: &str) -> Transcript {
    Transcript::new(
        AnalysedAudio {
            origin: 0,
            sample_rate: 48_000,
            duration_cs: 300,
        },
        vec![
            Word {
                text: first.into(),
                start_cs: 10,
                end_cs: 40,
                probability: 0.9,
                segment: 0,
            },
            Word {
                text: "day".into(),
                start_cs: 45,
                end_cs: 90,
                probability: 0.9,
                segment: 0,
            },
        ],
    )
    .unwrap()
}

fn activity(origin: i64) -> SpeechActivity {
    SpeechActivity::new(
        ActivityAudio {
            origin,
            sample_rate: 48_000,
            samples: 16_000,
        },
        vec![230; 32],
        vec![180; 100],
    )
    .unwrap()
}

fn save_transcript(
    service: &ProjectService,
    workspace: &Workspace,
    attempt: u64,
    epoch: u64,
    key: &TranscriptKey,
    value: Transcript,
) -> ProjectUpdate {
    let _ = service.take_update();
    service
        .submit(ProjectRequest::SaveTranscript {
            expected_session: workspace.session,
            attempt,
            lifecycle_epoch: Some(epoch),
            key: key.clone(),
            transcript: Arc::new(value),
        })
        .unwrap();
    wait(service, |update| {
        update
            .transcript_save
            .as_ref()
            .is_some_and(|save| save.attempt == attempt)
    })
}

fn save_activity(
    service: &ProjectService,
    workspace: &Workspace,
    attempt: u64,
    epoch: u64,
    key: &SpeechActivityKey,
    value: SpeechActivity,
) -> ProjectUpdate {
    let _ = service.take_update();
    service
        .submit(ProjectRequest::SaveSpeechActivity {
            expected_session: workspace.session,
            attempt,
            lifecycle_epoch: Some(epoch),
            key: key.clone(),
            activity: Arc::new(value),
        })
        .unwrap();
    wait(service, |update| {
        update
            .activity_save
            .as_ref()
            .is_some_and(|save| save.attempt == attempt)
    })
}

#[test]
fn sleep_wake_rejects_late_annotations_allows_fresh_retry_and_preserves_corrections() {
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
    let Some(SingleSourceState::Ready { asset, .. }) = &before.single_source else {
        panic!("Original not initialized")
    };
    let receipt = &before.sources[asset].receipt;
    let key = TranscriptKey {
        content: receipt.original().content().to_string(),
        audio_stream: receipt.snapshot().audio().unwrap().stream().stream_index,
        model_sha256: "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002".into(),
        language: "en".into(),
        engine: "whisper.cpp 1.8.3".into(),
    };
    let activity_key = SpeechActivityKey {
        content: key.content.clone(),
        audio_stream: key.audio_stream,
        model_sha256: "2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987".into(),
        engine: key.engine.clone(),
    };
    let lifecycle = service.jobs().lifecycle();
    let old_epoch = lifecycle.snapshot();
    lifecycle.will_sleep();
    lifecycle.did_wake();
    assert!(lifecycle.is_awake());
    let stale = save_transcript(&service, &before, 1, old_epoch, &key, transcript("to"));
    assert!(stale.transcript_save.as_ref().unwrap().interrupted);
    assert!(
        stale
            .transcript_save
            .unwrap()
            .error
            .unwrap()
            .contains("System sleep")
    );
    let stale = save_activity(&service, &before, 2, old_epoch, &activity_key, activity(0));
    assert!(stale.activity_save.as_ref().unwrap().interrupted);
    assert!(
        stale
            .activity_save
            .unwrap()
            .error
            .unwrap()
            .contains("System sleep")
    );
    let unchanged = stale.workspace.unwrap();
    assert!(unchanged.transcript.is_none());
    assert!(unchanged.speech_activity.is_none());
    assert_eq!(unchanged.document, before.document);
    assert!(!unchanged.can_undo);
    {
        let reader = ProjectStore::open(&before.path, AccessMode::ReadOnly).unwrap();
        assert!(reader.transcript(&key).unwrap().is_none());
        assert!(reader.speech_activity(&activity_key).unwrap().is_none());
        assert_eq!(
            reader.head_revision().unwrap(),
            *before.document.revision_id()
        );
    }

    let fresh_epoch = lifecycle.snapshot();
    let saved = save_transcript(&service, &before, 3, fresh_epoch, &key, transcript("to"));
    assert!(!saved.transcript_save.as_ref().unwrap().interrupted);
    assert!(saved.transcript_save.unwrap().error.is_none());
    let saved = save_activity(
        &service,
        &saved.workspace.unwrap(),
        4,
        fresh_epoch,
        &activity_key,
        activity(0),
    );
    assert!(!saved.activity_save.as_ref().unwrap().interrupted);
    assert!(saved.activity_save.unwrap().error.is_none());
    let saved = saved.workspace.unwrap();
    assert_eq!(saved.document, before.document);
    assert!(!saved.can_undo);
    assert_eq!(
        *saved.transcript.as_ref().unwrap().proposal,
        transcript("to")
    );
    assert_eq!(
        saved.speech_activity.as_ref().unwrap().activity,
        activity(0)
    );

    let joined = Corrections::empty(Corrections::clock_of(&transcript("to")))
        .merge_words(&CorrectedTranscript::recognized(transcript("to")), 0)
        .unwrap();
    let _ = service.take_update();
    service
        .submit(ProjectRequest::ChangeCorrections(
            crate::project::CorrectionRequest {
                expected_session: saved.session,
                attempt: 5,
                key: saved.corrections.as_ref().unwrap().key.clone(),
                expected_version: 0,
                change: CorrectionChange::Apply {
                    corrections: joined,
                    label: "join".into(),
                },
                transcript: saved.transcript.clone(),
                activity: saved.speech_activity.clone(),
            },
        ))
        .unwrap();
    let corrected = wait(&service, |update| {
        update
            .correction_save
            .as_ref()
            .is_some_and(|save| save.attempt == 5)
    });
    assert!(corrected.correction_save.unwrap().error.is_none());
    let corrected = corrected.workspace.unwrap();
    assert_eq!(
        corrected.transcript.as_ref().unwrap().transcript.words()[0].text,
        "today"
    );

    lifecycle.will_sleep();
    lifecycle.did_wake();
    let stale = save_transcript(
        &service,
        &corrected,
        6,
        fresh_epoch,
        &key,
        transcript("other"),
    );
    assert!(stale.transcript_save.as_ref().unwrap().interrupted);
    assert!(
        stale
            .transcript_save
            .unwrap()
            .error
            .unwrap()
            .contains("System sleep")
    );
    let stale = save_activity(
        &service,
        &corrected,
        7,
        fresh_epoch,
        &activity_key,
        activity(1_024),
    );
    assert!(stale.activity_save.as_ref().unwrap().interrupted);
    assert!(
        stale
            .activity_save
            .unwrap()
            .error
            .unwrap()
            .contains("System sleep")
    );
    let preserved = stale.workspace.unwrap();
    assert_eq!(preserved.document, before.document);
    assert!(!preserved.can_undo);
    assert!(Arc::ptr_eq(
        preserved.corrections.as_ref().unwrap(),
        corrected.corrections.as_ref().unwrap()
    ));
    assert_eq!(
        preserved.transcript.as_ref().unwrap().transcript.words()[0].text,
        "today"
    );
    assert_eq!(
        *preserved.transcript.as_ref().unwrap().proposal,
        transcript("to")
    );
    assert_eq!(
        preserved.speech_activity.as_ref().unwrap().activity,
        activity(0)
    );
    let reopened = command(&service, ProjectRequest::Open(before.path.clone()))
        .workspace
        .unwrap();
    assert_eq!(reopened.document, before.document);
    assert!(!reopened.can_undo);
    assert_eq!(
        reopened.transcript.as_ref().unwrap().transcript.words()[0].text,
        "today"
    );
    assert_eq!(
        *reopened.transcript.as_ref().unwrap().proposal,
        transcript("to")
    );
    assert_eq!(
        reopened.speech_activity.as_ref().unwrap().activity,
        activity(0)
    );
    assert_eq!(
        reopened.corrections.as_ref().unwrap().version(),
        corrected.corrections.as_ref().unwrap().version()
    );
}
