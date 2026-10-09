//! DP-21 operations the app previously refused through its endpoint:
//! transcript corrections, Original copies, storage cleanup, clock
//! confirmation and backup restore. Each runs on the app's own writer and
//! publishes the native state it changes.

use super::*;
use crate::project::registers::Value as RegisterRuntime;
use deadpan_analysis::{AnalysedAudio, Transcript, Word};

fn original_project(scratch: &Path) -> (ProjectService, Arc<Workspace>) {
    let service = ProjectService::start(
        Arc::new(|| {}),
        Some(ProjectLibrary::from_documents(scratch.join("Documents")).unwrap()),
    )
    .unwrap();
    service
        .submit(ProjectRequest::CreateFromSource {
            ownership: OriginalOwnership::Managed,
            path: fixture("cfr-bframes.mp4"),
        })
        .unwrap();
    let initialized = wait(&service, |update| {
        update.import.as_ref().is_some_and(|status| {
            matches!(status.stage, ImportStage::Complete | ImportStage::Failed)
        })
    });
    assert!(initialized.error.is_none(), "{:?}", initialized.error);
    (service, initialized.workspace.unwrap())
}

fn texts(workspace: &Workspace) -> Vec<String> {
    workspace
        .transcript
        .as_ref()
        .unwrap()
        .transcript
        .words()
        .iter()
        .map(|word| word.text.clone())
        .collect()
}

fn completed(reply: Reply) -> (Value, Option<deadpan_cli::macros::RegisterReceipt>) {
    match reply {
        Reply::Completed {
            output,
            committed_revision: None,
            committed_registers,
            refresh_error: None,
        } => (output, committed_registers),
        other => panic!("expected an operational receipt, got {other:?}"),
    }
}

#[test]
fn live_corrections_and_original_copies_publish_the_native_state() {
    let scratch = tempfile::tempdir().unwrap();
    let (service, before) = original_project(scratch.path());
    let Some(deadpan_store::single_source::SingleSourceState::Ready { asset, .. }) =
        &before.single_source
    else {
        panic!("original not initialized")
    };
    let receipt = before.sources[asset].receipt.clone();
    let word = |text: &str, start_cs, end_cs| Word {
        text: text.into(),
        start_cs,
        end_cs,
        probability: 0.9,
        segment: 0,
    };
    let _ = service.take_update();
    service
        .submit(ProjectRequest::SaveTranscript {
            expected_session: before.session,
            attempt: 1,
            key: deadpan_store::TranscriptKey {
                content: receipt.original().content().to_string(),
                audio_stream: receipt.snapshot().audio().unwrap().stream().stream_index,
                model_sha256: "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002"
                    .into(),
                language: "en".into(),
                engine: "whisper.cpp 1.8.3".into(),
            },
            transcript: Arc::new(
                Transcript::new(
                    AnalysedAudio {
                        origin: 0,
                        sample_rate: 48_000,
                        duration_cs: 300,
                    },
                    vec![
                        word("to", 10, 40),
                        word("day", 45, 90),
                        word("now", 120, 160),
                    ],
                )
                .unwrap(),
            ),
        })
        .unwrap();
    let saved = wait(&service, |update| {
        update
            .transcript_save
            .as_ref()
            .is_some_and(|save| save.attempt == 1)
    });
    let workspace = saved.workspace.unwrap();
    let project = workspace.document.project_id().clone();
    let mut client = client(&workspace.path);

    // `:correct` J through the endpoint: saved on the app's writer and
    // published as its corrected transcript.
    let join: deadpan_cli::corrections::Request = serde_json::from_value(serde_json::json!({
        "protocol": 1, "expected_version": 0,
        "change": {"type":"join_words","word":0,"expected_text":"to"}
    }))
    .unwrap();
    let correct = |request: &deadpan_cli::corrections::Request| Operation::Execute {
        project_id: project.clone(),
        command: Box::new(ShortOperation::Corrections {
            request: Box::new(request.clone()),
        }),
    };
    let mut dry = join.clone();
    dry.dry_run = true;
    let (preview, _) = completed(live_project::request(&mut client, correct(&dry)).unwrap());
    assert_eq!(preview["committed"], false, "{preview}");
    let (output, _) = completed(live_project::request(&mut client, correct(&join)).unwrap());
    assert_eq!(output["committed"], true, "{output}");
    assert_eq!(output["version"], 1);
    let corrected = wait(&service, |update| {
        update
            .workspace
            .as_ref()
            .is_some_and(|workspace| texts(workspace) == ["today", "now"])
    });
    let corrected_workspace = corrected.workspace.unwrap();
    assert_eq!(
        corrected_workspace.corrections.as_ref().unwrap().version(),
        1
    );
    assert!(corrected.message.unwrap().contains("command line"));
    assert_eq!(
        corrected_workspace.document.revision_id(),
        workspace.document.revision_id(),
        "corrections are not edits"
    );
    // The version it named is no longer current.
    let stale = live_project::request(&mut client, correct(&join)).unwrap_err();
    assert_eq!(stale.code, "AnalysisCorrectionsConflict");

    // `y` in Original through the endpoint: the native runtime bank gains
    // the same copy in the named and unnamed registers.
    let yank = |bank: u64, dry_run: bool| Operation::Execute {
        project_id: project.clone(),
        command: Box::new(ShortOperation::Macro {
            request: Box::new(
                serde_json::from_value(serde_json::json!({
                    "protocol": 1, "project_id": project,
                    "expected_revision": workspace.document.revision_id(),
                    "expected_bank_version": bank, "dry_run": dry_run,
                    "operation": {"type":"yank_original","register":"q",
                        "ordinals":{"start":2,"end":6}}
                }))
                .unwrap(),
            ),
        }),
    };
    let (preview, receipt) = completed(live_project::request(&mut client, yank(0, true)).unwrap());
    assert!(receipt.is_none(), "{preview}");
    let (output, receipt) = completed(live_project::request(&mut client, yank(0, false)).unwrap());
    assert_eq!(receipt.unwrap().bank_version, 1, "{output}");
    let copied = wait(&service, |update| {
        update
            .registers
            .as_ref()
            .is_some_and(|bank| bank.version == 1)
    });
    let bank = copied.registers.unwrap();
    for name in ['q', '"'] {
        assert!(
            matches!(&bank.entries[&name], RegisterRuntime::Original { asset: copied, ordinals, .. }
                if copied == asset && *ordinals == (2..6)),
            "{name}"
        );
    }
    let stale = live_project::request(&mut client, yank(0, false)).unwrap_err();
    assert_eq!(stale.code, "RegisterInvalid");
    shutdown_service(&service);
}

