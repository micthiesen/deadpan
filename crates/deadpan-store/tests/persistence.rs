use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::Path;
use std::process::Command as ProcessCommand;

use deadpan_core::{
    BeatNode, ColorPolicy, Command, CommandRequest, FrameDuration, FrameRate, HoldAudio,
    HoldRecipe, HoldVideo, NodeId, PresentationBasis, ProjectDocument, ProjectId, RevisionId,
    Subtree,
};
use deadpan_store::{AccessMode, ProjectStore, StoreError};
use rusqlite::Connection;

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn document() -> Result<ProjectDocument> {
    Ok(ProjectDocument::new(
        ProjectId::new("project")?,
        RevisionId::new("r0")?,
        PresentationBasis {
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::new(30_000, 1_001)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?)
}

fn insert(document: &ProjectDocument, revision: &str, node: &str) -> Result<CommandRequest> {
    let id = NodeId::new(node)?;
    Ok(CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new(revision)?,
        command: Command::Insert {
            parent: document.root().clone(),
            index: 0,
            subtree: Subtree {
                overrides: Default::default(),
                root: id.clone(),
                nodes: BTreeMap::from([(
                    id,
                    BeatNode::hold(
                        "Pause",
                        HoldRecipe {
                            duration: FrameDuration::new(12)?,
                            video: HoldVideo::Background,
                            audio: HoldAudio::Silence,
                        },
                    ),
                )]),
            },
        },
    })
}

fn revision_count(path: &Path) -> Result<i64> {
    Ok(Connection::open(path.join("project.sqlite"))?.query_row(
        "SELECT COUNT(*) FROM revisions",
        [],
        |row| row.get(0),
    )?)
}

#[test]
fn one_writer_readers_and_reopen_preserve_the_document() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("test.deadpan");
    let initial = document()?;
    let store = ProjectStore::create(&path, &initial)?;
    assert!(matches!(
        ProjectStore::open(&path, AccessMode::ReadWrite),
        Err(StoreError::AlreadyOpen)
    ));
    let mut reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert_eq!(reader.snapshot()?, initial);
    assert!(matches!(
        reader.commit(&insert(&initial, "r1", "hold")?),
        Err(StoreError::ReadOnly)
    ));
    assert!(ProjectStore::create(&path, &initial).is_err());
    drop(store);
    let reopened = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(reopened.snapshot()?, initial);
    assert!(path.join("Media/Generated").is_dir());
    Ok(())
}

