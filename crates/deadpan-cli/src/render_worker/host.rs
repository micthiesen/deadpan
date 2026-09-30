use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_core::{FrameRange, RevisionId};
use deadpan_jobs::artifact::{
    ArtifactError, ArtifactLimits, ArtifactWorkspace, HashedArtifactSnapshot, SnapshotInterruption,
};
use deadpan_jobs::process::{ProcessEvent, ProcessLimits, ProcessSpec, SupervisedProcess};
use deadpan_jobs::{CancellationToken, Sha256, WorkspaceRef};

use crate::export_picture::{ExportPictureContract, OutputFrameOrdinal, OutputFrameTiming};
use crate::picture::ProjectPictureSession;

use super::protocol::{
    self, RenderContract, RenderHostMessage, RenderIdentity, RenderManifest, RenderProtocol,
    RenderWorkerMessage,
};
use super::{PRIVATE_WORKER_ARGUMENT, RenderWorkerError, check_control, document_sha256};

/// Selected by the trusted application host. Authored project data and worker
/// messages never choose the executable, arguments or inherited environment.
#[derive(Debug, Clone)]
pub struct RenderWorkerRuntime {
    pub executable: PathBuf,
    /// For example `--headless` when using the native app's known executable.
    pub arguments: Vec<OsString>,
    pub environment: BTreeMap<OsString, OsString>,
}

#[derive(Debug, Clone, Copy)]
pub struct RenderWorkerLimits {
    pub maximum_output_bytes: u64,
    pub cancellation_grace: Duration,
    pub exit_grace: Duration,
}

#[derive(Debug, Clone)]
pub struct RenderPictureRequest {
    pub package: PathBuf,
    pub revision: RevisionId,
    pub range: Option<FrameRange>,
    pub identity: RenderIdentity,
    pub cancellation_token: CancellationToken,
}

