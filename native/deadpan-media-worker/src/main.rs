//! Private descriptor-only FFV1 conversion helper.

use std::ffi::OsString;
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::process::ExitCode;

use deadpan_media::protocol::{
    ConversionReport, ConversionRequest, MAX_REPLY_BYTES, MAX_REQUEST_BYTES, PROTOCOL_VERSION,
    WorkerReply,
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

    unsafe extern "C" {
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
) -> Result<ConversionRequest, (String, String)> {
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
    let request: ConversionRequest = serde_json::from_str(&encoded).map_err(|error| {
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

fn convert(request: &ConversionRequest) -> WorkerReply {
    let scratch = match tempfile::tempfile() {
        Ok(file) => file,
        Err(error) => return failure("io_failure", format!("create private RGB scratch: {error}")),
    };
    let native_request = ffi::Request {
        width: request.video.width,
        height: request.video.height,
        frames: request.video.frames,
        rate_num: request.video.rate_num,
        rate_den: request.video.rate_den,
        input_byte_length: request.input_byte_length,
        max_input_bytes: request.limits.max_input_bytes,
        max_output_bytes: request.limits.max_output_bytes,
        max_scratch_bytes: request.limits.max_scratch_bytes,
        timeout_ms: request.limits.timeout_ms,
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
        protocol: PROTOCOL_VERSION,
        video: request.video,
        output_bytes: native_report.output_bytes,
        input_rgb_sha256: bounded_c_string(&native_report.input_rgb_sha256),
        output_rgb_sha256: bounded_c_string(&native_report.output_rgb_sha256),
        input_time_base_num: native_report.input_time_base_num,
        input_time_base_den: native_report.input_time_base_den,
        output_time_base_num: native_report.output_time_base_num,
        output_time_base_den: native_report.output_time_base_den,
        first_output_pts: native_report.first_output_pts,
        last_output_pts: native_report.last_output_pts,
        ffv1_version: native_report.ffv1_version,
        slice_crc: native_report.slice_crc == 1,
        discarded_audio_streams: native_report.discarded_audio_streams,
    };
    match report.validate(request) {
        Ok(()) => WorkerReply::Success { report },
        Err(error) => failure(
            "internal_error",
            format!("native report violated contract: {error}"),
        ),
    }
}

fn emit(reply: &WorkerReply) -> io::Result<()> {
    let mut encoded = serde_json::to_vec(reply).map_err(io::Error::other)?;
    if encoded.len() + 1 > MAX_REPLY_BYTES {
        encoded = br#"{"status":"failure","code":"internal_error","message":"worker reply exceeded the wire limit"}"#.to_vec();
    }
    encoded.push(b'\n');
    let mut stderr = io::stderr().lock();
    stderr.write_all(&encoded)?;
    stderr.flush()
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
}