#[test]
fn imported_initial_allocations_stay_reserved_after_their_plays_are_removed() -> Result {
    let scratch = tempfile::tempdir()?;
    let base = document()?;
    let inserted = deadpan_core::apply(&base, &insert(&base, "insert-initial", "hold")?)?
        .forward
        .apply(&base)?;
    let request = CommandRequest {
        project_id: inserted.project_id().clone(),
        expected_revision: inserted.revision_id().clone(),
        new_revision: RevisionId::new("initial-import")?,
        command: Command::WrapRepeat {
            node: NodeId::new("hold")?,
            id: NodeId::new("repeat")?,
            plays: 4,
            gap: None,
            anchor_policy: deadpan_core::WrapAnchorPolicy::First,
        },
    };
    let wrapped = deadpan_core::apply(&inserted, &request)?
        .forward
        .apply(&inserted)?;
    let mut wire = serde_json::to_value(wrapped)?;
    wire["nodes"]["repeat"]["kind"]["iterations"]["runs"] = serde_json::json!([
        {"allocation":"base-allocation","first":0,"count":3},
        {"allocation":"reserved-name","first":0,"count":1}
    ]);
    let initial = ProjectDocument::from_json(&wire.to_string())?;
    let path = scratch.path().join("imported.deadpan");
    let mut store = ProjectStore::create(&path, &initial)?;
    store.commit(&CommandRequest {
        project_id: initial.project_id().clone(),
        expected_revision: initial.revision_id().clone(),
        new_revision: RevisionId::new("shrink-import")?,
        command: Command::SetRepeat {
            node: NodeId::new("repeat")?,
            plays: 3,
            gap: None,
        },
    })?;
    let current = store.snapshot()?;
    let grow = CommandRequest {
        project_id: current.project_id().clone(),
        expected_revision: current.revision_id().clone(),
        new_revision: RevisionId::new("reserved-name")?,
        command: Command::SetRepeat {
            node: NodeId::new("repeat")?,
            plays: 4,
            gap: None,
        },
    };
    assert!(matches!(
        store.preview(&grow),
        Err(StoreError::RevisionReused(_))
    ));
    assert!(matches!(
        store.commit(&grow),
        Err(StoreError::RevisionReused(_))
    ));
    assert!(matches!(
        store.undo(current.revision_id(), RevisionId::new("reserved-name")?),
        Err(StoreError::RevisionReused(_))
    ));
    assert_eq!(store.snapshot()?, current);
    drop(store);
    ProjectStore::open(&path, AccessMode::ReadOnly)?.validate()?;
    // Even otherwise self-consistent forged history cannot reuse a namespace
    // from the imported initial document. This checks the replay invariant too.
    let connection = Connection::open(path.join("project.sqlite"))?;
    connection.execute_batch("PRAGMA foreign_keys=OFF; BEGIN;
        UPDATE revisions SET id='reserved-name',document=json_set(document,'$.revision_id','reserved-name') WHERE id='shrink-import';
        UPDATE history SET revision_id='reserved-name',request=json_set(request,'$.new_revision','reserved-name'),edit=json_set(edit,'$.forward.to_revision','reserved-name','$.inverse.from_revision','reserved-name') WHERE revision_id='shrink-import';
        UPDATE state SET head_revision='reserved-name'; COMMIT;")?;
    assert!(matches!(
        ProjectStore::open(&path, AccessMode::ReadOnly),
        Err(StoreError::History(_))
    ));
    Ok(())
}

#[test]
fn imported_audio_lineage_allocations_stay_reserved_after_owners_are_removed() -> Result {
    let scratch = tempfile::tempdir()?;
    let base = document()?;
    let inserted = deadpan_core::apply(&base, &insert(&base, "insert-initial", "hold")?)?
        .forward
        .apply(&base)?;
    let split = CommandRequest {
        project_id: inserted.project_id().clone(),
        expected_revision: inserted.revision_id().clone(),
        new_revision: RevisionId::new("reserved-lineage")?,
        command: Command::Split {
            node: NodeId::new("hold")?,
            at: FrameDuration::new(5)?,
            identities: deadpan_core::SplitIdentities {
                nodes: ["left", "right", "copied"]
                    .into_iter()
                    .map(NodeId::new)
                    .collect::<std::result::Result<_, _>>()?,
            },
        },
    };
    let split = deadpan_core::apply(&inserted, &split)?
        .forward
        .apply(&inserted)?;
    let mut wire = serde_json::to_value(&split)?;
    wire["revision_id"] = serde_json::json!("imported-snapshot");
    let initial = ProjectDocument::from_json(&wire.to_string())?;
    assert!(
        initial
            .audio_lineage()
            .values()
            .any(|id| id.allocation.as_str() == "reserved-lineage")
    );
    let path = scratch.path().join("imported-lineage.deadpan");
    let mut store = ProjectStore::create(&path, &initial)?;
    for (node, revision) in [("left", "delete-left"), ("right", "delete-right")] {
        let current = store.snapshot()?;
        store.commit(&CommandRequest {
            project_id: current.project_id().clone(),
            expected_revision: current.revision_id().clone(),
            new_revision: RevisionId::new(revision)?,
            command: Command::Delete {
                node: NodeId::new(node)?,
            },
        })?;
    }
    let current = store.snapshot()?;
    assert!(current.audio_lineage().is_empty());
    let forged = insert(&current, "reserved-lineage", "new-hold")?;
    assert!(matches!(
        store.preview(&forged),
        Err(StoreError::RevisionReused(_))
    ));
    assert!(matches!(
        store.commit(&forged),
        Err(StoreError::RevisionReused(_))
    ));
    assert!(matches!(
        store.undo(current.revision_id(), RevisionId::new("reserved-lineage")?),
        Err(StoreError::RevisionReused(_))
    ));
    drop(store);
    ProjectStore::open(&path, AccessMode::ReadOnly)?.validate()?;
    let connection = Connection::open(path.join("project.sqlite"))?;
    connection.execute_batch("PRAGMA foreign_keys=OFF; BEGIN;
        UPDATE revisions SET id='reserved-lineage',document=json_set(document,'$.revision_id','reserved-lineage') WHERE id='delete-right';
        UPDATE history SET revision_id='reserved-lineage',request=json_set(request,'$.new_revision','reserved-lineage'),edit=json_set(edit,'$.forward.to_revision','reserved-lineage','$.inverse.from_revision','reserved-lineage') WHERE revision_id='delete-right';
        UPDATE state SET head_revision='reserved-lineage'; COMMIT;")?;
    assert!(matches!(
        ProjectStore::open(&path, AccessMode::ReadOnly),
        Err(StoreError::History(_))
    ));
    Ok(())
}

#[test]
fn undo_and_redo_survive_restart_without_reusing_revision_identity() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("history.deadpan");
    let initial = document()?;
    let mut store = ProjectStore::create(&path, &initial)?;
    let request = insert(&initial, "r1", "hold")?;
    let preview = store.preview(&request)?;
    assert_eq!(preview.duration_delta, 12);
    assert_eq!(store.snapshot()?, initial);
    assert_eq!(revision_count(&path)?, 1);
    store.commit(&request)?;
    let edited = store.snapshot()?;
    drop(store);

    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    store.undo(&RevisionId::new("r1")?, RevisionId::new("r2")?)?;
    assert_eq!(store.snapshot()?.nodes(), initial.nodes());
    assert_eq!(store.snapshot()?.revision_id().as_str(), "r2");
    assert!(
        store.commit(&request).is_err(),
        "an old request must not become current after undo"
    );
    drop(store);

    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    store.redo(&RevisionId::new("r2")?, RevisionId::new("r3")?)?;
    assert_eq!(store.snapshot()?.nodes(), edited.nodes());
    assert_eq!(store.snapshot()?.duration()?.frames(), 12);
    let current = store.snapshot()?;
    assert!(matches!(
        store.undo(current.revision_id(), RevisionId::new("r0")?),
        Err(StoreError::RevisionReused(_))
    ));
    assert_eq!(store.snapshot()?, current);
    assert_eq!(revision_count(&path)?, 4);
    Ok(())
}

#[test]
fn editing_after_undo_keeps_old_revisions_and_starts_a_new_branch() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("branches.deadpan");
    let mut store = ProjectStore::create(&path, &document()?)?;
    store.commit(&insert(&store.snapshot()?, "r1", "first")?)?;
    store.commit(&insert(&store.snapshot()?, "r2", "second")?)?;
    store.undo(&RevisionId::new("r2")?, RevisionId::new("r3")?)?;
    store.commit(&insert(&store.snapshot()?, "r4", "alternative")?)?;
    assert!(matches!(
        store.redo(&RevisionId::new("r4")?, RevisionId::new("r5")?),
        Err(StoreError::NothingToRedo)
    ));
    assert_eq!(revision_count(&path)?, 5);
    assert_eq!(store.snapshot()?.duration()?.frames(), 24);
    assert!(
        !store
            .snapshot()?
            .nodes()
            .contains_key(&NodeId::new("second")?)
    );
    let db = Connection::open(path.join("project.sqlite"))?;
    let saved: String =
        db.query_row("SELECT document FROM revisions WHERE id='r2'", [], |row| {
            row.get(0)
        })?;
    assert!(
        ProjectDocument::from_json(&saved)?
            .nodes()
            .contains_key(&NodeId::new("second")?)
    );
    Ok(())
}

