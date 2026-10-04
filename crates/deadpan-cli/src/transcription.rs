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

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
