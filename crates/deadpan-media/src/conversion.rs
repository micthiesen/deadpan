use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_core::{BridgeSamplingMap, GeneratedContentId, GeneratedObjectRef};
use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
use rustix::process::{Pid, WaitId, WaitIdOptions, waitid};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::protocol::{
    BridgeConversionRequest, ContractError, ConversionReport, ConversionRequest, MAX_REPLY_BYTES,
    MAX_REQUEST_BYTES, PROTOCOL_VERSION, WorkerReply, WorkerRequest,
};

const GROUP_CLEANUP_GRACE: Duration = Duration::from_millis(250);

/// SHA-256 of the closed worker artifact supplied by its declared manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputIdentity {
    pub sha256: [u8; 32],
}

#[derive(Debug, Error)]
pub enum ConversionError {
    #[error(transparent)]
    Contract(#[from] ContractError),
    #[error("media conversion I/O: {0}")]
    Io(#[from] io::Error),
    #[error("input bytes do not match their declared length and SHA-256")]
    InputIdentity,
    #[error("media conversion was cancelled")]
    Cancelled,
    #[error("media conversion exceeded its deadline")]
    Deadline,
    #[error("invalid converter response: {0}")]
    Protocol(String),
    #[error("converter failed ({code}): {message}")]
    Worker { code: String, message: String },
}

/// Private validated output. No writable descriptor or filesystem path escapes.
/// Durable publication still requires the store's verified-object transaction.
pub struct CanonicalMedia {
    file: File,
    object: GeneratedObjectRef,
    report: ConversionReport,
}

impl CanonicalMedia {
    pub fn object(&self) -> &GeneratedObjectRef {
        &self.object
    }

    pub fn report(&self) -> &ConversionReport {
        &self.report
    }
}

impl Read for CanonicalMedia {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.file.read(bytes)
    }
}

impl Seek for CanonicalMedia {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.file.seek(position)
    }
}

/// Two verified private masters derived from one immutable native snapshot.
/// This is media validation, not a selected-Ready receipt or authored acceptance.
pub struct CanonicalBridge {
    native: CanonicalMedia,
    sampled: CanonicalMedia,
    sampling: BridgeSamplingMap,
    source_identity: InputIdentity,
}

impl CanonicalBridge {
    pub fn native(&self) -> &CanonicalMedia {
        &self.native
    }

    pub fn sampled(&self) -> &CanonicalMedia {
        &self.sampled
    }

    pub fn sampling(&self) -> &BridgeSamplingMap {
        &self.sampling
    }

    pub fn source_identity(&self) -> InputIdentity {
        self.source_identity
    }

    /// Consume the pair to copy its private readers into verified object storage.
    /// Publication callers still own relevance, cancellation, and acceptance.
    pub fn into_parts(self) -> (CanonicalMedia, CanonicalMedia, BridgeSamplingMap) {
        (self.native, self.sampled, self.sampling)
    }
}

/// Run on a background job service, never the UI or audio callback and never
/// inside a database transaction. `source` must be a finite local snapshot;
/// arbitrary blocking readers cannot be interrupted by this synchronous API.
///
/// `executable` is selected by the host's vetted installation, never by a
/// project, model, or worker. The child inherits no environment or media paths.
/// Process groups provide cleanup, not an operating-system security sandbox.
pub fn canonicalize(
    executable: &Path,
    source: &mut impl Read,
    identity: InputIdentity,
    request: &ConversionRequest,
    cancelled: &AtomicBool,
) -> Result<CanonicalMedia, ConversionError> {
    convert(
        executable,
        source,
        identity,
        &WorkerRequest::Convert(request.clone()),
        cancelled,
    )
}

/// Derive exactly the authored interior frames from the declared native video.
/// Uses the same private snapshot, process, deadline, and output guarantees as
/// [`canonicalize`]. The original sampling map must be retained with acceptance.
pub fn sample_bridge(
    executable: &Path,
    source: &mut impl Read,
    identity: InputIdentity,
    request: &BridgeConversionRequest,
    cancelled: &AtomicBool,
) -> Result<CanonicalMedia, ConversionError> {
    convert(
        executable,
        source,
        identity,
        &WorkerRequest::Bridge(request.clone()),
        cancelled,
    )
}