#[test]
fn failed_history_write_rolls_back_both_revision_and_current_state() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("failure.deadpan");
    let initial = document()?;
    let mut store = ProjectStore::create(&path, &initial)?;
    let db = Connection::open(path.join("project.sqlite"))?;
    db.execute_batch("CREATE TRIGGER fail_history BEFORE INSERT ON history BEGIN SELECT RAISE(ABORT, 'injected storage failure'); END;")?;
    assert!(store.commit(&insert(&initial, "r1", "hold")?).is_err());
    assert_eq!(store.snapshot()?, initial);
    assert_eq!(revision_count(&path)?, 1);
    db.execute_batch("DROP TRIGGER fail_history")?;
    store.commit(&insert(&initial, "r1", "hold")?)?;
    assert_eq!(store.snapshot()?.duration()?.frames(), 12);
    Ok(())
}

#[test]
fn backup_contains_committed_wal_and_is_independently_readable() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("backup.deadpan");
    let mut store = ProjectStore::create(&path, &document()?)?;
    store.commit(&insert(&store.snapshot()?, "r1", "hold")?)?;
    let checkpoint = store.checkpoint()?;
    let backup =
        Connection::open_with_flags(checkpoint, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let json: String = backup.query_row(
        "SELECT document FROM revisions JOIN state ON head_revision=revisions.id",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(ProjectDocument::from_json(&json)?, store.snapshot()?);
    assert_eq!(
        backup.query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))?,
        "ok"
    );
    Ok(())
}

