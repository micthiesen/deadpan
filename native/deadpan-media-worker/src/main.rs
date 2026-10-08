//! Private descriptor-only FFV1 conversion and MP4 stream-copy helper.

use std::ffi::OsString;
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::process::ExitCode;

use deadpan_media::protocol::{
    ConversionReport, MAX_REPLY_BYTES, MAX_REQUEST_BYTES, REMUX_ARGUMENT, REMUX_PROTOCOL_VERSION,
    REPORT_PROTOCOL_VERSION, RemuxReply, RemuxReport, RemuxRequest, WorkerReply, WorkerRequest,
};
use deadpan_media::proxy::{
    MAX_PROXY_ASSEMBLE_REQUEST_BYTES, PROXY_ARGUMENT, PROXY_ASSEMBLE_ARGUMENT, ProxyAssembleReply,
    ProxyAssembleRequest, ProxyReply, ProxyRequest,
};

#[allow(unsafe_code)]
mod ffi {
    use std::os::raw::{c_char, c_int};

    #[repr(C)]
    pub struct Request {
        pub width: u32,
        pub height: u32,
        pub frames: u32,
        pub rate_num: u32,
        pub rate_den: u32,
        pub input_byte_length: u64,
        pub max_input_bytes: u64,
        pub max_output_bytes: u64,
        pub max_scratch_bytes: u64,
        pub timeout_ms: u64,
        pub output_frames: u32,
        pub output_rate_num: u32,
        pub output_rate_den: u32,
        pub sampling_kind: u32,
        pub generated_start: u32,
        pub generated_frames: u32,
    }

    #[repr(C)]
    pub struct Report {
        pub output_bytes: u64,
        pub input_rgb_sha256: [c_char; 65],
        pub output_rgb_sha256: [c_char; 65],
        pub input_time_base_num: u32,
        pub input_time_base_den: u32,
        pub output_time_base_num: u32,
        pub output_time_base_den: u32,
        pub first_output_pts: i64,
        pub last_output_pts: i64,
        pub last_output_duration: i64,
        pub ffv1_version: u32,
        pub slice_crc: u8,
        pub discarded_audio_streams: u32,
    }

    impl Default for Report {
        fn default() -> Self {
            Self {
                output_bytes: 0,
                input_rgb_sha256: [0; 65],
                output_rgb_sha256: [0; 65],
                input_time_base_num: 0,
                input_time_base_den: 0,
                output_time_base_num: 0,
                output_time_base_den: 0,
                first_output_pts: 0,
                last_output_pts: 0,
                last_output_duration: 0,
                ffv1_version: 0,
                slice_crc: 0,
                discarded_audio_streams: 0,
            }
        }
    }

    #[repr(C)]
    pub struct Error {
        pub code: [c_char; 48],
        pub message: [c_char; 256],
    }

    impl Default for Error {
        fn default() -> Self {
            Self {
                code: [0; 48],
                message: [0; 256],
            }
        }
    }

    #[repr(C)]
    pub struct RemuxRequest {
        pub video_byte_length: u64,
        pub audio_byte_length: u64,
        pub max_output_bytes: u64,
        pub timeout_ms: u64,
    }

    #[repr(C)]
    #[derive(Default)]
    pub struct RemuxReport {
        pub output_bytes: u64,
        pub video_packets: u64,
        pub audio_packets: u64,
        pub width: u32,
        pub height: u32,
        pub sample_rate: u32,
        pub channels: u32,
    }

    #[repr(C)]
    pub struct ProxyRequest {
        pub source_width: u32,
        pub source_height: u32,
        pub width: u32,
        pub height: u32,
        pub time_base_num: u32,
        pub time_base_den: u32,
        pub sar_num: u32,
        pub sar_den: u32,
        pub rotation_quarter_turns: u32,
        pub transfer: u32,
        pub primaries: u32,
        pub quality: u32,
        pub frames: u64,
        pub max_output_bytes: u64,
        pub timeout_ms: u64,
        pub output_offset: u64,
    }

    #[repr(C)]
    #[derive(Default)]
    pub struct ProxyReport {
        pub output_bytes: u64,
        pub packets: u64,
        pub keyframes: u64,
        pub width: u32,
        pub height: u32,
        pub extradata_sha256: [u8; 32],
    }

