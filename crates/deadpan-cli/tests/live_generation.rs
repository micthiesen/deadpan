#![cfg(any(target_os = "macos", target_os = "linux"))]

//! `generate-hold` and `accept-hold` route to an authenticated test owner when
//! the project's writer is held. The owner scripts the observations; the AI
//! job itself is the app's (covered by its service tests).

use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use deadpan_cli::host::Endpoint;
use deadpan_cli::live_project::generation::{GenerationOutcome, GenerationStatus};
use deadpan_cli::live_project::{Operation, Reply, Request, ShortOperation};
use deadpan_cli::render::RenderContext;
use deadpan_core::{NodeId, ProjectDocument, ProjectId, RevisionId};
use deadpan_jobs::RequestId;
use deadpan_store::ProjectStore;
use serde_json::{Value, json};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

const JOB: u64 = (1 << 62) + 3;

fn status(polls: usize) -> Result<GenerationStatus> {
    let finished = polls >= 2;
    Ok(GenerationStatus {
        controls_pending: false,
        options: deadpan_jobs::GenerationOptions::default(),
        job: JOB,
        hold: NodeId::new("pause")?,
        scope: deadpan_core::ScopedNodeTarget {
            node: NodeId::new("pause")?,
            repeats: Vec::new(),
        },
        request_id: (polls > 0)
            .then(|| RequestId::new("ai-hold-live"))
            .transpose()?,
        variants: 2,
        variant: if polls > 1 { 2 } else { 1 },
        ready: if finished { 2 } else { 0 },
        stage: if finished {
            "Validating"
        } else {
            "Generating pictures"
        }
        .into(),
        steps: (!finished).then_some([polls as u64 + 1, 8]),
        elapsed_ms: 10 * polls as u64,
        outcome: finished.then_some(GenerationOutcome::Ready {}),
        note: None,
    })
}

fn run(
    store: &ProjectStore,
    endpoint: &mut Endpoint,
    args: &[&str],
    status_fails: bool,
) -> Result<(Output, Vec<Operation>)> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_deadpan-cli"));
    command
        .args(args)
        // The live path needs no local runtime; make sure none is consulted.
        .env("DEADPAN_BRIDGE_RUNTIME_SOURCE", "/nonexistent/runtime")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = deadpan_native_process::spawn(&mut command)?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut operations = Vec::new();
    let mut polls = 0;
    while child.try_wait()?.is_none() {
        assert!(Instant::now() < deadline, "CLI owner integration timed out");
        for incoming in endpoint.poll() {
            let operation = Request::from_value(incoming.payload)?.operation;
            operations.push(operation.clone());
            let reply = match operation {
                Operation::Inspect => {
                    let document = store.snapshot()?;
                    Reply::Context {
                        context: RenderContext {
                            project_id: document.project_id().clone(),
                            revision_id: document.revision_id().clone(),
                        },
                        preview_active: false,
                    }
                }
                Operation::Generate { .. } => Reply::Generation {
                    status: Box::new(status(0)?),
                },
                Operation::GenerationStatus { .. } if status_fails => Reply::Failed {
                    error: deadpan_cli::live_project::LiveError::new(
                        "HostProtocolInvalid",
                        "injected observation failure",
                    ),
                },
                Operation::GenerationStatus { job, .. } => {
                    assert_eq!(job, JOB);
                    polls += 1;
                    Reply::Generation {
                        status: Box::new(status(polls)?),
                    }
                }
                Operation::Execute { command, .. } => {
                    assert!(matches!(*command, ShortOperation::AcceptHold { .. }));
                    Reply::Completed {
                        output: json!({"protocol":1,"committed":true,"outcome":{"revision_id":"accepted"}}),
                        committed_revision: Some(RevisionId::new("accepted")?),
                        committed_registers: None,
                        refresh_error: None,
                    }
                }
                Operation::ReleaseGenerationStatus { job, .. } => {
                    assert_eq!(job, JOB);
                    Reply::Released
                }
                operation => panic!("unexpected operation {operation:?}"),
            };
            endpoint.respond(incoming.ticket, serde_json::to_value(reply)?)?;
        }
        std::thread::yield_now();
    }
    Ok((child.wait_with_output()?, operations))
}

