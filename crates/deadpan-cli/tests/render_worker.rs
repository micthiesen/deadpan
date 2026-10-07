#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_cli::export_picture::OutputFrameOrdinal;
use deadpan_cli::render_worker::{
    PRIVATE_WORKER_ARGUMENT, RenderPictureRequest, RenderWorkerError, RenderWorkerLimits,
    RenderWorkerRuntime, prepare,
    protocol::{
        MAX_PICTURE_BYTES, OUTPUT_SCOPE, PICTURE_REF, PROTOCOL_VERSION, RenderContract,
        RenderHostMessage, RenderIdentity, RenderProtocol, RenderTimeBase, RenderWorkerMessage,
    },
};
use deadpan_core::{
    AudioSample, BeatNode, ColorPolicy, Command, CommandRequest, ExactRatio, FrameDuration,
    FrameRange, FrameRate, HoldAudio, HoldRecipe, HoldVideo, NodeId, PresentationBasis,
    ProjectDocument, ProjectFrame, ProjectId, RevisionId, Subtree,
};
use deadpan_jobs::{
    AttemptId, CancellationToken, RequestId, Sha256, WorkspaceRef,
    process::{ProcessEvent, ProcessLimits, ProcessSpec, SupervisedProcess},
};
use deadpan_store::{AccessMode, ProjectStore};
use sha2::{Digest, Sha256 as Sha256Hasher};

const PROCESS_LIMIT: Duration = Duration::from_secs(10);

fn identity() -> RenderIdentity {
    RenderIdentity {
        request_id: RequestId::new("render-integration").unwrap(),
        attempt_id: AttemptId::new("attempt-1").unwrap(),
    }
}

fn fixture(package: &Path) -> ProjectDocument {
    let document = ProjectDocument::new(
        ProjectId::new("render-project").unwrap(),
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
                                duration: FrameDuration::new(2).unwrap(),
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
    let encoded = serde_json::to_vec(document).unwrap();
    let digest = Sha256Hasher::digest(encoded);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut hex, "{byte:02x}").unwrap();
    }
    Sha256::new(hex).unwrap()
}

fn contract(document: &ProjectDocument) -> RenderContract {
    RenderContract {
        project_id: document.project_id().clone(),
        revision_id: document.revision_id().clone(),
        range: FrameRange::new(ProjectFrame(0), ProjectFrame(2)).unwrap(),
        canvas: [2, 2],
        raster: [2, 2],
        frame_rate: FrameRate::new(30_000, 1_001).unwrap(),
        color_policy: ColorPolicy::SdrRec709,
        time_base: RenderTimeBase {
            numerator: 1,
            denominator: 30_000,
        },
        frame_count: 2,
        terminal_pts: 2_002,
        project_audio_start: AudioSample(0),
        project_audio_end: AudioSample(3_203),
        relative_aspect_error: ExactRatio::ZERO,
        mastering_display: None,
    }
}

fn wire_request(contract: RenderContract, document_sha256: Sha256) -> RenderHostMessage {
    RenderHostMessage::Prepare {
        protocol: PROTOCOL_VERSION,
        identity: identity(),
        cancellation_token: CancellationToken::new("cancel-render-attempt-1").unwrap(),
        contract: Box::new(contract),
        document_sha256,
        output_scope: WorkspaceRef::new(OUTPUT_SCOPE).unwrap(),
        maximum_output_bytes: MAX_PICTURE_BYTES,
        timeout_millis: 10_000,
    }
}

fn host_request(package: &Path) -> RenderPictureRequest {
    RenderPictureRequest {
        package: package.into(),
        revision: RevisionId::new("committed").unwrap(),
        range: None,
        identity: identity(),
        cancellation_token: CancellationToken::new("cancel-render-attempt-1").unwrap(),
    }
}

fn finish(
    process: &mut SupervisedProcess<RenderProtocol>,
) -> Vec<ProcessEvent<RenderWorkerMessage>> {
    let deadline = Instant::now() + PROCESS_LIMIT + Duration::from_secs(5);
    let mut events = Vec::new();
    while !process.is_finished() {
        assert!(
            Instant::now() < deadline,
            "render child or pipes did not terminate"
        );
        let batch = process.poll(Instant::now()).unwrap();
        events.extend(batch);
        if !process.is_finished() {
            std::thread::park_timeout(Duration::from_millis(2));
        }
    }
    assert!(matches!(events.last(), Some(ProcessEvent::Exited { .. })));
    events
}

