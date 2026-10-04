//! Host side of the worker's MP4 stream-copy assembly.
//!
//! Remote services commonly deliver their best picture and sound as separate
//! single-stream MP4 files. The isolated media worker copies one H.264 and one
//! AAC stream, packet for packet, into one progressive MP4 without decoding
//! or re-encoding. The result is an ordinary candidate original: its retention,
//! stream qualification and admission still happen through the normal source
//! path, which decodes it independently.

use std::fs::File;
use std::io::{self, Read, Seek, Write};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use super::{ConversionError, Deadline, OwnedProcess};
use crate::protocol::{
    MAX_REMUX_OUTPUT_BYTES, MAX_REPLY_BYTES, MAX_REQUEST_BYTES, REMUX_ARGUMENT,
    REMUX_PROTOCOL_VERSION, RemuxReply, RemuxReport, RemuxRequest,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemuxLimits {
    pub max_output_bytes: u64,
    pub timeout: Duration,
}

impl Default for RemuxLimits {
    fn default() -> Self {
        Self {
            max_output_bytes: MAX_REMUX_OUTPUT_BYTES,
            timeout: Duration::from_secs(30 * 60),
        }
    }
}

/// Copy `video` then `audio` into one private input, run the worker and leave
/// the assembled MP4 in `output`, which must be an empty, readable and writable
/// regular file: the worker re-demuxes it to verify timing.
pub fn remux_av(
    executable: &Path,
    video: &mut File,
    audio: &mut File,
    output: &File,
    limits: RemuxLimits,
    cancelled: &AtomicBool,
) -> Result<RemuxReport, ConversionError> {
    let deadline = Deadline {
        end: Instant::now() + limits.timeout,
        cancelled,
    };
    let video_byte_length = video.metadata()?.len();
    let audio_byte_length = audio.metadata()?.len();
    let mut input = tempfile::tempfile()?;
    copy_exact(video, &mut input, video_byte_length, &deadline)?;
    copy_exact(audio, &mut input, audio_byte_length, &deadline)?;
    remux_joined(
        executable,
        input,
        video_byte_length,
        audio_byte_length,
        output,
        limits,
        cancelled,
    )
}

/// Append `audio` to the end of `video` in place, so a caller that owns a
/// disposable picture download can assemble without copying the picture.
pub fn append_for_remux(
    video: &mut File,
    audio: &mut File,
    cancelled: &AtomicBool,
) -> Result<(u64, u64), ConversionError> {
    let deadline = Deadline {
        end: Instant::now() + RemuxLimits::default().timeout,
        cancelled,
    };
    let video_byte_length = video.metadata()?.len();
    let audio_byte_length = audio.metadata()?.len();
    video.seek(std::io::SeekFrom::End(0))?;
    copy_exact(audio, video, audio_byte_length, &deadline)?;
    if video.metadata()?.len() != video_byte_length + audio_byte_length {
        return Err(ConversionError::Protocol(
            "remux input changed while appending".into(),
        ));
    }
    Ok((video_byte_length, audio_byte_length))
}

/// Run the worker on one file holding the picture input immediately followed
/// by the sound input, leaving the assembled MP4 in `output`.
pub fn remux_joined(
    executable: &Path,
    mut input: File,
    video_byte_length: u64,
    audio_byte_length: u64,
    output: &File,
    limits: RemuxLimits,
    cancelled: &AtomicBool,
) -> Result<RemuxReport, ConversionError> {
    let timeout_ms = u64::try_from(limits.timeout.as_millis())
        .map_err(|_| ConversionError::Protocol("remux deadline overflow".into()))?;
    let deadline = Deadline {
        end: Instant::now() + limits.timeout,
        cancelled,
    };
    let request = RemuxRequest {
        protocol: REMUX_PROTOCOL_VERSION,
        video_byte_length,
        audio_byte_length,
        max_output_bytes: limits.max_output_bytes,
        timeout_ms,
    };
    request.validate()?;
    if output.metadata()?.len() != 0 || !output.metadata()?.is_file() {
        return Err(ConversionError::Protocol(
            "remux output must be an empty regular file".into(),
        ));
    }
    if input.metadata()?.len() != video_byte_length + audio_byte_length {
        return Err(ConversionError::Protocol(
            "remux input does not hold exactly both streams".into(),
        ));
    }
    input.sync_all()?;
    input.rewind()?;
    let serialized = serde_json::to_string(&request)
        .map_err(|error| ConversionError::Protocol(error.to_string()))?;
    if serialized.len() > MAX_REQUEST_BYTES {
        return Err(ConversionError::Protocol(
            "request exceeded wire budget".into(),
        ));
    }
    deadline.check()?;
    let child = deadpan_native_process::spawn(
        Command::new(executable)
            .arg(REMUX_ARGUMENT)
            .arg(serialized)
            .env_clear()
            .current_dir(std::env::temp_dir())
            .stdin(Stdio::from(input))
            .stdout(Stdio::from(output.try_clone()?))
            .stderr(Stdio::piped())
            .process_group(0),
    )?;
    let mut process = OwnedProcess::new(child);
    let (status, reply) = process.collect(&deadline, output, request.max_output_bytes)?;
    if reply.len() > MAX_REPLY_BYTES {
        return Err(ConversionError::Protocol("oversized remux reply".into()));
    }
    let reply: RemuxReply = serde_json::from_slice(&reply)
        .map_err(|error| ConversionError::Protocol(error.to_string()))?;
    let report = match reply {
        RemuxReply::Failure { code, message } => {
            if code.is_empty() || code.len() > 128 || message.is_empty() || message.len() > 2048 {
                return Err(ConversionError::Protocol(
                    "invalid failure diagnostic".into(),
                ));
            }
            return Err(ConversionError::Worker { code, message });
        }
        RemuxReply::Success { report } if status.success() => report,
        RemuxReply::Success { .. } => {
            return Err(ConversionError::Protocol(format!(
                "success followed by {status}"
            )));
        }
    };
    report.validate_for(&request)?;
    if output.metadata()?.len() != report.output_bytes {
        return Err(ConversionError::Protocol(
            "output length differs from report".into(),
        ));
    }
    deadline.check()?;
    Ok(report)
}

fn copy_exact(
    source: &mut File,
    target: &mut File,
    length: u64,
    deadline: &Deadline<'_>,
) -> Result<(), ConversionError> {
    source.rewind()?;
    let mut remaining = length;
    let mut buffer = vec![0u8; 1024 * 1024];
    while remaining != 0 {
        deadline.check()?;
        let capacity = usize::try_from(remaining.min(buffer.len() as u64))
            .map_err(|_| ConversionError::Protocol("remux input length overflow".into()))?;
        let count = source.read(&mut buffer[..capacity])?;
        if count == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "remux input shrank").into());
        }
        target.write_all(&buffer[..count])?;
        remaining -= count as u64;
    }
    if source.read(&mut buffer[..1])? != 0 {
        return Err(ConversionError::Protocol(
            "remux input grew while copying".into(),
        ));
    }
    Ok(())
}