    #[repr(C)]
    pub struct ProxySegment {
        pub offset: u64,
        pub length: u64,
        pub frames: u64,
        pub start_pts: i64,
        pub end_pts: i64,
    }

    #[repr(C)]
    pub struct ProxyAssembleRequest {
        pub input_byte_length: u64,
        pub width: u32,
        pub height: u32,
        pub time_base_num: u32,
        pub time_base_den: u32,
        pub sar_num: u32,
        pub sar_den: u32,
        pub rotation_quarter_turns: u32,
        pub transfer: u32,
        pub primaries: u32,
        pub frames: u64,
        pub segments: *const ProxySegment,
        pub segment_count: u64,
        pub max_output_bytes: u64,
        pub timeout_ms: u64,
    }

    #[repr(C)]
    #[derive(Default)]
    pub struct ProxyAssembleReport {
        pub output_bytes: u64,
        pub packets: u64,
        pub width: u32,
        pub height: u32,
    }

    unsafe extern "C" {
        fn deadpan_proxy_open(
            output_fd: c_int,
            request: *const ProxyRequest,
            error: *mut Error,
        ) -> c_int;
        fn deadpan_proxy_push(
            rgba: *const u8,
            rgba_bytes: u64,
            stride: u64,
            pts: i64,
            duration: i64,
            error: *mut Error,
        ) -> c_int;
        fn deadpan_proxy_finish(
            output_fd: c_int,
            report: *mut ProxyReport,
            error: *mut Error,
        ) -> c_int;
        fn deadpan_proxy_assemble(
            input_fd: c_int,
            output_fd: c_int,
            request: *const ProxyAssembleRequest,
            report: *mut ProxyAssembleReport,
            error: *mut Error,
        ) -> c_int;
        fn deadpan_remux(
            input_fd: c_int,
            output_fd: c_int,
            request: *const RemuxRequest,
            report: *mut RemuxReport,
            error: *mut Error,
        ) -> c_int;
        fn deadpan_convert(
            input_fd: c_int,
            output_fd: c_int,
            scratch_fd: c_int,
            request: *const Request,
            report: *mut Report,
            error: *mut Error,
        ) -> c_int;
    }

    #[allow(unsafe_code)]
    pub fn proxy_open(output_fd: c_int, request: &ProxyRequest, error: &mut Error) -> c_int {
        // SAFETY: both pointers reference initialized repr(C) values borrowed
        // for this synchronous call; the adapter copies the request and keeps
        // only its own encoder state and the descriptor number.
        unsafe { deadpan_proxy_open(output_fd, request, error) }
    }

    #[allow(unsafe_code)]
    pub fn proxy_push(
        rgba: &[u8],
        stride: u64,
        pts: i64,
        duration: i64,
        error: &mut Error,
    ) -> c_int {
        // SAFETY: the pointer and length describe one initialized borrowed
        // slice that the adapter reads only during this synchronous call; it
        // checks the length against stride times the opened source height.
        unsafe {
            deadpan_proxy_push(
                rgba.as_ptr(),
                rgba.len() as u64,
                stride,
                pts,
                duration,
                error,
            )
        }
    }

    #[allow(unsafe_code)]
    pub fn proxy_finish(output_fd: c_int, report: &mut ProxyReport, error: &mut Error) -> c_int {
        // SAFETY: both pointers reference initialized repr(C) values that remain
        // exclusively borrowed for this synchronous call.
        unsafe { deadpan_proxy_finish(output_fd, report, error) }
    }

    #[allow(unsafe_code)]
    pub fn proxy_assemble(
        input_fd: c_int,
        output_fd: c_int,
        segments: &[ProxySegment],
        request: &ProxyAssembleRequest,
        report: &mut ProxyAssembleReport,
        error: &mut Error,
    ) -> c_int {
        if request.segments != segments.as_ptr() || request.segment_count != segments.len() as u64 {
            return 1;
        }
        // SAFETY: the request's segment pointer and count were just checked
        // to describe `segments`, an initialized slice borrowed for this
        // synchronous call; every other pointer references an initialized
        // repr(C) value exclusively borrowed for the call. The adapter
        // retains no pointer or descriptor.
        unsafe { deadpan_proxy_assemble(input_fd, output_fd, request, report, error) }
    }