/// Preserve the native sequence and derive its exact interior sampled master.
/// Copies the input once and applies one hard deadline across copying, both
/// helper processes, independent decode verification, and final object hashing.
/// The byte limits apply per file; scratch files are used sequentially.
/// See [`canonicalize`] for the local-reader and trusted-executable contract.
pub fn canonicalize_bridge(
    executable: &Path,
    source: &mut impl Read,
    identity: InputIdentity,
    request: &BridgeConversionRequest,
    cancelled: &AtomicBool,
) -> Result<CanonicalBridge, ConversionError> {
    request.validate()?;
    let deadline = Deadline {
        end: Instant::now() + Duration::from_millis(request.limits.timeout_ms),
        cancelled,
    };
    deadline.check()?;
    let mut input = snapshot(source, identity, request.input_byte_length, &deadline)?;
    input.rewind()?;
    let native_request = WorkerRequest::Convert(ConversionRequest {
        protocol: PROTOCOL_VERSION,
        video: request.native,
        input_byte_length: request.input_byte_length,
        limits: request.limits,
    });
    let native = convert_snapshot(executable, input.try_clone()?, &native_request, &deadline)?;
    input.rewind()?;
    let sampled = convert_snapshot(
        executable,
        input,
        &WorkerRequest::Bridge(request.clone()),
        &deadline,
    )?;
    if native.report.output_rgb_sha256 != sampled.report.input_rgb_sha256 {
        return Err(ConversionError::Protocol(
            "native and sampled masters decoded different source pixels".into(),
        ));
    }
    deadline.check()?;
    Ok(CanonicalBridge {
        native,
        sampled,
        sampling: request.sampling.clone(),
        source_identity: identity,
    })
}

fn convert(
    executable: &Path,
    source: &mut impl Read,
    identity: InputIdentity,
    request: &WorkerRequest,
    cancelled: &AtomicBool,
) -> Result<CanonicalMedia, ConversionError> {
    request.validate()?;
    let deadline = Deadline {
        end: Instant::now() + Duration::from_millis(request.limits().timeout_ms),
        cancelled,
    };
    deadline.check()?;
    let mut input = snapshot(source, identity, request.input_byte_length(), &deadline)?;
    input.rewind()?;
    convert_snapshot(executable, input, request, &deadline)
}

fn convert_snapshot(
    executable: &Path,
    input: File,
    request: &WorkerRequest,
    deadline: &Deadline<'_>,
) -> Result<CanonicalMedia, ConversionError> {
    deadline.check()?;
    let serialized = serde_json::to_string(request)
        .map_err(|error| ConversionError::Protocol(error.to_string()))?;
    if serialized.len() > MAX_REQUEST_BYTES {
        return Err(ConversionError::Protocol(
            "request exceeded wire budget".into(),
        ));
    }
    let mut output = tempfile::tempfile()?;
    deadline.check()?;
    let child = Command::new(executable)
        .arg(serialized)
        .env_clear()
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::from(input))
        .stdout(Stdio::from(output.try_clone()?))
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()?;
    let mut process = OwnedProcess::new(child);
    let (status, reply) = process.collect(deadline, &output, request.limits().max_output_bytes)?;
    // Every failure discards the anonymous output. A well-formed success from a
    // nonzero exit or an unclosed control pipe cannot publish bytes.
    let reply: WorkerReply = serde_json::from_slice(&reply)
        .map_err(|error| ConversionError::Protocol(error.to_string()))?;
    let report = match reply {
        WorkerReply::Failure { code, message } => {
            if code.is_empty() || code.len() > 128 || message.is_empty() || message.len() > 2048 {
                return Err(ConversionError::Protocol(
                    "invalid failure diagnostic".into(),
                ));
            }
            return Err(ConversionError::Worker { code, message });
        }
        WorkerReply::Success { report } if status.success() => report,
        WorkerReply::Success { .. } => {
            return Err(ConversionError::Protocol(format!(
                "success followed by {status}"
            )));
        }
    };
    report.validate_worker(request)?;
    if output.metadata()?.len() != report.output_bytes {
        return Err(ConversionError::Protocol(
            "output length differs from report".into(),
        ));
    }
    output.rewind()?;
    let digest = hash_output(&mut output, report.output_bytes, deadline)?;
    output.rewind()?;
    let content = GeneratedContentId::new(digest)
        .map_err(|error| ConversionError::Protocol(error.to_string()))?;
    let object = GeneratedObjectRef::new(content, report.output_bytes)
        .map_err(|error| ConversionError::Protocol(error.to_string()))?;
    deadline.check()?;
    Ok(CanonicalMedia {
        file: output,
        object,
        report,
    })
}

pub(crate) struct Deadline<'a> {
    pub(crate) end: Instant,
    pub(crate) cancelled: &'a AtomicBool,
}

impl Deadline<'_> {
    pub(crate) fn check(&self) -> Result<(), ConversionError> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err(ConversionError::Cancelled);
        }
        if Instant::now() >= self.end {
            return Err(ConversionError::Deadline);
        }
        Ok(())
    }
}

