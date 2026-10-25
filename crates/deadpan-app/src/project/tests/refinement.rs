//! Explicit review acceptance uses the real sole writer, correction history and sleep guard.
use super::*;
use deadpan_analysis::refinement::{
    self, PronunciationLexicon, RefinementCapture, RefinementControl,
};
use deadpan_analysis::{AnalysedAudio, Corrections, Transcript, Word};

fn review(workspace: &Workspace) -> Arc<refinement::RefinementReview> {
    let words = workspace.transcript.as_ref().unwrap();
    let corrections = workspace
        .corrections
        .as_ref()
        .unwrap()
        .stored
        .as_ref()
        .map_or_else(
            || Corrections::empty(Corrections::clock_of(&words.proposal)),
            |s| s.corrections.clone(),
        );
    let capture = RefinementCapture::new(
        &words.proposal,
        &words.current(),
        &corrections,
        1..2,
        48_000,
    )
    .unwrap();
    let phones = refinement::LABELS[1..40].join("\n");
    let lexicon = PronunciationLexicon::parse("to AA\nday AE\nnow AH\n", &phones).unwrap();
    let frames = 151;
    let mut logp = vec![0.0025f64.ln(); frames * 41];
    for frame in 0..frames {
        let label = match frame {
            5..20 => 1,
            24..44 => 2,
            60..80 => 3,
            _ => 0,
        };
        logp[frame * 41 + label] = 0.9f64.ln();
    }
    let proposal = refinement::propose(
        &logp,
        48_000,
        capture.context_text(),
        &lexicon,
        &mut RefinementControl::new(&mut || false),
    )
    .unwrap();
    let review = capture.review(proposal).unwrap();
    assert_eq!(
        review.offers()[0].availability,
        refinement::RefinementOfferAvailability::Approximate
    );
    Arc::new(review)
}

fn request(
    service: &ProjectService,
    workspace: &Workspace,
    attempt: u64,
) -> crate::project::refinement::Request {
    crate::project::refinement::Request {
        expected_session: workspace.session,
        expected_revision: workspace.document.revision_id().clone(),
        attempt,
        lifecycle_epoch: service.jobs().lifecycle().snapshot(),
        transcript: workspace.transcript.as_ref().unwrap().clone(),
        corrections: workspace.corrections.as_ref().unwrap().clone(),
        review: review(workspace),
        selected_words: vec![1],
    }
}

fn accept(service: &ProjectService, request: crate::project::refinement::Request) -> ProjectUpdate {
    let attempt = request.attempt;
    let _ = service.take_update();
    service
        .submit(ProjectRequest::AcceptRefinement(request))
        .unwrap();
    wait(service, |update| {
        update
            .correction_save
            .as_ref()
            .is_some_and(|save| save.attempt == attempt)
    })
}

#[test]
fn refinement_acceptance_refuses_stale_and_sleep_then_saves_one_undo_without_editing() {
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
    let opened = wait(&service, |update| {
        update.import.as_ref().is_some_and(|status| {
            matches!(status.stage, ImportStage::Complete | ImportStage::Failed)
        })
    });
    let before = opened.workspace.unwrap();
    let SingleSourceState::Ready { asset, .. } = before.single_source.as_ref().unwrap() else {
        panic!("Original not initialized");
    };
    let receipt = &before.sources[asset].receipt;
    let key = deadpan_store::TranscriptKey {
        content: receipt.original().content().to_string(),
        audio_stream: receipt.snapshot().audio().unwrap().stream().stream_index,
        model_sha256: "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002".into(),
        language: "en".into(),
        engine: "whisper.cpp 1.8.3".into(),
    };
    let transcript = Transcript::new(
        AnalysedAudio {
            origin: 0,
            sample_rate: 48_000,
            duration_cs: 300,
        },
        [("to", 10, 40), ("day", 45, 90), ("now", 120, 160)]
            .into_iter()
            .map(|(text, start_cs, end_cs)| Word {
                text: text.into(),
                start_cs,
                end_cs,
                probability: 0.9,
                segment: 0,
            })
            .collect(),
    )
    .unwrap();
    service
        .submit(ProjectRequest::SaveTranscript {
            expected_session: before.session,
            lifecycle_epoch: None,
            attempt: 1,
            key,
            transcript: Arc::new(transcript),
        })
        .unwrap();
    let saved = wait(&service, |update| {
        update
            .transcript_save
            .as_ref()
            .is_some_and(|save| save.attempt == 1)
    });
    assert!(saved.transcript_save.unwrap().error.is_none());
    let workspace = saved.workspace.unwrap();
    let mut stale = request(&service, &workspace, 40);
    stale.expected_revision = RevisionId::new("stale-refinement-review").unwrap();
    assert!(
        accept(&service, stale)
            .correction_save
            .unwrap()
            .error
            .is_some()
    );
    let asleep = request(&service, &workspace, 41);
    service.jobs().lifecycle().will_sleep();
    service.jobs().lifecycle().did_wake();
    let rejected = accept(&service, asleep).correction_save.unwrap();
    assert!(rejected.interrupted);
    assert!(rejected.error.is_some());
    let accepted = accept(&service, request(&service, &workspace, 42));
    assert!(accepted.correction_save.as_ref().unwrap().error.is_none());
    let after = accepted.workspace.unwrap();
    assert_eq!(
        after.document.revision_id(),
        workspace.document.revision_id()
    );
    let words = &after.transcript.as_ref().unwrap().transcript;
    assert_eq!(
        (words.words()[1].start_cs, words.words()[1].end_cs),
        (48, 88)
    );
    assert_eq!(
        words.words()[0],
        workspace.transcript.as_ref().unwrap().transcript.words()[0]
    );
    assert_eq!(
        words.words()[2],
        workspace.transcript.as_ref().unwrap().transcript.words()[2]
    );
    assert_eq!(after.corrections.as_ref().unwrap().version(), 1);
    let _ = service.take_update();
    service
        .submit(ProjectRequest::ChangeCorrections(
            crate::project::CorrectionRequest {
                expected_session: after.session,
                attempt: 43,
                key: after.corrections.as_ref().unwrap().key.clone(),
                expected_version: 1,
                change: deadpan_store::CorrectionChange::Undo,
                transcript: after.transcript.clone(),
                activity: after.speech_activity.clone(),
            },
        ))
        .unwrap();
    let undone = wait(&service, |update| {
        update
            .correction_save
            .as_ref()
            .is_some_and(|save| save.attempt == 43)
    });
    assert!(undone.correction_save.unwrap().error.is_none());
    let undone = undone.workspace.unwrap();
    assert_eq!(
        undone.transcript.as_ref().unwrap().transcript,
        workspace.transcript.as_ref().unwrap().transcript
    );
    assert_eq!(
        undone.document.revision_id(),
        workspace.document.revision_id()
    );
    service.shutdown();
}
