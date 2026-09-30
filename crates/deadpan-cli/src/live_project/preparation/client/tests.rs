use super::*;
use crate::host::Endpoint;
use crate::live_project::Request;
use crate::live_project::preparation::PreparationReceipt;
use deadpan_core::{NodeId, ProjectDocument, RevisionId};
use deadpan_store::ProjectStore;
use serde_json::json;
use std::thread;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
struct Observed {
    result: Result<PreparationStatus, LiveError>,
    operations: Vec<Operation>,
}
fn exercise(
    cancelled: Arc<AtomicBool>,
    dispatch: impl FnMut(&Operation) -> Option<Reply>,
) -> TestResult<Observed> {
    exercise_command(PreparationCommand::Checkpoint {}, cancelled, dispatch)
}

fn exercise_command(
    command: PreparationCommand,
    cancelled: Arc<AtomicBool>,
    mut dispatch: impl FnMut(&Operation) -> Option<Reply>,
) -> TestResult<Observed> {
    let root = tempfile::tempdir()?;
    let package = root.path().join("owner.deadpan");
    let project = ProjectId::new("preparation-client")?;
    let document = ProjectDocument::new_automatic(
        project.clone(),
        RevisionId::new("initial")?,
        NodeId::new("root")?,
    )?;
    let mut store = ProjectStore::create(&package, &document)?;
    let mut endpoint = Some(Endpoint::bind(&mut store)?);
    let worker = thread::spawn(move || -> Result<PreparationStatus, LiveError> {
        let mut client = Client::discover(&package)
            .map_err(LiveError::from)?
            .unwrap();
        observe(
            &mut client,
            &project,
            &PreparationTarget::fresh(),
            command,
            &cancelled,
        )
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut operations = Vec::new();
    while !worker.is_finished() {
        assert!(
            Instant::now() < deadline,
            "fake preparation owner timed out"
        );
        for incoming in endpoint.as_mut().map_or_else(Vec::new, Endpoint::poll) {
            let operation = Request::from_value(incoming.payload)?.operation;
            operations.push(operation.clone());
            if let Some(reply) = dispatch(&operation) {
                endpoint
                    .as_mut()
                    .unwrap()
                    .respond(incoming.ticket, serde_json::to_value(reply)?)?;
            } else {
                drop(endpoint.take());
            }
        }
        thread::yield_now();
    }
    assert_eq!(store.snapshot()?, document);
    Ok(Observed {
        result: worker.join().unwrap(),
        operations,
    })
}
fn status(target: &PreparationTarget, state: PreparationState) -> Reply {
    Reply::Preparation {
        status: Box::new(PreparationStatus {
            target: target.clone(),
            state,
        }),
    }
}
fn completed() -> PreparationState {
    PreparationState::Completed {
        output: json!({"protocol":1,"database_checkpoint":"/tmp/captured-checkpoint.sqlite"}),
        receipt: PreparationReceipt::Checkpoint {
            path: "/tmp/captured-checkpoint.sqlite".into(),
            project_id: ProjectId::new("preparation-client").unwrap(),
            revision_id: RevisionId::new("captured").unwrap(),
        },
        committed_revision: None,
        inventory_changed: false,
        completion_error: None,
        refresh_error: None,
    }
}

#[test]
fn asynchronous_observation_retains_one_target_until_worker_completed() -> TestResult {
    let mut polls = 0;
    let observed = exercise(
        Arc::new(AtomicBool::new(false)),
        |operation| match operation {
            Operation::Prepare {
                target, command, ..
            } => {
                assert!(matches!(
                    command.as_ref(),
                    PreparationCommand::Checkpoint {}
                ));
                Some(status(target, PreparationState::Preparing {}))
            }
            Operation::PreparationStatus { target, .. } => {
                polls += 1;
                Some(status(
                    target,
                    if polls == 1 {
                        PreparationState::AwaitingCommit {}
                    } else {
                        completed()
                    },
                ))
            }
            _ => panic!("unexpected operation"),
        },
    )?;
    assert_eq!(observed.operations.len(), 3);
    assert!(observed.result?.is_terminal());
    Ok(())
}

#[test]
fn cancellation_is_sent_once_then_observes_worker_drain() -> TestResult {
    let cancelled = Arc::new(AtomicBool::new(false));
    let set = cancelled.clone();
    let observed = exercise(cancelled, |operation| match operation {
        Operation::Prepare { target, .. } => {
            set.store(true, Ordering::Release);
            Some(status(target, PreparationState::Preparing {}))
        }
        Operation::CancelPreparation { target, .. } => {
            Some(status(target, PreparationState::Cancelling {}))
        }
        Operation::PreparationStatus { target, .. } => {
            Some(status(target, PreparationState::Cancelled {}))
        }
        _ => panic!("unexpected operation"),
    })?;
    assert!(matches!(
        observed.result?.state,
        PreparationState::Cancelled {}
    ));
    assert_eq!(
        observed
            .operations
            .iter()
            .filter(|op| matches!(op, Operation::CancelPreparation { .. }))
            .count(),
        1
    );
    assert_eq!(observed.operations.len(), 3);
    Ok(())
}

#[test]
fn commit_racing_cancel_keeps_completion_and_operational_receipt() -> TestResult {
    let cancelled = Arc::new(AtomicBool::new(false));
    let set = cancelled.clone();
    let observed = exercise(cancelled, |operation| match operation {
        Operation::Prepare { target, .. } => {
            set.store(true, Ordering::Release);
            Some(status(target, PreparationState::Preparing {}))
        }
        Operation::CancelPreparation { target, .. } => Some(status(target, completed())),
        _ => panic!("unexpected operation"),
    })?;
    let output = terminal_output(observed.result?)?;
    assert_eq!(
        output["database_checkpoint"],
        "/tmp/captured-checkpoint.sqlite"
    );
    assert_eq!(observed.operations.len(), 2);
    Ok(())
}

#[test]
fn admission_lost_reply_is_never_replayed() -> TestResult {
    let observed = exercise(Arc::new(AtomicBool::new(false)), |_| None)?;
    let error = observed.result.unwrap_err();
    assert_eq!(error.code, "HostOutcomeUnknown");
    assert!(error.message.contains("no replay was attempted"));
    assert_eq!(observed.operations.len(), 1);
    Ok(())
}

#[test]
fn wrong_target_and_backwards_state_are_unknown_not_cancelled() -> TestResult {
    let observed = exercise(Arc::new(AtomicBool::new(false)), |_| {
        Some(status(&PreparationTarget::fresh(), completed()))
    })?;
    assert_eq!(
        observed.result.unwrap_err().code,
        "HostPreparationOutcomeUnknown"
    );
    let observed = exercise(
        Arc::new(AtomicBool::new(false)),
        |operation| match operation {
            Operation::Prepare { target, .. } => {
                Some(status(target, PreparationState::AwaitingCommit {}))
            }
            Operation::PreparationStatus { target, .. } => {
                Some(status(target, PreparationState::Preparing {}))
            }
            _ => panic!("unexpected operation"),
        },
    )?;
    assert_eq!(
        observed.result.unwrap_err().code,
        "HostPreparationOutcomeUnknown"
    );
    Ok(())
}

#[test]
fn compact_receipt_and_post_commit_failure_are_preserved_in_cli_output() -> TestResult {
    let mut state = completed();
    let PreparationState::Completed {
        output,
        refresh_error,
        completion_error,
        ..
    } = &mut state
    else {
        unreachable!()
    };
    *output = json!({"protocol":1,"host_reply_detail_omitted":true});
    *refresh_error = Some("Refresh failed after commit".into());
    *completion_error = Some(LiveError::new(
        "CheckpointPublishedUnconfirmed",
        "checkpoint exists; directory sync failed",
    ));
    let output = terminal_output(PreparationStatus {
        target: PreparationTarget::fresh(),
        state,
    })?;
    assert_eq!(output["preparation_receipt"]["revision_id"], "captured");
    assert_eq!(
        output["completion_error"]["code"],
        "CheckpointPublishedUnconfirmed"
    );
    assert_eq!(output["host_refresh_error"], "Refresh failed after commit");
    Ok(())
}

#[test]
fn unchanged_relink_version_is_a_valid_completed_noop() -> TestResult {
    let content = deadpan_store::original_media::OriginalContentId::new("a".repeat(64))?;
    let command = PreparationCommand::Relink {
        content: content.clone(),
        expected_version: 3,
        location: deadpan_store::original_media::LinkedOriginal::new(
            "/tmp/unchanged.mp4".into(),
            None,
        )?,
    };
    let status = PreparationStatus {
        target: PreparationTarget::fresh(),
        state: PreparationState::Completed {
            output: json!({"protocol":1,"relinked_original":{"version":3}}),
            receipt: PreparationReceipt::Relinked {
                content,
                location_version: 3,
            },
            committed_revision: None,
            inventory_changed: true,
            completion_error: None,
            refresh_error: None,
        },
    };
    validate_completion(&ProjectId::new("preparation-client")?, &command, &status)?;
    Ok(())
}

#[test]
fn known_terminal_failures_release_but_failed_commit_output_keeps_receipt() {
    use std::cell::Cell;
    for state in [
        PreparationState::Failed {
            error: LiveError::new("SourceVideoDecodeFailed", "invalid fixture"),
        },
        PreparationState::Cancelled {},
    ] {
        let released = Cell::new(false);
        let result = finish(
            PreparationStatus {
                target: PreparationTarget::fresh(),
                state,
            },
            |_| panic!("failure has no stdout receipt"),
            || released.set(true),
        );
        assert!(result.is_err());
        assert!(released.get());
    }
    let released = Cell::new(false);
    let result = finish(
        PreparationStatus {
            target: PreparationTarget::fresh(),
            state: completed(),
        },
        |_| Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "closed output").into()),
        || released.set(true),
    );
    assert!(result.is_err());
    assert!(
        !released.get(),
        "committed receipt must remain queryable after output failure"
    );
}

