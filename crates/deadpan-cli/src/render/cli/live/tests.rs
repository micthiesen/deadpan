//! These fake owners exercise authenticated routing and observations only.
//! They do not encode media or construct publication capabilities.

use super::*;
use crate::{encoded_render::workflow::WorkflowStatus, host::Endpoint};
use deadpan_core::{NodeId, ProjectDocument, ProjectId};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn context() -> RenderContext {
    RenderContext {
        project_id: ProjectId::new("live-render-project").unwrap(),
        revision_id: RevisionId::new("initial").unwrap(),
    }
}

fn target() -> WorkflowTarget {
    WorkflowTarget {
        job_id: RequestId::new("live-job").unwrap(),
        attempt_id: AttemptId::new("live-attempt").unwrap(),
        cancellation_token: CancellationToken::new("live-token").unwrap(),
    }
}

fn request(operation: RenderOperation) -> RenderRequest {
    RenderRequest {
        schema_version: SCHEMA_VERSION,
        request_id: RequestId::new("external-request").unwrap(),
        context: context(),
        operation,
    }
}

fn start() -> RenderRequest {
    request(RenderOperation::Start {
        destination: "/tmp/deadpan-fake-owner-output.mp4".into(),
    })
}

fn status(stage: WorkflowStage, outcome: Option<WorkflowOutcome>, cleanup: bool) -> RenderStatus {
    RenderStatus::from_workflow(
        &context(),
        &WorkflowStatus {
            identity: Some(target().into()),
            stage,
            outcome,
            cleanup_confirmed: cleanup,
            ..WorkflowStatus::default()
        },
    )
}

fn active() -> Reply {
    Reply::Render {
        status: Box::new(status(WorkflowStage::Encoding, None, false)),
        finished: false,
    }
}

fn finished(outcome: WorkflowOutcome) -> Reply {
    Reply::Render {
        status: Box::new(status(WorkflowStage::Finished, Some(outcome), true)),
        finished: true,
    }
}

struct Observed {
    result: Result<(), PublicRenderError>,
    events: Vec<serde_json::Value>,
    errors: Vec<serde_json::Value>,
    operations: Vec<Operation>,
}

fn exercise(
    request: RenderRequest,
    cancelled: Arc<AtomicBool>,
    broken_output: bool,
    mut dispatch: impl FnMut(&Operation) -> Option<Reply>,
) -> Result<Observed, Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let package = root.path().join("live.deadpan");
    let document = ProjectDocument::new_automatic(
        context().project_id,
        context().revision_id,
        NodeId::new("root")?,
    )?;
    let mut store = ProjectStore::create(&package, &document)?;
    let mut endpoint = Some(Endpoint::bind(&mut store)?);
    assert!(matches!(
        open_writer(&package),
        Err(StoreError::AlreadyOpen)
    ));
    let output_path = root.path().join("events.jsonl");
    let error_path = root.path().join("errors.jsonl");
    let mut output = if broken_output {
        let (read, write) = io::pipe()?;
        drop(read);
        JsonOutput::from_file(File::from(std::os::fd::OwnedFd::from(write)))?
    } else {
        JsonOutput::from_file(File::create(&output_path)?)?
    };
    let mut error_output = Some(JsonOutput::from_file(File::create(&error_path)?)?);
    let request_file = root.path().join("request.json");
    std::fs::write(&request_file, serde_json::to_vec(&request)?)?;
    let parsed = read_json(&request_file)?;
    // Dispatch uses the bounded parsed request, never a second file read.
    std::fs::remove_file(&request_file)?;
    let invocation = Invocation::Json {
        path: request_file,
        cancel_only: matches!(request.operation, RenderOperation::Cancel { .. }),
    };
    let package_for_client = package.clone();
    let worker = thread::spawn(move || {
        if cfg!(target_os = "macos") {
            run_request(
                package_for_client,
                invocation,
                Some(parsed),
                &cancelled,
                &mut output,
                &mut error_output,
            )
        } else {
            run(
                &package_for_client,
                invocation,
                Some(parsed),
                &cancelled,
                &mut output,
                &mut error_output,
            )
        }
    });
    let end = Instant::now() + Duration::from_secs(5);
    let mut operations = Vec::new();
    while !worker.is_finished() {
        assert!(Instant::now() < end, "fake render owner did not complete");
        let incoming = endpoint.as_mut().map_or_else(Vec::new, Endpoint::poll);
        for incoming in incoming {
            let operation = live_project::Request::from_value(incoming.payload)?.operation;
            operations.push(operation.clone());
            if let Some(reply) = dispatch(&operation) {
                endpoint
                    .as_mut()
                    .expect("fake owner alive")
                    .respond(incoming.ticket, serde_json::to_value(reply)?)?;
            } else {
                // Simulate a delivered mutation with a lost response.
                drop(endpoint.take());
            }
        }
        thread::yield_now();
    }
    let result = worker.join().expect("client thread panicked");
    assert_eq!(store.snapshot()?, document);
    assert!(matches!(
        open_writer(&package),
        Err(StoreError::AlreadyOpen)
    ));
    let read_events = |path: &Path| -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
        if !path.exists() {
            return Ok(Vec::new());
        }
        std::fs::read_to_string(path)?
            .lines()
            .map(|line| serde_json::from_str(line).map_err(Into::into))
            .collect()
    };
    Ok(Observed {
        result,
        events: read_events(&output_path)?,
        errors: read_events(&error_path)?,
        operations,
    })
}

