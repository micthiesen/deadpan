use super::*;

#[test]
fn parser_keeps_retry_reencode_and_reconciliation_explicit() {
    assert!(matches!(
        parse(&[
            "retry",
            "p.deadpan",
            "--job",
            "j",
            "--checkpoint",
            "a",
            "--output",
            "/tmp"
        ])
        .unwrap()
        .1,
        Invocation::Retry {
            checkpoint: Some(_),
            ..
        }
    ));
    assert!(matches!(
        parse(&["reencode", "p.deadpan", "--job", "j", "--output", "/tmp"])
            .unwrap()
            .1,
        Invocation::Retry {
            checkpoint: None,
            ..
        }
    ));
    assert!(matches!(
        parse(&["reconcile", "p.deadpan", "--publication", "p"])
            .unwrap()
            .1,
        Invocation::Reconcile(_)
    ));
    for args in [
        vec!["p.deadpan", "--output", "/tmp", "--encoder", "software"],
        vec!["p.deadpan", "--output", "/tmp", "--output", "/tmp"],
        vec!["retry", "p.deadpan", "--job", "j", "--output", "/tmp"],
        vec![
            "reencode",
            "p.deadpan",
            "--job",
            "j",
            "--checkpoint",
            "a",
            "--output",
            "/tmp",
        ],
        vec![
            "reconcile",
            "p.deadpan",
            "--publication",
            "p",
            "--output",
            "/tmp",
        ],
        vec!["status", "p.deadpan", "--job", "j", "--publications"],
        vec!["status", "p.deadpan", "--after-attempt", "5"],
        vec!["status", "p.deadpan", "--after-attempt", "0"],
        vec![
            "status",
            "p.deadpan",
            "--publications",
            "--after-attempt",
            "0",
        ],
    ] {
        assert!(parse(&args).is_err(), "{args:?}");
    }
}

#[test]
fn request_file_rejects_fifo_symlink_directory_and_oversized_regular_file()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let fifo = root.path().join("request.fifo");
    #[cfg(target_os = "linux")]
    rustix::fs::mkfifoat(
        rustix::fs::CWD,
        &fifo,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )?;
    #[cfg(target_os = "macos")]
    {
        let mut command = std::process::Command::new("mkfifo");
        command.arg(&fifo);
        assert!(
            deadpan_native_process::spawn(&mut command)?
                .wait()?
                .success()
        );
    }
    assert_eq!(read_json(&fifo).unwrap_err().code, "RenderInvalidRequest");
    assert_eq!(
        read_json(root.path()).unwrap_err().code,
        "RenderInvalidRequest"
    );
    let oversized = root.path().join("large.json");
    File::create(&oversized)?.set_len((MAX_REQUEST_BYTES + 1) as u64)?;
    assert_eq!(
        read_json(&oversized).unwrap_err().code,
        "RenderInvalidRequest"
    );
    let link = root.path().join("link.json");
    std::os::unix::fs::symlink(&oversized, &link)?;
    assert!(read_json(&link).is_err());
    Ok(())
}

#[test]
fn json_output_bound_fails_before_writing_any_partial_record()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("output.jsonl");
    let mut output = JsonOutput::from_file(File::create(&path)?)?;
    let error = output.send(&"x".repeat(MAX_REPLY_BYTES)).unwrap_err();
    assert_eq!(error.code, "RenderOutputLimit");
    assert_eq!(std::fs::metadata(path)?.len(), 0);
    assert_eq!(
        output.send(&"small").unwrap_err().code,
        "RenderOutputUnavailable"
    );
    Ok(())
}

#[test]
fn owned_console_duplicates_close_on_exec() -> Result<(), Box<dyn std::error::Error>> {
    for output in [JsonOutput::stdout()?, JsonOutput::stderr()?] {
        assert!(rustix::io::fcntl_getfd(&output.file)?.contains(rustix::io::FdFlags::CLOEXEC));
    }
    Ok(())
}

// These controlled observations test transport only. They do not encode media,
// manufacture a verified candidate, or authorize a real publication.
fn important_observation(committed: bool) -> Result<RenderEvent, Box<dyn std::error::Error>> {
    use crate::encoded_render::{publication::PublicationReceipt, workflow::WorkflowStatus};
    let context = RenderContext {
        project_id: deadpan_core::ProjectId::new("output-project")?,
        revision_id: RevisionId::new("captured-revision")?,
    };
    let mut status = WorkflowStatus {
        identity: Some(WorkflowIdentity {
            job_id: RequestId::new("job")?,
            attempt_id: AttemptId::new("attempt")?,
            cancellation_token: CancellationToken::new("token")?,
        }),
        stage: if committed {
            WorkflowStage::Finished
        } else {
            WorkflowStage::Unresolved
        },
        outcome: Some(if committed {
            WorkflowOutcome::PublishedUnconfirmed
        } else {
            WorkflowOutcome::Unresolved
        }),
        observed_movie_commit: committed,
        cleanup_confirmed: committed,
        ..WorkflowStatus::default()
    };
    status.retained.partial_report = Some("/observation/report.partial".into());
    if committed {
        status.receipt = Some(PublicationReceipt {
            publication_id: "publication".into(),
            movie: "/observation/movie.mp4".into(),
            report: "/observation/report.json".into(),
            movie_sha256: deadpan_jobs::Sha256::new("a".repeat(64))?,
            movie_bytes: 10,
            report_sha256: deadpan_jobs::Sha256::new("b".repeat(64))?,
            report_bytes: 20,
            contains_generated_pictures: false,
        });
    }
    let status = Box::new(RenderStatus::from_workflow(&context, &status));
    let request_id = RequestId::new("important-request")?;
    Ok(if committed {
        RenderEvent::Finished {
            schema_version: SCHEMA_VERSION,
            request_id,
            status,
        }
    } else {
        RenderEvent::RecoveryRequired {
            schema_version: SCHEMA_VERSION,
            request_id,
            status,
            error: PublicRenderError::new(
                "RenderCleanupUnconfirmed",
                "Owned worker cleanup remains unconfirmed",
            ),
        }
    })
}