#[test]
fn generate_and_accept_route_to_the_open_projects_owner() -> Result {
    let root = tempfile::tempdir()?;
    let package = root.path().join("open.deadpan");
    let project = package.to_str().unwrap();
    let document = ProjectDocument::new_automatic(
        ProjectId::new("live-generation")?,
        RevisionId::new("initial")?,
        NodeId::new("root")?,
    )?;
    let mut store = ProjectStore::create(&package, &document)?;
    let mut endpoint = Endpoint::bind(&mut store)?;

    let (output, operations) = run(
        &store,
        &mut endpoint,
        &[
            "generate-hold",
            project,
            "--hold",
            "pause",
            "--variants",
            "2",
            "--seed",
            "9",
            "--motion",
            "moderate",
            "--instructions",
            "Keep the hands still.",
        ],
        false,
    )?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(report["routed"], "live_project");
    assert_eq!(report["job"], JOB);
    assert_eq!(report["request_id"], "ai-hold-live");
    assert_eq!(report["ready"], 2);
    assert_eq!(report["outcome"]["state"], "ready");
    let Some(Operation::Generate { request, .. }) = operations
        .iter()
        .find(|operation| matches!(operation, Operation::Generate { .. }))
    else {
        panic!("generation was routed: {operations:?}");
    };
    assert_eq!(request.hold.as_str(), "pause");
    assert_eq!(request.expected_revision.as_str(), "initial");
    assert_eq!((request.variants, request.seed), (2, Some(9)));
    let options = request.options.as_ref().unwrap();
    assert_eq!(options.motion, deadpan_jobs::MotionAmount::Moderate);
    assert_eq!(
        options.instructions.as_ref().unwrap().as_str(),
        "Keep the hands still."
    );
    assert_eq!(
        report["options"]["motion"], "still",
        "the report is the owner's observation"
    );
    assert!(matches!(
        operations.last(),
        Some(Operation::ReleaseGenerationStatus { job: JOB, .. })
    ));
    // Progress reaches stderr as JSON lines.
    let progress = String::from_utf8_lossy(&output.stderr);
    assert!(progress.contains("\"variants\":2"), "{progress}");

    let (output, operations) = run(
        &store,
        &mut endpoint,
        &[
            "accept-hold",
            project,
            "--request",
            "ai-hold-live",
            "--attempt",
            "attempt-2",
        ],
        false,
    )?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let accepted: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(accepted["committed"], true);
    let Some(Operation::Execute { command, .. }) = operations.last() else {
        panic!("acceptance was routed: {operations:?}");
    };
    let ShortOperation::AcceptHold {
        request,
        attempt,
        expected_revision,
        ..
    } = command.as_ref()
    else {
        panic!("expected AcceptHold");
    };
    assert_eq!(request.as_str(), "ai-hold-live");
    assert_eq!(
        attempt.as_ref().map(|attempt| attempt.as_str()),
        Some("attempt-2")
    );
    assert_eq!(
        expected_revision.as_ref().map(RevisionId::as_str),
        Some("initial"),
        "acceptance names the head observed before sending"
    );
    Ok(())
}

#[test]
fn a_lost_observation_reports_the_job_without_replaying() -> Result {
    let root = tempfile::tempdir()?;
    let package = root.path().join("lost.deadpan");
    let document = ProjectDocument::new_automatic(
        ProjectId::new("live-generation-lost")?,
        RevisionId::new("initial")?,
        NodeId::new("root")?,
    )?;
    let mut store = ProjectStore::create(&package, &document)?;
    let mut endpoint = Endpoint::bind(&mut store)?;
    let (output, operations) = run(
        &store,
        &mut endpoint,
        &[
            "generate-hold",
            package.to_str().unwrap(),
            "--hold",
            "pause",
        ],
        true,
    )?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("HostOutcomeUnknown"), "{stderr}");
    assert!(stderr.contains(&JOB.to_string()), "{stderr}");
    assert_eq!(
        operations
            .iter()
            .filter(|operation| matches!(operation, Operation::Generate { .. }))
            .count(),
        1,
        "never replayed"
    );
    Ok(())
}