#[test]
fn newer_schema_is_refused_without_rewriting_database() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("future.deadpan");
    drop(ProjectStore::create(&path, &document()?)?);
    let database = path.join("project.sqlite");
    let db = Connection::open(&database)?;
    db.pragma_update(None, "user_version", 999)?;
    db.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")?;
    drop(db);
    let before = fs::read(&database)?;
    assert!(matches!(
        ProjectStore::open(&path, AccessMode::ReadWrite),
        Err(StoreError::UnsupportedSchema(999))
    ));
    assert_eq!(fs::read(&database)?, before);
    Ok(())
}

#[cfg(unix)]
#[test]
fn storage_symlinks_are_rejected() -> Result {
    let scratch = tempfile::tempdir()?;
    let source = scratch.path().join("source.deadpan");
    drop(ProjectStore::create(&source, &document()?)?);
    let malicious = scratch.path().join("symlink.deadpan");
    fs::create_dir(&malicious)?;
    std::os::unix::fs::symlink(
        source.join("project.sqlite"),
        malicious.join("project.sqlite"),
    )?;
    assert!(matches!(
        ProjectStore::open(&malicious, AccessMode::ReadWrite),
        Err(StoreError::UnsafePath(_))
    ));
    Ok(())
}

#[test]
fn crash_during_transaction() -> Result {
    const CHILD_PATH: &str = "DEADPAN_TEST_CRASH_PACKAGE";
    if let Some(path) = std::env::var_os(CHILD_PATH) {
        let db = Connection::open(Path::new(&path).join("project.sqlite"))?;
        db.execute_batch("BEGIN IMMEDIATE; UPDATE revisions SET document='{}' WHERE id='r0';")?;
        // Leave the transaction uncommitted and bypass Rust destructors.
        std::process::exit(77);
    }
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("crash.deadpan");
    let initial = document()?;
    drop(ProjectStore::create(&path, &initial)?);
    let child = ProcessCommand::new(std::env::current_exe()?)
        .args(["--exact", "crash_during_transaction", "--nocapture"])
        .env(CHILD_PATH, &path)
        .output()?;
    assert_eq!(child.status.code(), Some(77));
    let store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, initial);
    store.validate()?;
    Ok(())
}

#[test]
fn preview_rejects_reused_revisions_exactly_as_commit() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("reuse.deadpan");
    let mut store = ProjectStore::create(&path, &document()?)?;
    store.commit(&insert(&store.snapshot()?, "r1", "hold")?)?;
    let request = insert(&store.snapshot()?, "r0", "another")?;
    assert!(matches!(
        store.preview(&request),
        Err(StoreError::RevisionReused(_))
    ));
    assert!(matches!(
        store.commit(&request),
        Err(StoreError::RevisionReused(_))
    ));
    assert_eq!(revision_count(&path)?, 2);
    assert_eq!(store.snapshot()?.revision_id().as_str(), "r1");
    Ok(())
}

#[test]
fn history_previews_share_validation_and_never_change_state() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("preview-history.deadpan");
    let mut store = ProjectStore::create(&path, &document()?)?;
    store.commit(&insert(&store.snapshot()?, "r1", "hold")?)?;
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    let r1 = RevisionId::new("r1")?;
    let r2 = RevisionId::new("r2")?;
    let preview = reader.preview_undo(&r1, r2.clone())?;
    assert_eq!(preview.edit.duration_delta, -12);
    assert_eq!(reader.snapshot()?.revision_id(), &r1);
    assert_eq!(revision_count(&path)?, 2);
    assert!(matches!(
        reader.preview_undo(&r1, RevisionId::new("r0")?),
        Err(StoreError::RevisionReused(_))
    ));
    assert!(matches!(
        reader.preview_undo(&RevisionId::new("stale")?, r2.clone()),
        Err(StoreError::RevisionConflict { .. })
    ));
    assert_eq!(preview.edit, store.undo(&r1, r2.clone())?.edit);
    let r3 = RevisionId::new("r3")?;
    let preview = reader.preview_redo(&r2, r3.clone())?;
    assert_eq!(preview.edit.duration_delta, 12);
    assert_eq!(reader.snapshot()?.revision_id(), &r2);
    assert_eq!(revision_count(&path)?, 3);
    assert_eq!(preview.edit, store.redo(&r2, r3)?.edit);
    store.validate()?;
    Ok(())
}

