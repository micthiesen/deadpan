//! Proxy mode: decode the Original on stdin through the qualified source
//! adapter and encode an intra-only preview proxy, or one range of its
//! pictures, to stdout. Assembly mode joins encoded ranges from stdin into
//! one proxy movie on stdout.

use std::fs::File;
use std::io::Write;
use std::os::fd::{AsFd, AsRawFd};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_media::proxy::{
    PROXY_PROTOCOL_VERSION, ProxyAssembleReply, ProxyAssembleReport, ProxyAssembleRequest,
    ProxyPrimaries, ProxyReply, ProxyReport, ProxyRequest, ProxyTransfer, hex,
};
use deadpan_source::{DecodeControl, DecodeLimits, SourceDecoder, SourceStreamInfo};

use super::{bounded_c_string, ffi};

fn failure(code: &str, message: impl Into<String>) -> ProxyReply {
    ProxyReply::Failure {
        code: code.to_owned(),
        message: message.into(),
    }
}

fn native_failure(error: &ffi::Error, fallback: &str) -> ProxyReply {
    let code = bounded_c_string(&error.code);
    let message = bounded_c_string(&error.message);
    failure(
        if code.is_empty() {
            "internal_error"
        } else {
            &code
        },
        if message.is_empty() {
            fallback.to_owned()
        } else {
            message
        },
    )
}

/// Progress heartbeat for the host's stall watch: a newline on the control
/// pipe before the reply, at most every 250 ms. It follows real work only
/// (an opened decoder, an accepted picture), so a hung encoder stops it.
struct Heartbeat(Option<Instant>);

impl Heartbeat {
    fn beat(&mut self) {
        if self
            .0
            .is_some_and(|last| last.elapsed() < Duration::from_millis(250))
        {
            return;
        }
        let mut stderr = std::io::stderr().lock();
        match stderr.write_all(b"\n").and_then(|()| stderr.flush()) {
            Ok(()) => self.0 = Some(Instant::now()),
            // The host is gone (killed or crashed): nobody will read the
            // output or record it, so stop writing at once. Its next build
            // waits for this process to release the output's lock.
            Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => {
                std::process::exit(3);
            }
            Err(_) => {}
        }
    }
}

fn decode_failure(error: deadpan_source::SourceDecodeError) -> ProxyReply {
    match error {
        deadpan_source::SourceDecodeError::Native { code, message } => {
            failure(&format!("source_{code}"), message)
        }
        other => failure("source_failure", other.to_string()),
    }
}

/// The decoded Original must be the stream and interpretation the host planned.
fn check_source(info: &SourceStreamInfo, request: &ProxyRequest) -> Result<(), String> {
    if info.stream_index != request.stream_index
        || info.width != request.source_width
        || info.height != request.source_height
        || info.time_base_num != request.time_base_num
        || info.time_base_den != request.time_base_den
        || info.sample_aspect_num != request.sar_num
        || info.sample_aspect_den != request.sar_den
        || info.rotation_quarter_turns != request.rotation_quarter_turns
        // An HDR Original has no proxy transfer and is refused explicitly.
        || ProxyTransfer::of(info.color.transfer) != Some(request.transfer)
        || ProxyPrimaries::of(info.color.primaries) != request.primaries
    {
        return Err("decoded Original differs from the planned stream".into());
    }
    Ok(())
}

