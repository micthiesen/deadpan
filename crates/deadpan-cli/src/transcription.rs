//! Host side of local transcription.
//!
//! The host writes mono 16 kHz analysis PCM into a fresh attempt workspace,
//! launches the trusted `deadpan-transcribe` executable through the shared
//! process supervisor, and admits the transcript only after clean teardown, a
//! hashed artifact snapshot, bounded parsing and full transcript validation.
//! Nothing here edits a project; a transcript is an annotation.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_analysis::{ANALYSIS_SAMPLE_RATE, AnalysedAudio, RawSegment, Transcript};
use deadpan_jobs::artifact::{ArtifactLimits, ArtifactWorkspace, SnapshotInterruption};
use deadpan_jobs::process::{ProcessEvent, ProcessLimits, ProcessSpec, SupervisedProcess};
use deadpan_jobs::transcription::{
    self, HostMessage, Language, ModelInput, RuntimeReport, TranscriptionProtocol, WorkerMessage,
};
use deadpan_jobs::{
    AttemptId, CancellationToken, RequestId, Sha256, WorkspaceArtifact, WorkspaceRef,
};
use sha2::Digest;

const INPUT: &str = "input/analysis.f32";
const OUTPUT_SCOPE: &str = "output";

/// Selected by the trusted application host, never by project data.
#[derive(Debug, Clone)]
pub struct TranscriptionRuntime {
    pub executable: PathBuf,
    pub environment: BTreeMap<OsString, OsString>,
}

impl TranscriptionRuntime {
    /// The worker installed beside the current executable.
    pub fn beside_current_executable() -> Result<Self, TranscriptionError> {
        let current = std::env::current_exe()?;
        let directory = current.parent().ok_or(TranscriptionError::Configuration(
            "executable has no directory",
        ))?;
        Ok(Self {
            executable: directory.join("deadpan-transcribe"),
            environment: BTreeMap::new(),
        })
    }
}

/// Mono analysis PCM at 16 kHz and the Original audio it came from.
#[derive(Debug, Clone)]
pub struct AnalysisInput {
    pub samples: Vec<f32>,
    /// Original audio sample at which `samples` begins.
    pub origin: i64,
    /// Original audio stream sample rate.
    pub source_rate: u32,
}

impl AnalysisInput {
    fn audio(&self) -> Result<AnalysedAudio, TranscriptionError> {
        let frames = u64::try_from(self.samples.len())
            .map_err(|_| TranscriptionError::Configuration("analysis length"))?;
        if frames == 0 || frames > transcription::MAX_ANALYSIS_FRAMES {
            return Err(TranscriptionError::Configuration(
                "analysis audio is empty or longer than its bound",
            ));
        }
        let rate = u64::from(ANALYSIS_SAMPLE_RATE);
        let duration_cs = u32::try_from((frames * 100).div_ceil(rate))
            .map_err(|_| TranscriptionError::Configuration("analysis duration"))?;
        Ok(AnalysedAudio {
            origin: self.origin,
            sample_rate: self.source_rate,
            duration_cs,
        })
    }
}

#[derive(Debug, Clone)]
pub struct TranscriptionResult {
    pub transcript: Transcript,
    pub runtime: RuntimeReport,
    pub elapsed: Duration,
}