#[test]
fn semantic_validation_preserves_valid_branches_and_nested_navigation() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("semantic-valid.deadpan");
    let mut store = ProjectStore::create(&path, &document()?)?;
    for number in 1..=3 {
        store.commit(&insert(
            &store.snapshot()?,
            &format!("r{number}"),
            &format!("n{number}"),
        )?)?;
        store.validate()?;
    }
    for number in 4..=6 {
        store.undo(
            store.snapshot()?.revision_id(),
            RevisionId::new(format!("r{number}"))?,
        )?;
        store.validate()?;
    }
    for number in 7..=8 {
        store.redo(
            store.snapshot()?.revision_id(),
            RevisionId::new(format!("r{number}"))?,
        )?;
        store.validate()?;
    }
    store.commit(&insert(&store.snapshot()?, "r9", "branch")?)?;
    store.validate()?;
    store.undo(&RevisionId::new("r9")?, RevisionId::new("r10")?)?;
    store.validate()?;
    drop(store);
    ProjectStore::open(&path, AccessMode::ReadOnly)?.validate()?;
    Ok(())
}

#[test]
fn semantic_validation_rejects_forged_history_and_state() -> Result {
    let changes = [
        "UPDATE state SET cursor=NULL",
        "UPDATE state SET head_revision='r0'",
        "UPDATE history SET parent_id=id WHERE id=1",
        "UPDATE history SET request=json_set(request,'$.new_revision','unrelated') WHERE id=1",
        "UPDATE history SET edit=json_set(edit,'$.duration_delta',999) WHERE id=1",
        "UPDATE history SET edit=json_set(edit,'$.inverse.to_revision','unrelated') WHERE id=1",
        "UPDATE revisions SET document=json_set(document,'$.revision_id','unrelated') WHERE id='r0'",
        "UPDATE revisions SET document=json_set(document,'$.nodes.root.label','changed') WHERE id='r0'",
        "UPDATE revisions SET document=json_set(document,'$.nodes.root.label','changed') WHERE id='r3'",
        "UPDATE revisions SET parent_id='r1' WHERE id='r3'",
        "UPDATE redo SET history_id=1",
        "DELETE FROM redo",
        "INSERT INTO history(parent_id,revision_id,request,edit) SELECT parent_id,revision_id,request,edit FROM history WHERE id=1",
    ];
    for (number, change) in changes.iter().enumerate() {
        let scratch = tempfile::tempdir()?;
        let path = scratch.path().join(format!("invalid-{number}.deadpan"));
        let mut store = ProjectStore::create(&path, &document()?)?;
        store.commit(&insert(&store.snapshot()?, "r1", "one")?)?;
        store.commit(&insert(&store.snapshot()?, "r2", "two")?)?;
        store.undo(&RevisionId::new("r2")?, RevisionId::new("r3")?)?;
        store.validate()?;
        let db = Connection::open(path.join("project.sqlite"))?;
        db.execute_batch(change)?;
        assert!(store.validate().is_err(), "accepted corruption: {change}");
        drop(store);
        assert!(
            ProjectStore::open(&path, AccessMode::ReadOnly).is_err(),
            "opened corruption: {change}"
        );
    }
    Ok(())
}

