#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};

use deadpan_core::{FrameDuration, FrameRate, NodeId, ProjectId, RevisionId};
use deadpan_jobs::supervisor::{ProcessEvent, ProcessLimits, ProcessSpec, WorkerProcess};
use deadpan_jobs::*;

fn identity() -> MessageIdentity {
    MessageIdentity::new(
        RequestId::new("job").unwrap(),
        AttemptId::new("attempt").unwrap(),
    )
}

fn provider() -> ProviderSelection {
    ProviderSelection {
        pack_id: ProviderPackId::new("pack").unwrap(),
        pack_version: ProviderPackVersion::new("1").unwrap(),
        runtime_id: RuntimeId::new("runtime").unwrap(),
        runtime_version: RuntimeVersion::new("1").unwrap(),
        seed: 1,
    }
}

fn video() -> VideoSpec {
    VideoSpec::new(
        FrameDuration::new(25).unwrap(),
        FrameRate::new(24, 1).unwrap(),
        512,
        320,
    )
    .unwrap()
}

fn request() -> HostMessage {
    HostMessage::GenerateHold {
        protocol: ProtocolVersion::V1,
        identity: identity(),
        cancellation_token: CancellationToken::new("cancel").unwrap(),
        project_id: ProjectId::new("project").unwrap(),
        revision_id: RevisionId::new("revision").unwrap(),
        target: HoldTarget {
            hold_id: NodeId::new("hold").unwrap(),
            request_version: RequestVersion::new(1).unwrap(),
        },
        input: ContextArtifact {
            manifest: WorkspaceRef::new("inputs/context.json").unwrap(),
            sha256: Sha256::new("a".repeat(64)).unwrap(),
        },
        output_workspace: WorkspaceRef::new("outputs").unwrap(),
        constraints: HoldConstraints {
            video: video(),
            conditioning: ConditioningMode::Bridge,
            motion: MotionAmount::Still,
        },
        provider: Box::new(provider()),
    }
}

fn bridge_request() -> HostMessage {
    let plan = BridgeGenerationPlan::new(
        FrameDuration::new(25).unwrap(),
        FrameRate::new(24, 1).unwrap(),
        &BridgeCapability::new(
            true,
            FrameRate::new(24, 1).unwrap(),
            FrameCountFormula::new(1, 0, 2, 97).unwrap(),
            DimensionLimits::new(
                AxisLimits::new(512, 512, 1).unwrap(),
                AxisLimits::new(320, 320, 1).unwrap(),
            ),
        ),
        NativeDimensions::new(512, 320).unwrap(),
    )
    .unwrap();
    HostMessage::GenerateBridge {
        protocol: ProtocolVersion::V2,
        identity: identity(),
        cancellation_token: CancellationToken::new("cancel").unwrap(),
        project_id: ProjectId::new("project").unwrap(),
        revision_id: RevisionId::new("revision").unwrap(),
        target: HoldTarget {
            hold_id: NodeId::new("hold").unwrap(),
            request_version: RequestVersion::new(1).unwrap(),
        },
        input: ContextArtifact {
            manifest: WorkspaceRef::new("inputs/context.json").unwrap(),
            sha256: Sha256::new("a".repeat(64)).unwrap(),
        },
        output_workspace: WorkspaceRef::new("outputs").unwrap(),
        constraints: HoldConstraints {
            video: video(),
            conditioning: ConditioningMode::Bridge,
            motion: MotionAmount::Still,
        },
        provider: Box::new(provider()),
        plan: Box::new(plan),
    }
}

fn completed() -> WorkerMessage {
    WorkerMessage::Completed {
        protocol: ProtocolVersion::V1,
        identity: identity(),
        candidate: CandidateManifest {
            media: WorkspaceArtifact::new(
                WorkspaceRef::new("outputs/candidate.mov").unwrap(),
                Sha256::new("b".repeat(64)).unwrap(),
                42,
            )
            .unwrap(),
            video: video(),
            provider: provider(),
        },
    }
}

fn executable() -> &'static Path {
    static FIXTURE: OnceLock<tempfile::TempDir> = OnceLock::new();
    FIXTURE
        .get_or_init(|| {
            let directory = tempfile::tempdir().unwrap();
            let compiled = Command::new("rustc")
                .args(["--edition=2024", "--crate-name=deadpan_worker_fixture"])
                .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/worker.rs"))
                .arg("-o")
                .arg(directory.path().join("worker"))
                .output()
                .unwrap();
            assert!(
                compiled.status.success(),
                "{}",
                String::from_utf8_lossy(&compiled.stderr)
            );
            directory
        })
        .path()
}

