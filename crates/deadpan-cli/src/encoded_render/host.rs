use std::io::{self, Read, Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_encode::EncodeLimits;
use deadpan_jobs::artifact::{
    ArtifactError, ArtifactLimits, ArtifactWorkspace, HashedArtifactSnapshot, SnapshotInterruption,
};
use deadpan_jobs::process::{
    ProcessEvent, ProcessLimits, ProcessSpec, SupervisedProcess, WorkerProtocol,
};
use deadpan_jobs::{Sha256, WorkspaceRef};
use deadpan_store::render_media::PreparedRenderSnapshot;

use crate::export_picture::ExportPictureContract;
use crate::picture::ProjectPictureSession;
use crate::render_worker::{RenderPictureRequest, RenderWorkerRuntime, document_hash};

use super::protocol::{
    self, EncodedHostMessage, EncodedManifest, EncodedProtocol, EncodedRenderContract,
    EncodedWorkerMessage, EncoderChoice,
};
use super::runtime::EncodingBinding;
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
    binding: Option<EncodingBinding>,
    snapshot: CandidateBytes,
}

enum CandidateBytes {
    Worker(HashedArtifactSnapshot),
    Retained(Box<PreparedRenderSnapshot>),
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

    pub fn encoding_binding(&self) -> Option<&EncodingBinding> {
        self.binding.as_ref()
    }

    pub fn byte_length(&self) -> u64 {
        match &self.snapshot {
            CandidateBytes::Worker(snapshot) => snapshot.declaration().byte_length(),
            CandidateBytes::Retained(snapshot) => snapshot.media().movie().byte_length(),
        }
    }

    pub(super) fn check_live(
        &self,
        cancelled: &AtomicBool,
        deadline: Instant,
    ) -> Result<(), EncodedRenderError> {
        check_control(cancelled, deadline)?;
        if let CandidateBytes::Retained(snapshot) = &self.snapshot {
            snapshot.check_live(cancelled)?;
        }
        Ok(())
    }

    /// A narrow recovery constructor for the sibling job adapter. The opaque
    /// store snapshot supplies fresh private bytes, never decoded-media trust.
    /// Only the independent verifier may create a VerifiedCandidate from this.
    pub(super) fn from_retained(
        contract: ExportPictureContract,
        document_sha256: Sha256,
        manifest: EncodedManifest,
        snapshot: PreparedRenderSnapshot,
        cancelled: &AtomicBool,
        deadline: Instant,
    ) -> Result<Self, EncodedRenderError> {
        check_control(cancelled, deadline)?;
        snapshot.check_live(cancelled)?;
        manifest.validate().map_err(EncodedRenderError::Protocol)?;
        if !manifest.contract.picture.matches(&contract)
            || manifest.document_sha256 != document_sha256
            || manifest.movie.byte_length() != snapshot.media().movie().byte_length()
            || manifest.movie.sha256() != snapshot.media().movie_sha256()
        {
            return Err(EncodedRenderError::Protocol(
                "retained candidate differs from its captured contract or actual byte identity"
                    .into(),
            ));
        }
        let candidate = Self {
            contract,
            document_sha256,
            manifest,
            binding: None,
            snapshot: CandidateBytes::Retained(Box::new(snapshot)),
        };
        candidate.check_live(cancelled, deadline)?;
        Ok(candidate)
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
        self.check_live(cancelled, deadline)?;
        if output.len() > COPY_BUFFER_BYTES || offset > self.byte_length() {
            return Err(EncodedRenderError::Configuration(
                "candidate read exceeds its byte or buffer bounds",
            ));
        }
        let count = usize::try_from(
            (self.byte_length() - offset).min(u64::try_from(output.len()).expect("bounded buffer")),
        )
        .expect("bounded count");
        match &mut self.snapshot {
            CandidateBytes::Worker(snapshot) => {
                snapshot.seek(SeekFrom::Start(offset))?;
                snapshot.read_exact(&mut output[..count])?;
            }
            CandidateBytes::Retained(snapshot) => {
                let observed =
                    snapshot.read_at(offset, &mut output[..count], cancelled, deadline)?;
                if observed != count {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "retained candidate ended before its exact extent",
                    )
                    .into());
                }
            }
        }
        self.check_live(cancelled, deadline)?;
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
        self.check_live(cancelled, deadline)?;
        let mut buffer = [0_u8; COPY_BUFFER_BYTES];
        let mut copied = 0;
        while copied < self.byte_length() {
            let count = self.read_at(copied, &mut buffer, cancelled, deadline)?;
            let mut written = 0;
            while written < count {
                self.check_live(cancelled, deadline)?;
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
            self.check_live(cancelled, deadline)?;
            copied += u64::try_from(count).expect("bounded count");
        }
        self.check_live(cancelled, deadline)?;
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
    progress: impl FnMut(EncodedProgress),
) -> Result<EncodedCandidate, EncodedRenderError> {
    encode_guarded(
        runtime,
        request,
        choice,
        limits,
        cancelled,
        deadline,
        progress,
        || Ok(()),
    )
}