fn fallback_preserves_observation(
    mut stdout: JsonOutput,
    event: RenderEvent,
) -> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("stderr.jsonl");
    let mut stderr = Some(JsonOutput::from_file(File::create(&path)?)?);
    assert_eq!(
        send_important(&mut stdout, &mut stderr, &event)
            .unwrap_err()
            .code,
        "RenderOutputUnavailable"
    );
    assert!(stdout.failed);
    drop(stderr);
    let bytes = std::fs::read_to_string(path)?;
    assert_eq!(bytes.lines().count(), 1);
    let actual: serde_json::Value = serde_json::from_str(&bytes)?;
    assert_eq!(actual, serde_json::to_value(event)?);
    assert_eq!(actual["request_id"], "important-request");
    assert_eq!(actual["status"]["target"]["cancellation_token"], "token");
    assert_eq!(
        actual["status"]["retained"]["partial_report"],
        "/observation/report.partial"
    );
    Ok(())
}

#[test]
fn broken_stdout_preserves_committed_and_unresolved_events_on_stderr()
-> Result<(), Box<dyn std::error::Error>> {
    for committed in [true, false] {
        let (reader, writer) = std::io::pipe()?;
        drop(reader);
        let output = JsonOutput::from_file(File::from(std::os::fd::OwnedFd::from(writer)))?;
        fallback_preserves_observation(output, important_observation(committed)?)?;
    }
    Ok(())
}

#[test]
fn full_stdout_preserves_committed_and_unresolved_events_on_stderr()
-> Result<(), Box<dyn std::error::Error>> {
    for committed in [true, false] {
        let (_reader, writer) = std::io::pipe()?;
        let mut output = JsonOutput::from_file(File::from(std::os::fd::OwnedFd::from(writer)))?;
        let mut full = false;
        for _ in 0..2048 {
            match output.file.write(&[0; 4096]) {
                Ok(count) => assert!(count > 0),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    full = true;
                    break;
                }
                Err(error) => return Err(error.into()),
            }
        }
        assert!(full, "fixture pipe did not fill within the bounded setup");
        fallback_preserves_observation(output, important_observation(committed)?)?;
    }
    Ok(())
}

#[test]
fn requested_cancel_pumps_actual_capture_and_releases_the_writer_without_media_claims()
-> Result<(), Box<dyn std::error::Error>> {
    use deadpan_core::{
        ColorPolicy, FrameRate, NodeId, PresentationBasis, ProjectDocument, ProjectId, RevisionId,
    };
    let root = tempfile::tempdir()?;
    let package = root.path().join("cancel.deadpan");
    let document = ProjectDocument::new(
        ProjectId::new("cancel-project")?,
        RevisionId::new("initial")?,
        PresentationBasis {
            width: 320,
            height: 180,
            frame_rate: FrameRate::new(30, 1)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?;
    let context = RenderContext::from_document(&document);
    let mut store = ProjectStore::create(&package, &document)?;
    let mut workflow = RenderWorkflow::new(&store, current_runtime_config(package.clone())?)?;
    let mut request = start_request(
        &context,
        RenderAutomaticAlgorithm::AutomaticSdrV1,
        root.path().join("not-created.mp4"),
        Instant::now(),
    )?;
    request.deadline = Instant::now() + Duration::from_secs(5);
    workflow.start(&mut store, request)?;
    let path = root.path().join("events.jsonl");
    let mut output = JsonOutput::from_file(File::create(&path)?)?;
    let error = pump(
        &mut workflow,
        &mut store,
        (&context, &RequestId::new("cancel-command")?),
        &AtomicBool::new(true),
        &mut output,
        &mut None,
        None,
    )
    .unwrap_err();
    assert_eq!(error.code, "Cancelled");
    assert!(workflow.can_release_writer());
    assert!(workflow.status().cleanup_confirmed);
    assert_eq!(workflow.status().outcome, Some(WorkflowOutcome::Cancelled));
    assert!(workflow.status().receipt.is_none());
    assert!(!root.path().join("not-created.mp4").exists());
    assert_eq!(store.snapshot()?, document);
    drop(workflow);
    drop(store);
    let _reopened = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let events = std::fs::read_to_string(path)?;
    let last: serde_json::Value =
        serde_json::from_str(events.lines().last().ok_or("missing terminal event")?)?;
    assert_eq!(last["event"], "finished");
    assert_eq!(last["request_id"], "cancel-command");
    assert_eq!(last["status"]["outcome"], "cancelled");
    Ok(())
}