    #[allow(unsafe_code)]
    pub fn remux(
        input_fd: c_int,
        output_fd: c_int,
        request: &RemuxRequest,
        report: &mut RemuxReport,
        error: &mut Error,
    ) -> c_int {
        // SAFETY: all pointers reference initialized repr(C) values that remain
        // exclusively borrowed for this synchronous call. The C adapter retains
        // no pointer or descriptor and bounds every write by the supplied arrays.
        unsafe { deadpan_remux(input_fd, output_fd, request, report, error) }
    }

    #[allow(unsafe_code)]
    pub fn convert(
        input_fd: c_int,
        output_fd: c_int,
        scratch_fd: c_int,
        request: &Request,
        report: &mut Report,
        error: &mut Error,
    ) -> c_int {
        // SAFETY: all pointers reference initialized repr(C) values that remain
        // exclusively borrowed for this synchronous call. The C adapter retains
        // no pointer or descriptor and bounds every write by the supplied arrays.
        unsafe { deadpan_convert(input_fd, output_fd, scratch_fd, request, report, error) }
    }
}

fn failure(code: &str, message: impl Into<String>) -> WorkerReply {
    WorkerReply::Failure {
        code: code.to_owned(),
        message: message.into(),
    }
}

fn parse_request(
    argument: Option<OsString>,
    extra: Option<OsString>,
) -> Result<WorkerRequest, (String, String)> {
    if extra.is_some() {
        return Err((
            "invalid_request".into(),
            "worker requires exactly one JSON argument".into(),
        ));
    }
    let argument = argument.ok_or_else(|| {
        (
            "invalid_request".into(),
            "worker requires exactly one JSON argument".into(),
        )
    })?;
    let encoded = argument.into_string().map_err(|_| {
        (
            "invalid_request".into(),
            "request argument is not UTF-8".into(),
        )
    })?;
    if encoded.len() > MAX_REQUEST_BYTES {
        return Err((
            "invalid_request".into(),
            "request JSON exceeds the wire limit".into(),
        ));
    }
    let request: WorkerRequest = serde_json::from_str(&encoded).map_err(|error| {
        (
            "invalid_request".into(),
            format!("invalid request JSON: {error}"),
        )
    })?;
    request
        .validate()
        .map_err(|error| ("invalid_request".into(), error.to_string()))?;
    Ok(request)
}

fn bounded_c_string<const N: usize>(bytes: &[std::os::raw::c_char; N]) -> String {
    let length = bytes.iter().position(|byte| *byte == 0).unwrap_or(N);
    let raw = bytes[..length]
        .iter()
        .map(|byte| *byte as u8)
        .collect::<Vec<_>>();
    String::from_utf8_lossy(&raw).into_owned()
}

