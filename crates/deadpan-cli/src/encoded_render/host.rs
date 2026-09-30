use std::io::{self, Read, Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_encode::EncodeLimits;
use deadpan_jobs::artifact::{
    ArtifactError, ArtifactLimits, ArtifactWorkspace, HashedArtifactSnapshot, SnapshotInterruption,
};
use deadpan_jobs::process::{ProcessEvent, ProcessLimits, ProcessSpec, SupervisedProcess};
use deadpan_jobs::{Sha256, WorkspaceRef};

use crate::export_picture::ExportPictureContract;
use crate::picture::ProjectPictureSession;
use crate::render_worker::{RenderPictureRequest, RenderWorkerRuntime, document_hash};

use super::protocol::{
    self, EncodedHostMessage, EncodedManifest, EncodedProtocol, EncodedRenderContract,
    EncodedWorkerMessage, EncoderChoice,
};
use super::{EncodedRenderError, PRIVATE_WORKER_ARGUMENT, check_control};

const COPY_BUFFER_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy)]
pub struct EncodedWorkerLimits {
    pub encode: EncodeLimits,
    pub cancellation_grace: Duration,
    pub exit_grace: Duration,
}

impl Default for EncodedWorkerLimits {
    fn default() -> Self {
        Self {
            encode: EncodeLimits::default(),
            cancellation_grace: Duration::from_millis(500),
            exit_grace: Duration::from_secs(2),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct EncodedProgress {
    pub completed_frames: u64,
    pub total_frames: u64,
    pub completed_audio_samples: u64,
    pub total_audio_samples: u64,
}

/// A private candidate copied after clean worker teardown and hash admission.
/// The manifest contains encoder claims. These bytes have not been independently
/// decoded or verified as media, and this object grants no publication authority.
pub struct EncodedCandidate {
    contract: ExportPictureContract,
    document_sha256: Sha256,
    manifest: EncodedManifest,
    snapshot: HashedArtifactSnapshot,
}

impl EncodedCandidate {
    pub fn contract(&self) -> &ExportPictureContract {
        &self.contract
    }

    pub fn document_sha256(&self) -> &Sha256 {
        &self.document_sha256
    }

    pub fn manifest(&self) -> &EncodedManifest {
        &self.manifest
    }

    pub fn byte_length(&self) -> u64 {
        self.snapshot.declaration().byte_length()
    }

    /// Read at most 64 KiB from the owned snapshot. Reading at EOF returns zero;
    /// offsets beyond EOF and larger buffers fail before accessing the file.
    pub fn read_at(
        &mut self,
        offset: u64,
        output: &mut [u8],
        cancelled: &AtomicBool,
        deadline: Instant,
    ) -> Result<usize, EncodedRenderError> {
        check_control(cancelled, deadline)?;
        if output.len() > COPY_BUFFER_BYTES || offset > self.byte_length() {
            return Err(EncodedRenderError::Configuration(
                "candidate read exceeds its byte or buffer bounds",
            ));
        }
        let count = usize::try_from(
            (self.byte_length() - offset).min(u64::try_from(output.len()).expect("bounded buffer")),
        )
        .expect("bounded count");
        self.snapshot.seek(SeekFrom::Start(offset))?;
        self.snapshot.read_exact(&mut output[..count])?;
        check_control(cancelled, deadline)?;
        Ok(count)
    }

    /// Copy from byte zero into a caller-owned sink under its verification
    /// budget. This chooses no destination path and performs no publication.
    /// On interruption the sink may contain a prefix and must remain private.
    pub fn copy_to(
        &mut self,
        output: &mut impl Write,
        cancelled: &AtomicBool,
        deadline: Instant,
    ) -> Result<u64, EncodedRenderError> {
        check_control(cancelled, deadline)?;
        let mut buffer = [0_u8; COPY_BUFFER_BYTES];
        let mut copied = 0;
        while copied < self.byte_length() {
            let count = self.read_at(copied, &mut buffer, cancelled, deadline)?;
            let mut written = 0;
            while written < count {
                check_control(cancelled, deadline)?;
                match output.write(&buffer[written..count]) {
                    Ok(0) => {
                        return Err(io::Error::new(
                            io::ErrorKind::WriteZero,
                            "candidate sink accepted no bytes",
                        )
                        .into());
                    }
                    Ok(length) => written += length,
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(error.into()),
                }
            }
            check_control(cancelled, deadline)?;
            copied += u64::try_from(count).expect("bounded count");
        }
        check_control(cancelled, deadline)?;
        Ok(copied)
    }
}

/// Run on a job service, never UI/audio threads. One caller deadline covers
/// capture, process execution, teardown, and the private artifact copy. The
/// callback must be cheap and nonblocking. Progress counts accepted inputs;
/// even complete progress is not encoder drain or finished-file verification.
pub fn encode(
    runtime: &RenderWorkerRuntime,
    request: RenderPictureRequest,
    choice: EncoderChoice,
    limits: EncodedWorkerLimits,
    cancelled: &AtomicBool,
    deadline: Instant,
    mut progress: impl FnMut(EncodedProgress),
) -> Result<EncodedCandidate, EncodedRenderError> {
    let RenderPictureRequest {
        package,
        revision,
        range,
        identity,
        cancellation_token,
    } = request;
    check_control(cancelled, deadline)?;
    limits.encode.validate()?;
    let package = std::fs::canonicalize(package)?;
    if package.to_str().is_none() {
        return Err(EncodedRenderError::Configuration(
            "worker package path must be UTF-8",
        ));
    }
    let pictures = ProjectPictureSession::open_revision(&package, &revision, range, cancelled);
    check_control(cancelled, deadline)?;
    let pictures = pictures?;
    let contract = ExportPictureContract::capture(&pictures)?;
    let evidence = EncodedRenderContract::from_contract(&contract, choice);
    let native = evidence
        .native_contract()
        .map_err(EncodedRenderError::Protocol)?;
    limits.encode.validate_for(&native)?;
    let document_sha256 = document_hash(pictures.document(), cancelled, deadline);
    check_control(cancelled, deadline)?;
    let document_sha256 = document_sha256?;
    drop(pictures);
    check_control(cancelled, deadline)?;

    let workspace = tempfile::Builder::new()
        .prefix("deadpan-encoded-")
        .tempdir()?;
    std::fs::create_dir(workspace.path().join(protocol::OUTPUT_SCOPE))?;
    let pinned = ArtifactWorkspace::open(workspace.path())?;
    let output_scope = WorkspaceRef::new(protocol::OUTPUT_SCOPE)
        .map_err(|error| EncodedRenderError::Protocol(error.to_string()))?;
    let remaining = deadline.saturating_duration_since(Instant::now());
    let timeout_millis = u64::try_from(remaining.as_millis())
        .map_err(|_| EncodedRenderError::Configuration("encode timeout overflow"))?;
    if timeout_millis == 0 {
        return Err(EncodedRenderError::Deadline);
    }
    let wire_request = EncodedHostMessage::Prepare {
        protocol: protocol::PROTOCOL_VERSION,
        identity,
        cancellation_token,
        contract: Box::new(evidence.clone()),
        document_sha256: document_sha256.clone(),
        output_scope: output_scope.clone(),
        limits: limits.encode,
        timeout_millis,
    };
    let mut arguments = runtime.arguments.clone();
    arguments.push(PRIVATE_WORKER_ARGUMENT.into());
    arguments.push(package.into_os_string());
    let mut process = SupervisedProcess::<EncodedProtocol>::spawn(
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
        wire_request,
    )?;
    let mut completion = None;
    let mut failure = None;
    let mut was_cancelled = false;
    let mut last_progress = (0, 0);
    while !process.is_finished() {
        let now = Instant::now();
        if cancelled.load(Ordering::Acquire) && !was_cancelled {
            was_cancelled = true;
            if let Err(error) = process.request_cancel(now) {
                return Err(failure.unwrap_or_else(|| error.into()));
            }
        }
        if now >= deadline {
            if let Err(error) = process.request_cancel(now) {
                return Err(failure.unwrap_or_else(|| error.into()));
            }
            return Err(failure.unwrap_or(if was_cancelled {
                EncodedRenderError::Cancelled
            } else {
                EncodedRenderError::Deadline
            }));
        }
        let events = match process.poll(now) {
            Ok(events) => events,
            Err(error) => return Err(failure.unwrap_or_else(|| error.into())),
        };
        let mut latest_progress = None;
        for event in events {
            match event {
                ProcessEvent::Message(message) => match *message {
                    EncodedWorkerMessage::Progress {
                        completed_frames,
                        total_frames,
                        completed_audio_samples,
                        total_audio_samples,
                        ..
                    } => {
                        if completed_frames < last_progress.0
                            || completed_audio_samples < last_progress.1
                        {
                            failure.get_or_insert_with(|| {
                                EncodedRenderError::Protocol(
                                    "encoded progress moved backward".into(),
                                )
                            });
                            if let Err(error) = process.request_cancel(now) {
                                return Err(failure.unwrap_or_else(|| error.into()));
                            }
                        }
                        last_progress = (completed_frames, completed_audio_samples);
                        latest_progress = Some(EncodedProgress {
                            completed_frames,
                            total_frames,
                            completed_audio_samples,
                            total_audio_samples,
                        });
                    }
                    EncodedWorkerMessage::Completed { manifest, .. } => {
                        completion = Some(*manifest)
                    }
                    EncodedWorkerMessage::Failed { diagnostic, .. } => {
                        failure.get_or_insert_with(|| {
                            EncodedRenderError::Worker(diagnostic.as_str().to_owned())
                        });
                    }
                    EncodedWorkerMessage::Cancelled { .. } => was_cancelled = true,
                },
                ProcessEvent::Fault(reason) => {
                    failure.get_or_insert(EncodedRenderError::Worker(reason));
                }
                ProcessEvent::Exited {
                    status,
                    cancellation_escalated,
                } => {
                    if (!status.success() || cancellation_escalated)
                        && !was_cancelled
                        && failure.is_none()
                    {
                        failure = Some(EncodedRenderError::Worker(format!(
                            "encoded worker exited {status}"
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
        return Err(EncodedRenderError::Cancelled);
    }
    check_control(cancelled, deadline)?;
    let manifest = completion.ok_or_else(|| {
        EncodedRenderError::Protocol("no clean completed encoded manifest".into())
    })?;
    // Keep admission explicit even though the protocol adapter also binds the
    // response before the supervisor releases completion after clean teardown.
    if manifest.contract != evidence || manifest.document_sha256 != document_sha256 {
        return Err(EncodedRenderError::Protocol(
            "completed output changed the captured contract".into(),
        ));
    }
    manifest
        .validate_for(limits.encode)
        .map_err(EncodedRenderError::Protocol)?;
    let snapshot = pinned
        .snapshot_with_control(
            &output_scope,
            &manifest.movie,
            ArtifactLimits::new(limits.encode.maximum_output_bytes)?,
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
    check_control(cancelled, deadline)?;
    Ok(EncodedCandidate {
        contract,
        document_sha256,
        manifest,
        snapshot,
    })
}

fn artifact_failure(error: ArtifactError) -> EncodedRenderError {
    match error {
        ArtifactError::Interrupted(SnapshotInterruption::Cancelled) => {
            EncodedRenderError::Cancelled
        }
        ArtifactError::Interrupted(SnapshotInterruption::Deadline) => EncodedRenderError::Deadline,
        error => EncodedRenderError::Artifact(error),
    }
}

#[cfg(test)]
mod tests;
