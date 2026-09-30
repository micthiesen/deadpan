use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use deadpan_encode::{BFramePolicy, EncodeFailureKind, EncoderMode};
use deadpan_jobs::{
    CancellationToken, Sha256, WorkspaceRef,
    artifact::{ArtifactLimits, ArtifactWorkspace, HashedArtifactSnapshot, SnapshotInterruption},
    process::{ProcessEvent, ProcessSpec, SupervisedProcess},
};
use sha2::{Digest, Sha256 as Hasher};

use super::{
    AdmissionLimits, AdmissionRuntime, EncoderDecision, POLICY_VERSION, PRIVATE_WORKER_ARGUMENT,
    ProbeReport, ProbeSpec, RejectedProbe,
    protocol::{self, HostMessage, ProbeProtocol, WorkerMessage},
};
use crate::{
    encoded_render::{
        EncodedRenderError, check_control,
        host::{finish_owned_result, invalidate_report},
        protocol::{EncodedFailureKind, EncoderChoice},
    },
    render_worker::{RenderWorkerRuntime, protocol::RenderIdentity},
};

#[derive(Debug, Clone)]
pub struct AdmissionRequest {
    pub identity: RenderIdentity,
    pub cancellation_token: CancellationToken,
    pub raster: [u32; 2],
    pub frame_rate: [u32; 2],
}

/// The exact rejected probes are retained even when no usable path is found.
#[derive(Debug)]
pub struct AdmissionFailure {
    pub error: EncodedRenderError,
    pub rejected: Vec<RejectedProbe>,
}
impl std::fmt::Display for AdmissionFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}
impl std::error::Error for AdmissionFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

/// An in-memory admission from fresh execution and private byte verification.
/// No deserializer, project publication capability or durable-job mutation is
/// exposed. A new encoding attempt must obtain a new admission.
pub struct QualifiedEncoder {
    decision: EncoderDecision,
    movie: HashedArtifactSnapshot,
}
impl QualifiedEncoder {
    pub fn decision(&self) -> &EncoderDecision {
        &self.decision
    }
    pub fn choice(&self) -> EncoderChoice {
        self.decision.selected.spec.choice
    }

    /// Retain the actual probe for engineering evidence. The sink stays private;
    /// this cannot create a verified project output or publish a movie.
    pub fn copy_probe_to(
        &mut self,
        sink: &mut impl Write,
        cancelled: &AtomicBool,
        deadline: Instant,
    ) -> Result<u64, EncodedRenderError> {
        check_control(cancelled, deadline)?;
        self.movie.seek(SeekFrom::Start(0))?;
        let mut copied = 0_u64;
        let mut bytes = [0_u8; 64 * 1024];
        loop {
            check_control(cancelled, deadline)?;
            let read = self.movie.read(&mut bytes)?;
            if read == 0 {
                break;
            }
            sink.write_all(&bytes[..read])?;
            copied += u64::try_from(read).expect("bounded read");
        }
        check_control(cancelled, deadline)?;
        if copied != self.decision.selected.manifest.movie.byte_length() {
            return Err(EncodedRenderError::Protocol(
                "private probe extent changed".into(),
            ));
        }
        Ok(copied)
    }
}