fn inspected(preview_active: bool) -> Reply {
    Reply::Context {
        context: context(),
        preview_active,
    }
}

#[test]
fn locked_writer_routes_once_and_waits_for_safe_completion_before_release() -> TestResult {
    let mut polls = 0;
    let observed = exercise(
        start(),
        Arc::new(AtomicBool::new(false)),
        false,
        |operation| {
            Some(match operation {
                Operation::Inspect => inspected(false),
                Operation::Render { .. } => active(),
                Operation::RenderStatus { target: actual, .. } => {
                    assert_eq!(*actual, target());
                    polls += 1;
                    if polls == 1 {
                        // The stage and cleanup do not prove worker Release acknowledgement.
                        Reply::Render {
                            status: Box::new(status(
                                WorkflowStage::Finished,
                                Some(WorkflowOutcome::Published),
                                true,
                            )),
                            finished: false,
                        }
                    } else {
                        finished(WorkflowOutcome::Published)
                    }
                }
                Operation::ReleaseRenderStatus { target: actual, .. } => {
                    assert!(polls >= 2);
                    assert_eq!(*actual, target());
                    Reply::Released
                }
                _ => panic!("unexpected fake-owner operation"),
            })
        },
    )?;
    observed.result?;
    assert_eq!(
        observed
            .operations
            .iter()
            .filter(|operation| matches!(operation, Operation::Render { .. }))
            .count(),
        1
    );
    assert_eq!(observed.events.first().unwrap()["event"], "admitted");
    assert_eq!(observed.events.last().unwrap()["event"], "finished");
    assert_eq!(
        observed.events.last().unwrap()["request_id"],
        "external-request"
    );
    assert_eq!(
        observed
            .events
            .iter()
            .filter(|event| event["event"] == "finished")
            .count(),
        1
    );
    Ok(())
}

#[test]
fn native_preview_blocks_start_before_any_render_mutation() -> TestResult {
    let observed = exercise(
        start(),
        Arc::new(AtomicBool::new(false)),
        false,
        |operation| {
            assert!(matches!(operation, Operation::Inspect));
            Some(inspected(true))
        },
    )?;
    assert_eq!(
        observed.result.unwrap_err().code,
        "RenderPreviewDecisionRequired"
    );
    assert_eq!(observed.operations.len(), 1);
    assert!(observed.events.is_empty());
    Ok(())
}

#[test]
fn signal_cancels_exact_admitted_target_then_keeps_observing() -> TestResult {
    let cancelled = Arc::new(AtomicBool::new(false));
    let trigger = cancelled.clone();
    let mut cancellations = 0;
    let observed = exercise(start(), cancelled, false, |operation| {
        Some(match operation {
            Operation::Inspect => inspected(false),
            Operation::Render { request } => match &request.operation {
                RenderOperation::Start { .. } => {
                    trigger.store(true, Ordering::Release);
                    active()
                }
                RenderOperation::Cancel { target: actual } => {
                    assert_eq!(*actual, target());
                    assert_eq!(request.context, context());
                    assert_ne!(request.request_id.as_str(), "external-request");
                    cancellations += 1;
                    let mut status = status(WorkflowStage::Cancelling, None, false);
                    status.cancellation_requested = true;
                    Reply::Render {
                        status: Box::new(status),
                        finished: false,
                    }
                }
                _ => panic!("unexpected request"),
            },
            Operation::RenderStatus { .. } => {
                let mut status = status(
                    WorkflowStage::Finished,
                    Some(WorkflowOutcome::Cancelled),
                    true,
                );
                status.cancellation_requested = true;
                Reply::Render {
                    status: Box::new(status),
                    finished: true,
                }
            }
            Operation::ReleaseRenderStatus { .. } => Reply::Released,
            _ => panic!("unexpected operation"),
        })
    })?;
    assert_eq!(cancellations, 1);
    assert_eq!(observed.result.unwrap_err().code, "Cancelled");
    assert_eq!(
        observed.events.last().unwrap()["status"]["outcome"],
        "cancelled"
    );
    Ok(())
}

