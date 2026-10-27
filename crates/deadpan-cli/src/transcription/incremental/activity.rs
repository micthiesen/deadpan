//! Detection-only continuation for projects whose complete transcript exists.

use super::*;

/// Detect pauses with the same cursor-priority ranges and exact global hop
/// stitching as transcription. Only complete coverage produces an activity.
#[allow(clippy::too_many_arguments)]
pub fn detect_original_ranges(
    store: &ProjectStore,
    runtime: &TranscriptionRuntime,
    vad: &ModelInput,
    cache_path: Option<&Path>,
    cursor: &AtomicU64,
    cancelled: &AtomicBool,
    deadline: Instant,
    mut progress: impl FnMut(u64, u64) -> Result<(), TranscriptionError>,
) -> Result<(SpeechActivityKey, SpeechActivity), TranscriptionError> {
    check(cancelled, deadline)?;
    let worker_sha256 = worker_hash(runtime, cancelled, deadline)?;
    let session = open_original_analysis(store, None, cancelled, deadline)?;
    let total = session.complete_analysis_frames();
    let mut schedule = RangeSchedule::audio(total).map_err(configuration)?;
    let cache = cache_path.and_then(|path| cache::RangeCache::open(path).ok());
    let mut parts = Vec::new();
    let mut final_key = None;
    let mut final_audio = None;
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
            model_sha256: vad.sha256.as_str().into(),
            vad_sha256: Some(vad.sha256.as_str().into()),
            language: "speech-activity".into(),
            worker_sha256: worker_sha256.clone(),
            owned: range.owned.clone(),
            context: range.context.clone(),
        };
        let name = cache::RangeCache::name(&key)?;
        let cached = cache
            .as_ref()
            .and_then(|cache| cache.read::<CachedRange>(&name).ok().flatten())
            .filter(|record| record.validate(&key).is_ok());
        let record = if let Some(record) = cached {
            record
        } else {
            let input = AnalysisInput {
                samples: window.samples,
                origin: key.origin,
                source_rate: key.source_rate,
            };
            let result = crate::activity::detect_speech(
                runtime,
                vad,
                &input,
                &uuid::Uuid::new_v4().simple().to_string(),
                cancelled,
                deadline,
            )?;
            let audio = result.activity.audio();
            let transcript = Transcript::new(
                AnalysedAudio {
                    origin: key.origin,
                    sample_rate: key.source_rate,
                    duration_cs: u32::try_from(audio.samples.div_ceil(160))
                        .map_err(|_| TranscriptionError::Configuration("analysis duration"))?,
                },
                vec![],
            )?;
            let record = CachedRange {
                key: key.clone(),
                engine: result.runtime.engine,
                transcript,
                activity: Some(CachedActivity {
                    audio,
                    speech: result.activity.speech().to_vec(),
                    energy: result.activity.energy().to_vec(),
                }),
            };
            record.validate(&key)?;
            check(cancelled, deadline)?;
            if let Some(cache) = &cache {
                let _ = cache.write(&name, &record);
            }
            record
        };
        check(cancelled, deadline)?;
        if final_key
            .as_ref()
            .is_some_and(|previous: &SpeechActivityKey| previous.engine != record.engine)
        {
            return Err(TranscriptionError::Configuration(
                "speech engine changed across ranges",
            ));
        }
        let activity = record
            .validate(&key)?
            .ok_or(TranscriptionError::Configuration("missing activity range"))?;
        parts.push(ActivityRange {
            range: range.clone(),
            activity,
        });
        schedule.finish(&range).map_err(configuration)?;
        final_key = Some(SpeechActivityKey {
            content: key.content,
            audio_stream: key.audio_stream,
            model_sha256: vad.sha256.as_str().into(),
            engine: record.engine,
        });
        final_audio = Some(ActivityAudio {
            origin: key.origin,
            sample_rate: key.source_rate,
            samples: total,
        });
        progress(schedule.completed_samples(), total)?;
    }
    check(cancelled, deadline)?;
    let key = final_key.ok_or(TranscriptionError::Configuration("empty speech analysis"))?;
    let audio = final_audio.ok_or(TranscriptionError::Configuration("empty speech analysis"))?;
    let activity = assemble_activity(audio, &parts).map_err(configuration)?;
    Ok((key, activity))
}