/// Probe the actual raster/rate on one bounded preparation worker. Every mode
/// runs in a fresh supervised process; no failed session switches encoders.
/// Cancellation/deadline and cleanup failures always stop selection.
pub fn qualify(
    runtime: &RenderWorkerRuntime,
    request: AdmissionRequest,
    limits: AdmissionLimits,
    cancelled: &AtomicBool,
    deadline: Instant,
    mut progress: impl FnMut(EncoderChoice, u64, u64),
) -> Result<QualifiedEncoder, AdmissionFailure> {
    let mut rejected = Vec::new();
    let result = (|| {
        check_control(cancelled, deadline)?;
        if !runtime.arguments.is_empty() || !runtime.environment.is_empty() {
            return Err(EncodedRenderError::Configuration(
                "automatic admission requires the direct host-selected helper without argument or environment overrides",
            ));
        }
        if limits.process.maximum_duration.is_zero()
            || limits.process.maximum_duration > Duration::from_secs(120)
            || limits.encode.maximum_output_bytes > super::MAX_PROBE_BYTES
            || limits.encode.maximum_packets > super::MAX_PROBE_PACKETS
        {
            return Err(EncodedRenderError::Configuration(
                "automatic admission exceeds its shared time or byte bound",
            ));
        }
        let deadline = deadline.min(Instant::now() + limits.process.maximum_duration);
        let runtime_before = runtime_identity(runtime, cancelled, deadline)?;
        let initial = ProbeSpec {
            raster: request.raster,
            frame_rate: request.frame_rate,
            choice: EncoderChoice {
                mode: EncoderMode::Hardware,
                b_frames: BFramePolicy::TargetTwo,
            },
        };
        let native = initial
            .contract()
            .map_err(EncodedRenderError::Protocol)?
            .native_contract()
            .map_err(EncodedRenderError::Protocol)?;
        let allow_b = native.policy().gop_frames > 2;
        let mut choice = EncoderChoice {
            mode: EncoderMode::Hardware,
            b_frames: if allow_b {
                BFramePolicy::TargetTwo
            } else {
                BFramePolicy::None
            },
        };
        for _ in 0..4 {
            check_control(cancelled, deadline)?;
            let spec = ProbeSpec {
                raster: request.raster,
                frame_rate: request.frame_rate,
                choice,
            };
            let identity = RenderIdentity {
                request_id: request.identity.request_id.clone(),
                attempt_id: deadpan_jobs::AttemptId::new(uuid::Uuid::new_v4().to_string())
                    .map_err(|error| EncodedRenderError::Protocol(error.to_string()))?,
            };
            let attempted = probe(
                runtime,
                &identity,
                &request.cancellation_token,
                &spec,
                limits,
                cancelled,
                deadline,
                |done, total| progress(choice, done, total),
            );
            match attempted {
                Ok((report, movie)) => {
                    let runtime_after = runtime_identity(runtime, cancelled, deadline)?;
                    if runtime_after != runtime_before {
                        return Err(EncodedRenderError::Protocol(
                            "encoder runtime changed during admission".into(),
                        ));
                    }
                    check_control(cancelled, deadline)?;
                    return Ok(QualifiedEncoder {
                        decision: EncoderDecision {
                            policy_version: POLICY_VERSION,
                            identity: request.identity,
                            runtime: runtime_before,
                            rejected: rejected.clone(),
                            selected_identity: identity,
                            selected: report,
                        },
                        movie,
                    });
                }
                Err(error) => {
                    // Preserve the returned teardown evidence. Control is
                    // checked again before a permitted next probe, never used
                    // here to replace an unconfirmed cleanup error.
                    let next = next_choice(choice, &error, allow_b);
                    if let EncodedRenderError::WorkerFailure(failure) = &error {
                        rejected.push(RejectedProbe {
                            identity,
                            spec,
                            failure: failure.clone(),
                        });
                    }
                    match next {
                        Some(next) => choice = next,
                        None => return Err(error),
                    }
                }
            }
        }
        Err(EncodedRenderError::Configuration(
            "no qualified automatic encoder path",
        ))
    })();
    result.map_err(|error| AdmissionFailure { error, rejected })
}

