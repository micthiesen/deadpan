#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::fs;
use std::io::{Cursor, Read};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_media::protocol::{
    ConversionLimits, ConversionReport, ConversionRequest, VideoContract, WorkerReply,
};
use deadpan_media::{ConversionError, InputIdentity, canonicalize};
use sha2::{Digest, Sha256};

fn request() -> ConversionRequest {
    ConversionRequest {
        protocol: 1,
        video: VideoContract {
            width: 2,
            height: 2,
            frames: 1,
            rate_num: 24,
            rate_den: 1,
        },
        input_byte_length: 5,
        limits: ConversionLimits {
            max_input_bytes: 1024,
            max_output_bytes: 1024,
            max_scratch_bytes: 1024,
            timeout_ms: 5000,
        },
    }
}

fn identity() -> InputIdentity {
    InputIdentity {
        sha256: Sha256::digest(b"input").into(),
    }
}

// These fake executables test transport and ownership only. Actual codec
// correctness is covered by native/deadpan-media-worker/tests with real media.
fn helper(directory: &Path, body: &str) -> std::path::PathBuf {
    let executable = directory.join("fake-codec");
    fs::write(&executable, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    executable
}

fn success() -> ConversionReport {
    ConversionReport {
        protocol: 1,
        video: request().video,
        output_bytes: 5,
        input_rgb_sha256: "a".repeat(64),
        output_rgb_sha256: "a".repeat(64),
        input_time_base_num: 1,
        input_time_base_den: 24,
        output_time_base_num: 1,
        output_time_base_den: 1000,
        first_output_pts: 0,
        last_output_pts: 0,
        ffv1_version: 3,
        slice_crc: true,
        discarded_audio_streams: 0,
    }
}

fn script_reply(report: ConversionReport) -> String {
    let wire = serde_json::to_string(&WorkerReply::Success { report }).unwrap();
    // This fixture JSON has no apostrophes or shell expansions.
    format!("printf media\nprintf '%s' '{wire}' >&2")
}

#[test]
fn input_identity_is_checked_before_executing_any_codec() {
    for input in [b"inpu".as_slice(), b"inputs", b"other"] {
        let result = canonicalize(
            Path::new("/missing/deadpan-codec"),
            &mut Cursor::new(input),
            identity(),
            &request(),
            &AtomicBool::new(false),
        );
        assert!(matches!(result, Err(ConversionError::InputIdentity)));
    }
    let mut too_large = request();
    too_large.input_byte_length = 1025;
    assert!(matches!(
        canonicalize(
            Path::new("/missing/deadpan-codec"),
            &mut Cursor::new(b"input"),
            identity(),
            &too_large,
            &AtomicBool::new(false)
        ),
        Err(ConversionError::Contract(_))
    ));
}

#[test]
fn output_is_hashed_only_after_clean_exit_and_complete_report() {
    let directory = tempfile::tempdir().unwrap();
    let executable = helper(directory.path(), &script_reply(success()));
    let mut output = canonicalize(
        &executable,
        &mut Cursor::new(b"input"),
        identity(),
        &request(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(output.object().byte_length(), 5);
    assert_eq!(
        output.object().content().digest(),
        blake3::hash(b"media").to_hex().as_str()
    );
    let mut bytes = Vec::new();
    output.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"media");
    assert_eq!(output.report(), &success());
}

#[test]
fn false_success_and_malformed_or_unbounded_replies_never_return_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let mut wrong_length = success();
    wrong_length.output_bytes = 6;
    let mut wrong_pixels = success();
    wrong_pixels.output_rgb_sha256 = "b".repeat(64);
    let mut wrong_time = success();
    wrong_time.last_output_pts = 1;
    let scripts = [
        format!("{}\nexit 7", script_reply(success())),
        script_reply(wrong_length),
        script_reply(wrong_pixels),
        script_reply(wrong_time),
        "printf '{broken' >&2".into(),
        format!("{}\nprintf '{{}}' >&2", script_reply(success())),
        "i=0; while [ $i -lt 9000 ]; do printf x >&2; i=$((i+1)); done".into(),
        "printf media".into(),
    ];
    for script in scripts {
        let executable = helper(directory.path(), &script);
        assert!(
            canonicalize(
                &executable,
                &mut Cursor::new(b"input"),
                identity(),
                &request(),
                &AtomicBool::new(false)
            )
            .is_err(),
            "accepted script: {script}"
        );
    }
}

#[test]
fn failure_diagnostic_and_output_budget_are_preserved() {
    let directory = tempfile::tempdir().unwrap();
    let executable = helper(
        directory.path(),
        "printf '%s' '{\"status\":\"failure\",\"code\":\"BadMedia\",\"message\":\"damaged frame\"}' >&2\nexit 1",
    );
    let error = canonicalize(
        &executable,
        &mut Cursor::new(b"input"),
        identity(),
        &request(),
        &AtomicBool::new(false),
    );
    assert!(
        matches!(error, Err(ConversionError::Worker { code, message }) if code == "BadMedia" && message == "damaged frame")
    );
    let executable = helper(directory.path(), &script_reply(success()));
    let mut small = request();
    small.limits.max_output_bytes = 4;
    assert!(
        canonicalize(
            &executable,
            &mut Cursor::new(b"input"),
            identity(),
            &small,
            &AtomicBool::new(false)
        )
        .is_err()
    );
}

#[test]
fn cancellation_and_hard_deadline_stop_the_process_group() {
    let directory = tempfile::tempdir().unwrap();
    let marker = directory.path().join("survived");
    let executable = helper(
        directory.path(),
        &format!("(sleep 1; printf alive > '{}') &\nwait", marker.display()),
    );
    let start = Instant::now();
    let mut short = request();
    short.limits.timeout_ms = 80;
    assert!(matches!(
        canonicalize(
            &executable,
            &mut Cursor::new(b"input"),
            identity(),
            &short,
            &AtomicBool::new(false)
        ),
        Err(ConversionError::Deadline)
    ));
    assert!(start.elapsed() < Duration::from_secs(2));
    let cancelled = AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(Duration::from_millis(50));
            cancelled.store(true, Ordering::Release);
        });
        assert!(matches!(
            canonicalize(
                &executable,
                &mut Cursor::new(b"input"),
                identity(),
                &request(),
                &cancelled
            ),
            Err(ConversionError::Cancelled)
        ));
    });
    std::thread::sleep(Duration::from_millis(1100));
    assert!(
        !marker.exists(),
        "a descendant continued after cancellation/deadline"
    );
}