#[test]
fn marks_and_loss_states_commit_atomically_and_survive_history_reopen() -> Result {
    use deadpan_core::{
        Anchor, AnchorLossPolicy, BoundaryAnchor, ExactRatio, InsertionBias, MarkId, MarkState,
    };
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("marks.deadpan");
    let mut store = ProjectStore::create(&path, &document()?)?;
    let commit = |store: &mut ProjectStore, command, name: &str| -> Result {
        let before = store.snapshot()?;
        store.commit(&CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: RevisionId::new(name)?,
            command,
        })?;
        Ok(())
    };
    store.commit(&insert(&store.snapshot()?, "insert", "hold")?)?;
    let mark = MarkId::new("a")?;
    commit(
        &mut store,
        Command::SetMark {
            id: mark.clone(),
            owner: NodeId::new("root")?,
            label: "The pause".into(),
            boundary: BoundaryAnchor {
                coordinate: Anchor::Local {
                    node: NodeId::new("hold")?,
                    position: ExactRatio::new(13, 2)?,
                },
                bias: InsertionBias::Right,
            },
            loss_policy: AnchorLossPolicy::KeepUnresolved,
        },
        "mark",
    )?;
    let marked = store.snapshot()?;
    let before_revisions = revision_count(&path)?;
    let request = CommandRequest {
        project_id: marked.project_id().clone(),
        expected_revision: marked.revision_id().clone(),
        new_revision: RevisionId::new("delete")?,
        command: Command::Delete {
            node: NodeId::new("hold")?,
        },
    };
    let preview = store.preview(&request)?;
    assert!(matches!(
        preview.forward.marks[&mark].after.as_ref().unwrap().state,
        MarkState::Unresolved { .. }
    ));
    assert_eq!(store.snapshot()?, marked);
    assert_eq!(revision_count(&path)?, before_revisions);
    store.commit(&request)?;
    let removed = store.snapshot()?;
    assert_eq!(preview.forward.apply(&marked)?, removed);
    assert!(matches!(
        removed.marks()[&mark].state,
        MarkState::Unresolved { .. }
    ));
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, removed);
    store.undo(
        store.snapshot()?.revision_id(),
        RevisionId::new("undo-delete")?,
    )?;
    assert_eq!(store.snapshot()?.marks(), marked.marks());
    assert_eq!(store.snapshot()?.nodes(), marked.nodes());
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    store.redo(
        store.snapshot()?.revision_id(),
        RevisionId::new("redo-delete")?,
    )?;
    assert_eq!(store.snapshot()?.marks(), removed.marks());
    // A new node at the same identity/time does not reattach the lost mark.
    store.commit(&insert(&store.snapshot()?, "new-content", "hold")?)?;
    assert_eq!(store.snapshot()?.marks(), removed.marks());
    let current = store.snapshot()?;
    commit(
        &mut store,
        Command::SetMark {
            id: mark.clone(),
            owner: NodeId::new("hold")?,
            label: "New deliberate target".into(),
            boundary: BoundaryAnchor {
                coordinate: Anchor::Local {
                    node: NodeId::new("hold")?,
                    position: ExactRatio::integer(4),
                },
                bias: InsertionBias::Left,
            },
            loss_policy: AnchorLossPolicy::DeleteOwned,
        },
        "reattach",
    )?;
    assert!(matches!(
        store.snapshot()?.marks()[&mark].state,
        MarkState::Bound
    ));
    // Stale destructive requests cannot change either marks or structure.
    let before = store.snapshot()?;
    let stale = CommandRequest {
        project_id: current.project_id().clone(),
        expected_revision: current.revision_id().clone(),
        new_revision: RevisionId::new("stale")?,
        command: Command::DeleteMark { id: mark.clone() },
    };
    assert_eq!(store.commit(&stale).unwrap_err().code(), "RevisionConflict");
    assert_eq!(store.snapshot()?, before);
    commit(
        &mut store,
        Command::Delete {
            node: NodeId::new("hold")?,
        },
        "delete-owned",
    )?;
    assert!(!store.snapshot()?.marks().contains_key(&mark));
    store.undo(
        store.snapshot()?.revision_id(),
        RevisionId::new("restore-owned")?,
    )?;
    assert_eq!(store.snapshot()?.marks(), before.marks());
    store.validate()?;
    Ok(())
}

