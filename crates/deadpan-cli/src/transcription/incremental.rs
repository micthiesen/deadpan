//! Local cursor-priority transcription and VAD in exact-phase source windows.
//!
//! Decoder opening still builds AudioSession's complete private PCM cache.
//! Subsequent resampling/model inputs are bounded below thirty seconds. Only a
//! fully covered result may enter the existing complete-analysis store APIs.

use std::ops::Range;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use deadpan_analysis::incremental::{
    ActivityRange, AnalysisRange, RangeSchedule, TranscriptRange, assemble_activity,
    assemble_transcript,
};
use deadpan_analysis::{ActivityAudio, AnalysedAudio, SpeechActivity, Transcript};
use deadpan_jobs::transcription::{Language, ModelInput};
use deadpan_store::{ProjectStore, SpeechActivityKey, TranscriptKey};
use serde::{Deserialize, Serialize};

use super::{AnalysisInput, TranscriptionError, TranscriptionRuntime, open_original_analysis};

mod activity;
pub(crate) mod cache;
#[cfg(test)]
mod tests;
pub use activity::detect_original_ranges;

/// Changes whenever window ownership, context, stitching or model settings change.
pub const RANGE_IMPLEMENTATION: &str = "deadpan-audio-ranges-1";

/// Completed proposals with explicit coverage. Missing intervals have no implied
/// words or silence. Display-only consumers must retain this coverage label.
pub struct RangeProgress {
    pub transcript: Transcript,
    pub coverage: Vec<Range<u64>>,
    pub total_samples: u64,
    pub completed_samples: u64,
    pub cached_ranges: usize,
    pub cache_failures: usize,
}

