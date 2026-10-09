#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::{
    collections::BTreeMap,
    fs::{self, File},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use deadpan_cli::{
    encoded_render::{
        EncodedCandidate, EncodedRenderError, EncodedWorkerLimits, encode,
        protocol::EncodedManifest,
        verification::{VerificationLimits, VerificationRequest, VerificationStage, verify},
    },
    render_worker::{RenderPictureRequest, RenderWorkerRuntime, protocol::RenderIdentity},
};
use deadpan_core::{
    BeatNode, Command, CommandRequest, FrameDuration, HoldAudio, HoldRecipe, HoldVideo, NodeId,
    PresentationBasis, ProjectDocument, RevisionId, Subtree,
};
use deadpan_jobs::{AttemptId, CancellationToken, RequestId};
use deadpan_source::{DecodeControl, DecodeLimits, Mp4PacketObservation, Mp4PacketReader};
use deadpan_store::{AccessMode, ProjectStore};
use sha2::{Digest, Sha256};

static NOT_CANCELLED: AtomicBool = AtomicBool::new(false);
const PROCESS_LIMIT: Duration = Duration::from_secs(60);

#[path = "encoded_verification/publication.rs"]
mod publication;

#[path = "encoded_verification/jobs.rs"]
mod jobs;

#[cfg(target_os = "macos")]
#[path = "encoded_verification/journal.rs"]
mod journal;

fn deadline() -> Instant {
    Instant::now() + PROCESS_LIMIT
}

fn assets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/encoded_verification")
}

fn identity(attempt: &str) -> RenderIdentity {
    RenderIdentity {
        request_id: RequestId::new("verification-integration").unwrap(),
        attempt_id: AttemptId::new(attempt).unwrap(),
    }
}

fn request(attempt: &str) -> VerificationRequest {
    VerificationRequest {
        identity: identity(attempt),
        cancellation_token: CancellationToken::new(format!("cancel-{attempt}")).unwrap(),
        limits: VerificationLimits::default(),
    }
}

fn python(arguments: impl IntoIterator<Item = std::ffi::OsString>) -> RenderWorkerRuntime {
    let executable = std::env::split_paths(&std::env::var_os("PATH").expect("test PATH"))
        .map(|directory| directory.join("python3"))
        .find(|candidate| candidate.is_file())
        .expect("Python 3 is required for verification transport fixtures");
    let mut args = vec![
        "-I".into(),
        "-u".into(),
        assets().join("fixture.py").into_os_string(),
    ];
    args.extend(arguments);
    RenderWorkerRuntime {
        executable: fs::canonicalize(executable).unwrap(),
        arguments: args,
        environment: BTreeMap::new(),
    }
}

fn native() -> RenderWorkerRuntime {
    RenderWorkerRuntime {
        executable: PathBuf::from(env!("CARGO_BIN_EXE_deadpan-cli")),
        arguments: Vec::new(),
        environment: BTreeMap::new(),
    }
}

fn hostile(mode: &str) -> RenderWorkerRuntime {
    python(["verify".into(), mode.into()])
}

