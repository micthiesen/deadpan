use std::{
    fs::File,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use deadpan_jobs::process::{ProcessEvent, ProcessLimits, ProcessSpec, SupervisedProcess};
use rustix::fs::{CWD, FileType, Mode, OFlags, fstat, openat};

use super::{
    PRIVATE_WORKER_ARGUMENT, VerificationFailure, VerificationProgress, VerificationReport,
    VerificationRequest, VerifiedCandidate,
    protocol::{self, HostMessage, VerificationProtocol, WorkerMessage},
};
use crate::{
    encoded_render::{EncodedCandidate, EncodedRenderError, check_control},
    render_worker::RenderWorkerRuntime,
};

/// Independent software decode and exact-clock verification of private bytes.
/// Run off UI/audio threads. One deadline includes staging, inspection and clean
/// teardown. No destination or project writer is opened.
pub fn verify(
    runtime: &RenderWorkerRuntime,
    mut candidate: EncodedCandidate,
    request: VerificationRequest,
    cancelled: &AtomicBool,
    deadline: Instant,
    progress: impl FnMut(VerificationProgress),
) -> Result<VerifiedCandidate, Box<VerificationFailure>> {
    let verification_identity = request.identity.clone();
    match inspect(
        runtime,
        &mut candidate,
        request,
        cancelled,
        deadline,
        progress,
    ) {
        Ok(report) => Ok(VerifiedCandidate {
            candidate,
            report,
            verification_identity,
        }),
        Err(error) => Err(Box::new(VerificationFailure { error, candidate })),
    }
}

fn inspect(
    runtime: &RenderWorkerRuntime,
    candidate: &mut EncodedCandidate,
    request: VerificationRequest,
    cancelled: &AtomicBool,
    deadline: Instant,
    mut progress: impl FnMut(VerificationProgress),
) -> Result<VerificationReport, EncodedRenderError> {
    check_control(cancelled, deadline)?;
    request
        .limits
        .validate()
        .map_err(EncodedRenderError::Protocol)?;
    let wire = |timeout_millis| HostMessage::Inspect {
        protocol: protocol::VERSION,
        identity: request.identity.clone(),
        cancellation_token: request.cancellation_token.clone(),
        manifest: Box::new(candidate.manifest().clone()),
        limits: request.limits,
        timeout_millis,
    };
    wire(1).validate().map_err(EncodedRenderError::Protocol)?;
    let workspace = tempfile::Builder::new()
        .prefix("deadpan-verify-")
        .tempdir()?;
    std::fs::create_dir(workspace.path().join("input"))?;
    let mut input = open_input(workspace.path(), true)?;
    candidate.copy_to(&mut input, cancelled, deadline)?;
    input.sync_all()?;
    drop(input);
    check_control(cancelled, deadline)?;
    let remaining = deadline.saturating_duration_since(Instant::now());
    let timeout_millis = u64::try_from(remaining.as_millis())
        .map_err(|_| EncodedRenderError::Configuration("verification timeout overflow"))?;
    if timeout_millis == 0 {
        return Err(EncodedRenderError::Deadline);
    }
    let message = HostMessage::Inspect {
        protocol: protocol::VERSION,
        identity: request.identity,
        cancellation_token: request.cancellation_token,
        manifest: Box::new(candidate.manifest().clone()),
        limits: request.limits,
        timeout_millis,
    };
    let mut arguments = runtime.arguments.clone();
    arguments.push(PRIVATE_WORKER_ARGUMENT.into());
    let mut child = SupervisedProcess::<VerificationProtocol>::spawn(
        ProcessSpec {
            executable: runtime.executable.clone(),
            arguments,
            environment: runtime.environment.clone(),
            workspace: workspace.path().to_path_buf(),
            limits: ProcessLimits {
                maximum_duration: remaining,
                cancellation_grace: Duration::from_millis(500).min(remaining),
                exit_grace: Duration::from_secs(2).min(remaining),
            },
        },
        message,
    )?;
    let mut failure = None;
    let mut completion = None;
    let mut was_cancelled = false;
    let mut previous: Option<VerificationProgress> = None;
    while !child.is_finished() {
        let now = Instant::now();
        if failure.is_none()
            && let Err(error) = candidate.check_live(cancelled, deadline)
        {
            failure = Some(error);
            if let Err(error) = child.request_cancel(now) {
                return Err(failure.unwrap_or_else(|| error.into()));
            }
        }
        if cancelled.load(Ordering::Acquire) && !was_cancelled && failure.is_none() {
            was_cancelled = true;
            if let Err(error) = child.request_cancel(now) {
                return Err(failure.unwrap_or_else(|| error.into()));
            }
        }
        if now >= deadline {
            let _ = child.request_cancel(now);
            return Err(failure.unwrap_or(if was_cancelled {
                EncodedRenderError::Cancelled
            } else {
                EncodedRenderError::Deadline
            }));
        }
        let events = match child.poll(now) {
            Ok(events) => events,
            Err(error) => return Err(failure.unwrap_or_else(|| error.into())),
        };
        let mut latest = None;
        for event in events {
            match event {
                ProcessEvent::Message(message) => match *message {
                    WorkerMessage::Progress {
                        progress: update, ..
                    } => {
                        if previous.is_some_and(|old| {
                            update.stage < old.stage
                                || (update.stage == old.stage && update.completed < old.completed)
                        }) {
                            failure.get_or_insert_with(|| {
                                EncodedRenderError::Protocol(
                                    "verification progress moved backward".into(),
                                )
                            });
                            if let Err(error) = child.request_cancel(now) {
                                return Err(failure.unwrap_or_else(|| error.into()));
                            }
                        }
                        previous = Some(update);
                        latest = Some(update);
                    }
                    WorkerMessage::Completed { report, .. } => completion = Some(*report),
                    WorkerMessage::Failed { diagnostic, .. } => {
                        failure.get_or_insert_with(|| {
                            EncodedRenderError::Worker(diagnostic.as_str().into())
                        });
                    }
                    WorkerMessage::Cancelled { .. } => was_cancelled = true,
                },
                ProcessEvent::Fault(reason) => {
                    failure.get_or_insert(EncodedRenderError::Worker(reason));
                }
                ProcessEvent::Exited {
                    status,
                    cancellation_escalated,
                } => {
                    if (!status.success() || cancellation_escalated) && !was_cancelled {
                        failure.get_or_insert_with(|| {
                            EncodedRenderError::Worker(format!(
                                "verification worker exited {status}"
                            ))
                        });
                    }
                }
            }
        }
        if failure.is_none()
            && !was_cancelled
            && let Some(update) = latest
        {
            progress(update);
        }
        if !child.is_finished() {
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
    let report = completion.ok_or_else(|| {
        EncodedRenderError::Protocol("no clean completed verification report".into())
    })?;
    report
        .validate(request.limits)
        .map_err(EncodedRenderError::Protocol)?;
    protocol::bind(&report, candidate.manifest()).map_err(EncodedRenderError::Protocol)?;
    candidate.check_live(cancelled, deadline)?;
    Ok(report)
}

pub(super) fn open_input(workspace: &Path, create: bool) -> Result<File, std::io::Error> {
    let directory_flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let root = openat(CWD, workspace, directory_flags, Mode::empty())?;
    let root_stat = fstat(&root)?;
    let input = openat(&root, "input", directory_flags, Mode::empty())?;
    let input_stat = fstat(&input)?;
    if root_stat.st_uid != input_stat.st_uid || root_stat.st_dev != input_stat.st_dev {
        return Err(std::io::Error::other(
            "verification input directory changed owner or filesystem",
        ));
    }
    let flags = if create {
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL
    } else {
        OFlags::RDONLY
    };
    let file = openat(
        &input,
        "movie.mp4",
        flags | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::RUSR | Mode::WUSR,
    )?;
    let metadata = fstat(&file)?;
    if FileType::from_raw_mode(metadata.st_mode) != FileType::RegularFile
        || metadata.st_nlink != 1
        || metadata.st_dev != input_stat.st_dev
        || metadata.st_uid != input_stat.st_uid
    {
        return Err(std::io::Error::other(
            "verification input is not a contained regular singleton",
        ));
    }
    Ok(File::from(file))
}
