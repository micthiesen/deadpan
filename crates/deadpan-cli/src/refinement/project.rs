//! Shared native/CLI preparation against one exact Original analysis snapshot.

use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use deadpan_analysis::refinement::{
    RefinementCapture, RefinementControl, RefinementReview, propose,
};
use deadpan_analysis::{CorrectedTranscript, Corrections};
use deadpan_core::RevisionId;
use deadpan_jobs::refinement::RuntimeReport;
use deadpan_store::{ProjectStore, TranscriptKey, TranscriptSelectionGuard};

use super::{RefinementError, RefinementRuntime};
use crate::CliError;
use crate::models::RefinementModels;
use crate::transcription::{OriginalAnalysisWindow, open_original_analysis};

#[derive(Debug, Clone)]
pub struct PreparedRefinement {
    pub review: RefinementReview,
    pub window: OriginalAnalysisWindow,
    pub selection_guard: TranscriptSelectionGuard,
    pub expected_version: u64,
    pub expected_revision: RevisionId,
    pub runtime: RuntimeReport,
}

fn refused(message: &'static str) -> CliError {
    RefinementError::Configuration(message).into()
}

/// Prepare and infer on an analysis thread. No annotation or authored state is
/// written. A result retains exact PCM for waveform review and context audition.
#[allow(clippy::too_many_arguments)]
pub fn prepare(
    store: &ProjectStore,
    key: &TranscriptKey,
    selected: Range<usize>,
    runtime: &RefinementRuntime,
    models: &RefinementModels,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<PreparedRefinement, CliError> {
    super::check(cancelled, deadline)?;
    super::validate_language(&key.language)?;
    let expected_revision = store.head_revision()?;
    let snapshot = store.transcript_selection(key)?;
    let expected_version = snapshot
        .corrections
        .as_ref()
        .map_or(0, |stored| stored.version);
    let corrections = snapshot
        .corrections
        .as_ref()
        .map(|stored| stored.corrections.clone())
        .unwrap_or_else(|| Corrections::empty(Corrections::clock_of(&snapshot.proposal)));
    let current = match &snapshot.corrections {
        Some(_) => corrections
            .apply_to_transcript(&snapshot.proposal)
            .map_err(|error| RefinementError::Protocol(error.to_string()))?,
        None => CorrectedTranscript::recognized(snapshot.proposal.clone()),
    };
    // Reject pinned/unavailable context before decoding or loading a model.
    if selected.is_empty()
        || selected.end > current.transcript.words().len()
        || selected.clone().all(|index| current.corrected(index))
    {
        return Err(refused("select an unpinned recognized word for refinement"));
    }
    let session = open_original_analysis(store, Some(&snapshot.guard.asset), cancelled, deadline)?;
    let capture = RefinementCapture::new(
        &snapshot.proposal,
        &current,
        &corrections,
        selected,
        session.complete_analysis_frames(),
    )
    .map_err(|error| RefinementError::Protocol(error.to_string()))?;
    let interval = capture.window();
    let window = session.prepare_window(interval.start.0..interval.end.0, cancelled, deadline)?;
    if window.content != key.content
        || window.audio_stream != key.audio_stream
        || window.analysis_origin != snapshot.proposal.audio().origin
        || window.source_rate != snapshot.proposal.audio().sample_rate
        || window.analysis_offset != interval.start.0
    {
        return Err(refused(
            "the prepared Original differs from the captured transcript",
        ));
    }
    super::check(cancelled, deadline)?;
    let attempt = uuid::Uuid::new_v4().to_string();
    let emissions = super::infer(
        runtime,
        &models.checkpoint,
        &models.statistics,
        &window.samples,
        &attempt,
        cancelled,
        deadline,
    )?;
    let mut interrupted = || cancelled.load(Ordering::Acquire) || Instant::now() >= deadline;
    let proposal = propose(
        &emissions.log_probs,
        emissions.input_samples,
        capture.context_text(),
        &models.lexicon,
        &mut RefinementControl::new(&mut interrupted),
    );
    // Preserve cancellation/deadline as distinct public outcomes.
    super::check(cancelled, deadline)?;
    let review = capture
        .review(proposal.map_err(|error| RefinementError::Protocol(error.to_string()))?)
        .map_err(|error| RefinementError::Protocol(error.to_string()))?;
    if store.head_revision()? != expected_revision
        || store.transcript_selection(key)?.guard != snapshot.guard
    {
        return Err(refused(
            "the project or analysis changed during refinement; request it again",
        ));
    }
    super::check(cancelled, deadline)?;
    Ok(PreparedRefinement {
        review,
        window,
        selection_guard: snapshot.guard,
        expected_version,
        expected_revision,
        runtime: emissions.runtime,
    })
}
