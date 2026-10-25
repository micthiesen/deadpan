//! Host admission of local phoneme emissions for explicit annotation proposals.
//!
//! The native worker owns inference. This host owns its private workspace,
//! deadline, process teardown and verified output snapshot. It neither accepts
//! a proposal nor changes a transcript or authored revision.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_jobs::artifact::{ArtifactLimits, ArtifactWorkspace, SnapshotInterruption};
use deadpan_jobs::process::{ProcessEvent, ProcessLimits, ProcessSpec, SupervisedProcess};
use deadpan_jobs::refinement::{
    self, HostMessage, RefinementProtocol, RuntimeReport, WorkerMessage,
};
use deadpan_jobs::transcription::ModelInput;
use deadpan_jobs::{
    AttemptId, CancellationToken, RequestId, Sha256, WorkspaceArtifact, WorkspaceRef,
};
use sha2::Digest;

const INPUT: &str = "input/analysis.f32le";
const OUTPUT: &str = "output";

mod project;
pub use project::{PreparedRefinement, prepare};
mod acceptance;
pub use acceptance::{AcceptanceRequest, execute_acceptance_on};
mod command;
pub use command::run;

#[derive(Debug, thiserror::Error)]
pub enum RefinementError {
    #[error("refinement configuration: {0}")]
    Configuration(&'static str),
    #[error("refinement cancelled")]
    Cancelled,
    #[error("refinement deadline elapsed")]
    Deadline,
    #[error("refinement worker: {0}")]
    Worker(String),
    #[error("refinement protocol: {0}")]
    Protocol(String),
    #[error(transparent)]
    Supervisor(#[from] deadpan_jobs::process::SupervisorError),
    #[error(transparent)]
    Artifact(#[from] deadpan_jobs::artifact::ArtifactError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// The qualified checkpoint and CMUDict phone vocabulary cover English only.
pub fn validate_language(language: &str) -> Result<(), RefinementError> {
    if language == "en" {
        Ok(())
    } else {
        Err(RefinementError::Configuration(
            "local word timing is qualified for English transcripts (en) only",
        ))
    }
}

/// Trusted executable selection, independent of project and model data.
#[derive(Debug, Clone)]
pub struct RefinementRuntime {
    pub executable: PathBuf,
    pub environment: BTreeMap<OsString, OsString>,
}

impl RefinementRuntime {
    pub fn beside_current_executable() -> Result<Self, RefinementError> {
        Ok(Self {
            executable: crate::bundle::qualify_refinement_worker()?,
            environment: BTreeMap::new(),
        })
    }
}

/// Validated, owned log probabilities. These remain approximate model evidence.
#[derive(Debug, Clone)]
pub struct RefinementEmissions {
    pub input_samples: u32,
    pub frames: u32,
    pub log_probs: Vec<f64>,
    pub runtime: RuntimeReport,
    pub elapsed: Duration,
}

fn protocol(error: impl std::fmt::Display) -> RefinementError {
    RefinementError::Protocol(error.to_string())
}

fn check(cancelled: &AtomicBool, deadline: Instant) -> Result<(), RefinementError> {
    if cancelled.load(Ordering::Acquire) {
        Err(RefinementError::Cancelled)
    } else if Instant::now() >= deadline {
        Err(RefinementError::Deadline)
    } else {
        Ok(())
    }
}

/// Infer a bounded captured context through the actual isolated worker.
/// Call on a preparation worker, never the UI or audio callback.
#[allow(clippy::too_many_arguments)]
pub fn infer(
    runtime: &RefinementRuntime,
    checkpoint: &ModelInput,
    statistics: &ModelInput,
    samples: &[f32],
    attempt: &str,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<RefinementEmissions, RefinementError> {
    check(cancelled, deadline)?;
    if deadline.saturating_duration_since(Instant::now())
        > Duration::from_millis(refinement::MAX_TIMEOUT_MS)
    {
        return Err(RefinementError::Configuration(
            "inference deadline exceeds the ten-minute worker bound",
        ));
    }
    if !(401..=480_000).contains(&samples.len()) || samples.iter().any(|value| !value.is_finite()) {
        return Err(RefinementError::Configuration(
            "context must contain 401..480000 finite mono 16 kHz samples",
        ));
    }
    let input_samples = u32::try_from(samples.len())
        .map_err(|_| RefinementError::Configuration("context sample count"))?;
    let frames = (input_samples / 160 + 1).div_ceil(2);
    let maximum_bytes = u64::from(frames) * 41 * 4;
    let workspace = tempfile::Builder::new()
        .prefix("deadpan-align-")
        .tempdir()?;
    std::fs::create_dir(workspace.path().join("input"))?;
    std::fs::create_dir(workspace.path().join(OUTPUT))?;
    let mut digest = sha2::Sha256::new();
    {
        let mut file = std::fs::File::create_new(workspace.path().join(INPUT))?;
        // Keep staging and hashing interruptible without another complete PCM copy.
        for chunk in samples.chunks(4096) {
            check(cancelled, deadline)?;
            let bytes: Vec<u8> = chunk.iter().flat_map(|value| value.to_le_bytes()).collect();
            file.write_all(&bytes)?;
            digest.update(&bytes);
        }
        file.sync_all()?;
    }
    check(cancelled, deadline)?;
    let digest: String = digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let audio = WorkspaceArtifact::new(
        WorkspaceRef::new(INPUT).map_err(protocol)?,
        Sha256::new(digest).map_err(protocol)?,
        u64::from(input_samples) * 4,
    )
    .map_err(protocol)?;
    let pinned = ArtifactWorkspace::open(workspace.path())?;
    let remaining = deadline.saturating_duration_since(Instant::now());
    let timeout_ms = u64::try_from(remaining.as_millis())
        .map_err(|_| RefinementError::Configuration("deadline exceeds protocol bound"))?;
    let request = HostMessage::Infer {
        protocol: refinement::VERSION,
        request: RequestId::new(format!("refinement-{attempt}")).map_err(protocol)?,
        attempt: AttemptId::new(attempt).map_err(protocol)?,
        cancellation_token: CancellationToken::new(format!("cancel-{attempt}"))
            .map_err(protocol)?,
        checkpoint: Box::new(checkpoint.clone()),
        statistics: Box::new(statistics.clone()),
        audio,
        output_scope: WorkspaceRef::new(OUTPUT).map_err(protocol)?,
        max_output_bytes: maximum_bytes,
        timeout_ms,
    };
    let mut process = SupervisedProcess::<RefinementProtocol>::spawn(
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
    let mut interruption = None;
    while !process.is_finished() {
        let now = Instant::now();
        if interruption.is_none()
            && let Err(error) = check(cancelled, deadline)
        {
            interruption = Some(error);
            process.request_cancel(now)?;
        }
        // Continue polling through confirmed teardown before returning interruption.
        for event in process.poll(now)? {
            match event {
                ProcessEvent::Message(message) => match *message {
                    WorkerMessage::Completed {
                        emissions,
                        runtime,
                        elapsed_millis,
                        input_samples: reported_samples,
                        frames: reported_frames,
                        ..
                    } => {
                        if reported_samples != input_samples || reported_frames != frames {
                            failure.get_or_insert_with(|| protocol("emission shape changed"));
                        } else {
                            completion = Some((emissions, runtime, elapsed_millis));
                        }
                    }
                    WorkerMessage::Failed { diagnostic, .. } => {
                        failure.get_or_insert_with(|| {
                            RefinementError::Worker(diagnostic.as_str().into())
                        });
                    }
                    WorkerMessage::Cancelled { .. } if interruption.is_some() => {}
                    WorkerMessage::Cancelled { .. } => {
                        failure.get_or_insert_with(|| protocol("unrequested cancellation"));
                    }
                },
                ProcessEvent::Fault(reason) => {
                    failure.get_or_insert(RefinementError::Worker(reason));
                }
                ProcessEvent::Exited {
                    status,
                    cancellation_escalated,
                } => {
                    if (!status.success() || cancellation_escalated) && interruption.is_none() {
                        failure.get_or_insert_with(|| {
                            RefinementError::Worker(format!("worker exited {status}"))
                        });
                    }
                }
            }
        }
        if !process.is_finished() {
            std::thread::park_timeout(Duration::from_millis(5));
        }
    }
    if let Some(error) = interruption.or(failure) {
        return Err(error);
    }
    check(cancelled, deadline)?;
    let (artifact, runtime, elapsed_millis) =
        completion.ok_or_else(|| protocol("no clean completed emissions"))?;
    let mut snapshot = pinned.snapshot_with_control(
        &WorkspaceRef::new(OUTPUT).map_err(protocol)?,
        &artifact,
        ArtifactLimits::new(maximum_bytes)?,
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
    let mut bytes = Vec::with_capacity(maximum_bytes as usize);
    snapshot
        .by_ref()
        .take(maximum_bytes + 1)
        .read_to_end(&mut bytes)?;
    check(cancelled, deadline)?;
    if bytes.len() as u64 != maximum_bytes {
        return Err(protocol("emission snapshot length"));
    }
    let log_probs: Vec<f64> = bytes
        .chunks_exact(4)
        .map(|value| f64::from(f32::from_le_bytes(value.try_into().expect("whole f32"))))
        .collect();
    if log_probs
        .iter()
        .any(|value| !value.is_finite() || *value > 0.0)
    {
        return Err(protocol("nonfinite or positive model log probability"));
    }
    Ok(RefinementEmissions {
        input_samples,
        frames,
        log_probs,
        runtime,
        elapsed: Duration::from_millis(elapsed_millis),
    })
}