fn shutdown_service(service: &ProjectService) {
    service.shutdown();
    until(|| service.is_shutdown_complete());
}

#[test]
fn live_cleanup_and_restore_run_on_the_owner_and_restore_replies_before_rebinding() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("live-restore.deadpan");
    let harness = quiet_harness();
    let initial = opened(&harness, &path);
    let project = initial.document.project_id().clone();
    // A backup taken beside the open app, as `project backup` does.
    let backup = deadpan_store::backups::create_backup(
        &initial.path,
        deadpan_store::backups::BackupReason::Manual,
        &deadpan_store::backups::BackupPolicy::default(),
        deadpan_store::backups::BackupLimits::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let mut client = client(&path);
    committed(
        live_project::request(&mut client, change(&initial, "live-later", 37)).unwrap(),
        "live-later",
    );
    wait(&harness.service, |update| {
        update
            .workspace
            .as_ref()
            .is_some_and(|workspace| workspace.document.revision_id().as_str() == "live-later")
    });

    for operation in [
        ShortOperation::CleanStorage {
            grace_seconds: 24 * 60 * 60,
            expire_variants: true,
            dry_run: false,
            plan: None,
        },
        ShortOperation::ConfirmVariantClock { dry_run: false },
    ] {
        let (output, _) = completed(
            live_project::request(
                &mut client,
                Operation::Execute {
                    project_id: project.clone(),
                    command: Box::new(operation),
                },
            )
            .unwrap(),
        );
        assert_eq!(output["variant_expiry"]["status"], "applied", "{output}");
        assert_eq!(output["variant_expiry"]["dry_run"], false, "{output}");
    }
    let cleaned = wait(&harness.service, |update| {
        update
            .message
            .as_deref()
            .is_some_and(|message| message.contains("clock"))
    });
    assert!(cleaned.error.is_none(), "{:?}", cleaned.error);

    let restore = |expected: &str| Operation::Execute {
        project_id: project.clone(),
        command: Box::new(ShortOperation::RestoreBackup {
            id: backup.backup.id.clone(),
            expected_revision: Some(RevisionId::new(expected).unwrap()),
        }),
    };
    let stale = live_project::request(&mut client, restore("not-the-head")).unwrap_err();
    assert_eq!(stale.code, "RevisionConflict");
    // The reply reaches the client although the restore replaced the owner
    // its request authenticated against.
    let (output, _) = completed(live_project::request(&mut client, restore("live-later")).unwrap());
    assert_eq!(
        output["restored"]["restored"]["revision_id"],
        initial.document.revision_id().as_str(),
        "{output}"
    );
    let restored = wait(&harness.service, |update| {
        update.workspace.as_ref().is_some_and(|workspace| {
            workspace.document.revision_id() == initial.document.revision_id()
                && workspace.session > initial.session
        })
    });
    assert!(restored.message.unwrap().contains("command line"));
    // The replaced owner serves nothing more; the new one is discoverable.
    assert!(live_project::inspect(&mut client).is_err());
    let mut successor = super::client(&path);
    let (context, _) = live_project::inspect(&mut successor).unwrap();
    assert_eq!(&context.revision_id, initial.document.revision_id());
    shutdown(&harness);
}