#[test]
fn broken_output_requests_cancel_but_preserves_terminal_observation_on_stderr() -> TestResult {
    let mut cancellations = 0;
    let observed = exercise(
        start(),
        Arc::new(AtomicBool::new(false)),
        true,
        |operation| {
            Some(match operation {
                Operation::Inspect => inspected(false),
                Operation::Render { request } => match &request.operation {
                    RenderOperation::Start { .. } => active(),
                    RenderOperation::Cancel { target: actual } => {
                        assert_eq!(*actual, target());
                        cancellations += 1;
                        active()
                    }
                    _ => panic!("unexpected request"),
                },
                Operation::RenderStatus { .. } => finished(WorkflowOutcome::Cancelled),
                Operation::ReleaseRenderStatus { .. } => Reply::Released,
                _ => panic!("unexpected operation"),
            })
        },
    )?;
    assert_eq!(cancellations, 1);
    assert_eq!(observed.result.unwrap_err().code, "RenderOutputUnavailable");
    assert_eq!(observed.errors.last().unwrap()["event"], "finished");
    assert_eq!(
        observed.errors.last().unwrap()["status"]["target"]["cancellation_token"],
        "live-token"
    );
    assert!(matches!(
        observed.operations.last(),
        Some(Operation::ReleaseRenderStatus { .. })
    ));
    Ok(())
}

#[test]
fn lost_admission_reply_is_unknown_and_never_replayed() -> TestResult {
    let observed = exercise(
        start(),
        Arc::new(AtomicBool::new(false)),
        false,
        |operation| match operation {
            Operation::Inspect => Some(inspected(false)),
            Operation::Render { .. } => None,
            _ => panic!("request was replayed or inferred after losing admission"),
        },
    )?;
    assert_eq!(observed.result.unwrap_err().code, "HostOutcomeUnknown");
    assert_eq!(observed.operations.len(), 2);
    assert!(observed.events.is_empty());
    Ok(())
}

#[test]
fn lost_observation_retains_last_known_status_without_false_finished() -> TestResult {
    let observed = exercise(
        start(),
        Arc::new(AtomicBool::new(false)),
        false,
        |operation| match operation {
            Operation::Inspect => Some(inspected(false)),
            Operation::Render { .. } => Some(active()),
            Operation::RenderStatus { .. } => None,
            _ => panic!("request after owner connection was lost"),
        },
    )?;
    assert_eq!(observed.result.unwrap_err().code, "HostOutcomeUnknown");
    assert_eq!(
        observed.events.last().unwrap()["event"],
        "recovery_required"
    );
    assert_eq!(
        observed.events.last().unwrap()["status"]["stage"],
        "encoding"
    );
    assert!(
        !observed
            .events
            .iter()
            .any(|event| event["event"] == "finished")
    );
    Ok(())
}

#[test]
fn historical_retry_context_is_retained_and_stale_poll_target_is_rejected() -> TestResult {
    let mut first = status(WorkflowStage::Verifying, None, false);
    first.context.revision_id = RevisionId::new("historical")?;
    first.captured_revision = first.context.revision_id.clone();
    let retry = request(RenderOperation::RetryCheckpoint {
        job_id: target().job_id,
        encoding_attempt_id: AttemptId::new("encoded-before")?,
        destination: "/tmp/retry.mp4".into(),
    });
    let observed = exercise(
        retry,
        Arc::new(AtomicBool::new(false)),
        false,
        |operation| {
            Some(match operation {
                Operation::Inspect => inspected(false),
                Operation::Render { .. } => Reply::Render {
                    status: Box::new(first.clone()),
                    finished: false,
                },
                Operation::RenderStatus { .. } => {
                    let mut stale = first.clone();
                    stale.target.as_mut().unwrap().attempt_id =
                        AttemptId::new("other-attempt").unwrap();
                    Reply::Render {
                        status: Box::new(stale),
                        finished: false,
                    }
                }
                _ => panic!("stale target must not be cancelled or released"),
            })
        },
    )?;
    assert_eq!(observed.result.unwrap_err().code, "HostOutcomeUnknown");
    assert_eq!(
        observed.events.first().unwrap()["status"]["context"]["revision_id"],
        "historical"
    );
    assert_eq!(
        observed.events.last().unwrap()["event"],
        "recovery_required"
    );
    assert!(
        !observed
            .events
            .iter()
            .any(|event| event["event"] == "finished")
    );
    Ok(())
}