fn spec(workspace: &Path, mode: &str) -> ProcessSpec {
    ProcessSpec {
        executable: executable().join("worker"),
        arguments: vec![mode.into()],
        environment: BTreeMap::new(),
        workspace: workspace.into(),
        limits: ProcessLimits {
            maximum_duration: Duration::from_secs(10),
            cancellation_grace: Duration::from_millis(50),
            exit_grace: Duration::from_secs(2),
        },
    }
}

fn responses(workspace: &Path, messages: &[WorkerMessage]) {
    let mut bytes = Vec::new();
    for message in messages {
        write_worker_message(&mut bytes, message).unwrap();
    }
    fs::write(workspace.join("responses.bin"), bytes).unwrap();
}

fn finish(process: &mut WorkerProcess) -> Vec<ProcessEvent> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut events = Vec::new();
    while !process.is_finished() {
        assert!(
            Instant::now() < deadline,
            "worker or its inherited pipes did not terminate"
        );
        let batch = process.poll(Instant::now()).unwrap();
        assert!(batch.len() <= 19, "poll must have a bounded event batch");
        if batch.iter().any(|event| matches!(event,
            ProcessEvent::Message(message) if matches!(message.as_ref(), WorkerMessage::Completed { .. } | WorkerMessage::CompletedBridge { .. }))) {
            assert!(matches!(batch.last(), Some(ProcessEvent::Exited { status, cancellation_escalated: false }) if status.success()),
                "candidate escaped before clean process/pipe teardown");
        }
        events.extend(batch);
        thread::sleep(Duration::from_millis(2));
    }
    assert!(matches!(events.last(), Some(ProcessEvent::Exited { .. })));
    events
}

