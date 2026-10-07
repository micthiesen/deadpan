#![cfg(target_os = "macos")]
//! Hostile stand-ins for the tracking, face detection and transcription
//! workers, launched through each host's real supervision path (the trusted
//! runtime executable seam, `SupervisedProcess`, the checked process adapter
//! and the contained artifact reader). See docs/ADVERSARIAL.md#hostile-workers.
//!
//! Each case asserts a typed failure, no admitted result, bounded wall time
//! and, where the fixture recorded its group, that no group member survived.
//! A setsid escapee is deliberately outside group ownership: the test proves
//! it outlived the host and then kills it itself.

#[path = "hostile_workers/support.rs"]
mod support;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_analysis::{NormalizedRect, TrackPolicy};
use deadpan_cli::faces::{FaceError, FaceRequest, detect_faces, prepare_faces};
use deadpan_cli::tracking::{
    TrackRequest, TrackingError, TrackingResult, TrackingRuntime, prepare_tracking, track,
};
use deadpan_cli::transcription::{
    AnalysisInput, TranscriptionError, TranscriptionResult, TranscriptionRuntime, transcribe,
};
use deadpan_core::AssetId;
use deadpan_jobs::Sha256;
use deadpan_jobs::transcription::{Language, ModelInput};
use deadpan_store::{AccessMode, ProjectStore};
use serde_json::json;
use sha2::Digest;
use support::{Record, assert_alive, assert_bounded, fixture};

/// Generous wall-time bound for a refusal that must not wait for a deadline.
const PROMPT: Duration = Duration::from_secs(10);
/// A long deadline proves the refusal came from the frame or artifact check.
const LONG: Duration = Duration::from_secs(60);
/// A short deadline for hangs, and the bound for observing its enforcement.
const SHORT: Duration = Duration::from_millis(1_500);
const SHORT_BOUND: Duration = Duration::from_secs(12);

/// Artifact claims that a contained regular-file reader must refuse.
const ESCAPING_FILES: [&str; 6] = [
    "symlink_outside",
    "hardlink_outside",
    "symlinked_scope",
    "fifo",
    "sparse",
    "directory",
];

/// A refused claim must be the host's refusal of what the fixture said, not
/// the fixture crashing or exiting early.
fn assert_claim_refused(message: &str, name: &str) {
    assert!(
        !message.contains("exited") && !message.contains("without a terminal"),
        "{name}: the fixture must deliver its hostile claim: {message}"
    );
}

fn assert_outside_untouched(record: &Record, expected: &[u8]) {
    assert_eq!(
        std::fs::read(record.path().join("outside.bin")).unwrap(),
        expected,
        "the outside file must be neither consumed nor changed"
    );
}

// ---------------------------------------------------------------- transcription

fn model(record: &Record) -> ModelInput {
    let bytes = b"hostile fixture model; never loaded";
    let path = record.path().join("model.bin");
    std::fs::write(&path, bytes).unwrap();
    ModelInput {
        path: std::fs::canonicalize(path).unwrap(),
        sha256: Sha256::new(hex(&sha2::Sha256::digest(bytes))).unwrap(),
        byte_length: bytes.len() as u64,
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn transcribe_hostile(
    record: &Record,
    name: &str,
    within: Duration,
) -> Result<TranscriptionResult, TranscriptionError> {
    transcribe_cancellable(record, name, within, &AtomicBool::new(false))
}

fn transcribe_cancellable(
    record: &Record,
    name: &str,
    within: Duration,
    cancelled: &AtomicBool,
) -> Result<TranscriptionResult, TranscriptionError> {
    let runtime = TranscriptionRuntime {
        executable: record.wrapper(&fixture("analysis.py"), name),
        environment: Default::default(),
    };
    transcribe(
        &runtime,
        &model(record),
        &AnalysisInput {
            samples: vec![0.0; 1_600],
            origin: 0,
            source_rate: 48_000,
        },
        Language::Code("en".into()),
        "hostile-attempt",
        cancelled,
        Instant::now() + within,
        |_| {},
    )
}

#[test]
fn transcription_admits_only_a_contained_regular_transcript() {
    // Control: the same fixture's honest completion is admitted, so every
    // refusal below comes from the hostile claim, not from a broken fixture.
    let record = Record::new();
    let admitted = transcribe_hostile(&record, "valid", LONG).unwrap();
    assert!(admitted.transcript.words().is_empty());
    record.assert_group_gone();

    for name in ESCAPING_FILES {
        let record = Record::new();
        let started = Instant::now();
        let error = transcribe_hostile(&record, name, LONG).expect_err(name);
        assert_bounded(started, PROMPT, name);
        assert!(
            matches!(error, TranscriptionError::Artifact(_)),
            "{name}: {error:?}"
        );
        assert_outside_untouched(&record, b"[]");
        record.assert_group_gone();
    }
    for name in ["absolute", "parent", "wrong_attempt"] {
        let record = Record::new();
        let started = Instant::now();
        let error = transcribe_hostile(&record, name, LONG).expect_err(name);
        assert_bounded(started, PROMPT, name);
        assert!(
            matches!(
                error,
                TranscriptionError::Worker(_) | TranscriptionError::Protocol(_)
            ),
            "{name}: {error:?}"
        );
        assert_claim_refused(&error.to_string(), name);
        record.assert_group_gone();
    }
}

#[test]
fn transcription_bounds_frames_stderr_stalls_and_descendants() {
    for name in [
        "malformed",
        "invalid_utf8",
        "zero_length",
        "truncated",
        "oversized",
        "just_over",
        "fork_spam_exit",
        "stderr_flood",
    ] {
        let record = Record::new();
        let started = Instant::now();
        let error = transcribe_hostile(&record, name, LONG).expect_err(name);
        assert_bounded(started, PROMPT, name);
        assert!(
            matches!(error, TranscriptionError::Worker(_)),
            "{name}: {error:?}"
        );
        // A flood never reaches the typed error or the caller unbounded.
        support::assert_generic_cause(name, &error.to_string());
        record.assert_group_gone();
    }
    for name in ["slow_loris", "fork_spam"] {
        let record = Record::new();
        let started = Instant::now();
        let error = transcribe_hostile(&record, name, SHORT).expect_err(name);
        assert_bounded(started, SHORT_BOUND, name);
        assert!(
            matches!(error, TranscriptionError::Deadline),
            "{name}: {error:?}"
        );
        record.assert_group_gone();
    }
}

#[test]
fn cancelling_a_worker_that_ignores_it_mid_frame_reports_cancellation() {
    // The worker dribbles a frame and ignores the cooperative cancel until the
    // supervisor kills its group. The frame that kill truncated is not a
    // worker failure: the person cancelled, and the outcome says so.
    for name in ["slow_loris", "fork_spam"] {
        let record = Record::new();
        let cancelled = AtomicBool::new(false);
        let started = Instant::now();
        let result = std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(300));
                cancelled.store(true, std::sync::atomic::Ordering::Release);
            });
            transcribe_cancellable(&record, name, LONG, &cancelled)
        });
        assert_bounded(started, SHORT_BOUND, name);
        let error = result.expect_err(name);
        assert!(
            matches!(error, TranscriptionError::Cancelled),
            "{name}: {error:?}"
        );
        record.assert_group_gone();
    }
}