fn next_choice(
    choice: EncoderChoice,
    error: &EncodedRenderError,
    allow_b: bool,
) -> Option<EncoderChoice> {
    let EncodedRenderError::WorkerFailure(failure) = error else {
        return None;
    };
    let kind = match failure.kind {
        EncodedFailureKind::Encoder(kind) => kind,
        _ => return None,
    };
    let software = EncoderChoice {
        mode: EncoderMode::Software,
        b_frames: if allow_b {
            BFramePolicy::TargetTwo
        } else {
            BFramePolicy::None
        },
    };
    match (choice.mode, choice.b_frames, kind) {
        (_, BFramePolicy::TargetTwo, EncodeFailureKind::VideoTimestampOrder) => {
            Some(EncoderChoice {
                b_frames: BFramePolicy::None,
                ..choice
            })
        }
        (EncoderMode::Hardware, _, EncodeFailureKind::EncoderUnavailable)
        | (EncoderMode::Hardware, BFramePolicy::None, EncodeFailureKind::VideoTimestampOrder) => {
            Some(software)
        }
        _ => None,
    }
}

fn runtime_identity(
    runtime: &RenderWorkerRuntime,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<AdmissionRuntime, EncodedRenderError> {
    use rustix::fs::{Mode, OFlags, open};
    if !runtime.executable.is_absolute() {
        return Err(EncodedRenderError::Configuration(
            "probe helper must have an absolute path",
        ));
    }
    let mut file = File::from(
        open(
            &runtime.executable,
            OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
            Mode::empty(),
        )
        .map_err(std::io::Error::from)?,
    );
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > 512 * 1024 * 1024 {
        return Err(EncodedRenderError::Configuration(
            "probe helper is not a bounded regular file",
        ));
    }
    let mut digest = Hasher::new();
    let mut bytes = [0_u8; 64 * 1024];
    let mut read_total = 0_u64;
    loop {
        check_control(cancelled, deadline)?;
        let read = file.read(&mut bytes)?;
        if read == 0 {
            break;
        }
        read_total += u64::try_from(read).expect("bounded read");
        if read_total > metadata.len() {
            return Err(EncodedRenderError::Protocol(
                "probe helper grew while hashing".into(),
            ));
        }
        digest.update(&bytes[..read]);
    }
    if read_total != metadata.len() {
        return Err(EncodedRenderError::Protocol(
            "probe helper changed while hashing".into(),
        ));
    }
    let uname = rustix::system::uname();
    let text = |value: &std::ffi::CStr| -> Result<String, EncodedRenderError> {
        let value = value
            .to_str()
            .map_err(|_| EncodedRenderError::Configuration("runtime identity is not UTF-8"))?;
        if value.is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
            return Err(EncodedRenderError::Configuration(
                "runtime identity exceeds text bounds",
            ));
        }
        Ok(value.to_owned())
    };
    Ok(AdmissionRuntime {
        helper_sha256: Sha256::new(
            digest
                .finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
        )
        .map_err(|error| EncodedRenderError::Protocol(error.to_string()))?,
        helper_bytes: read_total,
        system: text(uname.sysname())?,
        kernel_release: text(uname.release())?,
        kernel_build: text(uname.version())?,
        machine: text(uname.machine())?,
    })
}