#[test]
fn real_worker_rejects_changed_document_and_contract_before_output() {
    let scratch = tempfile::tempdir().unwrap();
    let package = fs::canonicalize(scratch.path())
        .unwrap()
        .join("pictures.deadpan");
    let document = fixture(&package);
    for (changed_contract, changed_hash, expected_diagnostic) in [
        (
            contract(&document),
            Sha256::new("0".repeat(64)).unwrap(),
            "committed document does not match the requested SHA-256",
        ),
        (
            RenderContract {
                canvas: [2, 4],
                raster: [2, 4],
                ..contract(&document)
            },
            document_hash(&document),
            "committed project does not match the requested render contract",
        ),
    ] {
        let workspace = tempfile::tempdir().unwrap();
        fs::create_dir(workspace.path().join(OUTPUT_SCOPE)).unwrap();
        let request = wire_request(changed_contract, changed_hash);
        request.validate().unwrap();
        let mut process = SupervisedProcess::<RenderProtocol>::spawn(
            ProcessSpec {
                executable: PathBuf::from(env!("CARGO_BIN_EXE_deadpan-cli")),
                arguments: vec![PRIVATE_WORKER_ARGUMENT.into(), package.as_os_str().into()],
                environment: BTreeMap::new(),
                workspace: workspace.path().into(),
                limits: ProcessLimits {
                    maximum_duration: PROCESS_LIMIT,
                    cancellation_grace: Duration::from_millis(100),
                    exit_grace: Duration::from_secs(2),
                },
            },
            request,
        )
        .unwrap();
        let events = finish(&mut process);
        let diagnostics: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                ProcessEvent::Message(message) => match message.as_ref() {
                    RenderWorkerMessage::Failed { diagnostic, .. } => Some(diagnostic.as_str()),
                    other => panic!("unexpected response before admission: {other:?}"),
                },
                _ => None,
            })
            .collect();
        assert_eq!(diagnostics, [expected_diagnostic], "{events:?}");
        assert!(
            matches!(events.last(), Some(ProcessEvent::Exited {
            status, cancellation_escalated: false,
        }) if status.code() == Some(1)),
            "{events:?}"
        );
        assert!(!workspace.path().join(PICTURE_REF).exists());
        let store = ProjectStore::open(&package, AccessMode::ReadOnly).unwrap();
        assert_eq!(store.snapshot().unwrap(), document);
    }
}

/// Only the hostile test fixture uses Python. Production selects its packaged
/// native executable and never relies on this test interpreter or script.
fn fixture_runtime(mode: &str) -> RenderWorkerRuntime {
    let executable = std::env::split_paths(&std::env::var_os("PATH").expect("test PATH"))
        .map(|directory| directory.join("python3"))
        .find(|candidate| candidate.is_file())
        .expect("Python 3 is required for render protocol fault fixtures");
    RenderWorkerRuntime {
        executable: fs::canonicalize(executable).unwrap(),
        arguments: vec![
            "-I".into(),
            "-u".into(),
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/render_worker/fixture.py")
                .into_os_string(),
            mode.into(),
        ],
        environment: BTreeMap::new(),
    }
}

fn host_failure(package: &Path, mode: &str) -> RenderWorkerError {
    prepare(
        &fixture_runtime(mode),
        host_request(package),
        RenderWorkerLimits::default(),
        &AtomicBool::new(false),
        Instant::now() + PROCESS_LIMIT,
        |_| panic!("host must not expose fixture progress after failure"),
    )
    .err()
    .unwrap_or_else(|| panic!("host admitted hostile fixture {mode}"))
}

#[test]
fn host_preserves_the_worker_diagnostic_across_a_failed_process_exit() {
    let scratch = tempfile::tempdir().unwrap();
    let package = scratch.path().join("failure.deadpan");
    fixture(&package);
    let failure = host_failure(&package, "failed_exit");
    assert!(
        matches!(failure, RenderWorkerError::Worker(ref message)
        if message == "fixture decoder rejected captured source"),
        "{failure:?}"
    );
}

