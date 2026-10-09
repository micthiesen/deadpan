#![cfg(any(target_os = "macos", target_os = "linux"))]
//! Named takes through the shipped CLI and the authenticated live writer.

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::Command as ProcessCommand;

use deadpan_cli::host::Endpoint;
use deadpan_cli::live_project::{ShortOperation, execute_short};
use deadpan_core::{
    BeatNode, ColorPolicy, Command, CommandRequest, FrameDuration, FrameRate, HoldAudio,
    HoldRecipe, HoldVideo, NodeId, PresentationBasis, ProjectDocument, ProjectId, RevisionId,
    Subtree,
};
use deadpan_store::takes::{Action, Request, TakeCatalog, TakeId, TakeName};
use deadpan_store::{AccessMode, ProjectStore};
use serde_json::{Value, json};

#[path = "support/live_owner.rs"]
mod live_owner;

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn cli(arguments: &[&str], succeeds: bool) -> Value {
    let output = ProcessCommand::new(env!("CARGO_BIN_EXE_deadpan-cli"))
        .args(arguments)
        .output()
        .unwrap();
    assert_eq!(
        output.status.success(),
        succeeds,
        "{arguments:?}: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(if succeeds {
        &output.stdout
    } else {
        &output.stderr
    })
    .unwrap()
}

fn text(path: &Path) -> &str {
    path.to_str().unwrap()
}

fn create(package: &Path) -> Result<ProjectStore> {
    let document = ProjectDocument::new(
        ProjectId::new("takes-cli")?,
        RevisionId::new("initial")?,
        PresentationBasis {
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::new(30_000, 1_001)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?;
    let mut store = ProjectStore::create(package, &document)?;
    let node = NodeId::new("pause")?;
    store.commit(&CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new("first-cut")?,
        command: Command::Insert {
            parent: document.root().clone(),
            index: 0,
            subtree: Subtree {
                root: node.clone(),
                nodes: BTreeMap::from([(
                    node,
                    BeatNode::hold(
                        "Pause",
                        HoldRecipe {
                            picture_context: None,
                            duration: FrameDuration::new(45)?,
                            video: HoldVideo::Background,
                            audio: HoldAudio::Silence,
                        },
                    ),
                )]),
                overrides: Default::default(),
                gap_overrides: Default::default(),
            },
        },
    })?;
    Ok(store)
}

fn change(store: &mut ProjectStore) -> Result {
    let before = store.snapshot()?;
    store.commit(&CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: RevisionId::new("second-cut")?,
        command: Command::SetHoldDuration {
            node: NodeId::new("pause")?,
            duration: FrameDuration::new(90)?,
        },
    })?;
    Ok(())
}

fn request(catalog: &TakeCatalog, action: Action) -> Request {
    Request {
        project_id: catalog.project_id.clone(),
        expected_revision: catalog.revision_id.clone(),
        expected_version: catalog.version,
        action,
    }
}

fn write_request(path: &Path, request: &Request) -> Result {
    fs::write(
        path,
        serde_json::to_vec(&json!({"protocol":1,"request":request}))?,
    )?;
    Ok(())
}

fn catalog(package: &Path) -> Result<TakeCatalog> {
    Ok(serde_json::from_value(
        cli(&["project", "takes", text(package)], true)["takes"].clone(),
    )?)
}

#[test]
fn cli_takes_survive_reopen_and_restore_as_one_undoable_edit() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("session.deadpan");
    let file = scratch.path().join("request.json");
    let first = create(&package)?.snapshot()?;
    let before = catalog(&package)?;
    let id = TakeId::new("take-one")?;
    write_request(
        &file,
        &request(
            &before,
            Action::Create {
                id: id.clone(),
                name: TakeName::new("第一稿")?,
            },
        ),
    )?;
    let database = fs::read(package.join("project.sqlite"))?;
    let preview = cli(
        &[
            "project",
            "take",
            text(&package),
            "--json",
            text(&file),
            "--dry-run",
        ],
        true,
    );
    assert_eq!(preview["committed"], false);
    assert_eq!(fs::read(package.join("project.sqlite"))?, database);
    assert!(catalog(&package)?.entries.is_empty());
    let saved = cli(
        &["project", "take", text(&package), "--json", text(&file)],
        true,
    );
    assert_eq!(saved["outcome"]["catalog"], preview["outcome"]["catalog"]);
    assert!(saved["outcome"]["commit"].is_null());
    assert_eq!(catalog(&package)?.revision_id, *first.revision_id());
    // A previously observed empty catalog cannot overwrite the saved label.
    cli(
        &["project", "take", text(&package), "--json", text(&file)],
        false,
    );
    {
        let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
        change(&mut store)?;
    }
    let current = catalog(&package)?;
    write_request(
        &file,
        &request(
            &current,
            Action::Restore {
                id: id.clone(),
                expected_snapshot: first.revision_id().clone(),
                new_revision: RevisionId::new("restored-first")?,
            },
        ),
    )?;
    let preview = cli(
        &[
            "project",
            "take",
            text(&package),
            "--json",
            text(&file),
            "--dry-run",
        ],
        true,
    );
    assert_eq!(catalog(&package)?.revision_id, current.revision_id);
    let restored = cli(
        &["project", "take", text(&package), "--json", text(&file)],
        true,
    );
    assert_eq!(restored["outcome"], preview["outcome"]);
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    assert_eq!(store.snapshot()?.nodes(), first.nodes());
    assert_eq!(store.take_catalog()?.version, current.version);
    drop(store);
    let undone = cli(
        &[
            "project",
            "undo",
            text(&package),
            "--expected",
            "restored-first",
        ],
        true,
    );
    let undo_revision = undone["outcome"]["revision_id"].as_str().unwrap();
    assert_eq!(
        ProjectStore::open(&package, AccessMode::ReadOnly)?
            .snapshot()?
            .duration()?
            .frames(),
        90
    );
    // Deleting a label leaves the restore's immutable proof and redo intact.
    write_request(
        &file,
        &request(
            &catalog(&package)?,
            Action::Delete {
                id,
                expected_snapshot: first.revision_id().clone(),
            },
        ),
    )?;
    cli(
        &["project", "take", text(&package), "--json", text(&file)],
        true,
    );
    cli(
        &[
            "project",
            "redo",
            text(&package),
            "--expected",
            undo_revision,
        ],
        true,
    );
    cli(&["project", "validate", text(&package)], true);
    assert!(catalog(&package)?.entries.is_empty());
    assert_eq!(
        ProjectStore::open(&package, AccessMode::ReadOnly)?
            .snapshot()?
            .nodes(),
        first.nodes()
    );
    Ok(())
}

