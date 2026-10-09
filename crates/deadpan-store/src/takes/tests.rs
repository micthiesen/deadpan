use super::*;
use deadpan_core::{
    BeatNode, ColorPolicy, FrameDuration, FrameRate, HoldAudio, HoldRecipe, HoldVideo, NodeId,
    PresentationBasis,
};

fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn document() -> ProjectDocument {
    let initial = ProjectDocument::new(
        ProjectId::new("takes-project").unwrap(),
        revision("initial"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    let mut wire = serde_json::to_value(initial).unwrap();
    wire["nodes"] = serde_json::json!({
        "root": BeatNode::sequence("Root",vec![node("pause")]),
        "pause": BeatNode::hold("Pause",HoldRecipe { duration: FrameDuration::new(12).unwrap(), picture_context: None, video: HoldVideo::Background, audio: HoldAudio::Silence })
    });
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}
fn request(store: &ProjectStore, action: Action) -> Request {
    let catalog = store.take_catalog().unwrap();
    Request {
        project_id: catalog.project_id,
        expected_revision: catalog.revision_id,
        expected_version: catalog.version,
        action,
    }
}
fn create(store: &mut ProjectStore, id: &str, name: &str) {
    store
        .apply_take(&request(
            store,
            Action::Create {
                id: TakeId::new(id).unwrap(),
                name: TakeName::new(name).unwrap(),
            },
        ))
        .unwrap();
}
fn edit(store: &mut ProjectStore, next: &str, duration: i64) {
    let before = store.snapshot().unwrap();
    store
        .commit(&CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: revision(next),
            command: Command::SetHoldDuration {
                node: node("pause"),
                duration: FrameDuration::new(duration).unwrap(),
            },
        })
        .unwrap();
}
fn assert_authored(actual: &ProjectDocument, expected: &ProjectDocument) {
    let mut wanted = serde_json::to_value(expected).unwrap();
    wanted["revision_id"] = serde_json::to_value(actual.revision_id()).unwrap();
    assert_eq!(serde_json::to_value(actual).unwrap(), wanted);
}

#[test]
fn takes_validate_unicode_names_and_nonreused_ids() {
    for bad in [
        "",
        " ",
        " name",
        "name ",
        "a\nb",
        "a\tb",
        "a\u{2028}b",
        "a\0b",
    ] {
        assert!(TakeName::new(bad).is_err());
    }
    assert!(TakeName::new("é".repeat(64)).is_ok());
    assert!(TakeName::new("é".repeat(65)).is_err());
    assert!(TakeName::new("Quiet début 🎞").is_ok());
    assert!(serde_json::from_str::<TakeName>("\"bad\\nname\"").is_err());
    assert!(TakeId::new("bad id").is_err());
}

#[test]
fn takes_crud_is_atomic_revision_bound_and_preview_reserves_nothing() {
    let scratch = tempfile::tempdir().unwrap();
    let mut store =
        ProjectStore::create(&scratch.path().join("takes.deadpan"), &document()).unwrap();
    let first = request(
        &store,
        Action::Create {
            id: TakeId::new("b").unwrap(),
            name: TakeName::new("Before").unwrap(),
        },
    );
    let preview = store.preview_take(&first).unwrap();
    assert_eq!(preview.catalog.version, 1);
    assert_eq!(store.take_catalog().unwrap().version, 0);
    assert_eq!(store.apply_take(&first).unwrap().catalog, preview.catalog);
    assert!(store.apply_take(&first).is_err());
    create(&mut store, "a", "Other");
    assert_eq!(
        store
            .take_catalog()
            .unwrap()
            .entries
            .iter()
            .map(|t| t.id.as_str())
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
    let rename = request(
        &store,
        Action::Rename {
            id: TakeId::new("b").unwrap(),
            expected_snapshot: revision("initial"),
            name: TakeName::new("Renamed").unwrap(),
        },
    );
    assert!(store.apply_take(&rename).unwrap().changed);
    let noop = request(&store, rename.action.clone());
    assert!(!store.apply_take(&noop).unwrap().changed);
    let deleted = request(
        &store,
        Action::Delete {
            id: TakeId::new("b").unwrap(),
            expected_snapshot: revision("initial"),
        },
    );
    store.apply_take(&deleted).unwrap();
    let reuse = request(&store, first.action);
    assert!(store.preview_take(&reuse).is_err());
    assert!(store.apply_take(&reuse).is_err());
    assert_eq!(store.snapshot().unwrap(), document());
    store.validate_full().unwrap();
}

#[test]
fn takes_restore_abandoned_revision_and_history_survives_deleted_label() {
    let scratch = tempfile::tempdir().unwrap();
    let package = scratch.path().join("takes.deadpan");
    let mut store = ProjectStore::create(&package, &document()).unwrap();
    edit(&mut store, "longer", 27);
    let saved = store.snapshot().unwrap();
    create(&mut store, "long", "Long pause");
    store
        .undo(&revision("longer"), revision("undo-longer"))
        .unwrap();
    edit(&mut store, "branch", 7);
    let branch = store.snapshot().unwrap();
    let restore = request(
        &store,
        Action::Restore {
            id: TakeId::new("long").unwrap(),
            expected_snapshot: revision("longer"),
            new_revision: revision("restored"),
        },
    );
    let preview = store.preview_take(&restore).unwrap();
    assert_eq!(store.snapshot().unwrap(), branch);
    let outcome = store.apply_take(&restore).unwrap();
    assert_eq!(preview.catalog, outcome.catalog);
    assert_eq!(outcome.catalog.version, 1);
    assert!(outcome.commit.is_some());
    assert_authored(&store.snapshot().unwrap(), &saved);
    store
        .apply_take(&request(
            &store,
            Action::Delete {
                id: TakeId::new("long").unwrap(),
                expected_snapshot: revision("longer"),
            },
        ))
        .unwrap();
    store
        .undo(&revision("restored"), revision("undo-restore"))
        .unwrap();
    assert_authored(&store.snapshot().unwrap(), &branch);
    store
        .redo(&revision("undo-restore"), revision("redo-restore"))
        .unwrap();
    assert_authored(&store.snapshot().unwrap(), &saved);
    store.validate_full().unwrap();
    drop(store);
    let reopened = ProjectStore::open(&package, crate::AccessMode::ReadOnly).unwrap();
    reopened.validate_full().unwrap();
    assert_authored(&reopened.snapshot().unwrap(), &saved);
    assert!(reopened.take_catalog().unwrap().entries.is_empty());
}

#[test]
fn takes_metadata_preserves_redo_registers_and_rejects_stale_targets() {
    let scratch = tempfile::tempdir().unwrap();
    let mut store =
        ProjectStore::create(&scratch.path().join("takes.deadpan"), &document()).unwrap();
    create(&mut store, "draft", "Draft");
    edit(&mut store, "long", 31);
    store
        .undo(&revision("long"), revision("undo-long"))
        .unwrap();
    let head = store.snapshot().unwrap();
    let history = store.history_availability().unwrap();
    let bank = store.registers().unwrap();
    let stale = request(
        &store,
        Action::Restore {
            id: TakeId::new("draft").unwrap(),
            expected_snapshot: revision("initial"),
            new_revision: revision("restored"),
        },
    );
    store
        .apply_take(&request(
            &store,
            Action::Update {
                id: TakeId::new("draft").unwrap(),
                expected_snapshot: revision("initial"),
            },
        ))
        .unwrap();
    assert!(store.apply_take(&stale).is_err());
    let mut refreshed = stale;
    refreshed.expected_version = store.take_catalog().unwrap().version;
    assert!(store.apply_take(&refreshed).is_err());
    assert_eq!(store.snapshot().unwrap(), head);
    assert_eq!(store.registers().unwrap(), bank);
    assert_eq!(store.history_availability().unwrap(), history);
    store
        .redo(&revision("undo-long"), revision("redo-long"))
        .unwrap();
    assert_eq!(store.snapshot().unwrap().duration().unwrap().frames(), 31);
}

#[test]
fn takes_write_failure_rolls_back_catalog_identity_restore_and_history() {
    let scratch = tempfile::tempdir().unwrap();
    let mut store =
        ProjectStore::create(&scratch.path().join("takes.deadpan"), &document()).unwrap();
    let create = request(
        &store,
        Action::Create {
            id: TakeId::new("draft").unwrap(),
            name: TakeName::new("Draft").unwrap(),
        },
    );
    let before = store.take_catalog().unwrap();
    store.connection.execute_batch("CREATE TRIGGER fail_take BEFORE UPDATE ON take_state BEGIN SELECT RAISE(FAIL,'forced'); END;").unwrap();
    assert!(store.apply_take(&create).is_err());
    assert_eq!(store.take_catalog().unwrap(), before);
    store
        .connection
        .execute_batch("DROP TRIGGER fail_take;")
        .unwrap();
    store.apply_take(&create).unwrap();
    edit(&mut store, "long", 29);
    let before = store.snapshot().unwrap();
    let restore = request(
        &store,
        Action::Restore {
            id: TakeId::new("draft").unwrap(),
            expected_snapshot: revision("initial"),
            new_revision: revision("restored"),
        },
    );
    store.connection.execute_batch("CREATE TRIGGER fail_take_history BEFORE INSERT ON history BEGIN SELECT RAISE(FAIL,'forced'); END;").unwrap();
    assert!(store.apply_take(&restore).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
    assert!(
        restore_proof(&store.connection, "restored")
            .unwrap()
            .is_none()
    );
    store
        .connection
        .execute_batch("DROP TRIGGER fail_take_history;")
        .unwrap();
    store.apply_take(&restore).unwrap();
    store.validate_full().unwrap();
}

#[test]
fn takes_capacity_readonly_and_catalog_corruption_fail_without_writes() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("takes.deadpan");
    let mut store = ProjectStore::create(&path, &document()).unwrap();
    for index in 0..MAX_TAKES {
        create(
            &mut store,
            &format!("t-{index:03}"),
            &format!("Take {index}"),
        );
    }
    let overflow = request(
        &store,
        Action::Create {
            id: TakeId::new("overflow").unwrap(),
            name: TakeName::new("Overflow").unwrap(),
        },
    );
    assert!(store.preview_take(&overflow).is_err());
    assert!(store.apply_take(&overflow).is_err());
    assert_eq!(store.take_catalog().unwrap().entries.len(), MAX_TAKES);
    let mut readonly = ProjectStore::open(&path, crate::AccessMode::ReadOnly).unwrap();
    assert!(matches!(
        readonly.apply_take(&overflow),
        Err(StoreError::ReadOnly)
    ));
    drop(readonly);
    store
        .connection
        .execute("UPDATE takes SET name='Corrupt label' WHERE id='t-000'", [])
        .unwrap();
    assert!(store.take_catalog().is_err());
    assert!(store.validate().is_err());
    assert!(store.validate_full().is_err());
    assert!(store.checkpoint().is_err());
    drop(store);
    assert!(ProjectStore::open(&path, crate::AccessMode::ReadOnly).is_err());
}

#[test]
fn takes_direct_commands_compounds_and_restore_proof_tampering_are_refused() {
    let scratch = tempfile::tempdir().unwrap();
    let mut store =
        ProjectStore::create(&scratch.path().join("takes.deadpan"), &document()).unwrap();
    create(&mut store, "draft", "Draft");
    let command = Command::RestoreSnapshot {
        snapshot: Box::new(document()),
    };
    assert!(deadpan_core::AtomicCommand::new(command.clone()).is_err());
    let forged = CommandRequest {
        project_id: document().project_id().clone(),
        expected_revision: revision("initial"),
        new_revision: revision("restore"),
        command,
    };
    assert!(store.preview(&forged).is_err());
    assert!(store.commit(&forged).is_err());
    store
        .apply_take(&request(
            &store,
            Action::Restore {
                id: TakeId::new("draft").unwrap(),
                expected_snapshot: revision("initial"),
                new_revision: revision("restore"),
            },
        ))
        .unwrap();
    store.validate_full().unwrap();
    store
        .connection
        .execute(
            "UPDATE take_restores SET snapshot_revision='restore' WHERE revision_id='restore'",
            [],
        )
        .unwrap();
    assert!(store.validate().is_err());
    assert!(store.validate_full().is_err());
}

#[test]
fn takes_backup_replacement_carries_identity_reservations_and_new_version() {
    let scratch = tempfile::tempdir().unwrap();
    let mut live = ProjectStore::create(&scratch.path().join("live.deadpan"), &document()).unwrap();
    let mut copy = ProjectStore::create(&scratch.path().join("copy.deadpan"), &document()).unwrap();
    create(&mut live, "old", "Old");
    create(&mut copy, "old", "Old");
    create(&mut live, "later", "Later");
    live.apply_take(&request(
        &live,
        Action::Delete {
            id: TakeId::new("later").unwrap(),
            expected_snapshot: revision("initial"),
        },
    ))
    .unwrap();
    let old_version = live.take_catalog().unwrap().version;
    crate::retired::carry_forward(&live.connection, &mut copy.connection).unwrap();
    let restored = copy.take_catalog().unwrap();
    assert_eq!(restored.version, old_version + 1);
    assert_eq!(restored.entries.len(), 1);
    let reuse = request(
        &copy,
        Action::Create {
            id: TakeId::new("later").unwrap(),
            name: TakeName::new("New").unwrap(),
        },
    );
    assert!(copy.apply_take(&reuse).is_err());
    copy.validate_full().unwrap();
}

#[test]
fn takes_portable_copy_checkpoint_and_real_backup_preserve_abandoned_snapshots() {
    use crate::backups::{BackupLimits, BackupPolicy, BackupReason, create_backup, verify_backup};
    use std::sync::atomic::AtomicBool;
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("takes.deadpan");
    let copy = scratch.path().join("portable.deadpan");
    let cancelled = AtomicBool::new(false);
    let mut store = ProjectStore::create(&path, &document()).unwrap();
    edit(&mut store, "long", 44);
    create(&mut store, "saved", "Long branch");
    store
        .undo(&revision("long"), revision("undo-long"))
        .unwrap();
    edit(&mut store, "short", 5);
    let catalog = store.take_catalog().unwrap();
    let checkpoint = store.checkpoint().unwrap();
    let check = Connection::open(&checkpoint).unwrap();
    assert_eq!(read_catalog(&check).unwrap(), catalog);
    let backup = create_backup(
        &path,
        BackupReason::Manual,
        &BackupPolicy::default(),
        BackupLimits::default(),
        &cancelled,
    )
    .unwrap();
    verify_backup(&backup.backup).unwrap();
    crate::portable::copy_portable(&path, &copy, &cancelled).unwrap();
    let mut copied = ProjectStore::open(&copy, crate::AccessMode::ReadWrite).unwrap();
    assert_eq!(copied.take_catalog().unwrap(), catalog);
    copied
        .apply_take(&request(
            &copied,
            Action::Restore {
                id: TakeId::new("saved").unwrap(),
                expected_snapshot: revision("long"),
                new_revision: revision("restore-in-copy"),
            },
        ))
        .unwrap();
    assert_eq!(copied.snapshot().unwrap().duration().unwrap().frames(), 44);
    copied.validate_full().unwrap();
    create(&mut store, "new-id", "Created after backup");
    let new_version = store.take_catalog().unwrap().version;
    store
        .restore_backup(
            &backup.backup.id,
            &BackupPolicy::default(),
            BackupLimits::default(),
            &cancelled,
        )
        .unwrap();
    assert_eq!(store.take_catalog().unwrap().version, new_version + 1);
    assert_eq!(store.take_catalog().unwrap().entries, catalog.entries);
    let reuse = request(
        &store,
        Action::Create {
            id: TakeId::new("new-id").unwrap(),
            name: TakeName::new("Attempted reuse").unwrap(),
        },
    );
    assert!(store.apply_take(&reuse).is_err());
    store.validate_full().unwrap();
}

#[test]
fn takes_names_are_exact_unique_and_collisions_reserve_nothing() {
    let scratch = tempfile::tempdir().unwrap();
    let mut store =
        ProjectStore::create(&scratch.path().join("takes.deadpan"), &document()).unwrap();
    create(&mut store, "first", "Draft");
    create(&mut store, "second", "Other");
    let before = store.take_catalog().unwrap();
    let duplicate = request(
        &store,
        Action::Create {
            id: TakeId::new("unreserved").unwrap(),
            name: TakeName::new("Draft").unwrap(),
        },
    );
    assert!(store.preview_take(&duplicate).is_err());
    assert!(store.apply_take(&duplicate).is_err());
    assert_eq!(store.take_catalog().unwrap(), before);
    assert_eq!(store.snapshot().unwrap(), document());
    let rename = request(
        &store,
        Action::Rename {
            id: TakeId::new("second").unwrap(),
            expected_snapshot: revision("initial"),
            name: TakeName::new("Draft").unwrap(),
        },
    );
    assert!(store.preview_take(&rename).is_err());
    assert!(store.apply_take(&rename).is_err());
    assert_eq!(store.take_catalog().unwrap(), before);
    let noop = request(
        &store,
        Action::Rename {
            id: TakeId::new("first").unwrap(),
            expected_snapshot: revision("initial"),
            name: TakeName::new("Draft").unwrap(),
        },
    );
    let preview = store.preview_take(&noop).unwrap();
    assert!(!preview.changed);
    assert_eq!(preview.catalog, before);
    let applied = store.apply_take(&noop).unwrap();
    assert!(!applied.changed);
    assert_eq!(applied.catalog, before);
    // The failed Create neither reserved its identity nor advanced the catalog;
    // names differing only in case are intentionally distinct.
    create(&mut store, "unreserved", "draft");
    assert_eq!(store.take_catalog().unwrap().version, before.version + 1);
    assert_eq!(store.take_catalog().unwrap().entries.len(), 3);
    assert_eq!(store.snapshot().unwrap(), document());
    store.validate_full().unwrap();
}

#[test]
fn takes_name_uniqueness_is_enforced_by_sql_and_catalog_validation() {
    let scratch = tempfile::tempdir().unwrap();
    let mut store =
        ProjectStore::create(&scratch.path().join("takes.deadpan"), &document()).unwrap();
    create(&mut store, "first", "Draft");
    create(&mut store, "second", "Other");
    let before = store.take_catalog().unwrap();
    assert!(
        store
            .connection
            .execute("UPDATE takes SET name='Draft' WHERE id='second'", [])
            .is_err()
    );
    assert_eq!(store.take_catalog().unwrap(), before);
    // A damaged schema must not let semantic validation trust duplicate names,
    // even when the catalog checksum has been recomputed to match those rows.
    store.connection.execute_batch(
        "DROP TABLE takes;
         CREATE TABLE takes(id TEXT PRIMARY KEY, name TEXT NOT NULL, snapshot_revision TEXT NOT NULL) STRICT;
         INSERT INTO takes VALUES('first','Draft','initial'),('second','Draft','initial');",
    ).unwrap();
    let mut malformed = before;
    malformed.entries[1].name = malformed.entries[0].name.clone();
    let forged = digest(&store.connection, malformed.version, &malformed.entries).unwrap();
    store
        .connection
        .execute(
            "UPDATE take_state SET digest=?1 WHERE singleton=1",
            [&forged[..]],
        )
        .unwrap();
    let error = store.take_catalog().unwrap_err().to_string();
    assert!(error.contains("duplicate names"), "{error}");
    assert!(store.validate().is_err());
    assert!(store.validate_full().is_err());
}