/// A durable job may revoke its owning session independently of user cancel.
/// Poll that ownership with the supervisor so revocation stops owned work.
#[allow(clippy::too_many_arguments)]
pub(super) fn encode_guarded(
    runtime: &RenderWorkerRuntime,
    request: RenderPictureRequest,
    choice: EncoderChoice,
    limits: EncodedWorkerLimits,
    cancelled: &AtomicBool,
    deadline: Instant,
    progress: impl FnMut(EncodedProgress),
    check_owner: impl Fn() -> Result<(), EncodedRenderError>,
) -> Result<EncodedCandidate, EncodedRenderError> {
    encode_with_binding_guarded(
        runtime,
        request,
        choice,
        None,
        limits,
        cancelled,
        deadline,
        progress,
        check_owner,
    )
}

/// Only a live qualified encoder may select this runtime-bound worker path.
#[allow(clippy::too_many_arguments)]
pub(super) fn encode_bound_guarded(
    runtime: &RenderWorkerRuntime,
    request: RenderPictureRequest,
    choice: EncoderChoice,
    binding: EncodingBinding,
    limits: EncodedWorkerLimits,
    cancelled: &AtomicBool,
    deadline: Instant,
    progress: impl FnMut(EncodedProgress),
    check_owner: impl Fn() -> Result<(), EncodedRenderError>,
) -> Result<EncodedCandidate, EncodedRenderError> {
    encode_with_binding_guarded(
        runtime,
        request,
        choice,
        Some(binding),
        limits,
        cancelled,
        deadline,
        progress,
        check_owner,
    )
}

