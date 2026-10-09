//! Speech activity is an annotation saved outside history and carried with
//! the workspace across edits and reopening.

use deadpan_analysis::{ActivityAudio, SpeechActivity};

use super::*;

fn activity() -> SpeechActivity {
    SpeechActivity::new(
        ActivityAudio {
            origin: 0,
            sample_rate: 48_000,
            samples: 16_000,
        },
        (0..32).map(|hop| if hop < 16 { 230 } else { 5 }).collect(),
        (0..100)
            .map(|frame| if frame < 50 { 180 } else { 20 })
            .collect(),
    )
    .unwrap()
}

fn save(
    service: &ProjectService,
    session: u64,
    attempt: u64,
    key: &deadpan_store::SpeechActivityKey,
) -> ProjectUpdate {
    let _ = service.take_update();
    service
        .submit(ProjectRequest::SaveSpeechActivity {
            expected_session: session,
            attempt,
            key: key.clone(),
            activity: Arc::new(activity()),
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
fn saved_speech_activity_is_published_carried_across_edits_and_reloaded() {
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
    assert!(before.speech_activity.is_none());
    let Some(SingleSourceState::Ready { asset, node, .. }) = &before.single_source else {
        panic!("original not initialized")
    };
    let content = before.sources[asset]
        .receipt
        .original()
        .content()
        .to_string();
    let key = deadpan_store::SpeechActivityKey {
        content,
        audio_stream: 1,
        model_sha256: "2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987".into(),
        engine: "whisper.cpp 1.8.3".into(),
    };

    let stale = save(&service, before.session + 1, 1, &key);
    assert!(stale.activity_save.unwrap().error.is_some());

    let saved = save(&service, before.session, 2, &key);
    assert_eq!(saved.activity_save.as_ref().unwrap().error, None);
    let workspace = saved.workspace.unwrap();
    let stored = workspace.speech_activity.as_ref().unwrap();
    assert_eq!((&stored.key, &stored.activity), (&key, &activity()));
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
        repeated.speech_activity.as_ref().unwrap(),
        workspace.speech_activity.as_ref().unwrap()
    ));

    let path = repeated.path.clone();
    let reopened = command(&service, ProjectRequest::Open(path));
    assert!(reopened.error.is_none(), "{:?}", reopened.error);
    let reopened = reopened.workspace.unwrap();
    assert_eq!(
        reopened.speech_activity.as_ref().unwrap().activity,
        activity()
    );
}