struct Fixture {
    scratch: tempfile::TempDir,
    package: PathBuf,
    document: ProjectDocument,
    manifest: EncodedManifest,
    manifest_path: PathBuf,
    movie_path: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let manifest_path = assets().join("fixtures").join(format!("{name}.json"));
        let movie_path = assets().join("fixtures").join(format!("{name}.mp4"));
        let manifest: EncodedManifest =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest.validate().unwrap();
        let bytes = fs::read(&movie_path).unwrap();
        assert_eq!(
            Sha256::digest(&bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            manifest.movie.sha256().as_str()
        );
        assert_eq!(bytes.len() as u64, manifest.movie.byte_length());
        let picture = &manifest.contract.picture;
        let scratch = tempfile::tempdir().unwrap();
        let package = scratch.path().join("capture.deadpan");
        let initial = ProjectDocument::new(
            picture.project_id.clone(),
            RevisionId::new("fixture-initial").unwrap(),
            PresentationBasis {
                width: picture.canvas[0],
                height: picture.canvas[1],
                frame_rate: picture.frame_rate,
                color_policy: picture.color_policy,
            },
            NodeId::new("root").unwrap(),
        )
        .unwrap();
        let mut store = ProjectStore::create(&package, &initial).unwrap();
        let beat = NodeId::new("background").unwrap();
        store
            .commit(&CommandRequest {
                project_id: initial.project_id().clone(),
                expected_revision: initial.revision_id().clone(),
                new_revision: picture.revision_id.clone(),
                command: Command::Insert {
                    parent: initial.root().clone(),
                    index: 0,
                    subtree: Subtree {
                        root: beat.clone(),
                        nodes: BTreeMap::from([(
                            beat,
                            BeatNode::hold(
                                "Transport fixture",
                                HoldRecipe {
                                    duration: FrameDuration::new(picture.range.end().0).unwrap(),
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
        let document = store.snapshot().unwrap();
        drop(store);
        Self {
            scratch,
            package,
            document,
            manifest,
            manifest_path,
            movie_path,
        }
    }

    /// The mock transport does not encode this background project. It replays
    /// independently retained bytes under the identical output clock contract.
    fn candidate(&self, movie: &Path) -> EncodedCandidate {
        let runtime = python([
            "encode".into(),
            self.manifest_path.clone().into_os_string(),
            movie.as_os_str().into(),
        ]);
        let candidate = encode(
            &runtime,
            RenderPictureRequest {
                package: self.package.clone(),
                revision: self.manifest.contract.picture.revision_id.clone(),
                range: Some(self.manifest.contract.picture.range),
                identity: identity("replay"),
                cancellation_token: CancellationToken::new("cancel-replay").unwrap(),
            },
            self.manifest.contract.choice,
            EncodedWorkerLimits::default(),
            &NOT_CANCELLED,
            deadline(),
            |_| {},
        )
        .unwrap();
        assert_eq!(candidate.manifest().contract, self.manifest.contract);
        assert_ne!(candidate.document_sha256(), &self.manifest.document_sha256);
        candidate
    }

    fn original_candidate(&self) -> EncodedCandidate {
        self.candidate(&self.movie_path)
    }

    fn assert_project_unchanged(&self) {
        let store = ProjectStore::open(&self.package, AccessMode::ReadOnly).unwrap();
        assert_eq!(store.snapshot().unwrap(), self.document);
    }
}

fn read_candidate(candidate: &mut EncodedCandidate) -> Vec<u8> {
    let mut bytes = Vec::new();
    let count = candidate
        .copy_to(&mut bytes, &NOT_CANCELLED, deadline())
        .unwrap();
    assert_eq!(count, bytes.len() as u64);
    bytes
}

#[test]
fn actual_finished_files_pass_complete_native_inspection_and_retain_exact_bytes() {
    for (name, frames, samples) in [
        ("nonzero", 1, 1601),
        ("software-two", 43, 68869),
        ("marker", 120, 96000),
    ] {
        let fixture = Fixture::new(name);
        let candidate = fixture.original_candidate();
        let captured_hash = candidate.document_sha256().clone();
        let mut previous = None;
        let mut verified = verify(
            &native(),
            candidate,
            request("native"),
            &NOT_CANCELLED,
            deadline(),
            |update| {
                assert!(update.completed <= update.total);
                if let Some((stage, completed)) = previous {
                    assert!(
                        update.stage > stage
                            || (update.stage == stage && update.completed >= completed)
                    );
                }
                previous = Some((update.stage, update.completed));
            },
        )
        .unwrap_or_else(|error| panic!("{name}: {error}"));
        let report = verified.report();
        report.validate(VerificationLimits::default()).unwrap();
        assert_eq!(report.video_frames, frames);
        assert_eq!(report.fresh_gop_frames, frames);
        assert_eq!(report.audio_samples, samples);
        assert_eq!(report.document_sha256, captured_hash);
        assert_eq!(report.movie_sha256, *fixture.manifest.movie.sha256());
        assert_eq!(report.manual_first_sample, -2048);
        assert_eq!(report.ordinary_first_sample, 0);
        assert_eq!(
            report.manual_physical_samples,
            report.ordinary_physical_samples + 2048
        );
        if name == "software-two" {
            assert!(report.maximum_b_run > 0 && report.maximum_b_run <= 2);
            assert!(report.gops > 1);
        } else {
            assert_eq!(report.maximum_b_run, 0);
        }
        let mut retained = Vec::new();
        verified
            .copy_to(&mut retained, &NOT_CANCELLED, deadline())
            .unwrap();
        assert_eq!(retained, fs::read(&fixture.movie_path).unwrap());
        fixture.assert_project_unchanged();
    }
}

#[test]
fn cancellation_after_real_progress_retains_a_retryable_candidate() {
    let fixture = Fixture::new("marker");
    let cancelled = AtomicBool::new(false);
    let mut seen = false;
    let failed = verify(
        &native(),
        fixture.original_candidate(),
        request("cancel"),
        &cancelled,
        deadline(),
        |update| {
            assert!(matches!(
                update.stage,
                VerificationStage::Packets
                    | VerificationStage::Pictures
                    | VerificationStage::ManualAudio
                    | VerificationStage::OrdinaryAudio
            ));
            seen = true;
            cancelled.store(true, Ordering::Release);
        },
    )
    .err()
    .expect("cancellation must not admit media");
    assert!(seen, "native verifier must report actual progress");
    assert!(
        matches!(failed.error, EncodedRenderError::Cancelled),
        "{}",
        failed.error
    );
    cancelled.store(false, Ordering::Release);
    let mut candidate = failed.candidate;
    assert_eq!(
        read_candidate(&mut candidate),
        fs::read(&fixture.movie_path).unwrap()
    );
    let verified = verify(
        &native(),
        candidate,
        request("retry"),
        &cancelled,
        deadline(),
        |_| {},
    )
    .unwrap();
    assert_eq!(verified.report().video_frames, 120);
    fixture.assert_project_unchanged();
}

#[test]
fn preflight_failures_and_missing_runtime_preserve_completed_bytes_for_retry() {
    let fixture = Fixture::new("nonzero");
    let mut candidate = fixture.original_candidate();
    let expected = fs::read(&fixture.movie_path).unwrap();
    for failure in [
        "cancelled",
        "deadline",
        "bytes",
        "packets",
        "limits",
        "runtime",
    ] {
        let cancelled = AtomicBool::new(failure == "cancelled");
        let mut selected = request(failure);
        if failure == "bytes" {
            selected.limits.maximum_bytes = candidate.byte_length() - 1;
        }
        if failure == "packets" {
            selected.limits.maximum_packets = 1;
        }
        if failure == "limits" {
            selected.limits.maximum_packets = 0;
        }
        let mut runtime = native();
        runtime.executable = fixture.scratch.path().join("missing-verifier");
        let until = if failure == "deadline" {
            Instant::now()
        } else {
            deadline()
        };
        let rejected = verify(&runtime, candidate, selected, &cancelled, until, |_| {
            panic!("preflight must not report progress")
        })
        .err()
        .expect("failed verification must retain the encode");
        match failure {
            "cancelled" => assert!(matches!(rejected.error, EncodedRenderError::Cancelled)),
            "deadline" => assert!(matches!(rejected.error, EncodedRenderError::Deadline)),
            "runtime" => assert!(matches!(rejected.error, EncodedRenderError::Supervisor(_))),
            _ => assert!(matches!(rejected.error, EncodedRenderError::Protocol(_))),
        }
        candidate = rejected.candidate;
        assert_eq!(read_candidate(&mut candidate), expected);
    }
    verify(
        &native(),
        candidate,
        request("recovered"),
        &NOT_CANCELLED,
        deadline(),
        |_| {},
    )
    .unwrap();
    fixture.assert_project_unchanged();
}

fn tag(bytes: &[u8], value: &[u8; 4], ordinal: usize) -> usize {
    let found = bytes
        .windows(4)
        .enumerate()
        .filter_map(|(index, item)| {
            if item != value || index < 4 {
                return None;
            }
            let size = usize::try_from(u32::from_be_bytes(
                bytes[index - 4..index].try_into().unwrap(),
            ))
            .unwrap();
            // Distinguish bounded boxes from ftyp brand strings, notably the
            // compatible avc1 brand preceding the actual sample description.
            (size >= 8 && size <= bytes.len() - (index - 4)).then_some(index + 4)
        })
        .nth(ordinal)
        .unwrap();
    assert!(found < bytes.windows(4).position(|part| part == b"mdat").unwrap());
    found
}

fn first_video_packet(path: &Path) -> Mp4PacketObservation {
    let control = DecodeControl {
        timeout: Duration::from_secs(10),
        cancelled: &NOT_CANCELLED,
    };
    let mut reader =
        Mp4PacketReader::open(File::open(path).unwrap(), DecodeLimits::default(), control).unwrap();
    loop {
        let packet = reader.next_packet(control).unwrap().unwrap();
        if packet.h264.is_some() {
            return packet;
        }
    }
}

fn mutate(fixture: &Fixture, mutation: &str) -> Vec<u8> {
    let mut bytes = fs::read(&fixture.movie_path).unwrap();
    match mutation {
        "geometry" => {
            let at = tag(&bytes, b"avc1", 0);
            bytes[at + 24..at + 26].copy_from_slice(&322_u16.to_be_bytes());
        }
        "color" => {
            let at = tag(&bytes, b"colr", 0);
            bytes[at + 4..at + 6].copy_from_slice(&9_u16.to_be_bytes());
        }
        "audio_edit" => {
            let at = tag(&bytes, b"elst", 1);
            assert_eq!(bytes[at], 0);
            bytes[at + 12..at + 16].copy_from_slice(&0_u32.to_be_bytes());
        }
        "duration" => {
            let at = tag(&bytes, b"mvhd", 0);
            assert_eq!(bytes[at], 0);
            bytes[at + 16..at + 20].copy_from_slice(&1_u32.to_be_bytes());
        }
        "sync" => {
            let at = tag(&bytes, b"stss", 0);
            bytes[at + 8..at + 12].copy_from_slice(&2_u32.to_be_bytes());
        }
        "nal_length" | "key_nal" | "slice_payload" => {
            let packet = first_video_packet(&fixture.movie_path);
            let start = usize::try_from(packet.offset).unwrap();
            let end = start + usize::try_from(packet.length).unwrap();
            if mutation == "nal_length" {
                bytes[start..start + 4].copy_from_slice(&(packet.length + 1).to_be_bytes());
            } else {
                let mut at = start;
                let mut changed = false;
                while at < end {
                    let size =
                        usize::try_from(u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()))
                            .unwrap();
                    at += 4;
                    assert!(size > 0 && at + size <= end);
                    if bytes[at] & 31 == 5 {
                        if mutation == "key_nal" {
                            bytes[at] = (bytes[at] & 0xe0) | 1;
                        } else {
                            bytes[at + 1..at + size].fill(0);
                        }
                        changed = true;
                    }
                    at += size;
                }
                assert!(changed, "retained opening packet must contain IDR slices");
            }
        }
        _ => panic!("unknown mutation"),
    }
    bytes
}

#[test]
fn recomputed_hashes_do_not_admit_altered_headers_clocks_or_h264_content() {
    let fixture = Fixture::new("software-two");
    for mutation in [
        "geometry",
        "color",
        "audio_edit",
        "duration",
        "sync",
        "nal_length",
        "key_nal",
        "slice_payload",
    ] {
        let bytes = mutate(&fixture, mutation);
        let path = fixture.scratch.path().join(format!("{mutation}.mp4"));
        fs::write(&path, &bytes).unwrap();
        let mut candidate = fixture.candidate(&path);
        assert_eq!(
            candidate.manifest().movie.sha256().as_str(),
            Sha256::digest(&bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        assert_eq!(read_candidate(&mut candidate), bytes);
        let rejected = verify(
            &native(),
            candidate,
            request(mutation),
            &NOT_CANCELLED,
            deadline(),
            |_| {},
        )
        .err()
        .unwrap_or_else(|| panic!("native verifier accepted {mutation}"));
        let EncodedRenderError::Worker(message) = &rejected.error else {
            panic!(
                "{mutation}: expected media rejection, got {}",
                rejected.error
            );
        };
        let expected: &[&str] = match mutation {
            "geometry" | "color" => {
                &["ExportVerificationFailed: video sample description or media clock differs"]
            }
            "audio_edit" => &[
                "ExportVerificationFailed: audio sample tables do not retain exact AAC priming and authored endpoint",
            ],
            "duration" => {
                &["ExportVerificationFailed: movie geometry, duration or fast-start differs"]
            }
            "sync" | "key_nal" => {
                &["ExportVerificationFailed: sync table differs from actual IDR picture NALs"]
            }
            "nal_length" => &["source decode invalid_input: AVC NAL escapes its packet"],
            "slice_payload" => &[
                "source decode ffmpeg_failure: send source packet:",
                "source decode ffmpeg_failure: receive source frame:",
                "source decode corrupt_frame: decoder reported a corrupt or concealed frame",
            ],
            _ => unreachable!("declared mutation"),
        };
        assert!(
            expected.iter().any(|expected| message.contains(*expected)),
            "{mutation}: expected {expected:?}, got {message}"
        );
        let mut retained = rejected.candidate;
        assert_eq!(read_candidate(&mut retained), bytes);
    }
    fixture.assert_project_unchanged();
}

#[test]
fn hostile_verifier_messages_never_replace_private_bytes_with_success() {
    let fixture = Fixture::new("nonzero");
    let mut candidate = fixture.original_candidate();
    let expected = fs::read(&fixture.movie_path).unwrap();
    for mode in [
        "malformed",
        "partial_body",
        "stale_attempt",
        "after_terminal",
        "duplicate_terminal",
        "stage_regression",
        "count_regression",
        "wrong_total",
        "wrong_stage",
        "over_progress",
        "inconsistent_report",
        "wrong_movie",
        "wrong_document",
        "wrong_runtime",
        "completed_exit_failure",
        "failed_exit",
    ] {
        // Completion is deliberately withheld by the host after a failed exit.
        // Retain an independent witness that this fixture emitted it first.
        let witness = fixture.scratch.path().join("failed-exit-completion.json");
        let runtime = if mode == "completed_exit_failure" {
            python([
                "verify".into(),
                mode.into(),
                witness.clone().into_os_string(),
            ])
        } else {
            hostile(mode)
        };
        let captured_manifest = candidate.manifest().clone();
        let rejected = verify(
            &runtime,
            candidate,
            request(mode),
            &NOT_CANCELLED,
            deadline(),
            |_| {},
        )
        .err()
        .unwrap_or_else(|| panic!("host accepted hostile verifier {mode}"));
        let message = match &rejected.error {
            EncodedRenderError::Worker(message) | EncodedRenderError::Protocol(message) => message,
            error => panic!("{mode}: expected protocol rejection, got {error}"),
        };
        let expected_diagnostic = match mode {
            "malformed" | "wrong_stage" => "worker payload is malformed",
            "partial_body" => "worker frame ended after 1 of 100 payload bytes",
            "stale_attempt" => "verification response belongs to another attempt",
            "after_terminal" | "duplicate_terminal" => {
                "worker emitted a message after its terminal response"
            }
            "stage_regression" | "count_regression" => "verification progress moved backward",
            "wrong_total" => "verification progress differs from captured work",
            "over_progress" => "verification progress exceeds its stage",
            "inconsistent_report" | "wrong_runtime" => {
                "verification report contradicts the exact output policy"
            }
            "wrong_movie" | "wrong_document" => {
                "verification result changed its captured bytes or contract"
            }
            "completed_exit_failure" => "worker exited unsuccessfully: exit status: 1",
            "failed_exit" => "fixture verifier rejected captured bytes",
            _ => unreachable!("declared hostile mode"),
        };
        assert!(
            message.contains(expected_diagnostic),
            "{mode}: expected {expected_diagnostic:?}, got {message}"
        );
        if mode == "completed_exit_failure" {
            let completed: deadpan_cli::encoded_render::verification::protocol::WorkerMessage =
                serde_json::from_slice(&fs::read(&witness).expect("completion emission witness"))
                    .unwrap();
            completed.validate().unwrap();
            let deadpan_cli::encoded_render::verification::protocol::WorkerMessage::Completed {
                identity: observed,
                report,
                ..
            } = completed
            else {
                panic!("failed-exit witness must contain the emitted completion");
            };
            assert_eq!(observed, identity(mode));
            assert_eq!(report.contract, captured_manifest.contract);
            assert_eq!(report.document_sha256, captured_manifest.document_sha256);
            assert_eq!(report.movie_sha256, *captured_manifest.movie.sha256());
            assert_eq!(report.movie_bytes, captured_manifest.movie.byte_length());
        }
        candidate = rejected.candidate;
        assert_eq!(read_candidate(&mut candidate), expected);
    }
    // A genuine native success after transport failures proves the owned
    // candidate remained usable, independently of the mock's report claims.
    verify(
        &native(),
        candidate,
        request("after-faults"),
        &NOT_CANCELLED,
        deadline(),
        |_| {},
    )
    .unwrap();
    fixture.assert_project_unchanged();
}

// Hostile stand-ins through the real verification host
// (docs/ADVERSARIAL.md#hostile-workers).

#[path = "hostile_workers/support.rs"]
mod support;

#[test]
fn hostile_verifier_processes_are_bounded_stopped_and_cannot_touch_the_candidate() {
    let fixture = Fixture::new("nonzero");
    let mut candidate = fixture.original_candidate();
    let expected = fs::read(&fixture.movie_path).unwrap();
    for (name, within) in [
        ("malformed", PROCESS_LIMIT),
        ("invalid_utf8", PROCESS_LIMIT),
        ("zero_length", PROCESS_LIMIT),
        ("truncated", PROCESS_LIMIT),
        ("oversized", PROCESS_LIMIT),
        ("just_over", PROCESS_LIMIT),
        ("fork_spam_exit", PROCESS_LIMIT),
        ("stderr_flood", PROCESS_LIMIT),
        ("tamper_input", PROCESS_LIMIT),
        ("slow_loris", Duration::from_millis(1_500)),
        ("fork_spam", Duration::from_millis(1_500)),
        ("escape", PROCESS_LIMIT),
    ] {
        let record = support::Record::new();
        let started = Instant::now();
        let rejected = verify(
            &hostile(&record.mode(name)),
            candidate,
            request(name),
            &NOT_CANCELLED,
            Instant::now() + within,
            |_| {},
        )
        .err()
        .unwrap_or_else(|| panic!("host accepted hostile verifier {name}"));
        support::assert_bounded(started, Duration::from_secs(12), name);
        match &rejected.error {
            EncodedRenderError::Deadline => {
                assert!(["slow_loris", "fork_spam"].contains(&name), "{name}");
            }
            EncodedRenderError::Worker(message) => {
                support::assert_generic_cause(name, message);
                if name == "tamper_input" {
                    assert!(message.contains("status: 5"), "{message}");
                }
                if name == "escape" {
                    assert!(message.contains("pipes stayed open"), "{message}");
                }
            }
            error => panic!("{name}: {error:?}"),
        }
        record.assert_group_gone();
        if name == "escape" {
            support::assert_alive(record.escaped(Duration::from_secs(5)));
        }
        // The host's private candidate never shares bytes with the verifier's
        // staged input copy.
        candidate = rejected.candidate;
        assert_eq!(read_candidate(&mut candidate), expected, "{name}");
    }
    fixture.assert_project_unchanged();
}