#[test]
fn host_rejects_broken_framing_stale_attempts_and_unclean_completion() {
    let scratch = tempfile::tempdir().unwrap();
    let package = scratch.path().join("protocol.deadpan");
    let document = fixture(&package);
    for mode in [
        "partial_header",
        "partial_body",
        "malformed",
        "stale_attempt",
        "completed_exit_failure",
        "after_terminal",
    ] {
        let failure = host_failure(&package, mode);
        assert!(
            matches!(
                failure,
                RenderWorkerError::Worker(_) | RenderWorkerError::Protocol(_)
            ),
            "{mode} failed for an unrelated reason: {failure:?}"
        );
    }
    let store = ProjectStore::open(&package, AccessMode::ReadOnly).unwrap();
    assert_eq!(store.snapshot().unwrap(), document);
}

#[test]
fn host_independently_rejects_wrong_bytes_after_a_valid_completed_manifest() {
    let scratch = tempfile::tempdir().unwrap();
    let package = scratch.path().join("artifact.deadpan");
    fixture(&package);
    for mode in ["wrong_hash", "short_file"] {
        let failure = host_failure(&package, mode);
        assert!(
            matches!(failure, RenderWorkerError::Artifact(_)),
            "{mode}: {failure:?}"
        );
    }
    for (mode, expected_plane) in [("invalid_luma", "Y"), ("invalid_chroma", "Cb")] {
        let failure = host_failure(&package, mode);
        assert!(
            matches!(failure, RenderWorkerError::InvalidPixels { frame: 0, plane }
            if plane == expected_plane),
            "{mode}: {failure:?}"
        );
    }
}

#[test]
fn admitted_fixture_bytes_outlive_the_child_workspace_and_project_path() {
    let scratch = tempfile::tempdir().unwrap();
    let package = scratch.path().join("retained.deadpan");
    let document = fixture(&package);
    let mut admitted = prepare(
        &fixture_runtime("valid"),
        host_request(&package),
        RenderWorkerLimits::default(),
        &AtomicBool::new(false),
        Instant::now() + PROCESS_LIMIT,
        |_| panic!("fixture sends no progress"),
    )
    .unwrap();
    assert_eq!(admitted.document_sha256(), &document_hash(&document));
    assert_eq!(admitted.contract().revision_id(), document.revision_id());
    assert_eq!(admitted.frame_bytes(), 6);
    drop(scratch);
    let mut bytes = [0_u8; 6];
    let timing = admitted
        .read_frame(OutputFrameOrdinal(1), &mut bytes)
        .unwrap();
    assert_eq!(bytes, [16, 16, 16, 16, 128, 128]);
    assert_eq!(timing.project_frame(), ProjectFrame(1));
    assert_eq!(timing.pts(), 1_001);
    assert_eq!(timing.duration(), 1_001);
    assert!(
        admitted
            .read_frame(OutputFrameOrdinal(2), &mut bytes)
            .is_err()
    );
    assert!(
        admitted
            .read_frame(OutputFrameOrdinal(0), &mut [0_u8; 5])
            .is_err()
    );
}

// Hostile stand-ins through the real host (docs/ADVERSARIAL.md#hostile-workers).

#[path = "hostile_workers/support.rs"]
mod hostile;

/// Generous bound for refusals that must not wait for the deadline.
const PROMPT: Duration = Duration::from_secs(10);
const SHORT: Duration = Duration::from_millis(1_500);
const SHORT_BOUND: Duration = Duration::from_secs(12);
const VALID_PLANES: [u8; 12] = [16, 16, 16, 16, 128, 128, 16, 16, 16, 16, 128, 128];

fn hostile_prepare(
    package: &Path,
    record: &hostile::Record,
    name: &str,
    within: Duration,
) -> Result<deadpan_cli::render_worker::PreparedPictureRange, RenderWorkerError> {
    prepare(
        &fixture_runtime(&record.mode(name)),
        host_request(package),
        RenderWorkerLimits::default(),
        &AtomicBool::new(false),
        Instant::now() + within,
        |_| {},
    )
}