#[allow(clippy::too_many_arguments)]
fn encode_with_binding_guarded(
    runtime: &RenderWorkerRuntime,
    request: RenderPictureRequest,
    choice: EncoderChoice,
    binding: Option<EncodingBinding>,
    limits: EncodedWorkerLimits,
    cancelled: &AtomicBool,
    deadline: Instant,
    mut progress: impl FnMut(EncodedProgress),
    check_owner: impl Fn() -> Result<(), EncodedRenderError>,
) -> Result<EncodedCandidate, EncodedRenderError> {
    let RenderPictureRequest {
        package,
        revision,
        range,
        identity,
        cancellation_token,
    } = request;
    check_control(cancelled, deadline)?;
    check_owner()?;
    limits.encode.validate()?;
    let package = std::fs::canonicalize(package)?;
    if package.to_str().is_none() {
        return Err(EncodedRenderError::Configuration(
            "worker package path must be UTF-8",
        ));
    }
    let pictures = ProjectPictureSession::open_revision(&package, &revision, range, cancelled);
    check_control(cancelled, deadline)?;
    check_owner()?;
    let pictures = pictures?;
    let contract = ExportPictureContract::capture(&pictures)?;
    let evidence = EncodedRenderContract::from_contract(&contract, choice);
    let native = evidence
        .native_contract()
        .map_err(EncodedRenderError::Protocol)?;
    limits.encode.validate_for(&native)?;
    if let Some(binding) = &binding {
        binding
            .validate_for(&evidence)
            .map_err(EncodedRenderError::Protocol)?;
    }
    let document_sha256 = document_hash(pictures.document(), cancelled, deadline);
    check_control(cancelled, deadline)?;
    let document_sha256 = document_sha256?;
    drop(pictures);
    check_control(cancelled, deadline)?;
    check_owner()?;

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
        binding: binding.clone().map(Box::new),
        document_sha256: document_sha256.clone(),
        output_scope: output_scope.clone(),
        limits: limits.encode,
        timeout_millis,
    };
    let mut arguments = runtime.arguments.clone();
    arguments.push(PRIVATE_WORKER_ARGUMENT.into());
    arguments.push(package.into_os_string());
    check_owner()?;
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
    let result = (|| {
        let mut completion = None;
        let mut failure = None;
        let mut was_cancelled = false;
        let mut last_progress = (0, 0);
        while !process.is_finished() {
            let now = Instant::now();
            if failure.is_none()
                && let Err(error) = check_owner()
            {
                failure = Some(error);
                if let Err(error) = process.request_cancel(now) {
                    return Err(invalidate_report(failure, error.into()));
                }
            }
            if cancelled.load(Ordering::Acquire) && !was_cancelled && failure.is_none() {
                was_cancelled = true;
                if let Err(error) = process.request_cancel(now) {
                    return Err(invalidate_report(failure, error.into()));
                }
            }
            if now >= deadline {
                if let Err(error) = process.request_cancel(now) {
                    return Err(invalidate_report(failure, error.into()));
                }
                return Err(invalidate_report(
                    failure,
                    if was_cancelled {
                        EncodedRenderError::Cancelled
                    } else {
                        EncodedRenderError::Deadline
                    },
                ));
            }
            let events = match process.poll(now) {
                Ok(events) => events,
                Err(error) => return Err(invalidate_report(failure, error.into())),
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
                                    return Err(invalidate_report(failure, error.into()));
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
                        EncodedWorkerMessage::Completed {
                            manifest, binding, ..
                        } => completion = Some((*manifest, binding.map(|value| *value))),
                        EncodedWorkerMessage::Failed {
                            failure: reported, ..
                        } => {
                            failure.get_or_insert(EncodedRenderError::WorkerFailure(reported));
                        }
                        EncodedWorkerMessage::Cancelled { .. } => was_cancelled = true,
                    },
                    ProcessEvent::Fault(reason) => {
                        failure = Some(supervision_fault(failure.take(), reason));
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
            return Err(match check_control(cancelled, deadline) {
                Ok(()) => error,
                Err(control) => invalidate_report(Some(error), control),
            });
        }
        if was_cancelled {
            return Err(EncodedRenderError::Cancelled);
        }
        check_control(cancelled, deadline)?;
        check_owner()?;
        let (manifest, completed_binding) = completion.ok_or_else(|| {
            EncodedRenderError::Protocol("no clean completed encoded manifest".into())
        })?;
        // Keep admission explicit even though the protocol adapter also binds the
        // response before the supervisor releases completion after clean teardown.
        if manifest.contract != evidence
            || manifest.document_sha256 != document_sha256
            || completed_binding != binding
        {
            return Err(EncodedRenderError::Protocol(
                "completed output changed the captured contract".into(),
            ));
        }
        manifest
            .validate_for(limits.encode)
            .map_err(EncodedRenderError::Protocol)?;
        let mut owner_failure = None;
        let snapshot = pinned
            .snapshot_with_control(
                &output_scope,
                &manifest.movie,
                ArtifactLimits::new(limits.encode.maximum_output_bytes)?,
                || {
                    if let Err(error) = check_owner() {
                        owner_failure = Some(error);
                        return Err(SnapshotInterruption::Cancelled);
                    }
                    if cancelled.load(Ordering::Acquire) {
                        return Err(SnapshotInterruption::Cancelled);
                    }
                    if Instant::now() >= deadline {
                        return Err(SnapshotInterruption::Deadline);
                    }
                    Ok(())
                },
            )
            .map_err(artifact_failure);
        if let Some(error) = owner_failure {
            return Err(error);
        }
        let snapshot = snapshot?;
        check_control(cancelled, deadline)?;
        check_owner()?;
        Ok(EncodedCandidate {
            contract,
            document_sha256,
            manifest,
            binding,
            snapshot: CandidateBytes::Worker(snapshot),
        })
    })();
    finish_owned_result(&mut process, result)
}

/// Centralize every post-spawn return so no error relies on Drop as evidence.
pub(super) fn finish_owned_result<T, P: WorkerProtocol>(
    process: &mut SupervisedProcess<P>,
    result: Result<T, EncodedRenderError>,
) -> Result<T, EncodedRenderError> {
    let cleanup = process
        .finish_owned_work(Instant::now() + Duration::from_secs(2))
        .and_then(|stopped| stopped.require_membership());
    match cleanup {
        Ok(stopped) if stopped.pump_panicked() => Err(supervision_fault(
            result.err(),
            "worker I/O pump panicked before finalization".into(),
        )),
        Ok(_) => result,
        Err(cleanup) => Err(EncodedRenderError::CleanupUnconfirmed {
            primary: Box::new(result.err().unwrap_or_else(|| {
                EncodedRenderError::Worker("worker completed without confirmed teardown".into())
            })),
            cleanup,
        }),
    }
}

fn supervision_fault(primary: Option<EncodedRenderError>, fault: String) -> EncodedRenderError {
    invalidate_report(primary, EncodedRenderError::Worker(fault))
}

pub(super) fn invalidate_report(
    primary: Option<EncodedRenderError>,
    fault: EncodedRenderError,
) -> EncodedRenderError {
    match primary {
        Some(primary @ EncodedRenderError::WorkerFailure(_)) => EncodedRenderError::WorkerFault {
            primary: Box::new(primary),
            fault: fault.to_string(),
        },
        Some(other) => other,
        None => fault,
    }
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