/// A harness whose automatic retention check never starts on its own, so a
/// remote cleanup is not refused because that check happens to be running.
fn quiet_harness() -> Harness {
    let harness = Harness::new();
    harness
        .service
        .shared
        .automatic_retention
        .store(false, Ordering::Release);
    harness
}

fn backup_now(path: &Path) -> deadpan_store::backups::BackupOutcome {
    deadpan_store::backups::create_backup(
        path,
        deadpan_store::backups::BackupReason::Manual,
        &deadpan_store::backups::BackupPolicy::default(),
        deadpan_store::backups::BackupLimits::default(),
        &AtomicBool::new(false),
    )
    .unwrap()
}

fn restore_request(project: &ProjectId, id: &str) -> Operation {
    Operation::Execute {
        project_id: project.clone(),
        command: Box::new(ShortOperation::RestoreBackup {
            id: id.to_owned(),
            expected_revision: None,
        }),
    }
}

fn clean_request(project: &ProjectId, dry_run: bool) -> Operation {
    Operation::Execute {
        project_id: project.clone(),
        command: Box::new(ShortOperation::CleanStorage {
            grace_seconds: 24 * 60 * 60,
            expire_variants: true,
            dry_run,
            plan: None,
        }),
    }
}

#[test]
fn remote_cleanup_runs_off_the_service_thread_while_edits_commit() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("live-clean.deadpan");
    let harness = quiet_harness();
    let initial = opened(&harness, &path);
    let project = initial.document.project_id().clone();
    let shared = harness.service.shared.clone();
    shared.remote_storage_paused.store(true, Ordering::Release);
    let pending = {
        let path = path.clone();
        let project = project.clone();
        std::thread::spawn(move || {
            live_project::request(&mut client(&path), clean_request(&project, false))
        })
    };
    until(|| shared.remote_storage_waiting.load(Ordering::Acquire));
    // The service keeps serving: an edit commits while the scan is held.
    let mut editor = client(&path);
    committed(
        live_project::request(&mut editor, change(&initial, "during-clean", 41)).unwrap(),
        "during-clean",
    );
    // One storage job at a time, dry runs included.
    let second = live_project::request(&mut editor, clean_request(&project, true)).unwrap_err();
    assert_eq!(second.code, "StorageBusy", "{second}");
    shared.remote_storage_paused.store(false, Ordering::Release);
    let (output, _) = completed(pending.join().unwrap().unwrap());
    assert_eq!(output["variant_expiry"]["status"], "applied", "{output}");
    assert_eq!(output["cleanup"]["dry_run"], false, "{output}");
    // A dry run also runs off-thread and writes nothing.
    let (preview, _) =
        completed(live_project::request(&mut editor, clean_request(&project, true)).unwrap());
    assert_eq!(
        preview["variant_expiry"]["status"], "previewed",
        "{preview}"
    );
    assert!(preview["plan_hash"].is_string(), "{preview}");
    assert_eq!(
        preview["plan"]["revision_id"].as_str(),
        Some("during-clean"),
        "{preview}"
    );
    // Storage R: exactly that plan, on the writer, files only.
    let plan: deadpan_cli::storage::CleanupPlan =
        serde_json::from_value(preview["plan"].clone()).unwrap();
    let (removed, _) = completed(
        live_project::request(
            &mut editor,
            Operation::Execute {
                project_id: project.clone(),
                command: Box::new(ShortOperation::CleanStorage {
                    grace_seconds: plan.grace_seconds,
                    expire_variants: false,
                    dry_run: false,
                    plan: Some(Box::new(plan)),
                }),
            },
        )
        .unwrap(),
    );
    assert_eq!(
        removed["variant_expiry"]["status"], "not_requested",
        "{removed}"
    );
    assert_eq!(removed["cleanup"]["dry_run"], false);
    shutdown(&harness);
}

