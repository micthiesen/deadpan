#![cfg(any(target_os = "macos", target_os = "linux"))]

use deadpan_analysis::{
    AnalysedAudio, CorrectedTranscript, Corrections, Transcript, Word, WordCorrection,
};
use deadpan_cli::refinement::{AcceptanceRequest, execute_acceptance_on};
use deadpan_store::{AccessMode, CorrectionChange, CorrectionsKey, ProjectStore, TranscriptKey};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

#[test]
fn headless_refinement_refuses_stale_pinned_and_crossing_requests_then_commits_one_undo()
-> Result<(), Box<dyn std::error::Error>> {
    let scratch = tempfile::tempdir()?;
    let source = scratch.path().join("original.mp4");
    std::fs::write(
        &source,
        include_bytes!("../../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4"),
    )?;
    let package = scratch.path().join("headless.deadpan");
    let cancelled = AtomicBool::new(false);
    let created =
        deadpan_cli::single_original::create(&package, &source, "Original", &cancelled, |_, _| {
            Ok(())
        })?;
    let store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let session = deadpan_cli::transcription::open_original_analysis(
        &store,
        Some(&created.asset_id),
        &cancelled,
        Instant::now() + Duration::from_secs(60),
    )?;
    let beginning =
        session.prepare_window(0..401, &cancelled, Instant::now() + Duration::from_secs(60))?;
    let transcript = Transcript::new(
        AnalysedAudio {
            origin: beginning.analysis_origin,
            sample_rate: beginning.source_rate,
            duration_cs: u32::try_from(session.complete_analysis_frames().div_ceil(160))?,
        },
        [("hello", 50, 90), ("world", 100, 150), ("today", 170, 200)]
            .into_iter()
            .map(|(text, start_cs, end_cs)| Word {
                text: text.into(),
                start_cs,
                end_cs,
                probability: 0.8,
                segment: 0,
            })
            .collect(),
    )?;
    let key = TranscriptKey {
        content: created.content,
        audio_stream: beginning.audio_stream,
        model_sha256: "a".repeat(64),
        language: "en".into(),
        engine: "acceptance-clock-fixture".into(),
    };
    store.save_transcript(&key, &transcript)?;
    let corrections_key = CorrectionsKey {
        content: key.content.clone(),
        audio_stream: key.audio_stream,
    };
    let base = Corrections::empty(Corrections::clock_of(&transcript));
    let pinned = base
        .set_word_bounds(
            &CorrectedTranscript::recognized(transcript.clone()),
            0,
            55,
            85,
        )?
        .0;
    let first = store.change_analysis_corrections(
        &corrections_key,
        0,
        CorrectionChange::Apply {
            corrections: pinned,
            label: "manual first word".into(),
        },
    )?;
    let head = store.head_revision()?;
    let request = AcceptanceRequest {
        protocol: 1,
        expected_revision: head.clone(),
        guard: store.transcript_selection(&key)?.guard,
        word: 1,
        start_cs: 105,
        end_cs: 145,
    };
    for variant in 0..6 {
        let mut changed = request.clone();
        match variant {
            0 => changed.protocol = 2,
            1 => changed.expected_revision = deadpan_core::RevisionId::new("stale")?,
            2 => changed.guard.proposal_sha256 = "b".repeat(64),
            3 => {
                changed.word = 0;
                changed.start_cs = 56;
                changed.end_cs = 84;
            }
            4 => changed.start_cs = 80,
            _ => changed.end_cs = 175,
        }
        assert!(execute_acceptance_on(&store, &changed).is_err());
        assert_eq!(
            store.analysis_corrections(&corrections_key)?,
            Some(first.clone())
        );
        assert_eq!(store.head_revision()?, head);
    }
    let reply = execute_acceptance_on(&store, &request)?;
    assert_eq!(reply["version"], 2);
    let after = store.analysis_corrections(&corrections_key)?.unwrap();
    let words = after.corrections.apply_to_transcript(&transcript)?;
    assert_eq!(
        (
            words.transcript.words()[1].start_cs,
            words.transcript.words()[1].end_cs
        ),
        (105, 145)
    );
    assert!(words.corrected(0) && words.corrected(1) && !words.corrected(2));
    assert_eq!(words.transcript.words()[2], transcript.words()[2]);
    assert!(
        after
            .corrections
            .word_regions()
            .contains(&first.corrections.word_regions()[0])
    );
    assert!(execute_acceptance_on(&store, &request).is_err());
    assert_eq!(
        store.analysis_corrections(&corrections_key)?,
        Some(after.clone())
    );
    let undone = store.change_analysis_corrections(
        &corrections_key,
        after.version,
        CorrectionChange::Undo,
    )?;
    assert_eq!(undone.corrections, first.corrections);
    assert_eq!(store.head_revision()?, head);

    // An in-bounds empty region means a person removed those words. Refining
    // another word must retain that intent, rather than treating it as skipped.
    let removed = WordCorrection {
        start_cs: 170,
        end_cs: 200,
        words: Vec::new(),
    };
    let mut regions = undone.corrections.word_regions().to_vec();
    regions.push(removed.clone());
    let removal = Corrections::new(Corrections::clock_of(&transcript), regions, Vec::new())?;
    let with_removal = store.change_analysis_corrections(
        &corrections_key,
        undone.version,
        CorrectionChange::Apply {
            corrections: removal,
            label: "remove today".into(),
        },
    )?;
    let before = with_removal.corrections.apply_to_transcript(&transcript)?;
    assert_eq!(before.transcript.words().len(), 2);
    assert_eq!(before.skipped, 0);
    let mut fresh = request;
    fresh.guard = store.transcript_selection(&key)?.guard;
    execute_acceptance_on(&store, &fresh)?;
    let accepted = store.analysis_corrections(&corrections_key)?.unwrap();
    assert!(accepted.corrections.word_regions().contains(&removed));
    let words = accepted.corrections.apply_to_transcript(&transcript)?;
    assert_eq!(words.transcript.words().len(), 2);
    assert_eq!(words.skipped, 0);
    assert_eq!(store.head_revision()?, head);
    store.validate_full()?;
    Ok(())
}