#[derive(Debug, thiserror::Error)]
pub enum TranscriptionError {
    #[error("transcription configuration: {0}")]
    Configuration(&'static str),
    #[error("transcription cancelled")]
    Cancelled,
    #[error("transcription deadline elapsed")]
    Deadline,
    #[error("transcription worker: {0}")]
    Worker(String),
    #[error("transcription protocol: {0}")]
    Protocol(String),
    #[error(transparent)]
    Transcript(#[from] deadpan_analysis::TranscriptError),
    #[error(transparent)]
    Supervisor(#[from] deadpan_jobs::process::SupervisorError),
    #[error(transparent)]
    Artifact(#[from] deadpan_jobs::artifact::ArtifactError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

fn protocol_error(error: impl std::fmt::Display) -> TranscriptionError {
    TranscriptionError::Protocol(error.to_string())
}

/// Run one supervised transcription attempt.
#[allow(clippy::too_many_arguments)]
pub fn transcribe(
    runtime: &TranscriptionRuntime,
    model: &ModelInput,
    input: &AnalysisInput,
    language: Language,
    attempt: &str,
    cancelled: &AtomicBool,
    deadline: Instant,
    mut progress: impl FnMut(u8),
) -> Result<TranscriptionResult, TranscriptionError> {
    let audio = input.audio()?;
    if input.samples.iter().any(|sample| !sample.is_finite()) {
        return Err(TranscriptionError::Configuration(
            "analysis audio contains non-finite samples",
        ));
    }
    let workspace = tempfile::Builder::new()
        .prefix("deadpan-transcribe-")
        .tempdir()?;
    std::fs::create_dir(workspace.path().join("input"))?;
    std::fs::create_dir(workspace.path().join(OUTPUT_SCOPE))?;
    let mut bytes = Vec::with_capacity(input.samples.len() * 4);
    for sample in &input.samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    {
        let mut file = std::fs::File::create_new(workspace.path().join(INPUT))?;
        file.write_all(&bytes)?;
        file.sync_all()?;
    }
    let audio_artifact = WorkspaceArtifact::new(
        WorkspaceRef::new(INPUT).map_err(protocol_error)?,
        Sha256::new(hex(&sha2::Sha256::digest(&bytes))).map_err(protocol_error)?,
        bytes.len() as u64,
    )
    .map_err(protocol_error)?;
    drop(bytes);
    let pinned = ArtifactWorkspace::open(workspace.path())?;
    let remaining = deadline.saturating_duration_since(Instant::now());
    let timeout_millis = u64::try_from(remaining.as_millis()).unwrap_or(u64::MAX);
    if timeout_millis == 0 {
        return Err(TranscriptionError::Deadline);
    }
    let request = HostMessage::Transcribe {
        protocol: transcription::VERSION,
        request: RequestId::new(format!("transcript-{attempt}")).map_err(protocol_error)?,
        attempt: AttemptId::new(attempt).map_err(protocol_error)?,
        cancellation_token: CancellationToken::new(format!("cancel-{attempt}"))
            .map_err(protocol_error)?,
        model: model.clone(),
        audio: audio_artifact,
        language,
        output_scope: WorkspaceRef::new(OUTPUT_SCOPE).map_err(protocol_error)?,
        maximum_output_bytes: transcription::MAX_TRANSCRIPT_BYTES,
        timeout_millis,
    };
    let mut process = SupervisedProcess::<TranscriptionProtocol>::spawn(
        ProcessSpec {
            executable: runtime.executable.clone(),
            arguments: Vec::new(),
            environment: runtime.environment.clone(),
            workspace: workspace.path().to_path_buf(),
            limits: ProcessLimits {
                maximum_duration: remaining,
                cancellation_grace: Duration::from_secs(2).min(remaining),
                exit_grace: Duration::from_secs(5).min(remaining),
            },
        },
        request,
    )?;
    let mut completion = None;
    let mut failure = None;
    let mut was_cancelled = false;
    let mut last_percent = 0;
    while !process.is_finished() {
        let now = Instant::now();
        if cancelled.load(Ordering::Acquire) && !was_cancelled {
            was_cancelled = true;
            process.request_cancel(now)?;
        }
        if now >= deadline {
            process.request_cancel(now)?;
            return Err(if was_cancelled {
                TranscriptionError::Cancelled
            } else {
                TranscriptionError::Deadline
            });
        }
        for event in process.poll(now)? {
            match event {
                ProcessEvent::Message(message) => match *message {
                    WorkerMessage::Progress { percent, .. } => {
                        if percent < last_percent {
                            failure.get_or_insert_with(|| {
                                TranscriptionError::Protocol("progress moved backward".into())
                            });
                            process.request_cancel(now)?;
                        } else {
                            last_percent = percent;
                            progress(percent);
                        }
                    }
                    WorkerMessage::Completed {
                        transcript,
                        runtime,
                        elapsed_millis,
                        ..
                    } => completion = Some((transcript, runtime, elapsed_millis)),
                    WorkerMessage::Failed { diagnostic, .. } => {
                        failure.get_or_insert_with(|| {
                            TranscriptionError::Worker(diagnostic.as_str().to_owned())
                        });
                    }
                    WorkerMessage::Cancelled { .. } => was_cancelled = true,
                },
                ProcessEvent::Fault(reason) => {
                    failure.get_or_insert(TranscriptionError::Worker(reason));
                }
                ProcessEvent::Exited {
                    status,
                    cancellation_escalated,
                } => {
                    if (!status.success() || cancellation_escalated)
                        && !was_cancelled
                        && failure.is_none()
                    {
                        failure = Some(TranscriptionError::Worker(format!(
                            "transcription worker exited {status}"
                        )));
                    }
                }
            }
        }
        if !process.is_finished() {
            std::thread::park_timeout(Duration::from_millis(5));
        }
    }
    if let Some(error) = failure {
        return Err(error);
    }
    if was_cancelled {
        return Err(TranscriptionError::Cancelled);
    }
    let (artifact, runtime_report, elapsed_millis) = completion
        .ok_or_else(|| TranscriptionError::Protocol("no clean completed transcript".into()))?;
    let mut snapshot = pinned.snapshot_with_control(
        &WorkspaceRef::new(OUTPUT_SCOPE).map_err(protocol_error)?,
        &artifact,
        ArtifactLimits::new(transcription::MAX_TRANSCRIPT_BYTES)?,
        || {
            if cancelled.load(Ordering::Acquire) {
                return Err(SnapshotInterruption::Cancelled);
            }
            if Instant::now() >= deadline {
                return Err(SnapshotInterruption::Deadline);
            }
            Ok(())
        },
    )?;
    let mut json = Vec::new();
    snapshot
        .by_ref()
        .take(transcription::MAX_TRANSCRIPT_BYTES + 1)
        .read_to_end(&mut json)?;
    if json.len() as u64 != artifact.byte_length() {
        return Err(TranscriptionError::Protocol(
            "transcript snapshot length changed".into(),
        ));
    }
    let segments: Vec<RawSegment> = serde_json::from_slice(&json).map_err(protocol_error)?;
    let transcript = Transcript::from_segments(audio, &segments)?;
    Ok(TranscriptionResult {
        transcript,
        runtime: runtime_report,
        elapsed: Duration::from_millis(elapsed_millis),
    })
}

/// The Original's analysis PCM and the identity transcripts are keyed by.
#[derive(Debug, Clone)]
pub struct OriginalAnalysis {
    pub input: AnalysisInput,
    /// The Original's content identity.
    pub content: String,
    /// The qualified audio stream within the Original container.
    pub audio_stream: u32,
}

/// Source frames read per decode; output blocks reuse this window.
const SOURCE_CHUNK_FRAMES: u32 = 32_768;

/// The qualification receipt of the audio to analyse: the single-Original
/// project's Original by default, or an explicitly registered source.
fn analysed_receipt(
    store: &deadpan_store::ProjectStore,
    asset: Option<&deadpan_core::AssetId>,
) -> Result<deadpan_store::source_registration::SourceQualificationReceipt, TranscriptionError> {
    use deadpan_store::single_source::SingleSourceState;
    let unavailable =
        |error: deadpan_store::StoreError| TranscriptionError::Worker(error.to_string());
    if let Some(asset) = asset {
        let head = store.head_revision().map_err(unavailable)?;
        return store.registered_source(&head, asset).map_err(unavailable);
    }
    let Some(SingleSourceState::Ready {
        asset,
        baseline_revision,
        ..
    }) = store.single_source_state().map_err(unavailable)?
    else {
        return Err(TranscriptionError::Configuration(
            "project has no ready Original; choose a registered source asset",
        ));
    };
    // The Original is immutable; its baseline revision always holds it.
    store
        .registered_source(&baseline_revision, &asset)
        .map_err(unavailable)
}

/// Decode a source's audio from its verified retained bytes and prepare mono
/// 16 kHz analysis PCM with the canonical exact-phase resampler. Analysis
/// sample `k` corresponds to source audio sample `origin + k · rate / 16000`.
pub fn prepare_original_audio(
    store: &deadpan_store::ProjectStore,
    asset: Option<&deadpan_core::AssetId>,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<OriginalAnalysis, TranscriptionError> {
    use deadpan_audio::{PcmWindow, ResampleRecipe, Resampler, StereoMatrix};
    use deadpan_core::{AudioSample, ExactRatio};
    use deadpan_media::audio_session::{AudioSession, AudioSessionLimits, SourceAudioSample};
    use deadpan_store::original_media::OriginalMediaLimits;

    let unavailable = |reason: String| TranscriptionError::Worker(reason);
    let check = || -> Result<(), TranscriptionError> {
        if cancelled.load(Ordering::Acquire) {
            return Err(TranscriptionError::Cancelled);
        }
        if Instant::now() >= deadline {
            return Err(TranscriptionError::Deadline);
        }
        Ok(())
    };
    let receipt = analysed_receipt(store, asset)?;
    let expected = receipt
        .snapshot()
        .audio()
        .ok_or(TranscriptionError::Configuration(
            "the Original has no qualified audio",
        ))?;
    let stream = expected.stream().clone();
    let bytes = expected
        .decoded_samples()
        .checked_mul(u64::from(stream.channel_layout.channels()))
        .and_then(|samples| samples.checked_mul(4))
        .ok_or(TranscriptionError::Configuration("Original audio size"))?;
    let limits = AudioSessionLimits {
        maximum_cache_bytes: bytes,
        maximum_index_frames: expected.frames().len(),
        ..AudioSessionLimits::default()
    };
    let remaining = deadline
        .saturating_duration_since(Instant::now())
        .min(Duration::from_secs(3_600));
    check()?;
    let mut original = store
        .snapshot_original(
            receipt.original().content(),
            OriginalMediaLimits::new(limits.decode.max_input_bytes, remaining)
                .map_err(|e| unavailable(e.to_string()))?,
            cancelled,
        )
        .map_err(|e| unavailable(e.to_string()))?;
    if original.record().object() != receipt.original() {
        return Err(TranscriptionError::Protocol(
            "retained Original differs from its receipt".into(),
        ));
    }
    let session = AudioSession::open_verified(
        &mut original,
        expected.content(),
        stream.stream_index,
        limits,
        cancelled,
    )
    .map_err(|e| unavailable(e.to_string()))?;
    let frames = session.index().frames();
    let (Some(first), Some(last)) = (frames.first(), frames.last()) else {
        return Err(TranscriptionError::Configuration(
            "the Original audio is empty",
        ));
    };
    let (valid_start, valid_end) = (first.valid_start, last.valid_end);
    let rate = stream.sample_rate;
    let source_frames = valid_end
        .checked_sub(valid_start)
        .filter(|frames| *frames > 0)
        .ok_or(TranscriptionError::Configuration(
            "the Original audio is empty",
        ))?;
    let output_frames = i64::try_from(
        i128::from(source_frames) * i128::from(ANALYSIS_SAMPLE_RATE) / i128::from(rate),
    )
    .map_err(|_| TranscriptionError::Configuration("analysis length"))?;
    if output_frames <= 0 || output_frames as u64 > transcription::MAX_ANALYSIS_FRAMES {
        return Err(TranscriptionError::Configuration(
            "the Original audio is empty or longer than the analysis bound",
        ));
    }
    let recipe = ResampleRecipe::new(
        valid_start..valid_end,
        ExactRatio::integer(valid_start),
        AudioSample(0),
        ExactRatio::new(i128::from(rate), i128::from(ANALYSIS_SAMPLE_RATE))
            .map_err(|_| TranscriptionError::Configuration("source rate"))?,
        AudioSample(0)..AudioSample(output_frames),
    )
    .map_err(|e| unavailable(e.to_string()))?;
    let resampler = Resampler::new(
        recipe,
        StereoMatrix::new(stream.channel_layout).map_err(|e| unavailable(e.to_string()))?,
    );
    let channels = stream.channel_layout.channels() as usize;
    let mut buffer_start = valid_start;
    let mut buffer: Vec<f32> = Vec::new();
    let mut samples = Vec::with_capacity(output_frames as usize);
    let block = i64::from(deadpan_audio::MAX_OUTPUT_FRAMES);
    let mut start = 0_i64;
    while start < output_frames {
        check()?;
        let count = (output_frames - start).min(block) as u32;
        let required = resampler
            .required_source_range(AudioSample(start), count)
            .map_err(|e| unavailable(e.to_string()))?;
        let window = match required {
            None => None,
            Some(required) => {
                let buffer_end = buffer_start + (buffer.len() / channels) as i64;
                if required.start < buffer_start || required.end > buffer_end {
                    let length = (valid_end - required.start)
                        .min(i64::from(SOURCE_CHUNK_FRAMES))
                        .max(required.end - required.start);
                    let read = session
                        .read_samples(
                            SourceAudioSample(required.start),
                            u32::try_from(length)
                                .map_err(|_| TranscriptionError::Configuration("read size"))?,
                            Duration::from_secs(30),
                            cancelled,
                        )
                        .map_err(|e| unavailable(e.to_string()))?;
                    buffer_start = read.start.0;
                    buffer = read.samples;
                }
                let offset = (required.start - buffer_start) as usize * channels;
                let length = (required.end - required.start) as usize * channels;
                Some(PcmWindow {
                    start: required.start,
                    samples: buffer[offset..offset + length].to_vec(),
                })
            }
        };
        let rendered = resampler
            .render(AudioSample(start), count, window, cancelled)
            .map_err(|e| unavailable(e.to_string()))?;
        samples.extend(
            rendered
                .samples
                .iter()
                .map(|[left, right]| (left + right) * 0.5),
        );
        start += i64::from(count);
    }
    Ok(OriginalAnalysis {
        input: AnalysisInput {
            samples,
            origin: valid_start,
            source_rate: rate,
        },
        content: receipt.original().content().to_string(),
        audio_stream: stream.stream_index,
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// `transcribe PROJECT --model PATH --sha256 HEX [--language CODE]`: prepare
/// the Original's analysis PCM, run the worker beside this executable, and
/// store the validated transcript. Requires the project's writer.
pub fn run_transcribe(arguments: &[&str]) -> Result<(), crate::CliError> {
    let usage = || {
        crate::CliError::Usage(
            "usage: transcribe <project.deadpan> [--model <ggml.bin> --sha256 <hex>] [--language <auto|xx>] [--asset <id>]"
                .into(),
        )
    };
    let [path, rest @ ..] = arguments else {
        return Err(usage());
    };
    let mut model = None;
    let mut sha256 = None;
    let mut language = Language::Code("en".into());
    let mut asset = None;
    let mut options = rest.iter();
    while let Some(option) = options.next() {
        let value = options.next().ok_or_else(usage)?;
        match *option {
            "--model" => model = Some(std::fs::canonicalize(value)?),
            "--sha256" => {
                sha256 =
                    Some(Sha256::new(*value).map_err(|e| crate::CliError::Usage(e.to_string()))?)
            }
            "--language" => {
                language =
                    Language::try_from((*value).to_owned()).map_err(crate::CliError::Usage)?;
            }
            "--asset" => {
                asset = Some(
                    deadpan_core::AssetId::new(*value)
                        .map_err(|e| crate::CliError::Usage(e.to_string()))?,
                );
            }
            _ => return Err(usage()),
        }
    }
    let model = match (model, sha256) {
        (Some(path), Some(sha256)) => ModelInput {
            byte_length: std::fs::metadata(&path)?.len(),
            path,
            sha256,
        },
        // Without an explicit model, use the installed approved pack.
        (None, None) => crate::models::installed_transcription_model(
            &crate::models::default_root()?,
        )?
        .ok_or(TranscriptionError::Configuration(
            "no transcription model is installed; run models install whisper-base-en",
        ))?,
        _ => return Err(usage()),
    };
    let store = deadpan_store::ProjectStore::open(
        std::path::Path::new(path),
        deadpan_store::AccessMode::ReadWrite,
    )?;
    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(6 * 60 * 60);
    let analysis = prepare_original_audio(&store, asset.as_ref(), &cancelled, deadline)?;
    let attempt = uuid::Uuid::new_v4().simple().to_string();
    let language_label: String = language.clone().into();
    let result = transcribe(
        &TranscriptionRuntime::beside_current_executable()?,
        &model,
        &analysis.input,
        language,
        &attempt,
        &cancelled,
        deadline,
        |_| {},
    )?;
    let key = deadpan_store::TranscriptKey {
        content: analysis.content,
        audio_stream: analysis.audio_stream,
        model_sha256: model.sha256.as_str().to_owned(),
        language: language_label,
        engine: result.runtime.engine.clone(),
    };
    store.save_transcript(&key, &result.transcript)?;
    let words = result.transcript.words();
    crate::write_json(&serde_json::json!({
        "protocol": 1,
        "key": key,
        "runtime": result.runtime,
        "worker_elapsed_ms": u64::try_from(result.elapsed.as_millis()).unwrap_or(u64::MAX),
        "audio_seconds": analysis.input.samples.len() as f64 / f64::from(ANALYSIS_SAMPLE_RATE),
        "words": words.len(),
        "approximate_words": words.iter().filter(|word| word.approximate()).count(),
    }))
}

/// `transcript PROJECT [--search WORDS]`: print stored transcripts of the
/// Original, or the word ranges matching a phrase, with exact Original timing.
pub fn run_transcript(arguments: &[&str]) -> Result<(), crate::CliError> {
    let usage = || {
        crate::CliError::Usage(
            "usage: transcript <project.deadpan> [--search <words>] [--asset <id>]".into(),
        )
    };
    let [path, rest @ ..] = arguments else {
        return Err(usage());
    };
    let mut search = None;
    let mut asset = None;
    let mut options = rest.iter();
    while let Some(option) = options.next() {
        let value = options.next().ok_or_else(usage)?;
        match *option {
            "--search" => search = Some(*value),
            "--asset" => {
                asset = Some(
                    deadpan_core::AssetId::new(*value)
                        .map_err(|e| crate::CliError::Usage(e.to_string()))?,
                );
            }
            _ => return Err(usage()),
        }
    }
    let store = deadpan_store::ProjectStore::open(
        std::path::Path::new(path),
        deadpan_store::AccessMode::ReadOnly,
    )?;
    let receipt = analysed_receipt(&store, asset.as_ref())?;
    let content = receipt.original().content().to_string();
    let mut transcripts = Vec::new();
    for (key, transcript) in store.transcripts_for_content(&content)? {
        let matches = search.map(|words| {
            transcript
                .search(words)
                .into_iter()
                .map(|range| {
                    let first = &transcript.words()[range.start];
                    let last = &transcript.words()[range.end - 1];
                    serde_json::json!({
                        "words": range,
                        "text": transcript.words()[range.clone()].iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" "),
                        "start_cs": first.start_cs,
                        "end_cs": last.end_cs,
                        "original_sample_start": transcript.original_sample(first.start_cs).ok(),
                        "original_sample_end": transcript.original_sample(last.end_cs).ok(),
                    })
                })
                .collect::<Vec<_>>()
        });
        transcripts.push(match matches {
            Some(matches) => serde_json::json!({ "key": key, "matches": matches }),
            None => serde_json::json!({ "key": key, "transcript": transcript }),
        });
    }
    crate::write_json(&serde_json::json!({ "protocol": 1, "transcripts": transcripts }))
}