pub struct IncrementalAudio {
    pub transcript_key: TranscriptKey,
    pub transcript: Transcript,
    pub activity: Option<(SpeechActivityKey, SpeechActivity)>,
    pub cached_ranges: usize,
    /// Cache failures do not hide successful analysis or discard its result.
    pub cache_failures: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Key {
    implementation: String,
    protocol: u32,
    content: String,
    audio_stream: u32,
    origin: i64,
    source_rate: u32,
    total_samples: u64,
    model_sha256: String,
    vad_sha256: Option<String>,
    language: String,
    /// Exact helper bytes include its engine, resampler and model settings.
    worker_sha256: String,
    owned: Range<u64>,
    context: Range<u64>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CachedActivity {
    audio: ActivityAudio,
    speech: Vec<u8>,
    energy: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CachedRange {
    key: Key,
    engine: String,
    transcript: Transcript,
    activity: Option<CachedActivity>,
}

fn configuration(error: impl std::fmt::Display) -> TranscriptionError {
    TranscriptionError::Protocol(error.to_string())
}

fn check(cancelled: &AtomicBool, deadline: Instant) -> Result<(), TranscriptionError> {
    if cancelled.load(Ordering::Acquire) {
        Err(TranscriptionError::Cancelled)
    } else if Instant::now() >= deadline {
        Err(TranscriptionError::Deadline)
    } else {
        Ok(())
    }
}

fn worker_hash(
    runtime: &TranscriptionRuntime,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<String, TranscriptionError> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = std::fs::File::from(
        rustix::fs::open(
            &runtime.executable,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )
        .map_err(std::io::Error::from)?,
    );
    let metadata = file.metadata()?;
    const MAX_WORKER_BYTES: u64 = 256 * 1024 * 1024;
    if !metadata.is_file() || metadata.len() > MAX_WORKER_BYTES {
        return Err(TranscriptionError::Configuration("analysis worker size"));
    }
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 65_536];
    let mut bytes = 0_u64;
    loop {
        check(cancelled, deadline)?;
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        bytes = bytes
            .checked_add(count as u64)
            .filter(|count| *count <= MAX_WORKER_BYTES)
            .ok_or(TranscriptionError::Configuration(
                "analysis worker grew beyond its bound",
            ))?;
        hash.update(&buffer[..count]);
    }
    if bytes != metadata.len() || file.metadata()?.len() != bytes {
        return Err(TranscriptionError::Configuration(
            "analysis worker changed while hashing",
        ));
    }
    Ok(super::hex(&hash.finalize()))
}

impl CachedRange {
    fn validate(&self, expected: &Key) -> Result<Option<SpeechActivity>, TranscriptionError> {
        if self.key != *expected || self.engine.is_empty() || self.engine.len() > 64 {
            return Err(TranscriptionError::Configuration("range cache identity"));
        }
        let range = AnalysisRange {
            owned: expected.owned.clone(),
            context: expected.context.clone(),
        };
        let duration_cs = u32::try_from(expected.total_samples.div_ceil(160))
            .map_err(|_| TranscriptionError::Configuration("analysis duration"))?;
        assemble_transcript(
            AnalysedAudio {
                origin: expected.origin,
                sample_rate: expected.source_rate,
                duration_cs,
            },
            expected.total_samples,
            &[TranscriptRange {
                range,
                transcript: self.transcript.clone(),
            }],
        )
        .map_err(configuration)?;
        match (&self.activity, &expected.vad_sha256) {
            (Some(activity), Some(_))
                if activity.audio.origin == expected.origin
                    && activity.audio.sample_rate == expected.source_rate
                    && activity.audio.samples == expected.context.end - expected.context.start =>
            {
                Ok(Some(SpeechActivity::new(
                    activity.audio,
                    activity.speech.clone(),
                    activity.energy.clone(),
                )?))
            }
            (None, None) => Ok(None),
            _ => Err(TranscriptionError::Configuration(
                "range cache activity identity",
            )),
        }
    }
}

/// One uncached range's actual worker pair, under one admission. Keeping this
/// boundary together makes it impossible for VAD to start before admission.
#[allow(clippy::too_many_arguments)]
fn infer_range(
    runtime: &TranscriptionRuntime,
    model: &ModelInput,
    vad: Option<&ModelInput>,
    input: &AnalysisInput,
    language: Language,
    cancelled: &AtomicBool,
    deadline: Instant,
    mut admission: impl FnMut() -> Result<(), TranscriptionError>,
) -> Result<(super::TranscriptionResult, Option<CachedActivity>), TranscriptionError> {
    admission()?;
    check(cancelled, deadline)?;
    let activity = vad
        .map(|vad| {
            let result = crate::activity::detect_speech(
                runtime,
                vad,
                input,
                &uuid::Uuid::new_v4().simple().to_string(),
                cancelled,
                deadline,
            )?;
            Ok::<_, TranscriptionError>(CachedActivity {
                audio: result.activity.audio(),
                speech: result.activity.speech().to_vec(),
                energy: result.activity.energy().to_vec(),
            })
        })
        .transpose()?;
    let transcript = super::transcribe(
        runtime,
        model,
        input,
        language,
        &uuid::Uuid::new_v4().simple().to_string(),
        cancelled,
        deadline,
        |_| {},
    )?;
    Ok((transcript, activity))
}

/// Run actual local workers range by range, consulting the latest cursor before
/// each range. Caller owns lifecycle/session cancellation and annotation saves.
/// Cache failures are counted and rebuilt; model/decoder failures remain errors.
#[allow(clippy::too_many_arguments)]
pub fn transcribe_original_ranges(
    store: &ProjectStore,
    runtime: &TranscriptionRuntime,
    model: &ModelInput,
    vad: Option<&ModelInput>,
    language: Language,
    cache_path: Option<&Path>,
    cursor: &AtomicU64,
    cancelled: &AtomicBool,
    deadline: Instant,
    mut before_inference: impl FnMut() -> Result<(), TranscriptionError>,
    mut progress: impl FnMut(RangeProgress) -> Result<(), TranscriptionError>,
) -> Result<IncrementalAudio, TranscriptionError> {
    check(cancelled, deadline)?;
    let worker_sha256 = worker_hash(runtime, cancelled, deadline)?;
    let session = open_original_analysis(store, None, cancelled, deadline)?;
    let total = session.complete_analysis_frames();
    let mut schedule = RangeSchedule::audio(total).map_err(configuration)?;
    let mut parts = Vec::new();
    let mut activities = Vec::new();
    let mut coverage = Vec::new();
    let mut cache_failures = 0;
    let cache = cache_path.and_then(|path| match cache::RangeCache::open(path) {
        Ok(cache) => Some(cache),
        Err(_) => {
            cache_failures += 1;
            None
        }
    });
    let mut cached_ranges = 0;
    let language_label: String = language.clone().into();
    let mut identity = None;
    let mut engine = None;
    while let Some(range) = schedule
        .next(cursor.load(Ordering::Acquire))
        .map_err(configuration)?
    {
        check(cancelled, deadline)?;
        let window = session.prepare_window(range.context.clone(), cancelled, deadline)?;
        let key = Key {
            implementation: RANGE_IMPLEMENTATION.into(),
            protocol: deadpan_jobs::transcription::VERSION,
            content: window.content,
            audio_stream: window.audio_stream,
            origin: window.analysis_origin,
            source_rate: window.source_rate,
            total_samples: total,
            model_sha256: model.sha256.as_str().into(),
            vad_sha256: vad.map(|model| model.sha256.as_str().into()),
            language: language_label.clone(),
            worker_sha256: worker_sha256.clone(),
            owned: range.owned.clone(),
            context: range.context.clone(),
        };
        let name = cache::RangeCache::name(&key)?;
        let cached = cache
            .as_ref()
            .and_then(|cache| match cache.read::<CachedRange>(&name) {
                Ok(Some(record)) => match record.validate(&key) {
                    Ok(_) => Some(record),
                    Err(_) => {
                        cache_failures += 1;
                        None
                    }
                },
                Ok(None) => None,
                Err(_) => {
                    cache_failures += 1;
                    None
                }
            });
        let record = if let Some(cached) = cached {
            cached_ranges += 1;
            cached
        } else {
            // The local input's integer origin labels the full Original. Its
            // word/hop coordinates are shifted only by the exact grid assembler.
            let input = AnalysisInput {
                samples: window.samples,
                origin: key.origin,
                source_rate: key.source_rate,
            };
            let (result, activity) = infer_range(
                runtime,
                model,
                vad,
                &input,
                language.clone(),
                cancelled,
                deadline,
                &mut before_inference,
            )?;
            let record = CachedRange {
                key: key.clone(),
                engine: result.runtime.engine,
                transcript: result.transcript,
                activity,
            };
            record.validate(&key)?;
            check(cancelled, deadline)?;
            if let Some(cache) = &cache
                && cache.write(&name, &record).is_err()
            {
                cache_failures += 1;
            }
            record
        };
        check(cancelled, deadline)?;
        if engine
            .as_ref()
            .is_some_and(|engine| *engine != record.engine)
        {
            return Err(TranscriptionError::Configuration(
                "analysis engine changed across ranges",
            ));
        }
        engine = Some(record.engine.clone());
        let activity = record.validate(&key)?;
        parts.push(TranscriptRange {
            range: range.clone(),
            transcript: record.transcript,
        });
        if let Some(activity) = activity {
            activities.push(ActivityRange {
                range: range.clone(),
                activity,
            });
        }
        coverage.push(range.owned.clone());
        coverage.sort_by_key(|range| range.start);
        schedule.finish(&range).map_err(configuration)?;
        let audio = AnalysedAudio {
            origin: key.origin,
            sample_rate: key.source_rate,
            duration_cs: u32::try_from(total.div_ceil(160))
                .map_err(|_| TranscriptionError::Configuration("analysis duration"))?,
        };
        let transcript = assemble_transcript(audio, total, &parts).map_err(configuration)?;
        identity = Some((key.content, key.audio_stream, audio));
        progress(RangeProgress {
            transcript,
            coverage: coverage.clone(),
            total_samples: total,
            completed_samples: schedule.completed_samples(),
            cached_ranges,
            cache_failures,
        })?;
    }
    check(cancelled, deadline)?;
    if !schedule.is_complete() {
        return Err(TranscriptionError::Configuration(
            "incomplete analysis coverage",
        ));
    }
    let (content, audio_stream, audio) =
        identity.ok_or(TranscriptionError::Configuration("empty analysis"))?;
    let engine = engine.ok_or(TranscriptionError::Configuration("missing analysis engine"))?;
    let transcript = assemble_transcript(audio, total, &parts).map_err(configuration)?;
    let activity = if let Some(vad) = vad {
        Some((
            SpeechActivityKey {
                content: content.clone(),
                audio_stream,
                model_sha256: vad.sha256.as_str().into(),
                engine: engine.clone(),
            },
            assemble_activity(
                ActivityAudio {
                    origin: audio.origin,
                    sample_rate: audio.sample_rate,
                    samples: total,
                },
                &activities,
            )
            .map_err(configuration)?,
        ))
    } else {
        None
    };
    Ok(IncrementalAudio {
        transcript_key: TranscriptKey {
            content,
            audio_stream,
            model_sha256: model.sha256.as_str().into(),
            language: language_label,
            engine,
        },
        transcript,
        activity,
        cached_ranges,
        cache_failures,
    })
}