#[test]
fn play_overrides_retire_with_identity_and_survive_durable_undo_redo() -> Result {
    use deadpan_core::{IterationId, NodeKind, WrapAnchorPolicy};
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("overrides.deadpan");
    let mut store = ProjectStore::create(&path, &document()?)?;
    let request = |store: &ProjectStore, command, name: &str| -> Result<CommandRequest> {
        let before = store.snapshot()?;
        Ok(CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: RevisionId::new(name)?,
            command,
        })
    };
    let identity = |document: &ProjectDocument, index| -> IterationId {
        let NodeKind::Repeat { iterations, .. } =
            &document.nodes()[&NodeId::new("repeat").unwrap()].kind
        else {
            panic!()
        };
        iterations.at(index).unwrap()
    };
    store.commit(&insert(&store.snapshot()?, "insert", "hold")?)?;
    let repeat = NodeId::new("repeat")?;
    store.commit(&request(
        &store,
        Command::WrapRepeat {
            node: NodeId::new("hold")?,
            id: repeat.clone(),
            plays: 3,
            gap: None,
            anchor_policy: WrapAnchorPolicy::First,
        },
        "wrap",
    )?)?;
    let original = store.snapshot()?;
    let retired = identity(&original, 2);
    let replacement = NodeId::new("replacement")?;
    let set = request(
        &store,
        Command::SetPlayOverride {
            node: repeat.clone(),
            iteration: retired.clone(),
            subtree: Subtree {
                root: replacement.clone(),
                overrides: Default::default(),
                nodes: BTreeMap::from([(
                    replacement.clone(),
                    BeatNode::hold(
                        "Alternate pause",
                        HoldRecipe {
                            duration: FrameDuration::new(5)?,
                            video: HoldVideo::Background,
                            audio: HoldAudio::Silence,
                        },
                    ),
                )]),
            },
        },
        "set-override",
    )?;
    let before_revisions = revision_count(&path)?;
    let preview = store.preview(&set)?;
    assert!(!preview.forward.overrides.is_empty());
    assert!(preview.forward.nodes.contains_key(&replacement));
    assert_eq!(store.snapshot()?, original);
    assert_eq!(revision_count(&path)?, before_revisions);
    store.commit(&set)?;
    let overridden = store.snapshot()?;
    assert_eq!(overridden.duration()?.frames(), 29);
    assert_eq!(preview.forward.apply(&original)?, overridden);
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, overridden);
    store.commit(&request(
        &store,
        Command::ClearPlayOverride {
            node: repeat.clone(),
            iteration: retired.clone(),
        },
        "clear-override",
    )?)?;
    let cleared = store.snapshot()?;
    assert!(cleared.overrides().is_empty());
    assert!(!cleared.nodes().contains_key(&replacement));
    assert_eq!(cleared.duration()?.frames(), 36);
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    store.undo(
        store.snapshot()?.revision_id(),
        RevisionId::new("undo-clear")?,
    )?;
    assert_eq!(store.snapshot()?.overrides(), overridden.overrides());
    assert_eq!(store.snapshot()?.nodes(), overridden.nodes());
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    store.redo(
        store.snapshot()?.revision_id(),
        RevisionId::new("redo-clear")?,
    )?;
    assert_eq!(store.snapshot()?.overrides(), cleared.overrides());
    assert_eq!(store.snapshot()?.nodes(), cleared.nodes());
    store.undo(
        store.snapshot()?.revision_id(),
        RevisionId::new("restore-before-shrink")?,
    )?;
    store.commit(&request(
        &store,
        Command::SetRepeat {
            node: repeat.clone(),
            plays: 2,
            gap: None,
        },
        "shrink",
    )?)?;
    let shrunk = store.snapshot()?;
    assert!(shrunk.overrides().is_empty());
    assert!(!shrunk.nodes().contains_key(&replacement));
    assert_eq!(shrunk.duration()?.frames(), 24);
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    store.undo(
        store.snapshot()?.revision_id(),
        RevisionId::new("undo-shrink")?,
    )?;
    assert_eq!(store.snapshot()?.overrides(), overridden.overrides());
    assert_eq!(store.snapshot()?.nodes(), overridden.nodes());
    assert_eq!(identity(&store.snapshot()?, 2), retired);
    store.redo(
        store.snapshot()?.revision_id(),
        RevisionId::new("redo-shrink")?,
    )?;
    assert_eq!(store.snapshot()?.overrides(), shrunk.overrides());
    assert_eq!(store.snapshot()?.nodes(), shrunk.nodes());
    store.commit(&request(
        &store,
        Command::SetRepeat {
            node: repeat.clone(),
            plays: 3,
            gap: None,
        },
        "grow",
    )?)?;
    let grown = store.snapshot()?;
    assert_ne!(identity(&grown, 2), retired);
    assert_eq!(identity(&grown, 2).allocation.as_str(), "grow");
    assert!(grown.overrides().is_empty());
    assert_eq!(grown.duration()?.frames(), 36);
    let before_revisions = revision_count(&path)?;
    assert!(
        store
            .commit(&request(
                &store,
                Command::ClearPlayOverride {
                    node: repeat,
                    iteration: retired,
                },
                "cannot-revive-retired"
            )?)
            .is_err()
    );
    assert_eq!(revision_count(&path)?, before_revisions);
    assert_eq!(store.snapshot()?, grown);
    store.validate()?;
    drop(store);
    assert_eq!(
        ProjectStore::open(&path, AccessMode::ReadOnly)?.snapshot()?,
        grown
    );
    Ok(())
}

