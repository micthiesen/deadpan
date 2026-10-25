//! Explicit headless acceptance of reviewed positions against a frozen analysis.
//! This semantic request supplies positions, never a trusted model artifact.

use deadpan_analysis::{CorrectedTranscript, CorrectedWord, Corrections};
use deadpan_core::RevisionId;
use deadpan_store::{CorrectionChange, CorrectionsKey, ProjectStore, TranscriptSelectionGuard};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::live_project::LiveError;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptanceRequest {
    pub protocol: u32,
    pub expected_revision: RevisionId,
    pub guard: TranscriptSelectionGuard,
    pub word: usize,
    pub start_cs: u32,
    pub end_cs: u32,
}

fn refused(message: &str) -> LiveError {
    LiveError::new("RefinementRefused", message)
}

/// Run on the sole writer, after a client explicitly chooses the offer. The
/// native sheet passes its retained immutable review through its own service;
/// this headless route additionally rejects all implicit neighboring changes.
pub fn execute_acceptance_on(
    store: &ProjectStore,
    request: &AcceptanceRequest,
) -> Result<Value, LiveError> {
    if request.protocol != 1 {
        return Err(refused("Refinement protocol must be 1"));
    }
    if store.head_revision().map_err(LiveError::store)? != request.expected_revision {
        return Err(refused("The edit changed since the proposal was reviewed"));
    }
    let snapshot = store
        .transcript_selection(&request.guard.key)
        .map_err(LiveError::store)?;
    if snapshot.guard != request.guard {
        return Err(refused("The Original or analysis changed since refinement"));
    }
    let version = snapshot
        .corrections
        .as_ref()
        .map_or(0, |stored| stored.version);
    let corrections = snapshot
        .corrections
        .as_ref()
        .map(|stored| stored.corrections.clone())
        .unwrap_or_else(|| Corrections::empty(Corrections::clock_of(&snapshot.proposal)));
    let current = match snapshot.corrections {
        Some(_) => corrections
            .apply_to_transcript(&snapshot.proposal)
            .map_err(|error| refused(&error.to_string()))?,
        None => CorrectedTranscript::recognized(snapshot.proposal.clone()),
    };
    let words = current.transcript.words();
    let word = words
        .get(request.word)
        .ok_or_else(|| refused("The selected word is absent"))?;
    if current.skipped != 0 || current.corrected(request.word) {
        return Err(refused(
            "Manually pinned timing cannot be replaced by refinement",
        ));
    }
    if request.start_cs >= request.end_cs
        || request.end_cs > current.transcript.audio().duration_cs
        || request
            .word
            .checked_sub(1)
            .is_some_and(|i| request.start_cs < words[i].end_cs)
        || words
            .get(request.word + 1)
            .is_some_and(|next| request.end_cs > next.start_cs)
    {
        return Err(refused(
            "Refinement crosses a neighboring word or the audio extent",
        ));
    }
    let changed = corrections
        .replace_words(
            &current,
            request.word..request.word + 1,
            vec![CorrectedWord {
                text: word.text.clone(),
                start_cs: request.start_cs,
                end_cs: request.end_cs,
            }],
        )
        .map_err(|error| refused(&error.to_string()))?;
    let after = changed
        .apply_to_transcript(&snapshot.proposal)
        .map_err(|error| refused(&error.to_string()))?;
    if after.transcript.words().len() != words.len()
        || after.skipped != current.skipped
        || changed.pause_regions() != corrections.pause_regions()
        || corrections
            .word_regions()
            .iter()
            .any(|region| !changed.word_regions().contains(region))
        || words
            .iter()
            .zip(after.transcript.words())
            .enumerate()
            .any(|(i, (old, new))| {
                old.text != new.text
                    || (i != request.word
                        && (old != new || current.corrected(i) != after.corrected(i)))
            })
    {
        return Err(refused(
            "Refinement would change an unselected or pinned word",
        ));
    }
    let key = CorrectionsKey {
        content: request.guard.key.content.clone(),
        audio_stream: request.guard.key.audio_stream,
    };
    let stored = store
        .change_analysis_corrections_guarded(
            &key,
            version,
            CorrectionChange::Apply {
                corrections: changed,
                label: "Accept approximate word timing".into(),
            },
            || {
                if store.head_revision()? != request.expected_revision
                    || store.transcript(&request.guard.key)?.as_ref() != Some(&snapshot.proposal)
                {
                    return Err(deadpan_store::StoreError::AnalysisInterrupted(
                        "The captured edit or transcript changed".into(),
                    ));
                }
                Ok(())
            },
        )
        .map_err(LiveError::store)?;
    Ok(
        json!({"protocol":1,"committed":true,"revision_id":request.expected_revision,
        "word":request.word,"start_cs":request.start_cs,"end_cs":request.end_cs,
        "before_version":version,"version":stored.version,"undo":stored.undo,"redo":stored.redo}),
    )
}