#[test]
fn hostile_artifact_claims_are_refused_without_following_them_outside() {
    let scratch = tempfile::tempdir().unwrap();
    let package = scratch.path().join("hostile-claims.deadpan");
    fixture(&package);
    for name in [
        "symlink_outside",
        "hardlink_outside",
        "symlinked_scope",
        "fifo",
        "sparse",
        "directory",
    ] {
        let record = hostile::Record::new();
        let started = Instant::now();
        let error = hostile_prepare(&package, &record, name, PROCESS_LIMIT)
            .err()
            .expect(name);
        hostile::assert_bounded(started, PROMPT, name);
        assert!(
            matches!(error, RenderWorkerError::Artifact(_)),
            "{name}: {error:?}"
        );
        assert_eq!(
            fs::read(record.path().join("outside.bin")).unwrap(),
            VALID_PLANES
        );
        record.assert_group_gone();
    }
    for name in ["absolute", "parent", "wrong_attempt"] {
        let record = hostile::Record::new();
        let error = hostile_prepare(&package, &record, name, PROCESS_LIMIT)
            .err()
            .expect(name);
        let message = match &error {
            RenderWorkerError::Worker(message) | RenderWorkerError::Protocol(message) => message,
            error => panic!("{name}: {error:?}"),
        };
        assert!(!message.contains("exited"), "{name}: {message}");
        record.assert_group_gone();
    }
}

#[test]
fn hostile_frames_floods_stalls_and_descendants_are_bounded_and_stopped() {
    let scratch = tempfile::tempdir().unwrap();
    let package = scratch.path().join("hostile-process.deadpan");
    fixture(&package);
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
        let record = hostile::Record::new();
        let started = Instant::now();
        let error = hostile_prepare(&package, &record, name, Duration::from_secs(60))
            .err()
            .expect(name);
        hostile::assert_bounded(started, PROMPT, name);
        assert!(
            matches!(error, RenderWorkerError::Worker(_)),
            "{name}: {error:?}"
        );
        hostile::assert_generic_cause(name, &error.to_string());
        record.assert_group_gone();
    }
    for name in ["slow_loris", "fork_spam"] {
        let record = hostile::Record::new();
        let started = Instant::now();
        let error = hostile_prepare(&package, &record, name, SHORT)
            .err()
            .expect(name);
        hostile::assert_bounded(started, SHORT_BOUND, name);
        assert!(
            matches!(error, RenderWorkerError::Deadline),
            "{name}: {error:?}"
        );
        record.assert_group_gone();
    }
    // A clean completion with two dozen sleeping group members is admitted
    // only after every one of them has been stopped.
    let record = hostile::Record::new();
    let admitted = hostile_prepare(&package, &record, "fork_spam_valid", PROCESS_LIMIT);
    assert!(admitted.is_ok(), "{:?}", admitted.err());
    assert_eq!(record.children().len(), 24);
    record.assert_group_gone();
}

#[test]
fn setsid_escapees_are_not_contained_but_cannot_change_admitted_bytes() {
    let scratch = tempfile::tempdir().unwrap();
    let package = scratch.path().join("hostile-escape.deadpan");
    fixture(&package);
    // An escapee holding the inherited pipes turns completion into a failure.
    let record = hostile::Record::new();
    let started = Instant::now();
    let error = hostile_prepare(&package, &record, "escape", PROCESS_LIMIT)
        .err()
        .expect("escape");
    hostile::assert_bounded(started, PROMPT, "escape");
    assert!(matches!(error, RenderWorkerError::Worker(_)), "{error:?}");
    record.assert_group_gone();
    hostile::assert_alive(record.escaped(PROMPT));

    // One that closes its pipes is invisible to group ownership: the host
    // admits the clean leader's output. The escapee is still alive after the
    // host returns (documented, not a sandbox) and its later rewrite of the
    // worker's output cannot reach the private admitted snapshot.
    let record = hostile::Record::new();
    let mut admitted = hostile_prepare(&package, &record, "escape_quiet", PROCESS_LIMIT)
        .expect("escape_quiet leader completed cleanly");
    record.assert_group_gone();
    let escaped = record.escaped(PROMPT);
    hostile::assert_alive(escaped);
    fs::write(record.path().join("host-returned"), b"").unwrap();
    assert!(record.wait_for("escaped-alive", PROMPT));
    assert_eq!(
        fs::read_to_string(record.path().join("escaped-rewritten")).unwrap(),
        "1",
        "the escapee must rewrite the worker's original output inode",
    );
    let mut frame = [0_u8; 6];
    admitted
        .read_frame(OutputFrameOrdinal(1), &mut frame)
        .unwrap();
    assert_eq!(frame, VALID_PLANES[6..]);
}