fn faults(events: &[ProcessEvent]) -> Vec<&str> {
    events
        .iter()
        .filter_map(|event| match event {
            ProcessEvent::Fault(reason) => Some(reason.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn private_environment_and_fragmented_messages_finish_before_exit() {
    for mode in ["environment", "fragmented"] {
        let workspace = tempfile::tempdir().unwrap();
        let mut messages = Vec::new();
        for _ in 0..40 {
            messages.push(WorkerMessage::Stage {
                protocol: ProtocolVersion::V1,
                identity: identity(),
                stage: WorkerStage::Inference,
            });
        }
        messages.push(completed());
        responses(workspace.path(), &messages);
        let mut specification = spec(workspace.path(), mode);
        specification
            .environment
            .insert("DEADPAN_WORKER_ALLOWED".into(), "private".into());
        let mut process = WorkerProcess::spawn(specification, request()).unwrap();
        let events = finish(&mut process);
        assert!(
            faults(&events).is_empty(),
            "{mode}: {} messages, {:?}",
            events
                .iter()
                .filter(|event| matches!(event, ProcessEvent::Message(_)))
                .count(),
            faults(&events)
        );
        let actual: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                ProcessEvent::Message(message) => Some(message.as_ref()),
                _ => None,
            })
            .collect();
        assert_eq!(actual, messages.iter().collect::<Vec<_>>());
        let received: HostMessage =
            serde_json::from_slice(&fs::read(workspace.path().join("received.json")).unwrap())
                .unwrap();
        assert_eq!(received, request());
        if mode == "environment" {
            assert_eq!(
                fs::read_to_string(workspace.path().join("location.txt")).unwrap(),
                fs::canonicalize(workspace.path())
                    .unwrap()
                    .to_string_lossy()
            );
        }
    }
}

#[test]
fn stderr_is_drained_but_retained_tail_is_bounded() {
    let workspace = tempfile::tempdir().unwrap();
    responses(workspace.path(), &[completed()]);
    let mut process = WorkerProcess::spawn(spec(workspace.path(), "stderr"), request()).unwrap();
    assert!(faults(&finish(&mut process)).is_empty());
    let logs = process.log_tail();
    assert_eq!(logs.bytes, vec![b'd'; 64 * 1024]);
    assert_eq!(logs.discarded_bytes, 192 * 1024);
}

#[test]
fn cooperative_cancel_preserves_identity_and_does_not_escalate() {
    for (request, protocol) in [
        (request(), ProtocolVersion::V1),
        (bridge_request(), ProtocolVersion::V2),
    ] {
        let workspace = tempfile::tempdir().unwrap();
        responses(
            workspace.path(),
            &[WorkerMessage::Cancelled {
                protocol,
                identity: identity(),
            }],
        );
        let mut specification = spec(workspace.path(), "cancel");
        specification.limits.cancellation_grace = Duration::from_secs(2);
        let mut process = WorkerProcess::spawn(specification, request).unwrap();
        assert!(process.request_cancel(Instant::now()).unwrap());
        assert!(!process.request_cancel(Instant::now()).unwrap());
        let events = finish(&mut process);
        assert!(faults(&events).is_empty());
        assert!(
            matches!(events.last(), Some(ProcessEvent::Exited { status, cancellation_escalated: false }) if status.success())
        );
        let cancel: HostMessage =
            serde_json::from_slice(&fs::read(workspace.path().join("cancellation.json")).unwrap())
                .unwrap();
        assert!(
            matches!(cancel, HostMessage::Cancel { protocol: actual_protocol, identity: actual, cancellation_token } if actual_protocol == protocol && actual == identity() && cancellation_token.as_str() == "cancel")
        );
    }
}

#[test]
fn unresponsive_worker_is_killed_after_grace_and_deadline() {
    for cancel in [true, false] {
        let workspace = tempfile::tempdir().unwrap();
        let mut specification = spec(workspace.path(), "no-read");
        specification.limits.maximum_duration = Duration::from_millis(250);
        specification.limits.exit_grace = Duration::from_millis(100);
        let mut process = WorkerProcess::spawn(specification, request()).unwrap();
        if cancel {
            process.request_cancel(Instant::now()).unwrap();
        }
        let events = finish(&mut process);
        assert!(
            matches!(events.last(), Some(ProcessEvent::Exited { status, cancellation_escalated }) if !status.success() && *cancellation_escalated == cancel)
        );
        if !cancel {
            assert!(
                faults(&events)
                    .iter()
                    .any(|reason| reason.contains("maximum duration"))
            );
        }
    }
}

#[test]
fn late_poll_observes_exit_before_applying_a_timeout() {
    use rustix::process::{Pid, WaitId, WaitIdOptions, waitid};
    for cancel in [false, true] {
        let workspace = tempfile::tempdir().unwrap();
        let response = if cancel {
            WorkerMessage::Cancelled {
                protocol: ProtocolVersion::V1,
                identity: identity(),
            }
        } else {
            completed()
        };
        responses(workspace.path(), &[response]);
        let mut process = WorkerProcess::spawn(
            spec(workspace.path(), if cancel { "cancel" } else { "messages" }),
            request(),
        )
        .unwrap();
        if cancel {
            process.request_cancel(Instant::now()).unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Ok(text) = fs::read_to_string(workspace.path().join("worker.pid"))
                && let Ok(raw) = text.parse()
                && let Some(pid) = Pid::from_raw(raw)
                && waitid(
                    WaitId::Pid(pid),
                    WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
                )
                .unwrap()
                .is_some()
            {
                break;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(2));
        }
        let mut events = process
            .poll(Instant::now() + Duration::from_secs(20))
            .unwrap();
        if !process.is_finished() {
            events.extend(finish(&mut process));
        }
        assert!(faults(&events).is_empty(), "{:?}", faults(&events));
        assert!(
            matches!(events.last(), Some(ProcessEvent::Exited { status, cancellation_escalated: false }) if status.success())
        );
    }
}

#[test]
fn invalid_protocol_missing_completion_and_bad_exit_never_pass() {
    for (mode, expected) in [
        ("huge", "payload bytes"),
        ("truncated", "ended after"),
        ("no-terminal", "without a terminal"),
        ("exit-failure", "unsuccessfully"),
        ("hang-after-terminal", "did not exit"),
    ] {
        let workspace = tempfile::tempdir().unwrap();
        responses(workspace.path(), &[completed()]);
        let mut process = WorkerProcess::spawn(spec(workspace.path(), mode), request()).unwrap();
        let events = finish(&mut process);
        assert!(
            faults(&events)
                .iter()
                .any(|reason| reason.contains(expected)),
            "{mode}: {:?}",
            faults(&events)
        );
        assert!(!events.iter().any(|event| matches!(event,
            ProcessEvent::Message(message) if matches!(message.as_ref(), WorkerMessage::Completed { .. }))),
            "{mode}: failed attempt exposed a completed candidate");
    }
}

#[test]
fn wrong_attempt_and_messages_after_completion_are_rejected() {
    let mut wrong = completed();
    if let WorkerMessage::Completed { identity, .. } = &mut wrong {
        identity.attempt_id = AttemptId::new("another-attempt").unwrap();
    }
    for messages in [vec![wrong], vec![completed(), completed()]] {
        let workspace = tempfile::tempdir().unwrap();
        responses(workspace.path(), &messages);
        let mut process =
            WorkerProcess::spawn(spec(workspace.path(), "messages"), request()).unwrap();
        let events = finish(&mut process);
        assert!(!faults(&events).is_empty());
        assert!(!events.iter().any(|event| matches!(event,
            ProcessEvent::Message(message) if matches!(message.as_ref(), WorkerMessage::Completed { .. }))));
    }
}

#[test]
fn response_protocol_must_match_initial_request() {
    let workspace = tempfile::tempdir().unwrap();
    responses(
        workspace.path(),
        &[WorkerMessage::Stage {
            protocol: ProtocolVersion::V2,
            identity: identity(),
            stage: WorkerStage::Preflight,
        }],
    );
    let mut process = WorkerProcess::spawn(spec(workspace.path(), "messages"), request()).unwrap();
    let events = finish(&mut process);
    assert!(
        faults(&events)
            .iter()
            .any(|reason| reason.contains("protocol differs")),
        "{:?}",
        faults(&events)
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, ProcessEvent::Message(_)))
    );
}