#[test]
fn transparent_partition_intent_is_durable_atomic_and_undoable() -> Result {
    use deadpan_core::{FrameRange, NodeKind, PitchPolicy, ProjectFrame, RetimePurpose};
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("partition.deadpan");
    let initial = document()?;
    let mut request = insert(&initial, "partition-insert", "context")?;
    let Command::Insert { subtree, .. } = &mut request.command else {
        unreachable!();
    };
    subtree.root = NodeId::new("partition")?;
    subtree.nodes.insert(
        subtree.root.clone(),
        BeatNode {
            label: "Transparent output interval".into(),
            kind: NodeKind::Retime {
                child: NodeId::new("context")?,
                duration: FrameDuration::new(4)?,
                mapping: FrameRange::new(ProjectFrame(3), ProjectFrame(7))?,
                pitch: PitchPolicy::Preserve,
                purpose: RetimePurpose::Partition,
            },
            audio_edges: Default::default(),
        },
    );
    let mut store = ProjectStore::create(&path, &initial)?;
    let mut invalid = request.clone();
    let Command::Insert { subtree, .. } = &mut invalid.command else {
        unreachable!();
    };
    let NodeKind::Retime { duration, .. } = &mut subtree.nodes.get_mut(&subtree.root).unwrap().kind
    else {
        unreachable!();
    };
    *duration = FrameDuration::new(5)?;
    assert!(store.commit(&invalid).is_err());
    assert_eq!(revision_count(&path)?, 1);
    assert_eq!(store.snapshot()?, initial);
    store.commit(&request)?;
    let inserted = store.snapshot()?;
    assert_eq!(inserted.duration()?.frames(), 4);
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, inserted);
    let undo = RevisionId::new("partition-undo")?;
    store.undo(inserted.revision_id(), undo.clone())?;
    assert_eq!(store.snapshot()?.nodes(), initial.nodes());
    store.redo(&undo, RevisionId::new("partition-redo")?)?;
    assert_eq!(store.snapshot()?.nodes(), inserted.nodes());
    store.validate()?;
    drop(store);
    let reopened = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert_eq!(reopened.snapshot()?.nodes(), inserted.nodes());
    Ok(())
}

#[test]
fn logical_mark_fragments_survive_partial_loss_reopen_and_durable_history() -> Result {
    use deadpan_core::{
        Anchor, AnchorLossPolicy, BoundaryAnchor, ExactRatio, InsertionBias, Mark, MarkFragment,
        MarkId, MarkState,
    };
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("fragments.deadpan");
    let mut initial = document()?;
    for id in ["a", "b"] {
        initial = deadpan_core::apply(&initial, &insert(&initial, id, id)?)?
            .forward
            .apply(&initial)?;
    }
    let coordinate = |id| {
        Ok::<_, Box<dyn Error>>(Anchor::Local {
            node: NodeId::new(id)?,
            position: ExactRatio::integer(4),
        })
    };
    let mark = Mark {
        owner: NodeId::new("a")?,
        label: "Cue".into(),
        boundary: BoundaryAnchor {
            coordinate: coordinate("a")?,
            bias: InsertionBias::Right,
        },
        loss_policy: AnchorLossPolicy::DeleteOwned,
        state: MarkState::Bound,
        fragments: vec![MarkFragment {
            owner: NodeId::new("b")?,
            coordinate: coordinate("b")?,
            state: MarkState::Bound,
        }],
    };
    let mut wire = serde_json::to_value(&initial)?;
    wire["marks"]["cue"] = serde_json::to_value(&mark)?;
    let initial = ProjectDocument::from_json(&wire.to_string())?;
    let id = MarkId::new("cue")?;
    let mut store = ProjectStore::create(&path, &initial)?;
    let delete = CommandRequest {
        project_id: initial.project_id().clone(),
        expected_revision: initial.revision_id().clone(),
        new_revision: RevisionId::new("delete-a")?,
        command: Command::Delete {
            node: NodeId::new("a")?,
        },
    };
    store.commit(&delete)?;
    let removed = store.snapshot()?;
    assert_eq!(removed.marks()[&id].owner, NodeId::new("b")?);
    assert_eq!(removed.marks()[&id].binding_count(), 1);
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, removed);
    store.undo(
        removed.revision_id(),
        RevisionId::new("undo-fragment-loss")?,
    )?;
    assert_eq!(store.snapshot()?.marks()[&id], mark);
    store.redo(
        &RevisionId::new("undo-fragment-loss")?,
        RevisionId::new("redo-fragment-loss")?,
    )?;
    assert_eq!(store.snapshot()?.marks(), removed.marks());
    let before = store.snapshot()?;
    let count = revision_count(&path)?;
    let invalid = CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: RevisionId::new("invalid-owner")?,
        command: Command::SetMark {
            id,
            owner: NodeId::new("missing")?,
            label: "Invalid".into(),
            boundary: mark.boundary,
            loss_policy: AnchorLossPolicy::DeleteOwned,
        },
    };
    assert!(store.commit(&invalid).is_err());
    assert_eq!(revision_count(&path)?, count);
    assert_eq!(store.snapshot()?, before);
    store.validate()?;
    drop(store);
    assert_eq!(
        ProjectStore::open(&path, AccessMode::ReadOnly)?.snapshot()?,
        before
    );
    Ok(())
}
