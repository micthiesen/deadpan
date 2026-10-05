use super::*;
use deadpan_cli::live_project::preparation::{
    PreparationCommand, PreparationOwnership, PreparationReceipt, PreparationState,
    PreparationStatus, PreparationTarget, SourceStreams,
};
use deadpan_store::source_registration::{
    SourceInsertionPurpose, SourceInsertionRequest, SourceRegistration,
};

fn start(
    client: &mut Client,
    workspace: &Workspace,
    command: PreparationCommand,
) -> PreparationTarget {
    let target = PreparationTarget::fresh();
    let Reply::Preparation { status } = live_project::request(
        client,
        Operation::Prepare {
            project_id: workspace.document.project_id().clone(),
            target: target.clone(),
            command: Box::new(command),
        },
    )
    .unwrap() else {
        panic!("preparation admission")
    };
    assert_eq!(status.target, target);
    assert!(matches!(status.state, PreparationState::Preparing {}));
    target
}

fn status(
    client: &mut Client,
    workspace: &Workspace,
    target: &PreparationTarget,
) -> PreparationStatus {
    let Reply::Preparation { status } = live_project::request(
        client,
        Operation::PreparationStatus {
            project_id: workspace.document.project_id().clone(),
            target: target.clone(),
        },
    )
    .unwrap() else {
        panic!("preparation status")
    };
    assert_eq!(&status.target, target);
    *status
}

fn terminal(
    client: &mut Client,
    workspace: &Workspace,
    target: &PreparationTarget,
) -> PreparationState {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let state = status(client, workspace, target).state;
        if state.is_terminal() {
            return state;
        }
        assert!(
            Instant::now() < deadline,
            "preparation did not finish: {state:?}"
        );
        std::thread::yield_now();
    }
}

fn finish(harness: &Harness, job: worker::Job) {
    let id = job.id;
    let result = worker::prepare(job);
    harness.replies.send(worker::Reply { id, result }).unwrap();
}

fn retain(
    harness: &Harness,
    client: &mut Client,
    workspace: &Workspace,
) -> deadpan_store::original_media::OriginalContentId {
    let target = start(
        client,
        workspace,
        PreparationCommand::Retain {
            path: fixture("cfr-bframes.mp4"),
            ownership: PreparationOwnership::Managed {},
        },
    );
    finish(harness, harness.job());
    let PreparationState::Completed {
        receipt: PreparationReceipt::Retained { content, .. },
        committed_revision,
        completion_error,
        refresh_error,
        ..
    } = terminal(client, workspace, &target)
    else {
        panic!("retention completed")
    };
    assert!(committed_revision.is_none() && completion_error.is_none() && refresh_error.is_none());
    harness.service.take_update();
    content
}

#[test]
fn retention_stays_operational_and_preserves_an_edit_during_worker_preparation() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("retain.deadpan");
    let harness = Harness::new();
    let initial = opened(&harness, &path);
    let mut client = client(&path);
    let target = start(
        &mut client,
        &initial,
        PreparationCommand::Retain {
            path: fixture("cfr-bframes.mp4"),
            ownership: PreparationOwnership::Managed {},
        },
    );
    let job = harness.job();
    assert!(!harness.service.is_busy());
    committed(
        live_project::request(&mut client, change(&initial, "during-retention", 17)).unwrap(),
        "during-retention",
    );
    let edited = harness.service.take_update().unwrap().workspace.unwrap();
    finish(&harness, job);
    let PreparationState::Completed {
        output,
        receipt,
        committed_revision,
        inventory_changed,
        completion_error,
        refresh_error,
    } = terminal(&mut client, &initial, &target)
    else {
        panic!("retention result")
    };
    assert!(matches!(receipt, PreparationReceipt::Retained { .. }));
    assert_eq!(output["authored_asset_registered"], false);
    assert!(committed_revision.is_none() && completion_error.is_none() && refresh_error.is_none());
    assert!(inventory_changed);
    assert!(
        harness.jobs.try_recv().is_err(),
        "retention must not implicitly start registration"
    );
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly).unwrap();
    assert_eq!(reader.snapshot().unwrap(), *edited.document);
    assert_eq!(reader.original_records(None, 100).unwrap().len(), 1);
    assert!(edited.sources.is_empty());
    let update = harness.service.take_update().unwrap();
    assert!(update.committed.is_none());
    assert_eq!(*update.workspace.unwrap().document, *edited.document);
    shutdown(&harness);
}