fn convert(request: &WorkerRequest) -> WorkerReply {
    let scratch = match tempfile::tempfile() {
        Ok(file) => file,
        Err(error) => return failure("io_failure", format!("create private RGB scratch: {error}")),
    };
    let native = request.native_video();
    let output = match request.output_video() {
        Ok(video) => video,
        Err(error) => return failure("invalid_request", error.to_string()),
    };
    let limits = request.limits();
    let (sampling_kind, generated_start, generated_frames) = match request {
        WorkerRequest::Convert(_) => (0, 0, 0),
        WorkerRequest::Bridge(_) => (1, 0, 0),
        WorkerRequest::Extension(extension) => {
            let interval = extension.sampling.generated_interval();
            let start = match u32::try_from(interval.start) {
                Ok(start) => start,
                Err(_) => {
                    return failure("invalid_request", "extension interval is not representable");
                }
            };
            let frames = match u32::try_from(extension.sampling.generated_frame_count().frames()) {
                Ok(frames) => frames,
                Err(_) => {
                    return failure(
                        "invalid_request",
                        "extension frame count is not representable",
                    );
                }
            };
            // The typed map guarantees a nonempty context: a generated interval
            // at zero is FromRight; FromLeft starts after all context handles.
            (if start == 0 { 3 } else { 2 }, start, frames)
        }
    };
    let native_request = ffi::Request {
        width: native.width,
        height: native.height,
        frames: native.frames,
        rate_num: native.rate_num,
        rate_den: native.rate_den,
        input_byte_length: request.input_byte_length(),
        max_input_bytes: limits.max_input_bytes,
        max_output_bytes: limits.max_output_bytes,
        max_scratch_bytes: limits.max_scratch_bytes,
        timeout_ms: limits.timeout_ms,
        output_frames: output.frames,
        output_rate_num: output.rate_num,
        output_rate_den: output.rate_den,
        sampling_kind,
        generated_start,
        generated_frames,
    };
    let mut native_report = ffi::Report::default();
    let mut native_error = ffi::Error::default();
    let status = ffi::convert(
        0,
        1,
        scratch.as_raw_fd(),
        &native_request,
        &mut native_report,
        &mut native_error,
    );
    if status != 0 {
        let code = bounded_c_string(&native_error.code);
        let message = bounded_c_string(&native_error.message);
        return failure(
            if code.is_empty() {
                "internal_error"
            } else {
                &code
            },
            if message.is_empty() {
                "native conversion failed without a diagnostic".to_owned()
            } else {
                message
            },
        );
    }
    let report = ConversionReport {
        protocol: REPORT_PROTOCOL_VERSION,
        video: output,
        output_bytes: native_report.output_bytes,
        input_rgb_sha256: bounded_c_string(&native_report.input_rgb_sha256),
        output_rgb_sha256: bounded_c_string(&native_report.output_rgb_sha256),
        input_time_base_num: native_report.input_time_base_num,
        input_time_base_den: native_report.input_time_base_den,
        output_time_base_num: native_report.output_time_base_num,
        output_time_base_den: native_report.output_time_base_den,
        first_output_pts: native_report.first_output_pts,
        last_output_pts: native_report.last_output_pts,
        last_output_duration: native_report.last_output_duration,
        ffv1_version: native_report.ffv1_version,
        slice_crc: native_report.slice_crc == 1,
        discarded_audio_streams: native_report.discarded_audio_streams,
    };
    match report.validate_worker(request) {
        Ok(()) => WorkerReply::Success { report },
        Err(error) => failure(
            "internal_error",
            format!("native report violated contract: {error}"),
        ),
    }
}

fn parse_remux(
    argument: Option<OsString>,
    extra: Option<OsString>,
) -> Result<RemuxRequest, String> {
    if extra.is_some() {
        return Err("remux requires exactly one JSON argument".into());
    }
    let encoded = argument
        .ok_or("remux requires exactly one JSON argument")?
        .into_string()
        .map_err(|_| "remux request is not UTF-8")?;
    if encoded.len() > MAX_REQUEST_BYTES {
        return Err("remux request exceeds the wire limit".into());
    }
    let request: RemuxRequest =
        serde_json::from_str(&encoded).map_err(|error| format!("invalid remux JSON: {error}"))?;
    request.validate().map_err(|error| error.to_string())?;
    Ok(request)
}

fn parse_proxy(
    argument: Option<OsString>,
    extra: Option<OsString>,
) -> Result<ProxyRequest, String> {
    if extra.is_some() {
        return Err("proxy requires exactly one JSON argument".into());
    }
    let encoded = argument
        .ok_or("proxy requires exactly one JSON argument")?
        .into_string()
        .map_err(|_| "proxy request is not UTF-8")?;
    if encoded.len() > MAX_REQUEST_BYTES {
        return Err("proxy request exceeds the wire limit".into());
    }
    let request: ProxyRequest =
        serde_json::from_str(&encoded).map_err(|error| format!("invalid proxy JSON: {error}"))?;
    request.validate().map_err(|error| error.to_string())?;
    Ok(request)
}

fn parse_proxy_assembly(
    argument: Option<OsString>,
    extra: Option<OsString>,
) -> Result<ProxyAssembleRequest, String> {
    if extra.is_some() {
        return Err("proxy assembly requires exactly one JSON argument".into());
    }
    let encoded = argument
        .ok_or("proxy assembly requires exactly one JSON argument")?
        .into_string()
        .map_err(|_| "proxy assembly request is not UTF-8")?;
    if encoded.len() > MAX_PROXY_ASSEMBLE_REQUEST_BYTES {
        return Err("proxy assembly request exceeds the wire limit".into());
    }
    let request: ProxyAssembleRequest = serde_json::from_str(&encoded)
        .map_err(|error| format!("invalid proxy assembly JSON: {error}"))?;
    request.validate().map_err(|error| error.to_string())?;
    Ok(request)
}