pub(crate) fn snapshot(
    source: &mut impl Read,
    identity: InputIdentity,
    length: u64,
    deadline: &Deadline<'_>,
) -> Result<File, ConversionError> {
    let mut file = tempfile::tempfile()?;
    let mut hash = Sha256::new();
    let mut remaining = length;
    let mut buffer = [0u8; 64 * 1024];
    while remaining != 0 {
        deadline.check()?;
        let capacity = usize::try_from(remaining.min(buffer.len() as u64))
            .map_err(|_| ConversionError::InputIdentity)?;
        let count = source.read(&mut buffer[..capacity])?;
        if count == 0 {
            return Err(ConversionError::InputIdentity);
        }
        file.write_all(&buffer[..count])?;
        hash.update(&buffer[..count]);
        remaining -= count as u64;
    }
    deadline.check()?;
    if source.read(&mut buffer[..1])? != 0 || <[u8; 32]>::from(hash.finalize()) != identity.sha256 {
        return Err(ConversionError::InputIdentity);
    }
    Ok(file)
}

fn hash_output(
    file: &mut File,
    length: u64,
    deadline: &Deadline<'_>,
) -> Result<String, ConversionError> {
    let mut hash = blake3::Hasher::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        deadline.check()?;
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total = total
            .checked_add(count as u64)
            .filter(|total| *total <= length)
            .ok_or_else(|| ConversionError::Protocol("output grew during hashing".into()))?;
        hash.update(&buffer[..count]);
    }
    if total != length || file.metadata()?.len() != length {
        return Err(ConversionError::Protocol(
            "output changed during hashing".into(),
        ));
    }
    Ok(hash.finalize().to_hex().to_string())
}

struct OwnedProcess {
    child: Child,
    pid: Pid,
    stopped: bool,
    reap_attempted: bool,
}

impl OwnedProcess {
    fn new(child: Child) -> Self {
        let pid = Pid::from_child(&child);
        Self {
            child,
            pid,
            stopped: false,
            reap_attempted: false,
        }
    }

    fn stop_group(&mut self) -> io::Result<()> {
        if self.stopped {
            return Ok(());
        }
        if self.pid == Pid::INIT {
            return Err(io::Error::other("invalid codec process-group identity"));
        }
        #[cfg(target_os = "macos")]
        deadpan_native_process::terminate_owned_group(
            &self.child,
            Instant::now() + GROUP_CLEANUP_GRACE,
        )?;
        #[cfg(target_os = "linux")]
        {
            deadpan_native_process::signal_owned_group(&self.child)?;
            deadpan_native_process::terminate_owned_leader(
                &self.child,
                Instant::now() + GROUP_CLEANUP_GRACE,
            )?;
        }
        self.stopped = true;
        Ok(())
    }

    fn reap_leader(&mut self) -> io::Result<ExitStatus> {
        if self.reap_attempted {
            return Err(io::Error::other("codec reaping was already attempted"));
        }
        self.reap_attempted = true;
        self.child.wait()
    }

    fn collect(
        &mut self,
        deadline: &Deadline<'_>,
        output: &File,
        max_output_bytes: u64,
    ) -> Result<(ExitStatus, Vec<u8>), ConversionError> {
        let mut pipe = self
            .child
            .stderr
            .take()
            .ok_or_else(|| ConversionError::Protocol("missing control pipe".into()))?;
        let flags = fcntl_getfl(&pipe).map_err(io::Error::from)?;
        fcntl_setfl(&pipe, flags | OFlags::NONBLOCK).map_err(io::Error::from)?;
        let mut bytes = Vec::with_capacity(MAX_REPLY_BYTES);
        let mut eof = false;
        let mut exit = None;
        let mut exited_at = None;
        loop {
            deadline.check()?;
            if !eof {
                eof = drain_control(&mut pipe, &mut bytes, deadline, exited_at)?;
            }
            if output.metadata()?.len() > max_output_bytes {
                return Err(ConversionError::Protocol(
                    "output exceeded byte budget".into(),
                ));
            }
            if exit.is_none()
                && waitid(
                    WaitId::Pid(self.pid),
                    WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
                )
                .map_err(io::Error::from)?
                .is_some()
            {
                // Retain the unreaped leader until group cleanup to prevent PID
                // reuse from signalling an unrelated group. See jobs supervisor.
                let cleanup = self.stop_group();
                deadline.check()?;
                cleanup?;
                // A failed wait may mean ownership was lost. Never retry it
                // from Drop using only the cached successful group cleanup.
                exit = Some(self.reap_leader()?);
                exited_at = Some(Instant::now());
            }
            if let Some(status) = exit
                && eof
            {
                return Ok((status, bytes));
            }
            std::thread::park_timeout(Duration::from_millis(2));
        }
    }
}