#[test]
fn new_project_cannot_allocate_a_package_while_remote_preparation_is_draining() {
    for rendering in [false, true] {
        let scratch = tempfile::tempdir().unwrap();
        let path = scratch.path().join("existing.deadpan");
        let library = ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap();
        let harness = Harness::with_library(Some(library.clone()));
        let initial = opened(&harness, &path);
        let mut client = client(&path);
        let paused = rendering.then(|| PausedRender::new(&harness.service));
        let render = if rendering {
            let Reply::Render { status, .. } = live_project::request(
                &mut client,
                render_request(
                    &initial,
                    RenderOperation::Start {
                        destination: scratch.path().join("cancelled.mp4"),
                    },
                ),
            )
            .unwrap() else {
                panic!("render admission")
            };
            status.target
        } else {
            None
        };
        let target = start(&mut client, &initial, PreparationCommand::Checkpoint {});
        let job = harness.job();
        for cancelling in [false, true] {
            if cancelling {
                live_project::request(
                    &mut client,
                    Operation::CancelPreparation {
                        project_id: initial.document.project_id().clone(),
                        target: target.clone(),
                    },
                )
                .unwrap();
            }
            let refused = command(
                &harness.service,
                ProjectRequest::CreateFromSource {
                    path: fixture("cfr-bframes.mp4"),
                },
            );
            assert!(refused.error.unwrap().contains("current import"));
            let retained = refused.workspace.unwrap();
            assert_eq!(retained.session, initial.session);
            assert_eq!(retained.path, initial.path);
            assert_eq!(*retained.document, *initial.document);
            assert!(harness.jobs.try_recv().is_err());
            if library.root().exists() {
                assert_eq!(std::fs::read_dir(library.root()).unwrap().count(), 0);
            }
        }
        finish(&harness, job);
        assert!(matches!(
            terminal(&mut client, &initial, &target),
            PreparationState::Cancelled {}
        ));
        if let Some(target) = render {
            live_project::request(
                &mut client,
                render_request(&initial, RenderOperation::Cancel { target }),
            )
            .unwrap();
        }
        drop(paused);
        shutdown(&harness);
    }
}

#[test]
fn registration_keeps_all_caller_ids_and_refuses_a_late_revision() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("register.deadpan");
    let harness = Harness::new();
    let initial = opened(&harness, &path);
    let mut client = client(&path);
    let content = retain(&harness, &mut client, &initial);
    let registration = SourceRegistration {
        expected_revision: initial.document.revision_id().clone(),
        new_revision: RevisionId::new("caller-registration").unwrap(),
        original: content,
        new_asset_id: AssetId::new("caller-video").unwrap(),
        label: "Caller source label".into(),
        insertion: Some(SourceInsertionRequest {
            parent: initial.document.root().clone(),
            index: 1,
            node: node("caller-insertion"),
            label: "Caller insertion label".into(),
            purpose: SourceInsertionPurpose::Secondary,
        }),
    };
    let target = start(
        &mut client,
        &initial,
        PreparationCommand::Register {
            registration: registration.clone(),
            streams: SourceStreams::VideoOnly {},
        },
    );
    let job = harness.job();
    committed(
        live_project::request(&mut client, change(&initial, "new-head", 13)).unwrap(),
        "new-head",
    );
    let current = harness.service.take_update().unwrap().workspace.unwrap();
    finish(&harness, job);
    let PreparationState::Failed { error } = terminal(&mut client, &initial, &target) else {
        panic!("stale registration refused")
    };
    assert_eq!(error.code, "RevisionConflict");
    assert_eq!(
        error.current_revision.as_ref(),
        Some(current.document.revision_id())
    );
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly).unwrap();
    assert_eq!(reader.snapshot().unwrap(), *current.document);
    let registration = SourceRegistration {
        expected_revision: current.document.revision_id().clone(),
        ..registration
    };
    let target = start(
        &mut client,
        &current,
        PreparationCommand::Register {
            registration: registration.clone(),
            streams: SourceStreams::VideoOnly {},
        },
    );
    finish(&harness, harness.job());
    let PreparationState::Completed {
        receipt: PreparationReceipt::Registered { asset, .. },
        committed_revision,
        completion_error,
        refresh_error,
        ..
    } = terminal(&mut client, &current, &target)
    else {
        panic!("registration result")
    };
    assert_eq!(asset, registration.new_asset_id);
    assert_eq!(committed_revision, Some(registration.new_revision.clone()));
    assert!(completion_error.is_none() && refresh_error.is_none());
    let update = harness.service.take_update().unwrap();
    assert!(
        update.committed.is_none(),
        "remote registration cannot invent native selection"
    );
    let saved = update.workspace.unwrap();
    assert_eq!(saved.document.revision_id(), &registration.new_revision);
    assert!(
        saved
            .document
            .assets()
            .contains_key(&registration.new_asset_id)
    );
    assert_eq!(
        saved.document.nodes()[&node("caller-insertion")].label,
        "Caller insertion label"
    );
    assert!(saved.sources.contains_key(&registration.new_asset_id));
    shutdown(&harness);
}

