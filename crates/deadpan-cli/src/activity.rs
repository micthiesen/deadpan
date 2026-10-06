//! Host side of speech activity detection.
//!
//! The same isolated worker that transcribes runs the Silero voice activity
//! detector over the prepared analysis PCM. The host admits the probabilities
//! only after clean teardown and a hashed artifact snapshot, measures the
//! energy of the same PCM and stores both as validated [`SpeechActivity`].
//! Pauses derived from it are proposals; nothing here edits a project.

use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_analysis::{ANALYSIS_SAMPLE_RATE, ActivityAudio, SILENCE_RULE, SpeechActivity};
use deadpan_core::ExactRatio;
use deadpan_jobs::transcription::{self, HostMessage, ModelInput, RuntimeReport};
use deadpan_jobs::{AttemptId, CancellationToken, RequestId, WorkspaceRef};
use deadpan_store::SpeechActivityKey;

use crate::transcription::{
    AnalysisInput, TranscriptionError, TranscriptionRuntime, analysed_receipt, supervise,
};

#[derive(Debug, Clone)]
pub struct ActivityResult {
    pub activity: SpeechActivity,
    pub runtime: RuntimeReport,
    pub elapsed: Duration,
}

fn protocol_error(error: impl std::fmt::Display) -> TranscriptionError {
    TranscriptionError::Protocol(error.to_string())
}

/// Run one supervised speech detection attempt over the analysis PCM.
pub fn detect_speech(
    runtime: &TranscriptionRuntime,
    model: &ModelInput,
    input: &AnalysisInput,
    attempt: &str,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<ActivityResult, TranscriptionError> {
    let samples = input.samples.len() as u64;
    let hops = samples.div_ceil(transcription::VAD_HOP);
    let expected_bytes = hops
        .checked_mul(4)
        .filter(|bytes| *bytes > 0 && *bytes <= transcription::MAX_PROBABILITY_BYTES)
        .ok_or(TranscriptionError::Configuration(
            "analysis audio is empty or longer than its bound",
        ))?;
    let completed = supervise(
        runtime,
        input,
        expected_bytes,
        cancelled,
        deadline,
        |_| {},
        |audio, timeout_millis| {
            Ok(HostMessage::DetectSpeech {
                protocol: transcription::VERSION,
                request: RequestId::new(format!("activity-{attempt}")).map_err(protocol_error)?,
                attempt: AttemptId::new(attempt).map_err(protocol_error)?,
                cancellation_token: CancellationToken::new(format!("cancel-{attempt}"))
                    .map_err(protocol_error)?,
                model: model.clone(),
                audio,
                output_scope: WorkspaceRef::new("output").map_err(protocol_error)?,
                maximum_output_bytes: expected_bytes,
                timeout_millis,
            })
        },
    )?;
    if completed.bytes.len() as u64 != expected_bytes {
        return Err(TranscriptionError::Protocol(format!(
            "speech probabilities have {} bytes, expected {expected_bytes}",
            completed.bytes.len()
        )));
    }
    let probabilities: Vec<f32> = completed
        .bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();
    let activity = SpeechActivity::measure(
        ActivityAudio {
            origin: input.origin,
            sample_rate: input.source_rate,
            samples,
        },
        &input.samples,
        &probabilities,
    )?;
    Ok(ActivityResult {
        activity,
        runtime: completed.runtime,
        elapsed: completed.elapsed,
    })
}

/// The preferred stored speech activity of one Original: an approved pack's
/// detector first. Activity is a rebuildable annotation, so an unreadable row
/// is skipped rather than failing its caller.
pub fn stored_activity(
    store: &deadpan_store::ProjectStore,
    content: &str,
) -> Option<(SpeechActivityKey, SpeechActivity)> {
    let mut keys = store.speech_activity_keys_for_content(content).ok()?;
    let approved = deadpan_models::packs::approved_packs()
        .into_iter()
        .filter_map(|pack| pack.speech_activity_file().map(|file| file.sha256.clone()))
        .collect::<Vec<_>>();
    keys.sort_by_key(|key| !approved.contains(&key.model_sha256));
    keys.into_iter()
        .find_map(|key| Some((key.clone(), store.speech_activity(&key).ok()??)))
}

/// Original audio sample of an analysis sample: `origin + k · rate / 16000`.
pub fn original_sample(
    activity: &SpeechActivity,
    sample: u64,
) -> Result<ExactRatio, deadpan_core::TimeError> {
    let audio = activity.audio();
    let offset = ExactRatio::new(
        i128::from(sample) * i128::from(audio.sample_rate),
        i128::from(ANALYSIS_SAMPLE_RATE),
    )?;
    ExactRatio::integer(audio.origin).checked_add(offset)
}

/// Container seconds of an analysis sample, rounded for display only.
fn seconds(activity: &SpeechActivity, sample: u64) -> Result<f64, crate::CliError> {
    let exact = activity.seconds(sample).map_err(TranscriptionError::from)?;
    Ok(exact.numerator() as f64 / exact.denominator() as f64)
}

/// `pauses PROJECT [--asset ID]`: print the stored speech activity's pauses
/// with exact Original sample bounds.
pub fn run_pauses(arguments: &[&str]) -> Result<(), crate::CliError> {
    let usage = || crate::CliError::Usage("usage: pauses <project.deadpan> [--asset <id>]".into());
    let asset = match arguments {
        [_] => None,
        [_, "--asset", asset] => Some(
            deadpan_core::AssetId::new(*asset)
                .map_err(|e| crate::CliError::Usage(e.to_string()))?,
        ),
        _ => return Err(usage()),
    };
    let store = deadpan_store::ProjectStore::open(
        std::path::Path::new(arguments[0]),
        deadpan_store::AccessMode::ReadOnly,
    )?;
    let receipt = analysed_receipt(&store, asset.as_ref())?;
    let content = receipt.original().content().to_string();
    let Some((key, activity)) = stored_activity(&store, &content) else {
        return crate::write_json(&serde_json::json!({
            "protocol": 1,
            "rule": SILENCE_RULE,
            "activity": null,
            "pauses": [],
        }));
    };
    let (corrected, corrections_error) =
        crate::speech::corrected_pauses(&store, &content, key.audio_stream, &activity);
    let pauses = corrected
        .pauses
        .iter()
        .zip(&corrected.corrected)
        .map(|(pause, manual)| {
            Ok(serde_json::json!({
                "corrected": manual,
                "analysis_start": pause.start,
                "analysis_end": pause.end,
                "original_sample_start": original_sample(&activity, pause.start)?,
                "original_sample_end": original_sample(&activity, pause.end)?,
                "seconds_start": seconds(&activity, pause.start)?,
                "seconds_end": seconds(&activity, pause.end)?,
            }))
        })
        .collect::<Result<Vec<_>, crate::CliError>>()?;
    let audio = activity.audio();
    crate::write_json(&serde_json::json!({
        "protocol": 1,
        "rule": SILENCE_RULE,
        "activity": {
            "key": key,
            "origin": audio.origin,
            "sample_rate": audio.sample_rate,
            "analysis_samples": audio.samples,
        },
        "detected_pauses": activity.pauses().len(),
        "correction_rule": deadpan_analysis::CORRECTION_RULE,
        "corrections_skipped": corrected.skipped,
        "corrections_error": corrections_error,
        "pauses": pauses,
    }))
}
