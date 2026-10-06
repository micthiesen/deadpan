//! Host side of the worker's preview proxy encoding.

use std::fs::File;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use super::{ConversionError, Deadline, OwnedProcess, Watch};
use crate::protocol::{MAX_REPLY_BYTES, MAX_REQUEST_BYTES};
use crate::proxy::{
    MAX_PROXY_ASSEMBLE_REQUEST_BYTES, PROXY_ARGUMENT, PROXY_ASSEMBLE_ARGUMENT, ProxyAssembleReply,
    ProxyAssembleReport, ProxyAssembleRequest, ProxyReply, ProxyReport, ProxyRequest,
};
use crate::source_input::VerifiedSourceInput;

/// A proxy worker that neither writes output nor sends a heartbeat for this
/// long is stopped. Observed once under heavy concurrent load: VideoToolbox
/// blocked indefinitely while invalidating a failed compression session.
pub const PROXY_STALL_TIMEOUT: Duration = Duration::from_secs(60);

/// Supervision of proxy encoding beyond the request's own deadline.
#[derive(Clone, Copy)]
pub struct ProxyEncodeOptions<'a> {
    /// Stop a worker without progress for this long.
    pub stall: Duration,
    /// While set, the worker's process group is suspended. Paused time never
    /// counts as a stall; the request deadline still runs.
    pub pause: Option<&'a AtomicBool>,
}

impl Default for ProxyEncodeOptions<'_> {
    fn default() -> Self {
        Self {
            stall: PROXY_STALL_TIMEOUT,
            pause: None,
        }
    }
}

/// Run the isolated worker on verified Original bytes and leave the encoded
/// proxy (or the request's range of it) in `output` at the request's output
/// offset, which must be the private regular file's current length. The
/// child receives no path or environment; it lowers its own scheduling
/// priority. The result is unverified: call [`crate::proxy::verify_proxy`]
/// on a complete proxy next.
pub fn encode_proxy(
    executable: &Path,
    input: &VerifiedSourceInput,
    request: &ProxyRequest,
    output: &File,
    cancelled: &AtomicBool,
    options: ProxyEncodeOptions<'_>,
) -> Result<ProxyReport, ConversionError> {
    request.validate()?;
    if options.stall.is_zero() {
        return Err(ConversionError::Protocol(
            "stall period must be positive".into(),
        ));
    }
    let deadline = Deadline {
        end: Instant::now() + Duration::from_millis(request.timeout_ms),
        cancelled,
    };
    deadline.check()?;
    if input.identity().byte_length() != request.input_byte_length {
        return Err(ConversionError::InputIdentity);
    }
    let metadata = output.metadata()?;
    if metadata.len() != request.output_offset || !metadata.is_file() {
        return Err(ConversionError::Protocol(
            "proxy output must be a regular file ending at the output offset".into(),
        ));
    }
    let serialized = serde_json::to_string(request)
        .map_err(|error| ConversionError::Protocol(error.to_string()))?;
    if serialized.len() > MAX_REQUEST_BYTES {
        return Err(ConversionError::Protocol(
            "request exceeded wire budget".into(),
        ));
    }
    let reply = run_worker(
        executable,
        PROXY_ARGUMENT,
        serialized,
        input.decoder_file()?,
        output,
        request
            .output_offset
            .saturating_add(request.max_output_bytes),
        &deadline,
        options,
    )?;
    let (status, bytes) = reply;
    let report = match serde_json::from_slice::<ProxyReply>(&bytes)
        .map_err(|error| ConversionError::Protocol(error.to_string()))?
    {
        ProxyReply::Failure { code, message } => return Err(worker_failure(code, message)),
        ProxyReply::Success { report } if status.success() => report,
        ProxyReply::Success { .. } => {
            return Err(ConversionError::Protocol(format!(
                "success followed by {status}"
            )));
        }
    };
    report.validate_for(request)?;
    if output.metadata()?.len() != request.output_offset.saturating_add(report.output_bytes) {
        return Err(ConversionError::Protocol(
            "output length differs from report".into(),
        ));
    }
    deadline.check()?;
    Ok(report)
}

/// Join encoded proxy ranges into one movie in the isolated worker. `ranges`
/// holds every range at its offset (the worker reads it only); `output` must
/// be an empty private regular file. The worker requires every packet to be
/// an intra picture at exactly its range's expected times, and identical
/// decoder configurations; the result is still unverified: call
/// [`crate::proxy::verify_proxy`] next.
pub fn assemble_proxy(
    executable: &Path,
    ranges: &File,
    request: &ProxyAssembleRequest,
    output: &File,
    cancelled: &AtomicBool,
    options: ProxyEncodeOptions<'_>,
) -> Result<ProxyAssembleReport, ConversionError> {
    request.validate()?;
    let deadline = Deadline {
        end: Instant::now() + Duration::from_millis(request.timeout_ms),
        cancelled,
    };
    deadline.check()?;
    if ranges.metadata()?.len() != request.input_byte_length {
        return Err(ConversionError::InputIdentity);
    }
    let metadata = output.metadata()?;
    if metadata.len() != 0 || !metadata.is_file() {
        return Err(ConversionError::Protocol(
            "proxy output must be an empty regular file".into(),
        ));
    }
    let serialized = serde_json::to_string(request)
        .map_err(|error| ConversionError::Protocol(error.to_string()))?;
    if serialized.len() > MAX_PROXY_ASSEMBLE_REQUEST_BYTES {
        return Err(ConversionError::Protocol(
            "request exceeded wire budget".into(),
        ));
    }
    let reply = run_worker(
        executable,
        PROXY_ASSEMBLE_ARGUMENT,
        serialized,
        ranges.try_clone()?,
        output,
        request.max_output_bytes,
        &deadline,
        options,
    )?;
    let (status, bytes) = reply;
    let report = match serde_json::from_slice::<ProxyAssembleReply>(&bytes)
        .map_err(|error| ConversionError::Protocol(error.to_string()))?
    {
        ProxyAssembleReply::Failure { code, message } => {
            return Err(worker_failure(code, message));
        }
        ProxyAssembleReply::Success { report } if status.success() => report,
        ProxyAssembleReply::Success { .. } => {
            return Err(ConversionError::Protocol(format!(
                "success followed by {status}"
            )));
        }
    };
    report.validate_for(request)?;
    if output.metadata()?.len() != report.output_bytes {
        return Err(ConversionError::Protocol(
            "output length differs from report".into(),
        ));
    }
    deadline.check()?;
    Ok(report)
}