impl Default for RenderWorkerLimits {
    fn default() -> Self {
        Self {
            maximum_output_bytes: protocol::MAX_PICTURE_BYTES,
            cancellation_grace: Duration::from_millis(500),
            exit_grace: Duration::from_secs(2),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RenderProgress {
    pub completed_frames: u64,
    pub total_frames: u64,
}

/// Private immutable bytes admitted after clean child/group/pipe teardown and
/// host hash, byte-layout and code-range checks. This is raw SDR picture input,
/// not encoded-media verification or permission to publish an export.
pub struct PreparedPictureRange {
    contract: ExportPictureContract,
    document_sha256: Sha256,
    manifest: RenderManifest,
    snapshot: HashedArtifactSnapshot,
    frame_bytes: u64,
}

impl PreparedPictureRange {
    pub fn contract(&self) -> &ExportPictureContract {
        &self.contract
    }
    pub fn document_sha256(&self) -> &Sha256 {
        &self.document_sha256
    }
    pub fn manifest(&self) -> &RenderManifest {
        &self.manifest
    }
    pub const fn frame_bytes(&self) -> u64 {
        self.frame_bytes
    }

    /// Fill the caller's exactly sized frame buffer. The owned snapshot remains
    /// valid after the process workspace and original package paths disappear.
    pub fn read_frame(
        &mut self,
        ordinal: OutputFrameOrdinal,
        output: &mut [u8],
    ) -> Result<OutputFrameTiming, RenderWorkerError> {
        let timing = self.contract.timing(ordinal)?;
        if u64::try_from(output.len()).ok() != Some(self.frame_bytes) {
            return Err(RenderWorkerError::Configuration(
                "frame buffer has the wrong length",
            ));
        }
        let offset = ordinal
            .0
            .checked_mul(self.frame_bytes)
            .ok_or(RenderWorkerError::Configuration("frame offset overflow"))?;
        self.snapshot.seek(SeekFrom::Start(offset))?;
        self.snapshot.read_exact(output)?;
        Ok(timing)
    }
}

/// Run on the host's job/preparation service, never UI/audio threads. Preflight
/// and validation perform bounded local I/O; the child alone owns media decode,
/// GPU composition and readback. One caller deadline spans preflight, process
/// execution, clean teardown and artifact validation. `progress` must be cheap
/// and nonblocking so the service can continue enforcing that deadline.
pub fn prepare(
    runtime: &RenderWorkerRuntime,
    request: RenderPictureRequest,
    limits: RenderWorkerLimits,
    cancelled: &AtomicBool,
    deadline: Instant,
    mut progress: impl FnMut(RenderProgress),
) -> Result<PreparedPictureRange, RenderWorkerError> {
    let RenderPictureRequest {
        package,
        revision,
        range,
        identity,
        cancellation_token,
    } = request;
    check_control(cancelled, deadline)?;
    if limits.maximum_output_bytes == 0 || limits.maximum_output_bytes > protocol::MAX_PICTURE_BYTES
    {
        return Err(RenderWorkerError::Configuration(
            "invalid raw picture byte budget",
        ));
    }
    let package = std::fs::canonicalize(package)?;
    if package.to_str().is_none() {
        return Err(RenderWorkerError::Configuration(
            "worker package path must be UTF-8",
        ));
    }
    let pictures = ProjectPictureSession::open_revision(&package, &revision, range, cancelled);
    check_control(cancelled, deadline)?;
    let pictures = pictures?;
    let contract = ExportPictureContract::capture(&pictures)?;
    let evidence = RenderContract::from_contract(&contract);
    let frame_bytes = evidence
        .frame_bytes()
        .map_err(RenderWorkerError::Protocol)?;
    let expected_bytes = evidence
        .total_bytes()
        .map_err(RenderWorkerError::Protocol)?;
    if expected_bytes > limits.maximum_output_bytes {
        return Err(RenderWorkerError::Configuration(
            "captured picture range exceeds raw byte budget",
        ));
    }
    let document_hash = document_sha256(&pictures, cancelled, deadline)?;
    drop(pictures);
    check_control(cancelled, deadline)?;
    let workspace = tempfile::Builder::new()
        .prefix("deadpan-render-")
        .tempdir()?;
    std::fs::create_dir(workspace.path().join(protocol::OUTPUT_SCOPE))?;
    let pinned = ArtifactWorkspace::open(workspace.path())?;
    let remaining = deadline.saturating_duration_since(Instant::now());
    let timeout_millis = u64::try_from(remaining.as_millis())
        .map_err(|_| RenderWorkerError::Configuration("render timeout overflow"))?;
    if timeout_millis == 0 {
        return Err(RenderWorkerError::Deadline);
    }
    let request = RenderHostMessage::Prepare {
        protocol: protocol::PROTOCOL_VERSION,
        identity,
        cancellation_token,
        contract: Box::new(evidence),
        document_sha256: document_hash.clone(),
        output_scope: WorkspaceRef::new(protocol::OUTPUT_SCOPE)
            .map_err(|error| RenderWorkerError::Protocol(error.to_string()))?,
        maximum_output_bytes: limits.maximum_output_bytes,
        timeout_millis,
    };
    let mut arguments = runtime.arguments.clone();
    arguments.push(PRIVATE_WORKER_ARGUMENT.into());
    arguments.push(package.into_os_string());
    let mut process = SupervisedProcess::<RenderProtocol>::spawn(
        ProcessSpec {
            executable: runtime.executable.clone(),
            arguments,
            environment: runtime.environment.clone(),
            workspace: workspace.path().to_path_buf(),
            limits: ProcessLimits {
                maximum_duration: remaining,
                cancellation_grace: limits.cancellation_grace.min(remaining),
                exit_grace: limits.exit_grace.min(remaining),
            },
        },
        request,
    )?;
    let mut completion = None;
    let mut failure = None;
    let mut was_cancelled = false;
    let mut last_progress = 0;
    while !process.is_finished() {
        let now = Instant::now();
        if cancelled.load(Ordering::Acquire) && !was_cancelled {
            was_cancelled = true;
            process.request_cancel(now)?;
        }
        if now >= deadline {
            process.request_cancel(now)?;
            return Err(if was_cancelled {
                RenderWorkerError::Cancelled
            } else {
                RenderWorkerError::Deadline
            });
        }
        let mut latest_progress = None;
        for event in process.poll(now)? {
            match event {
                ProcessEvent::Message(message) => match *message {
                    RenderWorkerMessage::Progress {
                        completed_frames,
                        total_frames,
                        ..
                    } => {
                        if completed_frames < last_progress {
                            failure.get_or_insert_with(|| {
                                RenderWorkerError::Protocol("render progress moved backward".into())
                            });
                            process.request_cancel(now)?;
                        }
                        last_progress = completed_frames;
                        latest_progress = Some(RenderProgress {
                            completed_frames,
                            total_frames,
                        });
                    }
                    RenderWorkerMessage::Completed { manifest, .. } => completion = Some(*manifest),
                    RenderWorkerMessage::Failed { diagnostic, .. } => {
                        failure.get_or_insert_with(|| {
                            RenderWorkerError::Worker(diagnostic.as_str().to_owned())
                        });
                    }
                    RenderWorkerMessage::Cancelled { .. } => was_cancelled = true,
                },
                ProcessEvent::Fault(reason) => {
                    failure.get_or_insert(RenderWorkerError::Worker(reason));
                }
                ProcessEvent::Exited {
                    status,
                    cancellation_escalated,
                } => {
                    if (!status.success() || cancellation_escalated)
                        && !was_cancelled
                        && failure.is_none()
                    {
                        failure = Some(RenderWorkerError::Worker(format!(
                            "render worker exited {status}"
                        )));
                    }
                }
            }
        }
        if !was_cancelled
            && failure.is_none()
            && let Some(update) = latest_progress
        {
            progress(update);
        }
        if !process.is_finished() {
            std::thread::park_timeout(Duration::from_millis(2));
        }
    }
    if let Some(error) = failure {
        return Err(error);
    }
    if was_cancelled {
        return Err(RenderWorkerError::Cancelled);
    }
    check_control(cancelled, deadline)?;
    let manifest = completion
        .ok_or_else(|| RenderWorkerError::Protocol("no clean completed picture manifest".into()))?;
    // The protocol adapter also compares these claims before releasing its held
    // completion. Keep the host admission boundary explicit and self-contained.
    if !manifest.contract.matches(&contract)
        || manifest.document_sha256 != document_hash
        || manifest.planes.byte_length() != expected_bytes
        || manifest.planes.reference().as_str() != protocol::PICTURE_REF
    {
        return Err(RenderWorkerError::Protocol(
            "completed output changed the captured contract".into(),
        ));
    }
    let mut snapshot = pinned
        .snapshot_with_control(
            &WorkspaceRef::new(protocol::OUTPUT_SCOPE)
                .map_err(|error| RenderWorkerError::Protocol(error.to_string()))?,
            &manifest.planes,
            ArtifactLimits::new(limits.maximum_output_bytes)?,
            || {
                if cancelled.load(Ordering::Acquire) {
                    return Err(SnapshotInterruption::Cancelled);
                }
                if Instant::now() >= deadline {
                    return Err(SnapshotInterruption::Deadline);
                }
                Ok(())
            },
        )
        .map_err(artifact_failure)?;
    validate_planes(&mut snapshot, &contract, cancelled, deadline)?;
    check_control(cancelled, deadline)?;
    Ok(PreparedPictureRange {
        contract,
        document_sha256: document_hash,
        manifest,
        snapshot,
        frame_bytes,
    })
}

fn artifact_failure(error: ArtifactError) -> RenderWorkerError {
    match error {
        ArtifactError::Interrupted(SnapshotInterruption::Cancelled) => RenderWorkerError::Cancelled,
        ArtifactError::Interrupted(SnapshotInterruption::Deadline) => RenderWorkerError::Deadline,
        error => RenderWorkerError::Artifact(error),
    }
}

fn validate_planes(
    snapshot: &mut HashedArtifactSnapshot,
    contract: &ExportPictureContract,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<(), RenderWorkerError> {
    let [width, height] = contract.raster();
    let y_length = u64::from(width) * u64::from(height);
    let mut buffer = [0_u8; 64 * 1024];
    for frame in 0..contract.frame_count() {
        for (plane, length, maximum) in [
            ("Y", y_length, 235),
            ("Cb", y_length / 4, 240),
            ("Cr", y_length / 4, 240),
        ] {
            let mut remaining = length;
            while remaining != 0 {
                check_control(cancelled, deadline)?;
                let count = usize::try_from(
                    remaining.min(u64::try_from(buffer.len()).expect("fixed buffer fits u64")),
                )
                .expect("bounded count fits usize");
                snapshot.read_exact(&mut buffer[..count])?;
                if buffer[..count]
                    .iter()
                    .any(|code| !(16..=maximum).contains(code))
                {
                    return Err(RenderWorkerError::InvalidPixels { frame, plane });
                }
                remaining -= u64::try_from(count).expect("bounded count fits u64");
            }
        }
    }
    check_control(cancelled, deadline)?;
    snapshot.seek(SeekFrom::Start(0))?;
    Ok(())
}

#[cfg(test)]
mod tests;