mod proxy;

fn remux(request: &RemuxRequest) -> RemuxReply {
    let native_request = ffi::RemuxRequest {
        video_byte_length: request.video_byte_length,
        audio_byte_length: request.audio_byte_length,
        max_output_bytes: request.max_output_bytes,
        timeout_ms: request.timeout_ms,
    };
    let mut native_report = ffi::RemuxReport::default();
    let mut native_error = ffi::Error::default();
    let status = ffi::remux(
        io::stdin().as_raw_fd(),
        io::stdout().as_raw_fd(),
        &native_request,
        &mut native_report,
        &mut native_error,
    );
    if status != 0 {
        let code = bounded_c_string(&native_error.code);
        let message = bounded_c_string(&native_error.message);
        return RemuxReply::Failure {
            code: if code.is_empty() {
                "internal_error".into()
            } else {
                code
            },
            message: if message.is_empty() {
                "native remux failed without a diagnostic".into()
            } else {
                message
            },
        };
    }
    let report = RemuxReport {
        protocol: REMUX_PROTOCOL_VERSION,
        output_bytes: native_report.output_bytes,
        video_packets: native_report.video_packets,
        audio_packets: native_report.audio_packets,
        width: native_report.width,
        height: native_report.height,
        sample_rate: native_report.sample_rate,
        channels: native_report.channels,
    };
    match report.validate_for(request) {
        Ok(()) => RemuxReply::Success { report },
        Err(error) => RemuxReply::Failure {
            code: "internal_error".into(),
            message: format!("native remux report violated contract: {error}"),
        },
    }
}

fn emit_json(encoded: serde_json::Result<Vec<u8>>) -> io::Result<()> {
    let mut encoded = encoded.map_err(io::Error::other)?;
    if encoded.len() + 1 > MAX_REPLY_BYTES {
        encoded = br#"{"status":"failure","code":"internal_error","message":"worker reply exceeded the wire limit"}"#.to_vec();
    }
    encoded.push(b'\n');
    let mut stderr = io::stderr().lock();
    stderr.write_all(&encoded)?;
    stderr.flush()
}

fn emit(reply: &WorkerReply) -> io::Result<()> {
    emit_json(serde_json::to_vec(reply))
}

fn run() -> (WorkerReply, ExitCode) {
    let mut arguments = std::env::args_os().skip(1);
    match parse_request(arguments.next(), arguments.next()) {
        Ok(request) => {
            let reply = convert(&request);
            let code = if matches!(reply, WorkerReply::Success { .. }) {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            };
            (reply, code)
        }
        Err((code, message)) => (failure(&code, message), ExitCode::FAILURE),
    }
}

