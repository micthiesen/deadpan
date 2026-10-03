#![cfg(any(target_os = "macos", target_os = "linux"))]

//! Actual CLI routing with an authenticated test owner. Media work uses the
//! production shared admission/prepare/commit boundary; native responsiveness
//! and focus behavior belong to the app's separate service qualification.

use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_cli::host::Endpoint;
use deadpan_cli::live_project::preparation::{self, PreparationState, PreparationStatus};
use deadpan_cli::live_project::{Operation, Reply, Request, execute_short};
use deadpan_cli::render::RenderContext;
use deadpan_core::{NodeId, ProjectDocument, ProjectId, RevisionId};
use deadpan_store::{AccessMode, ProjectStore};
use serde_json::{Value, json};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn run(
    store: &mut ProjectStore,
    endpoint: &mut Endpoint,
    args: &[&str],
    remove_after_parse: Option<&Path>,
) -> Result<(Output, Vec<Operation>)> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_deadpan-cli"));
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = deadpan_native_process::spawn(&mut command)?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut operations = Vec::new();
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
                Operation::Prepare {
                    project_id,
                    target,
                    command,
                } => {
                    assert_eq!(&project_id, store.snapshot()?.project_id());
                    if let Some(path) = remove_after_parse {
                        std::fs::remove_file(path)?;
                    }
                    let cancelled = AtomicBool::new(false);
                    let result = preparation::admit(store, &command)
                        .and_then(|work| preparation::prepare(work, &cancelled))
                        .and_then(|prepared| {
                            preparation::commit(store, &command, prepared, &cancelled)
                        });
                    let state = match result {
                        Ok(result) => PreparationState::Completed {
                            output: result.output,
                            receipt: result.receipt,
                            committed_revision: result.committed_revision,
                            inventory_changed: result.inventory_changed,
                            completion_error: result.completion_error,
                            refresh_error: None,
                        },
                        Err(error) => PreparationState::Failed { error },
                    };
                    Reply::Preparation {
                        status: Box::new(PreparationStatus { target, state }),
                    }
                }
                Operation::Execute {
                    project_id,
                    command,
                } => {
                    let deadpan_cli::macros::Execution {
                        output,
                        committed_revision,
                        committed_registers,
                    } = execute_short(store, &project_id, &command)?;
                    Reply::Completed {
                        output,
                        committed_revision,
                        committed_registers,
                        refresh_error: None,
                    }
                }
                Operation::ReleasePreparationStatus { .. } => Reply::Released,
                operation => panic!("unexpected operation {operation:?}"),
            };
            endpoint.respond(incoming.ticket, serde_json::to_value(reply)?)?;
        }
        std::thread::yield_now();
    }
    Ok((child.wait_with_output()?, operations))
}
fn success(output: Output) -> Result<Value> {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}

