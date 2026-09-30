#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_cli::encoded_render::{
    EncodedRenderError, EncodedWorkerLimits, encode, protocol::EncoderChoice,
};
use deadpan_cli::render_worker::{
    RenderPictureRequest, RenderWorkerRuntime, protocol::RenderIdentity,
};
use deadpan_core::{
    AudioSample, BeatNode, ColorPolicy, Command, CommandRequest, FrameDuration, FrameRange,
    FrameRate, HoldAudio, HoldRecipe, HoldVideo, NodeId, PresentationBasis, ProjectDocument,
    ProjectFrame, ProjectId, RevisionId, Subtree,
};
use deadpan_encode::{BFramePolicy, EncoderMode};
use deadpan_jobs::{AttemptId, CancellationToken, RequestId, Sha256};
use deadpan_store::{AccessMode, ProjectStore};
use sha2::{Digest, Sha256 as Sha256Hasher};

const PROCESS_LIMIT: Duration = Duration::from_secs(10);
const PAYLOAD: &[u8] = b"not an MP4: host-admission fixture\n";

fn fixture(package: &Path) -> ProjectDocument {
    let document = ProjectDocument::new(
        ProjectId::new("encoded-project").unwrap(),
        RevisionId::new("initial").unwrap(),
        PresentationBasis {
            width: 2,
            height: 2,
            frame_rate: FrameRate::new(30_000, 1_001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root").unwrap(),
    )
    .unwrap();
    let mut store = ProjectStore::create(package, &document).unwrap();
    let beat = NodeId::new("background").unwrap();
    store
        .commit(&CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new("committed").unwrap(),
            command: Command::Insert {
                parent: document.root().clone(),
                index: 0,
                subtree: Subtree {
                    root: beat.clone(),
                    nodes: BTreeMap::from([(
                        beat,
                        BeatNode::hold(
                            "Black",
                            HoldRecipe {
                                duration: FrameDuration::new(3).unwrap(),
                                video: HoldVideo::Background,
                                audio: HoldAudio::Silence,
                                picture_context: None,
                            },
                        ),
                    )]),
                    overrides: BTreeMap::new(),
                    gap_overrides: BTreeMap::new(),
                },
            },
        })
        .unwrap();
    store.snapshot().unwrap()
}

fn document_hash(document: &ProjectDocument) -> Sha256 {
    let digest = Sha256Hasher::digest(serde_json::to_vec(document).unwrap());
    Sha256::new(
        digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
    )
    .unwrap()
}

fn request(package: &Path) -> RenderPictureRequest {
    RenderPictureRequest {
        package: package.into(),
        revision: RevisionId::new("committed").unwrap(),
        range: Some(FrameRange::new(ProjectFrame(1), ProjectFrame(3)).unwrap()),
        identity: RenderIdentity {
            request_id: RequestId::new("encoded-integration").unwrap(),
            attempt_id: AttemptId::new("attempt-1").unwrap(),
        },
        cancellation_token: CancellationToken::new("cancel-encoded-attempt-1").unwrap(),
    }
}

fn choice() -> EncoderChoice {
    EncoderChoice {
        mode: EncoderMode::Hardware,
        b_frames: BFramePolicy::TargetTwo,
    }
}

fn limits() -> EncodedWorkerLimits {
    EncodedWorkerLimits {
        cancellation_grace: Duration::from_millis(100),
        exit_grace: Duration::from_secs(1),
        ..EncodedWorkerLimits::default()
    }
}

/// Only adversarial fixtures use Python. Production uses its host-selected
/// packaged native executable; these fixtures never encode or qualify media.
fn runtime(mode: &str) -> RenderWorkerRuntime {
    let executable = std::env::split_paths(&std::env::var_os("PATH").expect("test PATH"))
        .map(|directory| directory.join("python3"))
        .find(|candidate| candidate.is_file())
        .expect("Python 3 is required for encoded protocol fault fixtures");
    RenderWorkerRuntime {
        executable: fs::canonicalize(executable).unwrap(),
        arguments: vec![
            "-I".into(),
            "-u".into(),
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/encoded_render/fixture.py")
                .into_os_string(),
            mode.into(),
        ],
        environment: BTreeMap::new(),
    }
}

fn failure(package: &Path, mode: &str) -> EncodedRenderError {
    let mut previous = (0, 0);
    encode(
        &runtime(mode),
        request(package),
        choice(),
        limits(),
        &AtomicBool::new(false),
        Instant::now() + PROCESS_LIMIT,
        |update| {
            assert!(update.completed_frames >= previous.0);
            assert!(update.completed_audio_samples >= previous.1);
            previous = (update.completed_frames, update.completed_audio_samples);
        },
    )
    .err()
    .unwrap_or_else(|| panic!("host admitted hostile fixture {mode}"))
}

fn assert_rejected_claim(error: EncodedRenderError, mode: &str) {
    let message = match error {
        EncodedRenderError::Worker(message) | EncodedRenderError::Protocol(message) => message,
        error => panic!("{mode}: expected claim rejection before snapshot, got {error:?}"),
    };
    assert!(
        !message.contains("worker exited"),
        "{mode}: fixture must emit its changed claim, not crash: {message}"
    );
}

#[test]
fn host_preserves_the_first_worker_diagnostic_and_never_snapshots_failed_completion() {
    let scratch = tempfile::tempdir().unwrap();
    let package = scratch.path().join("failure.deadpan");
    fixture(&package);
    let error = failure(&package, "failed_exit");
    assert!(
        matches!(error, EncodedRenderError::Worker(ref message)
            if message == "fixture encoder rejected captured input"),
        "{error:?}"
    );
    for mode in [
        "partial_header",
        "partial_body",
        "malformed",
        "oversized",
        "stale_attempt",
        "wrong_request",
        "wrong_version",
        "completed_exit_failure",
        "after_terminal",
        "duplicate_terminal",
        "video_regression",
        "audio_regression",
        "wrong_video_total",
        "wrong_audio_total",
        "over_video",
        "over_audio",
    ] {
        let error = failure(&package, mode);
        // Each of these leaves a FIFO at the claimed movie path. An Artifact
        // error would expose an incorrect attempt to snapshot before admission.
        assert!(
            matches!(
                error,
                EncodedRenderError::Worker(_) | EncodedRenderError::Protocol(_)
            ),
            "{mode}: {error:?}"
        );
    }
}

#[test]
fn completed_claims_bind_every_captured_clock_field_and_explicit_encoder_choice() {
    let scratch = tempfile::tempdir().unwrap();
    let package = scratch.path().join("contract.deadpan");
    let document = fixture(&package);
    for mode in [
        "wrong_document",
        "wrong_movie_path",
        "outside_scope",
        "wrong_mode",
        "wrong_b_frames",
        "contract:project_id",
        "contract:revision_id",
        "contract:range",
        "contract:canvas",
        "contract:raster",
        "contract:frame_rate",
        "contract:color_policy",
        "contract:time_base",
        "contract:frame_count",
        "contract:terminal_pts",
        "contract:project_audio_start",
        "contract:project_audio_end",
        "contract:relative_aspect_error",
    ] {
        assert_rejected_claim(failure(&package, mode), mode);
    }
    let store = ProjectStore::open(&package, AccessMode::ReadOnly).unwrap();
    assert_eq!(store.snapshot().unwrap(), document);
}

#[test]
fn inconsistent_encoder_reports_are_rejected_before_artifact_access() {
    let scratch = tempfile::tempdir().unwrap();
    let package = scratch.path().join("report.deadpan");
    fixture(&package);
    for mode in [
        "report:video_frames",
        "report:audio_samples",
        "report:video_packets",
        "report:audio_packets",
        "report:output_bytes",
        "report:packet_bytes",
        "report:video_duration_from_contract_packets",
        "report:faststart_read_opens",
        "report:faststart_read_closes",
        "report:video_eof",
        "report:audio_eof",
        "info:abi_version",
        "info:avcodec_version",
        "info:movie_timescale",
        "info:video_time_base_num",
        "info:video_time_base_den",
        "info:audio_time_base_num",
        "info:audio_time_base_den",
        "info:audio_frame_size",
        "info:video_profile",
        "info:audio_profile",
        "info:video_max_b_frames",
        "info:video_has_b_frames",
        "info:video_gop_size",
        "info:audio_initial_padding",
        "info:audio_trailing_padding",
        "info:requested_mode",
        "info:video_bitrate",
        "info:audio_bitrate",
        "info:maximum_moov_bytes",
    ] {
        assert_rejected_claim(failure(&package, mode), mode);
    }
}

#[test]
fn artifact_hash_size_and_descriptor_containment_are_independent_admission_checks() {
    let scratch = tempfile::tempdir().unwrap();
    let package = scratch.path().join("artifact.deadpan");
    fixture(&package);
    for mode in [
        "wrong_hash",
        "short_file",
        "long_file",
        "symlink",
        "hardlink",
    ] {
        let error = failure(&package, mode);
        assert!(
            matches!(error, EncodedRenderError::Artifact(_)),
            "{mode}: {error:?}"
        );
    }
    let mut constrained = limits();
    constrained.encode.maximum_output_bytes = u64::try_from(PAYLOAD.len() - 1).unwrap();
    let error = encode(
        &runtime("valid"),
        request(&package),
        choice(),
        constrained,
        &AtomicBool::new(false),
        Instant::now() + PROCESS_LIMIT,
        |_| {},
    )
    .err()
    .expect("declared output over budget");
    assert!(
        matches!(
            error,
            EncodedRenderError::Worker(_) | EncodedRenderError::Protocol(_)
        ),
        "{error:?}"
    );
}

#[test]
fn candidate_keeps_exact_nonzero_origin_and_private_bytes_after_paths_disappear() {
    let scratch = tempfile::tempdir().unwrap();
    let package = scratch.path().join("retained.deadpan");
    let document = fixture(&package);
    let mut selected = request(&package);
    selected.range = Some(FrameRange::new(ProjectFrame(1), ProjectFrame(2)).unwrap());
    let mut candidate = encode(
        &runtime("valid"),
        selected,
        choice(),
        limits(),
        &AtomicBool::new(false),
        Instant::now() + PROCESS_LIMIT,
        |_| panic!("fixture sends no progress"),
    )
    .unwrap();
    assert_eq!(candidate.document_sha256(), &document_hash(&document));
    assert_eq!(candidate.contract().revision_id(), document.revision_id());
    assert_eq!(
        candidate.manifest().contract.picture.project_audio_start,
        AudioSample(1_602)
    );
    assert_eq!(
        candidate.manifest().contract.picture.project_audio_end,
        AudioSample(3_203)
    );
    assert_eq!(candidate.manifest().report.audio_samples, 1_601);
    assert_eq!(
        candidate.byte_length(),
        u64::try_from(PAYLOAD.len()).unwrap()
    );
    drop(scratch);

    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() + PROCESS_LIMIT;
    let mut prefix = [0; 7];
    assert_eq!(
        candidate
            .read_at(0, &mut prefix, &cancelled, deadline)
            .unwrap(),
        7
    );
    assert_eq!(prefix, PAYLOAD[..7]);
    let mut tail = [0; 8];
    assert_eq!(
        candidate
            .read_at(candidate.byte_length() - 2, &mut tail, &cancelled, deadline)
            .unwrap(),
        2
    );
    assert_eq!(&tail[..2], &PAYLOAD[PAYLOAD.len() - 2..]);
    assert_eq!(
        candidate
            .read_at(candidate.byte_length(), &mut tail, &cancelled, deadline)
            .unwrap(),
        0
    );
    assert!(matches!(
        candidate.read_at(candidate.byte_length() + 1, &mut tail, &cancelled, deadline),
        Err(EncodedRenderError::Configuration(_))
    ));
    assert!(matches!(
        candidate.read_at(0, &mut [0; 65_537], &cancelled, deadline),
        Err(EncodedRenderError::Configuration(_))
    ));
    let mut bytes = Vec::new();
    assert_eq!(
        candidate.copy_to(&mut bytes, &cancelled, deadline).unwrap(),
        candidate.byte_length()
    );
    assert_eq!(bytes, PAYLOAD);

    let mut untouched = Vec::new();
    assert!(matches!(
        candidate.copy_to(&mut untouched, &AtomicBool::new(true), deadline),
        Err(EncodedRenderError::Cancelled)
    ));
    assert!(matches!(
        candidate.copy_to(&mut untouched, &cancelled, Instant::now()),
        Err(EncodedRenderError::Deadline)
    ));
    assert!(untouched.is_empty());
    struct CancellingSink<'a> {
        cancelled: &'a AtomicBool,
        bytes: Vec<u8>,
    }
    impl Write for CancellingSink<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.bytes.push(bytes[0]);
            self.cancelled.store(true, Ordering::Release);
            Ok(1)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let copy_cancelled = AtomicBool::new(false);
    let mut interrupted = CancellingSink {
        cancelled: &copy_cancelled,
        bytes: Vec::new(),
    };
    assert!(matches!(
        candidate.copy_to(&mut interrupted, &copy_cancelled, deadline),
        Err(EncodedRenderError::Cancelled)
    ));
    assert_eq!(interrupted.bytes, PAYLOAD[..1]);
    struct RefusingSink;
    impl Write for RefusingSink {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "fixture sink refused bytes",
            ))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    assert!(matches!(
        candidate.copy_to(&mut RefusingSink, &cancelled, deadline),
        Err(EncodedRenderError::Io(_))
    ));
    let mut recovered = Vec::new();
    candidate
        .copy_to(&mut recovered, &cancelled, deadline)
        .unwrap();
    assert_eq!(recovered, PAYLOAD);
}