pub(super) fn run(request: &ProxyRequest) -> ProxyReply {
    // Background work: interactive decoding and the UI come first.
    deadpan_source::lower_current_thread_priority();
    let deadline = Instant::now() + Duration::from_millis(request.timeout_ms);
    let cancelled = AtomicBool::new(false);
    let control = || -> Result<DecodeControl<'_>, ProxyReply> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(failure(
                "deadline_exceeded",
                "proxy encoding exceeded its deadline",
            ));
        }
        Ok(DecodeControl {
            timeout: remaining.min(Duration::from_secs(60)),
            cancelled: &cancelled,
        })
    };
    let input = match std::io::stdin().as_fd().try_clone_to_owned() {
        Ok(descriptor) => File::from(descriptor),
        Err(error) => return failure("invalid_descriptor", format!("duplicate input: {error}")),
    };
    match input.metadata() {
        Ok(metadata) if metadata.is_file() && metadata.len() == request.input_byte_length => {}
        _ => {
            return failure(
                "invalid_descriptor",
                "input must be exact-length regular data",
            );
        }
    }
    let limits = DecodeLimits {
        threads: request.decode_threads,
        ..DecodeLimits::default()
    };
    let mut decoder = match control().map(|control| SourceDecoder::open(input, limits, control)) {
        Ok(Ok(decoder)) => decoder,
        Ok(Err(error)) => return decode_failure(error),
        Err(reply) => return reply,
    };
    if let Err(message) = check_source(decoder.info(), request) {
        return failure("invalid_media", message);
    }
    let mut heartbeat = Heartbeat(None);
    heartbeat.beat();
    let output = std::io::stdout().as_raw_fd();
    let mut error = ffi::Error::default();
    let native = ffi::ProxyRequest {
        source_width: request.source_width,
        source_height: request.source_height,
        width: request.width,
        height: request.height,
        time_base_num: request.time_base_num,
        time_base_den: request.time_base_den,
        sar_num: request.sar_num,
        sar_den: request.sar_den,
        rotation_quarter_turns: u32::from(request.rotation_quarter_turns),
        transfer: request.transfer.code(),
        primaries: request.primaries.code(),
        quality: request.quality,
        frames: request.frames,
        max_output_bytes: request.max_output_bytes,
        timeout_ms: request.timeout_ms,
        output_offset: request.output_offset,
    };
    // A range starts at its first picture: seek to its keyframe and decode
    // only metadata until the first picture of the range.
    let mut first = None;
    if let Some(range) = request.range {
        if range.start_frame > 0 {
            match control().map(|control| decoder.seek_to(range.seek_pts, range.start_pts, control))
            {
                Ok(Ok(())) => {}
                Ok(Err(error)) => return decode_failure(error),
                Err(reply) => return reply,
            }
        }
        loop {
            let metadata = match control().map(|control| decoder.next_metadata(control)) {
                Ok(Ok(Some(metadata))) => metadata,
                Ok(Ok(None)) => {
                    return failure("invalid_media", "the Original ended before the range");
                }
                Ok(Err(error)) => return decode_failure(error),
                Err(reply) => return reply,
            };
            heartbeat.beat();
            if metadata.pts < range.start_pts {
                continue;
            }
            if metadata.pts != range.start_pts {
                return failure(
                    "invalid_media",
                    "the range does not start at an Original picture",
                );
            }
            match control().map(|control| decoder.copy_current_rgba(control)) {
                Ok(Ok(frame)) => first = Some(frame),
                Ok(Err(error)) => return decode_failure(error),
                Err(reply) => return reply,
            }
            break;
        }
    }
    if ffi::proxy_open(output, &native, &mut error) != 0 {
        return native_failure(&error, "proxy encoder failed to open");
    }
    let mut frames = 0_u64;
    let mut end = None;
    loop {
        if request.range.is_some() && frames == request.frames {
            break;
        }
        let frame = match first.take() {
            Some(frame) => frame,
            None => match control().map(|control| decoder.next_rgba(control)) {
                Ok(Ok(Some(frame))) => frame,
                Ok(Ok(None)) => break,
                Ok(Err(error)) => return decode_failure(error),
                Err(reply) => return reply,
            },
        };
        frames += 1;
        if frames > request.frames {
            return failure("invalid_media", "Original has more pictures than planned");
        }
        let Some(duration) = frame.metadata.reported_duration.filter(|value| *value > 0) else {
            return failure("invalid_media", "an Original picture has no duration");
        };
        end = frame.metadata.pts.checked_add(duration);
        if ffi::proxy_push(
            &frame.rgba,
            frame.row_stride_bytes as u64,
            frame.metadata.pts,
            duration,
            &mut error,
        ) != 0
        {
            return native_failure(&error, "proxy encoder rejected a picture");
        }
        heartbeat.beat();
    }
    if let Some(range) = request.range
        && end != Some(range.end_pts)
    {
        return failure("invalid_media", "the range does not end where planned");
    }
    let mut native_report = ffi::ProxyReport::default();
    if ffi::proxy_finish(output, &mut native_report, &mut error) != 0 {
        return native_failure(&error, "proxy encoder failed to finish");
    }
    let report = ProxyReport {
        protocol: PROXY_PROTOCOL_VERSION,
        output_bytes: native_report.output_bytes,
        frames,
        packets: native_report.packets,
        keyframes: native_report.keyframes,
        width: native_report.width,
        height: native_report.height,
        extradata_sha256: hex(&native_report.extradata_sha256),
    };
    match report.validate_for(request) {
        Ok(()) => ProxyReply::Success { report },
        Err(error) => failure(
            "internal_error",
            format!("native proxy report violated contract: {error}"),
        ),
    }
}

/// Join the encoded ranges on stdin into one proxy movie on stdout.
pub(super) fn assemble(request: &ProxyAssembleRequest) -> ProxyAssembleReply {
    deadpan_source::lower_current_thread_priority();
    let segments: Vec<ffi::ProxySegment> = request
        .segments
        .iter()
        .map(|segment| ffi::ProxySegment {
            offset: segment.offset,
            length: segment.length,
            frames: segment.frames,
            start_pts: segment.start_pts,
            end_pts: segment.end_pts,
        })
        .collect();
    let native = ffi::ProxyAssembleRequest {
        input_byte_length: request.input_byte_length,
        width: request.width,
        height: request.height,
        time_base_num: request.time_base_num,
        time_base_den: request.time_base_den,
        sar_num: request.sar_num,
        sar_den: request.sar_den,
        rotation_quarter_turns: u32::from(request.rotation_quarter_turns),
        transfer: request.transfer.code(),
        primaries: request.primaries.code(),
        frames: request.frames,
        segments: segments.as_ptr(),
        segment_count: segments.len() as u64,
        max_output_bytes: request.max_output_bytes,
        timeout_ms: request.timeout_ms,
    };
    let mut report = ffi::ProxyAssembleReport::default();
    let mut error = ffi::Error::default();
    if ffi::proxy_assemble(
        std::io::stdin().as_raw_fd(),
        std::io::stdout().as_raw_fd(),
        &segments,
        &native,
        &mut report,
        &mut error,
    ) != 0
    {
        return match native_failure(&error, "proxy assembly failed") {
            ProxyReply::Failure { code, message } => ProxyAssembleReply::Failure { code, message },
            ProxyReply::Success { .. } => unreachable!("a failure is never a success"),
        };
    }
    let report = ProxyAssembleReport {
        protocol: PROXY_PROTOCOL_VERSION,
        output_bytes: report.output_bytes,
        packets: report.packets,
        width: report.width,
        height: report.height,
    };
    match report.validate_for(request) {
        Ok(()) => ProxyAssembleReply::Success { report },
        Err(error) => ProxyAssembleReply::Failure {
            code: "internal_error".into(),
            message: format!("native proxy assembly report violated contract: {error}"),
        },
    }
}