#[test]
fn cancellation_requires_the_exact_target_and_waits_for_the_worker_reply() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("cancel.deadpan");
    let harness = Harness::new();
    let initial = opened(&harness, &path);
    let mut client = client(&path);
    let command = PreparationCommand::Retain {
        path: fixture("cfr-bframes.mp4"),
        ownership: PreparationOwnership::Managed {},
    };
    let target = start(&mut client, &initial, command.clone());
    let job = harness.job();
    let mut wrong = target.clone();
    wrong.cancellation_token = uuid::Uuid::new_v4();
    assert_eq!(
        live_project::request(
            &mut client,
            Operation::CancelPreparation {
                project_id: initial.document.project_id().clone(),
                target: wrong,
            }
        )
        .unwrap_err()
        .code,
        "HostPreparationMissing"
    );
    assert!(!job.cancelled.load(Ordering::Acquire));
    assert_eq!(
        live_project::request(
            &mut client,
            Operation::Prepare {
                project_id: initial.document.project_id().clone(),
                target: target.clone(),
                command: Box::new(command),
            }
        )
        .unwrap_err()
        .code,
        "HostPreparationIdentityUsed"
    );
    let _busy = Busy::new(&harness.service);
    let Reply::Preparation { status: cancelling } = live_project::request(
        &mut client,
        Operation::CancelPreparation {
            project_id: initial.document.project_id().clone(),
            target: target.clone(),
        },
    )
    .unwrap() else {
        panic!("cancel reply")
    };
    assert!(matches!(cancelling.state, PreparationState::Cancelling {}));
    assert!(job.cancelled.load(Ordering::Acquire));
    assert_eq!(
        live_project::request(
            &mut client,
            Operation::ReleasePreparationStatus {
                project_id: initial.document.project_id().clone(),
                target: target.clone(),
            }
        )
        .unwrap_err()
        .code,
        "HostPreparationBusy"
    );
    finish(&harness, job);
    assert!(matches!(
        terminal(&mut client, &initial, &target),
        PreparationState::Cancelled {}
    ));
    // Status and cancellation work even while a native command holds admission.
    assert!(harness.service.is_busy());
    drop(_busy);
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly).unwrap();
    assert!(reader.original_records(None, 100).unwrap().is_empty());
    assert_eq!(reader.snapshot().unwrap(), *initial.document);
    shutdown(&harness);
}

#[test]
fn checkpoint_reports_the_worker_snapshot_and_preserves_the_later_edit() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("checkpoint.deadpan");
    let harness = Harness::new();
    let initial = opened(&harness, &path);
    let mut client = client(&path);
    let target = start(&mut client, &initial, PreparationCommand::Checkpoint {});
    let job = harness.job();
    committed(
        live_project::request(&mut client, change(&initial, "captured-checkpoint", 11)).unwrap(),
        "captured-checkpoint",
    );
    let captured = harness.service.take_update().unwrap().workspace.unwrap();
    let id = job.id;
    let prepared = worker::prepare(job);
    committed(
        live_project::request(&mut client, change(&captured, "after-checkpoint-copy", 12)).unwrap(),
        "after-checkpoint-copy",
    );
    let later = harness.service.take_update().unwrap().workspace.unwrap();
    harness
        .replies
        .send(worker::Reply {
            id,
            result: prepared,
        })
        .unwrap();
    let PreparationState::Completed {
        output,
        receipt:
            PreparationReceipt::Checkpoint {
                path: checkpoint,
                project_id,
                revision_id,
            },
        committed_revision,
        inventory_changed,
        completion_error,
        refresh_error,
    } = terminal(&mut client, &initial, &target)
    else {
        panic!("checkpoint result")
    };
    assert_eq!(&project_id, captured.document.project_id());
    assert_eq!(&revision_id, captured.document.revision_id());
    assert_eq!(
        output["database_checkpoint"],
        serde_json::to_value(&checkpoint).unwrap()
    );
    assert!(
        committed_revision.is_none()
            && !inventory_changed
            && completion_error.is_none()
            && refresh_error.is_none()
    );
    // The head may be stored as a patch: read the standalone database copy
    // through a read-only package around it.
    let copy = tempfile::tempdir().unwrap();
    let package = copy.path().join("checkpoint-copy.deadpan");
    for directory in [
        "Media/Originals",
        "Media/Generated",
        "Media/RenderCandidates",
    ] {
        std::fs::create_dir_all(package.join(directory)).unwrap();
    }
    std::fs::copy(&checkpoint, package.join("project.sqlite")).unwrap();
    assert_eq!(
        ProjectStore::open(&package, AccessMode::ReadOnly)
            .unwrap()
            .snapshot()
            .unwrap(),
        *captured.document
    );
    assert_eq!(
        ProjectStore::open(&path, AccessMode::ReadOnly)
            .unwrap()
            .snapshot()
            .unwrap(),
        *later.document
    );
    shutdown(&harness);
}