#[test]
fn actual_cli_routes_preparations_and_current_migration_coexists_with_owner() -> Result {
    let root = tempfile::tempdir()?;
    let package = root.path().join("open.deadpan");
    let project = package.to_str().unwrap();
    let document = ProjectDocument::new_automatic(
        ProjectId::new("live-preparations")?,
        RevisionId::new("initial")?,
        NodeId::new("root")?,
    )?;
    let mut store = ProjectStore::create(&package, &document)?;
    let mut endpoint = Endpoint::bind(&mut store)?;
    let source = root.path().join("source.mp4");
    std::fs::write(
        &source,
        include_bytes!("../../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4"),
    )?;
    let (output, operations) = run(
        &mut store,
        &mut endpoint,
        &[
            "project",
            "retain-original",
            project,
            source.to_str().unwrap(),
            "--linked",
        ],
        None,
    )?;
    let retained = success(output)?;
    assert_eq!(retained["retained_original"]["method"], "linked");
    assert_eq!(
        operations
            .iter()
            .filter(|op| matches!(op, Operation::Prepare { .. }))
            .count(),
        1
    );
    assert!(matches!(
        operations.last(),
        Some(Operation::ReleasePreparationStatus { .. })
    ));
    assert_eq!(store.snapshot()?, document);
    let content = retained["retained_original"]["record"]["object"]["content"].clone();
    let digest = content["digest"].as_str().unwrap();
    let moved = root.path().join("moved.mp4");
    std::fs::rename(&source, &moved)?;
    let (output, _) = run(
        &mut store,
        &mut endpoint,
        &[
            "project",
            "relink-original",
            project,
            digest,
            moved.to_str().unwrap(),
            "--expected-version",
            "1",
        ],
        None,
    )?;
    assert_eq!(success(output)?["relinked_original"]["version"], 2);
    let request_path = root.path().join("registration.json");
    let request = json!({"protocol":1,"registration":{
        "expected_revision":"initial","new_revision":"registered-through-owner","original":content,
        "new_asset_id":"caller-asset","label":"Caller label","insertion":{"parent":"root","index":0,"node":"caller-node","label":"Caller insertion"}},
        "streams":{"type":"video_and_audio","audio_stream":1}});
    std::fs::write(&request_path, serde_json::to_vec(&request)?)?;
    let (output, _) = run(
        &mut store,
        &mut endpoint,
        &[
            "project",
            "register-source",
            project,
            "--request-json",
            request_path.to_str().unwrap(),
        ],
        Some(&request_path),
    )?;
    let registered = success(output)?;
    assert_eq!(registered["committed"], true);
    assert_eq!(registered["outcome"]["asset_id"], "caller-asset");
    assert_eq!(
        store.snapshot()?.revision_id().as_str(),
        "registered-through-owner"
    );
    assert!(!request_path.exists());
    let before = store.snapshot()?;
    let (output, _) = run(
        &mut store,
        &mut endpoint,
        &["project", "checkpoint", project],
        None,
    )?;
    let checkpoint = success(output)?;
    assert!(Path::new(checkpoint["database_checkpoint"].as_str().unwrap()).is_file());
    let snapshots_before_migration = std::fs::read_dir(package.join("Snapshots"))?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<std::io::Result<std::collections::BTreeSet<_>>>()?;
    let (output, operations) = run(
        &mut store,
        &mut endpoint,
        &["project", "migrate", project],
        None,
    )?;
    let schema = deadpan_store::DATABASE_SCHEMA_VERSION;
    assert_eq!(
        success(output)?,
        json!({"protocol":1,"migration":{"from_schema":schema,"to_schema":schema,"backup":null}})
    );
    // Current-schema migration validates through a read-only store before it
    // needs a writer lock, so the CLI has no conflict to route to the owner.
    assert!(operations.is_empty());
    let snapshots_after_migration = std::fs::read_dir(package.join("Snapshots"))?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<std::io::Result<std::collections::BTreeSet<_>>>()?;
    assert_eq!(snapshots_after_migration, snapshots_before_migration);
    assert_eq!(store.snapshot()?, before);
    store.check_writer_owner(endpoint.owner_handle())?;
    assert!(matches!(
        ProjectStore::open(&package, AccessMode::ReadWrite),
        Err(deadpan_store::StoreError::AlreadyOpen)
    ));
    Ok(())
}

#[test]
fn actual_cli_source_dry_run_coexists_without_contacting_owner() -> Result {
    let root = tempfile::tempdir()?;
    let package = root.path().join("dry-run.deadpan");
    let document = ProjectDocument::new_automatic(
        ProjectId::new("live-preparation-preview")?,
        RevisionId::new("initial")?,
        NodeId::new("root")?,
    )?;
    let mut store = ProjectStore::create(&package, &document)?;
    let source = root.path().join("source.mp4");
    std::fs::write(
        &source,
        include_bytes!("../../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4"),
    )?;
    let retained = store.retain_original(
        &source,
        deadpan_store::original_media::OriginalOwnership::Managed,
        deadpan_store::original_media::OriginalMediaLimits::default(),
        &AtomicBool::new(false),
    )?;
    let mut endpoint = Endpoint::bind(&mut store)?;
    let request_path = root.path().join("preview.json");
    std::fs::write(
        &request_path,
        serde_json::to_vec(&json!({"protocol":1,"registration":{
        "expected_revision":"initial","new_revision":"preview-only","original":retained.record.object().content(),
        "new_asset_id":"preview-asset","label":"Preview","insertion":null},"streams":{"type":"video_only"}}))?,
    )?;
    let (output, operations) = run(
        &mut store,
        &mut endpoint,
        &[
            "project",
            "register-source",
            package.to_str().unwrap(),
            "--request-json",
            request_path.to_str().unwrap(),
            "--dry-run",
        ],
        None,
    )?;
    assert_eq!(success(output)?["committed"], false);
    assert!(operations.is_empty());
    assert_eq!(store.snapshot()?, document);
    Ok(())
}