fn drain_control(
    pipe: &mut impl Read,
    bytes: &mut Vec<u8>,
    deadline: &Deadline<'_>,
    exited_at: Option<Instant>,
) -> Result<bool, ConversionError> {
    let mut buffer = [0u8; 4096];
    loop {
        deadline.check()?;
        match pipe.read(&mut buffer) {
            Ok(0) => return Ok(true),
            Ok(count) => {
                if count > MAX_REPLY_BYTES.saturating_sub(bytes.len()) {
                    return Err(ConversionError::Protocol(
                        "control reply exceeded byte budget".into(),
                    ));
                }
                bytes.extend_from_slice(&buffer[..count]);
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                // Drain all available bytes and observe EOF before judging pipe
                // liveness. Host scheduling delay is not a surviving writer.
                if exited_at.is_some_and(|at| at.elapsed() > GROUP_CLEANUP_GRACE) {
                    return Err(ConversionError::Protocol(
                        "control pipe stayed open after worker exit".into(),
                    ));
                }
                return Ok(false);
            }
            Err(error) => return Err(error.into()),
        }
    }
}

impl Drop for OwnedProcess {
    fn drop(&mut self) {
        if self.reap_attempted {
            return;
        }
        let can_reap = self.stop_group().is_ok();
        // A failed group inspection must not strand an owned leader. The
        // fallback independently checks ownership and never signals on ECHILD.
        let can_reap = can_reap
            || deadpan_native_process::terminate_owned_leader(
                &self.child,
                Instant::now() + GROUP_CLEANUP_GRACE,
            )
            .is_ok();
        if can_reap {
            let _ = self.reap_leader();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    struct PendingPipe;

    #[test]
    fn failed_reap_is_terminal_even_after_confirmed_group_cleanup() {
        let child = Command::new("/bin/sh")
            .args(["-c", "exit 0"])
            .process_group(0)
            .spawn()
            .unwrap();
        let mut process = OwnedProcess::new(child);
        process.stop_group().unwrap();
        // Simulate a competing reaper after group confirmation. Child's cached
        // status stays empty, so its next wait really receives ECHILD.
        waitid(WaitId::Pid(process.pid), WaitIdOptions::EXITED).unwrap();
        assert_eq!(
            process.reap_leader().unwrap_err().raw_os_error(),
            Some(rustix::io::Errno::CHILD.raw_os_error())
        );
        assert!(process.reap_attempted);
        assert_eq!(
            process.reap_leader().unwrap_err().to_string(),
            "codec reaping was already attempted"
        );
        drop(process);
    }

    impl Read for PendingPipe {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::ErrorKind::WouldBlock.into())
        }
    }

    #[test]
    fn buffered_reply_reaches_eof_before_expired_pipe_grace_is_judged() {
        let cancelled = AtomicBool::new(false);
        let deadline = Deadline {
            end: Instant::now() + Duration::from_secs(1),
            cancelled: &cancelled,
        };
        let reply = vec![b'x'; MAX_REPLY_BYTES];
        let mut bytes = Vec::new();
        assert!(
            drain_control(
                &mut Cursor::new(&reply),
                &mut bytes,
                &deadline,
                Some(Instant::now() - Duration::from_secs(1)),
            )
            .unwrap()
        );
        assert_eq!(bytes, reply);
    }

    #[test]
    fn actually_open_pipe_still_fails_after_grace() {
        let cancelled = AtomicBool::new(false);
        let deadline = Deadline {
            end: Instant::now() + Duration::from_secs(1),
            cancelled: &cancelled,
        };
        assert!(!drain_control(&mut PendingPipe, &mut Vec::new(), &deadline, None).unwrap());
        assert!(matches!(
            drain_control(
                &mut PendingPipe,
                &mut Vec::new(),
                &deadline,
                Some(Instant::now() - Duration::from_secs(1)),
            ),
            Err(ConversionError::Protocol(message)) if message == "control pipe stayed open after worker exit"
        ));
    }

    #[test]
    fn draining_keeps_reply_budget_and_hard_deadline() {
        let cancelled = AtomicBool::new(false);
        let mut deadline = Deadline {
            end: Instant::now() + Duration::from_secs(1),
            cancelled: &cancelled,
        };
        let mut bytes = Vec::new();
        assert!(matches!(
            drain_control(
                &mut Cursor::new(vec![0; MAX_REPLY_BYTES + 1]),
                &mut bytes,
                &deadline,
                None,
            ),
            Err(ConversionError::Protocol(message)) if message == "control reply exceeded byte budget"
        ));
        assert_eq!(bytes.len(), MAX_REPLY_BYTES);
        deadline.end = Instant::now();
        assert!(matches!(
            drain_control(&mut Cursor::new([]), &mut bytes, &deadline, None),
            Err(ConversionError::Deadline)
        ));
        cancelled.store(true, Ordering::Release);
        assert!(matches!(
            drain_control(&mut Cursor::new([]), &mut bytes, &deadline, None),
            Err(ConversionError::Cancelled)
        ));
    }
}