#[test]
fn transcription_escapee_keeping_pipes_fails_and_outlives_group_cleanup() {
    let record = Record::new();
    let started = Instant::now();
    let error = transcribe_hostile(&record, "escape", LONG).expect_err("escape");
    assert_bounded(started, PROMPT, "escape");
    assert!(matches!(error, TranscriptionError::Worker(_)), "{error:?}");
    record.assert_group_gone();
    // Not contained: setsid left the owned group. `Record` kills it.
    assert_alive(record.escaped(PROMPT));
}

// ---------------------------------------------------------------- tracking

fn cli(arguments: &[&str]) -> serde_json::Value {
    let output = Command::new(env!("CARGO_BIN_EXE_deadpan-cli"))
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn clip() -> AssetId {
    AssetId::new("clip").unwrap()
}

/// A project with the tracking fixture registered as asset `clip`.
fn registered(scratch: &Path) -> PathBuf {
    let package = scratch.join("hostile.deadpan");
    let path = package.to_str().unwrap();
    cli(&[
        "project", "create", path, "--fps", "24/1", "--size", "320x180",
    ]);
    let source = scratch.join("source.mkv");
    std::fs::write(
        &source,
        include_bytes!("../../../native/deadpan-track/tests/fixtures/moving-square-cut.mkv"),
    )
    .unwrap();
    let retained = cli(&["project", "retain-original", path, source.to_str().unwrap()]);
    let original = retained["retained_original"]["record"]["object"]["content"].clone();
    let head = ProjectStore::open(&package, AccessMode::ReadOnly)
        .unwrap()
        .snapshot()
        .unwrap();
    let request = json!({
        "protocol": 1,
        "registration": {
            "expected_revision": head.revision_id(), "new_revision": "registered",
            "original": original, "new_asset_id": "clip", "label": "Clip",
            "insertion": {"parent": head.root(), "index": 0, "node": "clip-source", "label": "Clip"}
        },
        "streams": {"type": "video_only"}
    });
    let request_path = scratch.join("request.json");
    std::fs::write(&request_path, serde_json::to_vec(&request).unwrap()).unwrap();
    cli(&[
        "project",
        "register-source",
        path,
        "--request-json",
        request_path.to_str().unwrap(),
    ]);
    package
}

fn track_hostile(
    package: &Path,
    record: &Record,
    name: &str,
    within: Duration,
) -> Result<TrackingResult, TrackingError> {
    let store = ProjectStore::open(package, AccessMode::ReadOnly).unwrap();
    let prepared = prepare_tracking(
        &store,
        &TrackRequest {
            asset: Some(clip()),
            from_pts: 0,
            to_pts: 400,
            region: NormalizedRect::new(0.125, 0.22, 0.1125, 0.2).unwrap(),
            stride: 2,
            stop_at_shots: false,
        },
        &AtomicBool::new(false),
        Instant::now() + LONG,
    )
    .unwrap();
    drop(store);
    std::fs::write(
        record.path().join("pictures.json"),
        serde_json::to_vec(prepared.pictures()).unwrap(),
    )
    .unwrap();
    let runtime = TrackingRuntime {
        executable: record.wrapper(&fixture("analysis.py"), name),
        environment: Default::default(),
    };
    track(
        &runtime,
        &prepared,
        TrackPolicy::default(),
        "hostile-attempt",
        &AtomicBool::new(false),
        Instant::now() + within,
        |_| {},
    )
}

#[test]
fn tracking_admits_only_contained_observations_and_stops_every_group() {
    let scratch = tempfile::tempdir().unwrap();
    let package = registered(scratch.path());
    let record = Record::new();
    let admitted = track_hostile(&package, &record, "valid", LONG).unwrap();
    assert_eq!(admitted.runtime.engine, "hostile-fixture");
    record.assert_group_gone();

    for name in ESCAPING_FILES {
        let record = Record::new();
        let started = Instant::now();
        let error = track_hostile(&package, &record, name, LONG).expect_err(name);
        assert_bounded(started, PROMPT, name);
        assert!(
            matches!(error, TrackingError::Artifact(_)),
            "{name}: {error:?}"
        );
        assert!(record.path().join("outside.bin").is_file());
        record.assert_group_gone();
    }
    for name in [
        "absolute",
        "parent",
        "wrong_attempt",
        "malformed",
        "invalid_utf8",
        "zero_length",
        "truncated",
        "oversized",
        "just_over",
        "fork_spam_exit",
        "stderr_flood",
    ] {
        let record = Record::new();
        let started = Instant::now();
        let error = track_hostile(&package, &record, name, LONG).expect_err(name);
        assert_bounded(started, PROMPT, name);
        assert!(
            matches!(error, TrackingError::Worker(_) | TrackingError::Protocol(_)),
            "{name}: {error:?}"
        );
        support::assert_generic_cause(name, &error.to_string());
        if ["absolute", "parent", "wrong_attempt"].contains(&name) {
            assert_claim_refused(&error.to_string(), name);
        }
        record.assert_group_gone();
    }
    for name in ["slow_loris", "fork_spam"] {
        let record = Record::new();
        let started = Instant::now();
        let error = track_hostile(&package, &record, name, SHORT).expect_err(name);
        assert_bounded(started, SHORT_BOUND, name);
        assert!(
            matches!(error, TrackingError::Deadline),
            "{name}: {error:?}"
        );
        record.assert_group_gone();
    }
    let record = Record::new();
    let error = track_hostile(&package, &record, "escape", LONG).expect_err("escape");
    assert!(matches!(error, TrackingError::Worker(_)), "{error:?}");
    record.assert_group_gone();
    assert_alive(record.escaped(PROMPT));
}

// ---------------------------------------------------------------- faces

fn faces_hostile(
    package: &Path,
    record: &Record,
    name: &str,
    within: Duration,
) -> Result<deadpan_cli::faces::FaceDetection, FaceError> {
    let store = ProjectStore::open(package, AccessMode::ReadOnly).unwrap();
    let prepared = prepare_faces(
        &store,
        &FaceRequest {
            asset: Some(clip()),
            at_pts: 0,
        },
        &AtomicBool::new(false),
        Instant::now() + LONG,
    )
    .unwrap();
    drop(store);
    let runtime = TrackingRuntime {
        executable: record.wrapper(&fixture("analysis.py"), name),
        environment: Default::default(),
    };
    detect_faces(
        &runtime,
        &prepared,
        "hostile-attempt",
        &AtomicBool::new(false),
        Instant::now() + within,
    )
}

#[test]
fn face_detection_refuses_hostile_frames_stalls_and_descendants() {
    let scratch = tempfile::tempdir().unwrap();
    let package = registered(scratch.path());
    for name in [
        "wrong_attempt",
        "malformed",
        "invalid_utf8",
        "zero_length",
        "truncated",
        "oversized",
        "just_over",
        "fork_spam_exit",
        "stderr_flood",
    ] {
        let record = Record::new();
        let started = Instant::now();
        let error = faces_hostile(&package, &record, name, LONG).expect_err(name);
        assert_bounded(started, PROMPT, name);
        assert!(
            matches!(error, FaceError::Worker(_) | FaceError::Protocol(_)),
            "{name}: {error:?}"
        );
        support::assert_generic_cause(name, &error.to_string());
        if name == "wrong_attempt" {
            assert_claim_refused(&error.to_string(), name);
        }
        record.assert_group_gone();
    }
    for name in ["slow_loris", "fork_spam"] {
        let record = Record::new();
        let started = Instant::now();
        let error = faces_hostile(&package, &record, name, SHORT).expect_err(name);
        assert_bounded(started, SHORT_BOUND, name);
        assert!(matches!(error, FaceError::Deadline), "{name}: {error:?}");
        record.assert_group_gone();
    }
    let record = Record::new();
    let error = faces_hostile(&package, &record, "escape", LONG).expect_err("escape");
    assert!(matches!(error, FaceError::Worker(_)), "{error:?}");
    record.assert_group_gone();
    assert_alive(record.escaped(PROMPT));
}