#[test]
fn remote_cleanup_is_refused_while_the_automatic_check_runs() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("live-retention.deadpan");
    let harness = Harness::new();
    let shared = harness.service.shared.clone();
    shared.retention_paused.store(true, Ordering::Release);
    let initial = opened(&harness, &path);
    until(|| shared.retention_waiting.load(Ordering::Acquire));
    let refused = live_project::request(
        &mut client(&path),
        clean_request(initial.document.project_id(), false),
    )
    .unwrap_err();
    assert_eq!(refused.code, "StorageBusy");
    assert!(refused.message.contains("automatic"), "{refused}");
    shared.retention_paused.store(false, Ordering::Release);
    shutdown(&harness);
}

#[test]
fn restores_in_one_batch_or_while_draining_answer_every_client() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("live-batch.deadpan");
    let harness = quiet_harness();
    let initial = opened(&harness, &path);
    let project = initial.document.project_id().clone();
    let backup = backup_now(&initial.path);
    let shared = harness.service.shared.clone();

    // Two restores read in one batch: the first replaces the owner the
    // second authenticated against, so the second never runs.
    shared.host_poll_paused.store(true, Ordering::Release);
    let clients: Vec<_> = (0..2)
        .map(|_| {
            let (path, project, id) = (path.clone(), project.clone(), backup.backup.id.clone());
            std::thread::spawn(move || {
                live_project::request(&mut client(&path), restore_request(&project, &id))
            })
        })
        .collect();
    std::thread::sleep(Duration::from_millis(300));
    shared.host_poll_paused.store(false, Ordering::Release);
    let replies: Vec<_> = clients.into_iter().map(|c| c.join().unwrap()).collect();
    assert_eq!(
        replies.iter().filter(|reply| reply.is_ok()).count(),
        1,
        "{replies:?}"
    );
    assert!(
        replies
            .iter()
            .any(|reply| matches!(reply, Err(error) if error.code == "HostOwnerChanged")),
        "{replies:?}"
    );

    let session_after = |session: u64| {
        wait(&harness.service, |update| {
            update
                .workspace
                .as_ref()
                .is_some_and(|workspace| workspace.session > session)
        })
        .workspace
        .unwrap()
        .session
    };
    let restore_in_background = || {
        let (path, project, id) = (path.clone(), project.clone(), backup.backup.id.clone());
        std::thread::spawn(move || {
            live_project::request(&mut client(&path), restore_request(&project, &id))
        })
    };
    let batch = session_after(initial.session);

    // Two owners retired in turn keep both replies until they are written.
    shared.retired_drain_paused.store(true, Ordering::Release);
    let first = restore_in_background();
    let after_first = session_after(batch);
    let second = restore_in_background();
    let after_second = session_after(after_first);
    shared.retired_drain_paused.store(false, Ordering::Release);
    for pending in [first, second] {
        completed(pending.join().unwrap().unwrap());
    }

    // Shutdown writes a produced reply that has not drained yet.
    shared.retired_drain_paused.store(true, Ordering::Release);
    let last = restore_in_background();
    session_after(after_second);
    shutdown(&harness);
    completed(last.join().unwrap().unwrap());
}

#[test]
fn restore_refuses_open_drafts_and_reports_a_changed_database_it_could_not_show() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("live-restore-failure.deadpan");
    let harness = quiet_harness();
    let initial = opened(&harness, &path);
    let project = initial.document.project_id().clone();
    let backup = backup_now(&initial.path);
    let shared = harness.service.shared.clone();
    let mut client = client(&path);

    shared.preview_active.store(true, Ordering::Release);
    let refused = live_project::request(&mut client, restore_request(&project, &backup.backup.id))
        .unwrap_err();
    assert_eq!(refused.code, "RestoreDraftOpen", "{refused}");
    shared.preview_active.store(false, Ordering::Release);

    shared.restore_show_failure.store(true, Ordering::Release);
    let failed = live_project::request(&mut client, restore_request(&project, &backup.backup.id))
        .unwrap_err();
    assert_eq!(failed.code, "BackupRestoredNotShown", "{failed}");
    assert_eq!(
        failed.committed_revision.as_ref(),
        Some(initial.document.revision_id()),
        "the database now holds the backup's head"
    );
    let safety = deadpan_store::backups::list_backups(&initial.path)
        .unwrap()
        .into_iter()
        .find(|info| info.reason == deadpan_store::backups::BackupReason::BeforeRestore)
        .unwrap();
    assert!(failed.message.contains(&safety.id), "{failed}");
    // The app closed the project rather than show a replaced database.
    wait(&harness.service, |update| update.workspace.is_none());
    shutdown(&harness);
}