#[allow(clippy::too_many_arguments)]
fn probe(
    runtime: &RenderWorkerRuntime,
    identity: &RenderIdentity,
    token: &CancellationToken,
    spec: &ProbeSpec,
    limits: AdmissionLimits,
    cancelled: &AtomicBool,
    deadline: Instant,
    mut progress: impl FnMut(u64, u64),
) -> Result<(ProbeReport, HashedArtifactSnapshot), EncodedRenderError> {
    check_control(cancelled, deadline)?;
    let workspace = tempfile::Builder::new()
        .prefix("deadpan-probe-")
        .tempdir()?;
    std::fs::create_dir(workspace.path().join(protocol::OUTPUT_SCOPE))?;
    let pinned = ArtifactWorkspace::open(workspace.path())?;
    let scope = WorkspaceRef::new(protocol::OUTPUT_SCOPE)
        .map_err(|error| EncodedRenderError::Protocol(error.to_string()))?;
    let remaining = deadline.saturating_duration_since(Instant::now());
    let timeout_millis = u64::try_from(remaining.as_millis())
        .map_err(|_| EncodedRenderError::Configuration("probe timeout overflow"))?;
    let wire = HostMessage::Probe {
        protocol: protocol::PROTOCOL_VERSION,
        identity: identity.clone(),
        cancellation_token: token.clone(),
        spec: spec.clone(),
        limits: limits.encode,
        timeout_millis,
    };
    let mut process = SupervisedProcess::<ProbeProtocol>::spawn(
        ProcessSpec {
            executable: runtime.executable.clone(),
            arguments: vec![PRIVATE_WORKER_ARGUMENT.into()],
            environment: runtime.environment.clone(),
            workspace: workspace.path().to_path_buf(),
            limits: deadpan_jobs::process::ProcessLimits {
                maximum_duration: remaining,
                cancellation_grace: limits.process.cancellation_grace.min(remaining),
                exit_grace: limits.process.exit_grace.min(remaining),
            },
        },
        wire,
    )?;
    let result = (|| {
        let mut report = None;
        let mut failure = None;
        let mut was_cancelled = false;
        let mut last_progress = 0;
        while !process.is_finished() {
            let now = Instant::now();
            if cancelled.load(Ordering::Acquire) && !was_cancelled {
                was_cancelled = true;
                if let Err(error) = process.request_cancel(now) {
                    return Err(invalidate_report(failure, error.into()));
                }
            }
            if now >= deadline {
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
            for event in events {
                match event {
                    ProcessEvent::Message(message) => match *message {
                        WorkerMessage::Progress {
                            completed_frames,
                            total_frames,
                            ..
                        } => {
                            if completed_frames < last_progress {
                                failure = Some(invalidate_report(
                                    failure.take(),
                                    EncodedRenderError::Protocol("probe progress regressed".into()),
                                ));
                                if let Err(error) = process.request_cancel(now) {
                                    return Err(invalidate_report(failure, error.into()));
                                }
                            }
                            last_progress = completed_frames;
                            if failure.is_none() && !was_cancelled {
                                progress(completed_frames, total_frames);
                            }
                        }
                        WorkerMessage::Completed {
                            report: completed, ..
                        } => report = Some(*completed),
                        WorkerMessage::Failed {
                            failure: reported, ..
                        } => {
                            failure.get_or_insert(EncodedRenderError::WorkerFailure(reported));
                        }
                        WorkerMessage::Cancelled { .. } => was_cancelled = true,
                    },
                    ProcessEvent::Fault(reason) => {
                        failure = Some(invalidate_report(
                            failure.take(),
                            EncodedRenderError::Worker(reason),
                        ))
                    }
                    ProcessEvent::Exited {
                        status,
                        cancellation_escalated,
                    } => {
                        if (!status.success() || cancellation_escalated)
                            && failure.is_none()
                            && !was_cancelled
                        {
                            failure = Some(EncodedRenderError::Worker(format!(
                                "probe exited: {status}"
                            )));
                        }
                    }
                }
            }
            if !process.is_finished() {
                std::thread::park_timeout(Duration::from_millis(2));
            }
        }
        if let Some(error) = failure {
            return Err(match check_control(cancelled, deadline) {
                Ok(()) if !was_cancelled => error,
                Ok(()) => invalidate_report(Some(error), EncodedRenderError::Cancelled),
                Err(control) => invalidate_report(Some(error), control),
            });
        }
        if was_cancelled {
            return Err(EncodedRenderError::Cancelled);
        }
        check_control(cancelled, deadline)?;
        let report = report
            .ok_or_else(|| EncodedRenderError::Protocol("probe did not complete cleanly".into()))?;
        report
            .validate(limits.encode)
            .map_err(EncodedRenderError::Protocol)?;
        if report.spec != *spec {
            return Err(EncodedRenderError::Protocol(
                "probe changed its captured specification".into(),
            ));
        }
        let snapshot = pinned.snapshot_with_control(
            &scope,
            &report.manifest.movie,
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
        );
        check_control(cancelled, deadline)?;
        Ok((report, snapshot?))
    })();
    finish_owned_result(&mut process, result)
}

#[cfg(test)]
mod tests;
