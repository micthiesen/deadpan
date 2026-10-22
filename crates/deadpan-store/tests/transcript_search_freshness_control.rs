//! Old-code control: analysis changes independently of authored head and bank.
//! This exercises public annotation APIs, not a future publication guard.

use deadpan_analysis::{AnalysedAudio, CorrectedTranscript, Corrections, Transcript, Word};
use deadpan_core::{
    NodeId, ProjectDocument, ProjectId, RegisterName, RegisterValue, RevisionId,
    SemanticInstruction, SemanticProgram,
};
use deadpan_store::registers::RegisterBank;
use deadpan_store::{AccessMode, CorrectionChange, CorrectionsKey, ProjectStore, TranscriptKey};
use std::{error::Error, num::NonZeroU32, sync::Arc};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn transcript(first: &str) -> Transcript {
    Transcript::new(
        AnalysedAudio {
            origin: 2_048,
            sample_rate: 48_000,
            duration_cs: 500,
        },
        [(first, 10, 20), ("world", 21, 40), ("again", 50, 90)]
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
    .unwrap()
}

fn key() -> TranscriptKey {
    TranscriptKey {
        content: "blake3:search-control-original".into(),
        audio_stream: 1,
        model_sha256: "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002".into(),
        language: "en".into(),
        engine: "whisper.cpp 1.8.3".into(),
    }
}

fn correction_key(key: &TranscriptKey) -> CorrectionsKey {
    CorrectionsKey {
        content: key.content.clone(),
        audio_stream: key.audio_stream,
    }
}

fn effective(
    store: &ProjectStore,
    key: &TranscriptKey,
) -> Result<(CorrectedTranscript, Option<u64>)> {
    let proposal = store.transcript(key)?.unwrap();
    let (current, version) = match store.analysis_corrections(&correction_key(key))? {
        Some(stored) => (
            stored.corrections.apply_to_transcript(&proposal)?,
            Some(stored.version),
        ),
        None => (CorrectedTranscript::recognized(proposal), None),
    };
    assert_eq!(current.skipped, 0);
    Ok((current, version))
}

fn words(transcript: &Transcript) -> Vec<(&str, u32, u32)> {
    transcript
        .words()
        .iter()
        .map(|word| (word.text.as_str(), word.start_cs, word.end_cs))
        .collect()
}

fn unchanged(store: &ProjectStore, document: &ProjectDocument, bank: &RegisterBank) -> Result {
    let (current, current_bank) = store.snapshot_with_registers()?;
    assert_eq!(current, *document);
    assert_eq!(current_bank, *bank);
    assert_eq!(store.head_revision()?, *document.revision_id());
    Ok(())
}

#[test]
fn same_key_proposals_and_correction_apply_undo_redo_leave_head_and_bank_unchanged() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("transcript-search-freshness.deadpan");
    let document = ProjectDocument::new_automatic(
        ProjectId::new("transcript-search-freshness")?,
        RevisionId::new("base")?,
        NodeId::new("root")?,
    )?;
    let mut store = ProjectStore::create(&path, &document)?;
    let bank = store.save_register(
        document.project_id(),
        document.revision_id(),
        RegisterName::new('m')?,
        RegisterValue::Macro {
            program: Arc::new(SemanticProgram::new(vec![
                SemanticInstruction::MoveFrames {
                    forward: true,
                    count: NonZeroU32::new(1).unwrap(),
                },
            ])?),
        },
    )?;
    assert_eq!(bank.entries.len(), 1);
    assert_eq!(bank.version, 1);
    let key = key();
    let correction_key = correction_key(&key);
    assert!(store.transcript(&key)?.is_none());
    assert_eq!(store.analysis_corrections(&correction_key)?, None);
    unchanged(&store, &document, &bank)?;

    let first = transcript("hello");
    store.save_transcript(&key, &first)?;
    let (recognized, absent) = effective(&store, &key)?;
    assert_eq!(recognized.transcript, first);
    assert_eq!(absent, None);
    assert_eq!(recognized.transcript.search("hello world"), vec![0..2]);
    unchanged(&store, &document, &bank)?;

    // The exact same key now returns different proposal bytes and results.
    let replacement = transcript("hullo");
    assert_ne!(replacement, first);
    store.save_transcript(&key, &replacement)?;
    assert_eq!(
        store.transcript_keys_for_content(&key.content)?,
        std::slice::from_ref(&key)
    );
    assert_eq!(store.transcript(&key)?, Some(replacement.clone()));
    let (replaced, absent) = effective(&store, &key)?;
    assert_eq!(replaced.transcript, replacement);
    assert_eq!(absent, None);
    assert!(replaced.transcript.search("hello world").is_empty());
    unchanged(&store, &document, &bank)?;

    let corrections = Corrections::empty(Corrections::clock_of(&replacement)).edit_word_text(
        &replaced,
        0,
        "hello",
        &[],
    )?;
    let applied = store.change_analysis_corrections(
        &correction_key,
        0,
        CorrectionChange::Apply {
            corrections: corrections.clone(),
            label: "Correct hullo to hello".into(),
        },
    )?;
    assert_eq!(applied.version, 1);
    assert_eq!(applied.corrections, corrections);
    assert_eq!(store.analysis_corrections(&correction_key)?, Some(applied));
    let (corrected, version) = effective(&store, &key)?;
    assert_eq!(version, Some(1));
    assert_eq!(
        words(&corrected.transcript),
        [("hello", 10, 20), ("world", 21, 40), ("again", 50, 90),]
    );
    assert!(corrected.corrected(0));
    assert_eq!(corrected.transcript.search("hello world"), vec![0..2]);
    assert_eq!(store.transcript(&key)?, Some(replacement.clone()));
    unchanged(&store, &document, &bank)?;

    let undone = store.change_analysis_corrections(&correction_key, 1, CorrectionChange::Undo)?;
    assert_eq!(undone.version, 2);
    assert!(undone.corrections.is_empty());
    // Undo keeps a present empty row; it does not restore initial absence.
    assert_eq!(store.analysis_corrections(&correction_key)?, Some(undone));
    let (uncorrected, version) = effective(&store, &key)?;
    assert_eq!(version, Some(2));
    assert_eq!(uncorrected.transcript, replacement);
    assert!(!uncorrected.corrected(0));
    assert!(uncorrected.transcript.search("hello world").is_empty());
    unchanged(&store, &document, &bank)?;

    let redone = store.change_analysis_corrections(&correction_key, 2, CorrectionChange::Redo)?;
    assert_eq!(redone.version, 3);
    assert_eq!(redone.corrections, corrections);
    assert_eq!(store.analysis_corrections(&correction_key)?, Some(redone));
    let (restored, version) = effective(&store, &key)?;
    assert_eq!(version, Some(3));
    assert_eq!(restored, corrected);
    unchanged(&store, &document, &bank)?;
    assert!(
        store
            .preview_undo(document.revision_id(), RevisionId::new("undo")?)
            .is_err()
    );
    store.validate_full()?;
    drop(store);

    let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    unchanged(&reader, &document, &bank)?;
    assert_eq!(reader.transcript(&key)?, Some(replacement));
    let (reopened, version) = effective(&reader, &key)?;
    assert_eq!(version, Some(3));
    assert_eq!(reopened, corrected);
    reader.validate_full()?;
    Ok(())
}