#[test]
fn explicit_cancel_preserves_target_and_leaves_terminal_status_for_the_initiating_observer()
-> TestResult {
    let observed = exercise(
        request(RenderOperation::Cancel { target: target() }),
        Arc::new(AtomicBool::new(false)),
        false,
        |operation| {
            Some(match operation {
                Operation::Render { request } => {
                    assert!(
                        matches!(&request.operation, RenderOperation::Cancel { target: actual } if *actual == target())
                    );
                    finished(WorkflowOutcome::Cancelled)
                }
                _ => panic!(
                    "independent cancellation must not inspect previews or release the initiating observer's status"
                ),
            })
        },
    )?;
    assert_eq!(observed.result.unwrap_err().code, "Cancelled");
    assert_eq!(observed.operations.len(), 1);
    Ok(())
}

#[test]
fn unavailable_cancel_owner_does_not_open_or_recover_a_writer() -> TestResult {
    let root = tempfile::tempdir()?;
    let package = root.path().join("closed.deadpan");
    let document = ProjectDocument::new_automatic(
        context().project_id,
        context().revision_id,
        NodeId::new("root")?,
    )?;
    drop(ProjectStore::create(&package, &document)?);
    let mut output = JsonOutput::from_file(File::create(root.path().join("events.jsonl"))?)?;
    let error = run_request(
        package.clone(),
        Invocation::Json {
            path: root.path().join("unused.json"),
            cancel_only: true,
        },
        Some(request(RenderOperation::Cancel { target: target() })),
        &AtomicBool::new(false),
        &mut output,
        &mut None,
    )
    .unwrap_err();
    assert_eq!(error.code, "RenderOwnerUnavailable");
    assert_eq!(
        ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?,
        document
    );
    Ok(())
}

#[test]
fn unconfirmed_cleanup_never_finishes_until_owner_reports_safe_unresolved_release() -> TestResult {
    let observed = exercise(
        start(),
        Arc::new(AtomicBool::new(false)),
        false,
        |operation| {
            Some(match operation {
                Operation::Inspect => inspected(false),
                Operation::Render { .. } => Reply::Render {
                    status: Box::new(status(
                        WorkflowStage::Unresolved,
                        Some(WorkflowOutcome::Unresolved),
                        false,
                    )),
                    finished: false,
                },
                Operation::RenderStatus { .. } => Reply::Render {
                    status: Box::new(status(
                        WorkflowStage::Unresolved,
                        Some(WorkflowOutcome::Unresolved),
                        true,
                    )),
                    finished: true,
                },
                Operation::ReleaseRenderStatus { .. } => Reply::Released,
                _ => panic!("unexpected operation"),
            })
        },
    )?;
    assert_eq!(observed.result.unwrap_err().code, "RenderFailed");
    assert_eq!(observed.events[1]["event"], "recovery_required");
    assert_eq!(observed.events[1]["status"]["cleanup_confirmed"], false);
    assert_eq!(observed.events.last().unwrap()["event"], "finished");
    assert_eq!(
        observed.events.last().unwrap()["status"]["cleanup_confirmed"],
        true
    );
    Ok(())
}

#[test]
fn inconsistent_finished_claim_cannot_release_status_or_fabricate_completion() -> TestResult {
    let observed = exercise(
        start(),
        Arc::new(AtomicBool::new(false)),
        false,
        |operation| {
            Some(match operation {
                Operation::Inspect => inspected(false),
                Operation::Render { .. } => active(),
                Operation::RenderStatus { .. } => Reply::Render {
                    status: Box::new(status(
                        WorkflowStage::Finished,
                        Some(WorkflowOutcome::Published),
                        false,
                    )),
                    finished: true,
                },
                _ => panic!("inconsistent cleanup must not release status"),
            })
        },
    )?;
    assert_eq!(observed.result.unwrap_err().code, "HostOutcomeUnknown");
    assert!(
        !observed
            .events
            .iter()
            .any(|event| event["event"] == "finished")
    );
    assert_eq!(
        observed.events.last().unwrap()["event"],
        "recovery_required"
    );
    Ok(())
}

#[test]
fn owner_admission_rejection_is_returned_without_polling_or_retry() -> TestResult {
    let observed = exercise(
        start(),
        Arc::new(AtomicBool::new(false)),
        false,
        |operation| {
            Some(match operation {
                Operation::Inspect => inspected(false),
                Operation::Render { .. } => Reply::Failed {
                    error: LiveError::new("RenderBusy", "Existing render still owns the slot"),
                },
                _ => panic!("rejected admission must not poll or retry"),
            })
        },
    )?;
    assert_eq!(observed.result.unwrap_err().code, "RenderBusy");
    assert!(observed.events.is_empty());
    assert_eq!(observed.operations.len(), 2);
    Ok(())
}