#[test]
fn live_takes_metadata_is_versioned_and_restore_keeps_its_commit_receipt() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("live.deadpan");
    let file = scratch.path().join("request.json");
    let mut store = create(&package)?;
    let mut endpoint = Endpoint::bind(&mut store)?;
    let initial = store.take_catalog()?;
    let id = TakeId::new("live-take")?;
    write_request(
        &file,
        &request(
            &initial,
            Action::Create {
                id: id.clone(),
                name: TakeName::new("First")?,
            },
        ),
    )?;
    let (saved, count) = live_owner::serve(&mut store, &mut endpoint, || {
        cli(
            &["project", "take", text(&package), "--json", text(&file)],
            true,
        )
    });
    assert_eq!(count, 1);
    assert!(saved["outcome"]["commit"].is_null());
    change(&mut store)?;
    let current = store.take_catalog()?;
    write_request(
        &file,
        &request(
            &current,
            Action::Update {
                id: id.clone(),
                expected_snapshot: initial.revision_id.clone(),
            },
        ),
    )?;
    let (_, count) = live_owner::serve(&mut store, &mut endpoint, || {
        cli(
            &["project", "take", text(&package), "--json", text(&file)],
            true,
        )
    });
    assert_eq!(count, 1);
    assert_eq!(
        store.take_catalog()?.entries[0].revision_id,
        current.revision_id
    );
    let current = store.take_catalog()?;
    let renamed = execute_short(
        &mut store,
        &current.project_id,
        &ShortOperation::Take {
            request: Box::new(request(
                &current,
                Action::Rename {
                    id: id.clone(),
                    expected_snapshot: current.revision_id.clone(),
                    name: TakeName::new("Long pause")?,
                },
            )),
            dry_run: false,
        },
    )?;
    assert!(renamed.committed_revision.is_none());
    let current = store.take_catalog()?;
    store.undo(&current.revision_id, RevisionId::new("short-again")?)?;
    let current = store.take_catalog()?;
    let restored = execute_short(
        &mut store,
        &current.project_id,
        &ShortOperation::Take {
            request: Box::new(request(
                &current,
                Action::Restore {
                    id,
                    expected_snapshot: RevisionId::new("second-cut")?,
                    new_revision: RevisionId::new("long-again")?,
                },
            )),
            dry_run: false,
        },
    )?;
    assert_eq!(
        restored.committed_revision,
        Some(RevisionId::new("long-again")?)
    );
    assert_eq!(store.snapshot()?.duration()?.frames(), 90);
    store.validate_full()?;
    Ok(())
}

#[test]
fn takes_wire_commands_reject_unknown_fields_and_wrong_project_before_writes() -> Result {
    for value in [
        json!({"command":"take_catalog","extra":true}),
        json!({"command":"take","request":{},"dry_run":false}),
    ] {
        assert!(serde_json::from_value::<ShortOperation>(value).is_err());
    }
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("session.deadpan");
    let file = scratch.path().join("request.json");
    let store = create(&package)?;
    let before = store.take_catalog()?;
    drop(store);
    let mut request = request(
        &before,
        Action::Create {
            id: TakeId::new("take")?,
            name: TakeName::new("First")?,
        },
    );
    request.project_id = ProjectId::new("other-project")?;
    write_request(&file, &request)?;
    cli(
        &["project", "take", text(&package), "--json", text(&file)],
        false,
    );
    request.project_id = before.project_id.clone();
    for invalid in [
        json!({"protocol":2,"request":request}),
        json!({"protocol":1,"request":request,"extra":true}),
    ] {
        fs::write(&file, serde_json::to_vec(&invalid)?)?;
        cli(
            &["project", "take", text(&package), "--json", text(&file)],
            false,
        );
    }
    assert!(catalog(&package)?.entries.is_empty());
    assert_eq!(catalog(&package)?.revision_id, before.revision_id);
    Ok(())
}