#[test]
fn registration_reuses_qualified_identity_but_rejects_inconsistent_detail_without_replay()
-> TestResult {
    use crate::live_project::preparation::SourceStreams;
    use deadpan_core::{AssetId, SourceQualificationId};
    use deadpan_store::original_media::OriginalContentId;
    use deadpan_store::source_registration::SourceRegistration;

    let command = PreparationCommand::Register {
        registration: SourceRegistration {
            expected_revision: RevisionId::new("initial")?,
            new_revision: RevisionId::new("proposed-new-revision")?,
            original: OriginalContentId::new("b".repeat(64))?,
            new_asset_id: AssetId::new("unused-proposed-alias")?,
            label: "Existing qualification".into(),
            insertion: None,
        },
        streams: SourceStreams::VideoOnly {},
    };
    let state = PreparationState::Completed {
        output: json!({"protocol":1,"committed":false,"outcome":{
            "asset_id":"existing-qualified-asset","qualification":"a".repeat(64),"commit":null,
        }}),
        receipt: PreparationReceipt::Registered {
            asset: AssetId::new("existing-qualified-asset")?,
            qualification: SourceQualificationId::new("a".repeat(64))?,
        },
        committed_revision: None,
        inventory_changed: false,
        completion_error: None,
        refresh_error: None,
    };
    for change in [
        None,
        Some("asset_id"),
        Some("qualification"),
        Some("compact"),
    ] {
        let mut observed_state = state.clone();
        let PreparationState::Completed { output, .. } = &mut observed_state else {
            unreachable!()
        };
        match change {
            Some("asset_id") => output["outcome"]["asset_id"] = json!("unrelated-asset"),
            Some("qualification") => output["outcome"]["qualification"] = json!("c".repeat(64)),
            Some("compact") => *output = json!({"protocol":1,"host_reply_detail_omitted":true}),
            _ => {}
        }
        let observed = exercise_command(
            command.clone(),
            Arc::new(AtomicBool::new(false)),
            |operation| {
                let Operation::Prepare { target, .. } = operation else {
                    panic!("unexpected replay or poll")
                };
                Some(status(target, observed_state.clone()))
            },
        )?;
        assert_eq!(observed.operations.len(), 1);
        if matches!(change, Some("asset_id" | "qualification")) {
            assert_eq!(
                observed.result.unwrap_err().code,
                "HostPreparationOutcomeUnknown"
            );
        } else {
            assert!(observed.result?.is_terminal());
        }
    }
    Ok(())
}