#[test]
fn owner_replacement_revokes_late_preparation_and_keeps_the_worker_bounded() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("old.deadpan");
    let harness = Harness::new();
    let initial = opened(&harness, &path);
    let mut old_client = client(&path);
    let target = start(
        &mut old_client,
        &initial,
        PreparationCommand::Retain {
            path: fixture("cfr-bframes.mp4"),
            ownership: PreparationOwnership::Managed {},
        },
    );
    let job = harness.job();
    let id = job.id;
    let prepared = worker::prepare(job);
    command(&harness.service, ProjectRequest::Close);
    assert!(
        live_project::request(
            &mut old_client,
            Operation::PreparationStatus {
                project_id: initial.document.project_id().clone(),
                target,
            }
        )
        .is_err()
    );
    let reopened = ProjectStore::open(&path, AccessMode::ReadWrite).unwrap();
    assert!(reopened.original_records(None, 100).unwrap().is_empty());
    drop(reopened);
    let new_path = scratch.path().join("new.deadpan");
    let current = opened(&harness, &new_path);
    let mut new_client = client(&new_path);
    assert_eq!(
        live_project::request(
            &mut new_client,
            Operation::Prepare {
                project_id: current.document.project_id().clone(),
                target: PreparationTarget::fresh(),
                command: Box::new(PreparationCommand::Checkpoint {}),
            }
        )
        .unwrap_err()
        .code,
        "HostPreparationBusy"
    );
    harness
        .replies
        .send(worker::Reply {
            id,
            result: prepared,
        })
        .unwrap();
    // The old reply cannot register anything into the new project.
    until(|| {
        let reply = live_project::request(
            &mut new_client,
            Operation::Prepare {
                project_id: current.document.project_id().clone(),
                target: PreparationTarget::fresh(),
                command: Box::new(PreparationCommand::Checkpoint {}),
            },
        );
        match reply {
            Ok(Reply::Preparation { .. }) => true,
            Err(error) if error.code == "HostPreparationBusy" => false,
            _ => panic!("unexpected new-owner admission"),
        }
    });
    let new_job = harness.job();
    assert!(
        ProjectStore::open(&new_path, AccessMode::ReadOnly)
            .unwrap()
            .original_records(None, 100)
            .unwrap()
            .is_empty()
    );
    drop(new_job);
    shutdown(&harness);
}

#[test]
fn completion_waits_for_native_receipt_and_retains_operational_success_on_refresh_failure() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("completion.deadpan");
    let harness = Harness::new();
    let initial = opened(&harness, &path);
    let mut client = client(&path);
    let target = start(
        &mut client,
        &initial,
        PreparationCommand::Retain {
            path: fixture("cfr-bframes.mp4"),
            ownership: PreparationOwnership::Managed {},
        },
    );
    let job = harness.job();
    // Publish a genuine native edit without consuming its cursor/selection receipt.
    harness
        .service
        .submit(ProjectRequest::Edit {
            expected_session: initial.session,
            expected_revision: initial.document.revision_id().clone(),
            cursor: deadpan_core::ProjectFrame(0),
            scope: SequenceScope::default(),
            edit: ProjectEdit::HoldDuration {
                node: node("a"),
                duration: FrameDuration::new(19).unwrap(),
            },
        })
        .unwrap();
    until(|| {
        harness
            .service
            .shared
            .update
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|update| update.committed.is_some())
    });
    finish(&harness, job);
    until(|| {
        matches!(
            status(&mut client, &initial, &target).state,
            PreparationState::AwaitingCommit {}
        )
    });
    assert!(
        ProjectStore::open(&path, AccessMode::ReadOnly)
            .unwrap()
            .original_records(None, 100)
            .unwrap()
            .is_empty()
    );
    harness
        .service
        .shared
        .host_refresh_failure
        .store(true, Ordering::Release);
    let native = harness.service.take_update().unwrap();
    assert_eq!(native.committed.unwrap().selected_node, Some(node("a")));
    let PreparationState::Completed {
        receipt,
        committed_revision,
        inventory_changed,
        refresh_error,
        ..
    } = terminal(&mut client, &initial, &target)
    else {
        panic!("operational commit receipt survives refresh error")
    };
    assert!(matches!(receipt, PreparationReceipt::Retained { .. }));
    assert!(committed_revision.is_none() && inventory_changed && refresh_error.is_some());
    assert_eq!(
        ProjectStore::open(&path, AccessMode::ReadOnly)
            .unwrap()
            .original_records(None, 100)
            .unwrap()
            .len(),
        1
    );
    shutdown(&harness);
}