#[test]
fn cancellation_after_progress_and_deadline_stop_the_process_before_snapshotting() {
    let scratch = tempfile::tempdir().unwrap();
    let package = scratch.path().join("control.deadpan");
    fixture(&package);
    let cancelled = AtomicBool::new(false);
    let mut saw_progress = false;
    let error = encode(
        &runtime("cancel"),
        request(&package),
        choice(),
        limits(),
        &cancelled,
        Instant::now() + PROCESS_LIMIT,
        |update| {
            assert!(update.completed_frames < update.total_frames);
            assert!(update.completed_audio_samples < update.total_audio_samples);
            saw_progress = true;
            cancelled.store(true, Ordering::Release);
        },
    )
    .err()
    .expect("cancelled attempt must not return a candidate");
    assert!(saw_progress);
    assert!(matches!(error, EncodedRenderError::Cancelled), "{error:?}");

    let mut saw_progress = false;
    let error = encode(
        &runtime("ignore_cancel"),
        request(&package),
        choice(),
        limits(),
        &AtomicBool::new(false),
        Instant::now() + Duration::from_secs(2),
        |_| saw_progress = true,
    )
    .err()
    .expect("expired attempt must not return a candidate");
    assert!(saw_progress, "fixture must run before deadline expires");
    assert!(matches!(error, EncodedRenderError::Deadline), "{error:?}");
    // A fresh attempt succeeds after both groups have been stopped and reaped.
    let mut retry = request(&package);
    retry.identity.attempt_id = AttemptId::new("attempt-2").unwrap();
    retry.cancellation_token = CancellationToken::new("cancel-encoded-attempt-2").unwrap();
    assert!(
        encode(
            &runtime("valid"),
            retry,
            choice(),
            limits(),
            &AtomicBool::new(false),
            Instant::now() + PROCESS_LIMIT,
            |_| {},
        )
        .is_ok()
    );
}

#[test]
fn cancelled_expired_and_invalid_limit_preflight_never_launch_the_runtime() {
    let missing = RenderWorkerRuntime {
        executable: "/missing-encoded-worker".into(),
        arguments: vec![],
        environment: BTreeMap::new(),
    };
    let package = Path::new("/missing-encoded-project.deadpan");
    assert!(matches!(
        encode(
            &missing,
            request(package),
            choice(),
            limits(),
            &AtomicBool::new(true),
            Instant::now() + PROCESS_LIMIT,
            |_| {},
        ),
        Err(EncodedRenderError::Cancelled)
    ));
    assert!(matches!(
        encode(
            &missing,
            request(package),
            choice(),
            limits(),
            &AtomicBool::new(false),
            Instant::now(),
            |_| {},
        ),
        Err(EncodedRenderError::Deadline)
    ));
    let mut invalid = limits();
    invalid.encode.maximum_output_bytes = 0;
    assert!(matches!(
        encode(
            &missing,
            request(package),
            choice(),
            invalid,
            &AtomicBool::new(false),
            Instant::now() + PROCESS_LIMIT,
            |_| {},
        ),
        Err(EncodedRenderError::Encode(_))
    ));
}