/// Code of a worker stopped by SIGKILL or SIGTERM from outside, for example
/// by the system under memory pressure or by a user. Not a verdict about the
/// media; a resumable build keeps its completed ranges.
pub const WORKER_TERMINATED: &str = "worker_terminated";

fn worker_failure(code: String, message: String) -> ConversionError {
    if code.is_empty() || code.len() > 128 || message.is_empty() || message.len() > 2048 {
        return ConversionError::Protocol("invalid failure diagnostic".into());
    }
    ConversionError::Worker { code, message }
}

/// Spawn one proxy worker mode with `input` on stdin and `output` on stdout
/// in its own process group, supervised for deadline, cancellation, output
/// bound, stalls and pause, and return its reply.
#[allow(clippy::too_many_arguments)]
fn run_worker(
    executable: &Path,
    mode: &str,
    serialized: String,
    input: File,
    output: &File,
    max_output_length: u64,
    deadline: &Deadline<'_>,
    options: ProxyEncodeOptions<'_>,
) -> Result<(std::process::ExitStatus, Vec<u8>), ConversionError> {
    use std::os::unix::process::ExitStatusExt;
    let child = deadpan_native_process::spawn(
        Command::new(executable)
            .arg(mode)
            .arg(serialized)
            .env_clear()
            .current_dir(std::env::temp_dir())
            .stdin(Stdio::from(input))
            .stdout(Stdio::from(output.try_clone()?))
            .stderr(Stdio::piped())
            .process_group(0),
    )?;
    let mut process = OwnedProcess::new(child);
    // Output grows with every packet and the worker sends a heartbeat for
    // every decoded picture; a hung VideoToolbox session does neither.
    let watch = Watch {
        stall: options.stall,
        pause: options.pause,
    };
    let (status, reply) = process.collect(deadline, output, max_output_length, Some(&watch))?;
    if reply.len() > MAX_REPLY_BYTES {
        return Err(ConversionError::Protocol("oversized proxy reply".into()));
    }
    if reply.is_empty()
        && let Some(signal) = status.signal()
    {
        let terminated = signal == rustix::process::Signal::KILL.as_raw()
            || signal == rustix::process::Signal::TERM.as_raw();
        return Err(ConversionError::Worker {
            code: if terminated {
                WORKER_TERMINATED
            } else {
                "worker_crashed"
            }
            .into(),
            message: format!("the proxy worker was stopped by signal {signal}"),
        });
    }
    Ok((status, reply))
}

/// Whether a failed attempt may be retried once in a new process: a refused
/// out-of-contract packet or a failed VideoToolbox session (the worker
/// reported and exited without tearing the session down), or a stall whose
/// process group was confirmed gone. An unconfirmed teardown never retries,
/// so two encoders can never write at once.
pub fn retryable(error: &ConversionError) -> bool {
    match error {
        ConversionError::Stalled { torn_down, .. } => *torn_down,
        // The worker reports both before touching the failed session again.
        ConversionError::Worker { code, .. } => {
            matches!(code.as_str(), "invalid_packet" | "encoder_session_failed")
        }
        _ => false,
    }
}

/// Encode with at most one retry. `stage` creates a fresh private output for
/// each attempt (its value is dropped, and so removed, when that attempt
/// fails); the retry's deadline is what remains of the first one's.
pub fn encode_proxy_retrying<S>(
    executable: &Path,
    input: &VerifiedSourceInput,
    request: &ProxyRequest,
    mut stage: impl FnMut() -> Result<(S, File), ConversionError>,
    cancelled: &AtomicBool,
    options: ProxyEncodeOptions<'_>,
) -> Result<(S, ProxyReport), ConversionError> {
    let started = Instant::now();
    let (first, output) = stage()?;
    match encode_proxy(executable, input, request, &output, cancelled, options) {
        Ok(report) => Ok((first, report)),
        Err(error) if retryable(&error) => {
            drop(output);
            drop(first);
            let remaining = Duration::from_millis(request.timeout_ms)
                .checked_sub(started.elapsed())
                .filter(|left| !left.is_zero())
                .ok_or(ConversionError::Deadline)?;
            let retry = ProxyRequest {
                timeout_ms: u64::try_from(remaining.as_millis())
                    .unwrap_or(u64::MAX)
                    .max(1),
                ..*request
            };
            let (second, output) = stage()?;
            let report = encode_proxy(executable, input, &retry, &output, cancelled, options)?;
            Ok((second, report))
        }
        Err(error) => Err(error),
    }
}