#[test]
fn orphaned_descendant_cannot_keep_worker_pipes_open() {
    let workspace = tempfile::tempdir().unwrap();
    let mut process = WorkerProcess::spawn(spec(workspace.path(), "orphan"), request()).unwrap();
    let events = finish(&mut process);
    assert!(workspace.path().join("descendant.pid").exists());
    assert!(
        faults(&events)
            .iter()
            .any(|reason| reason.contains("without a terminal"))
    );
}

#[test]
fn dropping_a_flooding_worker_releases_pipe_backpressure() {
    let workspace = tempfile::tempdir().unwrap();
    responses(
        workspace.path(),
        &[WorkerMessage::Stage {
            protocol: ProtocolVersion::V1,
            identity: identity(),
            stage: WorkerStage::Inference,
        }],
    );
    let process = WorkerProcess::spawn(spec(workspace.path(), "flood"), request()).unwrap();
    thread::sleep(Duration::from_millis(30));
    let started = Instant::now();
    drop(process);
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn escaped_descendant_cannot_hang_pipe_cleanup() {
    let workspace = tempfile::tempdir().unwrap();
    let mut process = WorkerProcess::spawn(spec(workspace.path(), "escaped"), request()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let pid_file = workspace.path().join("descendant.pid");
    let escaped_pid = loop {
        if let Ok(text) = fs::read_to_string(&pid_file)
            && let Ok(pid) = text.parse()
            && let Some(pid) = rustix::process::Pid::from_raw(pid)
        {
            break pid;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(2));
    };
    // This deliberately escaped test child is owned by the test. Cleanup it
    // independently; the product supervisor is explicitly not an OS sandbox.
    struct Cleanup(rustix::process::Pid);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = rustix::process::kill_process(self.0, rustix::process::Signal::KILL);
        }
    }
    let _cleanup = Cleanup(escaped_pid);
    let events = finish(&mut process);
    assert!(
        faults(&events)
            .iter()
            .any(|reason| reason.contains("pipes stayed open"))
    );
    let started = Instant::now();
    drop(process);
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn workspace_symlinks_relative_executables_and_unbounded_deadlines_are_rejected() {
    let parent = tempfile::tempdir().unwrap();
    let workspace = parent.path().join("workspace");
    fs::create_dir(&workspace).unwrap();
    let link = parent.path().join("link");
    std::os::unix::fs::symlink(&workspace, &link).unwrap();
    assert!(WorkerProcess::spawn(spec(&link, "messages"), request()).is_err());
    let mut relative = spec(&workspace, "messages");
    relative.executable = "worker".into();
    assert!(WorkerProcess::spawn(relative, request()).is_err());
    let mut unbounded = spec(&workspace, "messages");
    unbounded.limits.maximum_duration = Duration::ZERO;
    assert!(WorkerProcess::spawn(unbounded, request()).is_err());
}