fn main() -> ExitCode {
    let mut arguments = std::env::args_os().skip(1);
    let mode = arguments.next();
    if mode.as_deref() == Some(std::ffi::OsStr::new(PROXY_ARGUMENT)) {
        let reply = match parse_proxy(arguments.next(), arguments.next()) {
            Ok(request) => proxy::run(&request),
            Err(message) => ProxyReply::Failure {
                code: "invalid_request".into(),
                message,
            },
        };
        let success = matches!(reply, ProxyReply::Success { .. });
        return if emit_json(serde_json::to_vec(&reply)).is_ok() && success {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }
    if mode.as_deref() == Some(std::ffi::OsStr::new(PROXY_ASSEMBLE_ARGUMENT)) {
        let reply = match parse_proxy_assembly(arguments.next(), arguments.next()) {
            Ok(request) => proxy::assemble(&request),
            Err(message) => ProxyAssembleReply::Failure {
                code: "invalid_request".into(),
                message,
            },
        };
        let success = matches!(reply, ProxyAssembleReply::Success { .. });
        return if emit_json(serde_json::to_vec(&reply)).is_ok() && success {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }
    if mode.as_deref() == Some(std::ffi::OsStr::new(REMUX_ARGUMENT)) {
        let reply = match parse_remux(arguments.next(), arguments.next()) {
            Ok(request) => remux(&request),
            Err(message) => RemuxReply::Failure {
                code: "invalid_request".into(),
                message,
            },
        };
        let success = matches!(reply, RemuxReply::Success { .. });
        return if emit_json(serde_json::to_vec(&reply)).is_ok() && success {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }
    let (reply, code) = run();
    if emit(&reply).is_err() {
        ExitCode::FAILURE
    } else {
        code
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_string_conversion_is_bounded_and_lossy() {
        let mut bytes = [0; 4];
        bytes[0] = b'a' as _;
        bytes[1] = -1;
        assert_eq!(bounded_c_string(&bytes), "a�");
    }

    #[test]
    fn argument_parser_rejects_extra_oversized_and_non_utf8_requests() {
        use std::os::unix::ffi::OsStringExt;

        assert!(parse_request(Some("{}".into()), Some("{}".into())).is_err());
        assert!(parse_request(Some("x".repeat(MAX_REQUEST_BYTES + 1).into()), None).is_err());
        assert!(parse_request(Some(OsString::from_vec(vec![0xff])), None).is_err());
        assert!(
            parse_request(
                Some(
                    r#"{"protocol":1,"video":{"width":1,"height":1,"frames":1,"rate_num":24,"rate_den":1},"input_byte_length":1,"limits":{"max_input_bytes":1,"max_output_bytes":1,"max_scratch_bytes":3,"timeout_ms":1},"path":"forbidden"}"#
                        .into(),
                ),
                None,
            )
            .is_err()
        );
    }

    fn native_request() -> ffi::Request {
        ffi::Request {
            width: 1,
            height: 1,
            frames: 2,
            rate_num: 24,
            rate_den: 1,
            input_byte_length: 1,
            max_input_bytes: 1,
            max_output_bytes: 1,
            max_scratch_bytes: 6,
            timeout_ms: 1,
            output_frames: 1,
            output_rate_num: 24,
            output_rate_den: 1,
            sampling_kind: 0,
            generated_start: 0,
            generated_frames: 0,
        }
    }

    fn rejects_native_request(request: &ffi::Request, message: &str) {
        // The production executable handles one conversion; its C state is
        // process-global. Unit probes must not race each other's diagnostics.
        static NATIVE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = NATIVE_TEST_LOCK.lock().unwrap();
        let mut report = ffi::Report::default();
        let mut error = ffi::Error::default();

        assert_ne!(
            ffi::convert(-1, -1, -1, request, &mut report, &mut error),
            0
        );
        assert_eq!(bounded_c_string(&error.code), "invalid_request");
        assert!(bounded_c_string(&error.message).contains(message));
    }

    #[test]
    fn native_boundary_rejects_inconsistent_sampling_configuration() {
        let mut request = native_request();
        request.sampling_kind = 4;
        rejects_native_request(&request, "sampling configuration");
        request.sampling_kind = 1;
        request.frames = 1;
        rejects_native_request(&request, "sampling configuration");
        request.frames = 2;
        request.generated_frames = 8;
        rejects_native_request(&request, "non-extension");
    }

    #[test]
    fn native_boundary_rejects_extension_intervals_and_count_overflow_before_io() {
        for kind in [2, 3] {
            let mut request = native_request();
            request.frames = 17;
            request.output_frames = 12;
            request.sampling_kind = kind;
            request.generated_start = if kind == 2 { 9 } else { 0 };
            for generated in [0, 17, u32::MAX] {
                request.generated_frames = generated;
                rejects_native_request(&request, "extension counts");
            }
            request.generated_frames = 8;
            request.generated_start = if kind == 2 { 8 } else { 1 };
            rejects_native_request(&request, "extension interval");
            request.generated_start = u32::MAX;
            rejects_native_request(&request, "extension interval");
            request.generated_start = if kind == 2 { 9 } else { 0 };
            if kind == 2 {
                request.frames = 18;
                rejects_native_request(&request, "extension interval");
            }
            request.frames = u32::MAX;
            rejects_native_request(&request, "outside worker bounds");
            request.frames = 17;
            request.output_frames = 10_001;
            rejects_native_request(&request, "outside worker bounds");
        }
    }
}