#[test]
fn successful_leader_exit_cleans_up_descendants_with_inherited_pipes() {
    let directory = tempfile::tempdir().unwrap();
    let marker = directory.path().join("survived");
    let executable = helper(
        directory.path(),
        &format!(
            "(sleep 1; printf alive > '{}') &\n{}\nexit 0",
            marker.display(),
            script_reply(success())
        ),
    );
    let start = Instant::now();
    assert!(
        canonicalize(
            &executable,
            &mut Cursor::new(b"input"),
            identity(),
            &request(),
            &AtomicBool::new(false)
        )
        .is_ok()
    );
    assert!(start.elapsed() < Duration::from_secs(2));
    std::thread::sleep(Duration::from_millis(1100));
    assert!(!marker.exists());
}

#[test]
fn cancellation_during_snapshot_prevents_codec_start() {
    struct CancellingReader<'a>(&'a AtomicBool);
    impl Read for CancellingReader<'_> {
        fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
            output.copy_from_slice(&b"input"[..output.len()]);
            self.0.store(true, Ordering::Release);
            Ok(output.len())
        }
    }
    let cancelled = AtomicBool::new(false);
    assert!(matches!(
        canonicalize(
            Path::new("/missing/deadpan-codec"),
            &mut CancellingReader(&cancelled),
            identity(),
            &request(),
            &cancelled
        ),
        Err(ConversionError::Cancelled)
    ));
}
