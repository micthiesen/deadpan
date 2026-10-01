use super::*;

use std::{collections::BTreeMap, ops::Range};

use deadpan_core::{
    AudioEditorialEdges, AudioTimingId, BeatNode, EditErrorCode, EditTransaction, ExactRatio,
    FrameRange, ProjectFrame, SourceRollSide, SourceTrimClamp, Subtree,
};
use deadpan_media::source_import_timing::derive_source_moment;

fn node(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

fn edit(current: &ProjectDocument, next: &str, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: current.project_id().clone(),
        expected_revision: current.revision_id().clone(),
        new_revision: revision(next),
        command,
    }
}

fn roll(
    current: &ProjectDocument,
    next: &str,
    left: &str,
    right: &str,
    delta: i64,
    wrappers: (Option<&str>, Option<&str>),
) -> CommandRequest {
    edit(
        current,
        next,
        Command::RollSources {
            parent: node("root"),
            left: node(left),
            right: node(right),
            delta_frames: delta,
            left_wrapper: wrappers.0.map(node),
            right_wrapper: wrappers.1.map(node),
            timing: AudioTimingId {
                allocation: revision(next),
                ordinal: 0,
            },
        },
    )
}

fn ready(parent: &Path, right_ordinals: Range<u64>) -> Result<(PathBuf, ProjectStore)> {
    let (path, mut store) = project(parent)?;
    // Generic compatibility projects admit two different Originals. Reusing one
    // receipt twice would not prove admission checks the right-hand asset.
    for (asset, file, registered, inserted, slot, ordinals) in [
        (
            "left",
            "cfr-bframes.mp4",
            "left-registered",
            "left-inserted",
            0,
            10..13,
        ),
        (
            "right",
            "offset-bframes.mp4",
            "right-registered",
            "ready",
            1,
            right_ordinals,
        ),
    ] {
        let original = retain(&mut store, file)?;
        let decoded = decode(&store, &original)?;
        let registration = request(&store, &original, registered, asset, None)?;
        store.register_source(&registration, &decoded, None, limits(), &active())?;
        let current = store.snapshot()?;
        let receipt = store.registered_source(current.revision_id(), &id(asset))?;
        let timing = derive_source_moment(
            receipt.snapshot().video().unwrap().index(),
            receipt.snapshot().audio(),
            ordinals,
            current.presentation_basis().frame_rate,
        )?;
        store.commit(&edit(
            &current,
            inserted,
            Command::Insert {
                parent: node("root"),
                index: slot,
                subtree: Subtree {
                    root: node(asset),
                    nodes: BTreeMap::from([(
                        node(asset),
                        BeatNode {
                            label: format!("{asset} measured moment"),
                            framing: None,
                            audio_treatments: Default::default(),
                            audio_editorial_edges: Default::default(),
                            audio_edges: Default::default(),
                            kind: NodeKind::Source {
                                source: timing.source_node(id(asset)),
                            },
                        },
                    )]),
                    overrides: Default::default(),
                    gap_overrides: Default::default(),
                },
            },
        ))?;
    }
    let current = store.snapshot()?;
    let left = store.registered_source(current.revision_id(), &id("left"))?;
    let right = store.registered_source(current.revision_id(), &id("right"))?;
    assert_ne!(left.id(), right.id());
    assert_ne!(left.original().content(), right.original().content());
    assert_eq!(current.assets().len(), 2);
    let db = Connection::open(path.join("project.sqlite"))?;
    let profile: String = db.query_row("SELECT workflow FROM state", [], |row| row.get(0))?;
    assert_eq!(profile, "generic");
    Ok((path, store))
}

fn range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}

// Compare every authored/admission row and cursor, including a nonempty redo
// branch, so rejection cannot silently repair evidence or partly write an edit.
fn stored(database: &Connection) -> Result<String> {
    Ok(database.query_row(
        "SELECT json_array(
         (SELECT json_group_array(json_array(id,parent_id,kind,document)) FROM
             (SELECT * FROM revisions ORDER BY id)),
         (SELECT json_group_array(json_array(id,parent_id,revision_id,request,edit)) FROM
             (SELECT * FROM history ORDER BY id)),
         (SELECT json_group_array(json_array(singleton,head_revision,cursor,workflow)) FROM state),
         (SELECT json_group_array(json_array(position,history_id)) FROM
             (SELECT * FROM redo ORDER BY position)),
         (SELECT json_group_array(json_array(id,original_content_id,original_ref,hex(snapshot))) FROM
             (SELECT * FROM source_qualifications ORDER BY id)),
         (SELECT json_group_array(json_array(content_id,version,record)) FROM
             (SELECT * FROM original_media ORDER BY content_id)))",
        [], |row| row.get(0),
    )?)
}

fn same_content(actual: &ProjectDocument, expected: &ProjectDocument) -> Result {
    let mut expected = serde_json::to_value(expected)?;
    expected["revision_id"] = serde_json::to_value(actual.revision_id())?;
    assert_eq!(serde_json::to_value(actual)?, expected);
    Ok(())
}

#[test]
fn roll_qualifies_two_originals_clamps_once_and_replays_one_durable_edit() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = ready(scratch.path(), 10..13)?;
    let before = store.snapshot()?;
    let database = Connection::open(path.join("project.sqlite"))?;
    let rows = stored(&database)?;
    let initial_counts = counts(&path)?;
    assert_eq!(before.duration()?.frames(), 6);
    // Two three-frame moments require at least one output frame on each side.
    // The farther source handles cannot enlarge that shared [-2, 2] interval.
    for (delta, applied, wrappers, side) in [
        (-100, -2, (Some("left-crop"), None), SourceRollSide::Left),
        (100, 2, (None, Some("right-crop")), SourceRollSide::Right),
    ] {
        let request = roll(&before, "rolled", "left", "right", delta, wrappers);
        let preview = store.preview_source_roll(&request)?;
        let resolution = &preview.resolution;
        assert_eq!(resolution.requested_delta_frames, delta);
        assert_eq!(resolution.applied_delta_frames, applied);
        assert_eq!(resolution.minimum_delta_frames, -2);
        assert_eq!(resolution.maximum_delta_frames, 2);
        let clamp = resolution.clamp.unwrap();
        assert_eq!(clamp.side, side);
        assert_eq!(clamp.reason, SourceTrimClamp::MinimumOutputDuration);
        assert_eq!(clamp.delta, ExactRatio::integer(applied));
        assert!(clamp.inclusive);
        assert_eq!(resolution.pair_output, range(0, 6));
        assert_eq!(resolution.seam_before, ProjectFrame(3));
        assert_eq!(resolution.seam_after, ProjectFrame(3 + applied));
        assert_eq!(resolution.left.output_after, range(0, 3 + applied));
        assert_eq!(resolution.right.output_after, range(3 + applied, 6));
        assert_eq!(resolution.left.asset, id("left"));
        assert_eq!(resolution.right.asset, id("right"));
        assert_ne!(
            resolution.left.qualification,
            resolution.right.qualification
        );
        assert_eq!(resolution.left.needs_wrapper, applied < 0);
        assert_eq!(resolution.right.needs_wrapper, applied > 0);
        assert_eq!(resolution.right.physical_prefix.frames(), (-applied).max(0));
        assert_eq!(preview.edit.as_ref().unwrap().duration_delta, 0);
        assert_eq!(stored(&database)?, rows);
    }
    let request: CommandRequest = serde_json::from_str(&serde_json::to_string(&roll(
        &before,
        "rolled",
        "left",
        "right",
        100,
        (None, Some("right-crop")),
    ))?)?;
    let preview = store.preview_source_roll(&request)?;
    let transaction = preview.edit.unwrap();
    assert_eq!(store.preview(&request)?, transaction);
    assert_eq!(store.commit(&request)?.edit, transaction);
    let after = store.snapshot()?;
    assert_eq!(after, transaction.forward.apply(&before)?);
    assert_eq!(transaction.inverse.apply(&after)?, before);
    assert_eq!(after.duration()?.frames(), 6);
    assert_eq!(after.assets(), before.assets());
    assert_eq!(
        after.nodes()[&node("left")].audio_editorial_edges,
        AudioEditorialEdges {
            start: false,
            end: true
        }
    );
    assert_eq!(
        after.nodes()[&node("right-crop")].audio_editorial_edges,
        AudioEditorialEdges {
            start: true,
            end: false
        }
    );
    assert!(
        after.nodes()[&node("right")]
            .audio_editorial_edges
            .is_empty()
    );
    assert_eq!(
        counts(&path)?,
        (initial_counts.0 + 1, initial_counts.1 + 1, initial_counts.2)
    );
    let (wire, patch): (String, String) = database.query_row(
        "SELECT request,edit FROM history WHERE revision_id='rolled'",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    assert_eq!(serde_json::from_str::<CommandRequest>(&wire)?, request);
    assert_eq!(
        serde_json::from_str::<EditTransaction>(&patch)?,
        transaction
    );
    store.validate()?;
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, after);
    store.undo(after.revision_id(), revision("undo-roll"))?;
    let undone = store.snapshot()?;
    same_content(&undone, &before)?;
    assert_ne!(undone.revision_id(), before.revision_id());
    assert!(store.preview_source_roll(&request).is_err());
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    store.redo(undone.revision_id(), revision("redo-roll"))?;
    let redone = store.snapshot()?;
    same_content(&redone, &after)?;
    assert_ne!(redone.revision_id(), after.revision_id());
    assert_ne!(redone.revision_id(), undone.revision_id());
    assert_eq!(store.snapshot_at(before.revision_id())?, before);
    assert_eq!(store.snapshot_at(after.revision_id())?, after);
    for asset in ["left", "right"] {
        assert_eq!(
            store
                .registered_source(redone.revision_id(), &id(asset))?
                .id(),
            before.assets()[&id(asset)]
                .source_qualification
                .as_ref()
                .unwrap()
        );
    }
    assert_eq!(
        counts(&path)?,
        (initial_counts.0 + 3, initial_counts.1 + 1, initial_counts.2)
    );
    store.validate()?;
    Ok(())
}

#[test]
fn zero_roll_preview_checks_metadata_and_never_reserves_identity_or_history() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = ready(scratch.path(), 10..11)?;
    let before = store.snapshot()?;
    let database = Connection::open(path.join("project.sqlite"))?;
    let rows = stored(&database)?;
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    for delta in [0, 100] {
        let request = roll(&before, "unused", "left", "right", delta, (None, None));
        let preview = reader.preview_source_roll(&request)?;
        assert_eq!(preview.resolution.applied_delta_frames, 0);
        assert_eq!(preview.resolution.maximum_delta_frames, 0);
        assert_eq!(
            preview.resolution.left.before,
            preview.resolution.left.after
        );
        assert_eq!(
            preview.resolution.right.before,
            preview.resolution.right.after
        );
        assert!(!preview.resolution.left.needs_wrapper);
        assert!(!preview.resolution.right.needs_wrapper);
        assert_eq!(
            preview.resolution.clamp,
            (delta != 0).then_some(preview.resolution.maximum_delta)
        );
        assert!(preview.edit.is_none());
        for result in [
            store.preview(&request),
            store.commit(&request).map(|outcome| outcome.edit),
        ] {
            assert!(matches!(result, Err(StoreError::Edit(error))
                if error.code == EditErrorCode::InvalidCommand
                && error.message == "Source roll resolves to no change"));
        }
        for side in [true, false] {
            let wrappers = if side {
                (Some("unexpected"), None)
            } else {
                (None, Some("unexpected"))
            };
            assert!(
                reader
                    .preview_source_roll(&roll(&before, "unused", "left", "right", delta, wrappers))
                    .is_err()
            );
        }
        let mut wrong_timing = request;
        let Command::RollSources { timing, .. } = &mut wrong_timing.command else {
            unreachable!()
        };
        timing.allocation = revision("different");
        assert!(reader.preview_source_roll(&wrong_timing).is_err());
        assert_eq!(stored(&database)?, rows);
    }
    // A preview did not reserve the requested revision or wrapper ID.
    let nonzero = roll(
        &before,
        "unused",
        "left",
        "right",
        -1,
        (Some("unexpected"), None),
    );
    let preview = reader.preview_source_roll(&nonzero)?.edit.unwrap();
    let mut reader = reader;
    assert!(matches!(reader.commit(&nonzero), Err(StoreError::ReadOnly)));
    assert_eq!(store.commit(&nonzero)?.edit, preview);
    assert_eq!(store.snapshot()?.revision_id(), &revision("unused"));
    store.validate()?;
    Ok(())
}

#[test]
fn roll_rejects_wrong_resources_reused_revisions_and_a_stale_captured_pair() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = ready(scratch.path(), 10..13)?;
    let before = store.snapshot()?;
    let database = Connection::open(path.join("project.sqlite"))?;
    let rows = stored(&database)?;
    for wrappers in [
        (None, None),
        (Some("wrong-side"), None),
        (None, Some("root")),
        (Some("extra"), Some("right-crop")),
        (Some("same"), Some("same")),
    ] {
        let request = roll(&before, "rejected", "left", "right", 1, wrappers);
        assert!(store.preview_source_roll(&request).is_err());
        assert!(store.preview(&request).is_err());
        assert!(store.commit(&request).is_err());
        assert_eq!(stored(&database)?, rows);
    }
    for delta in [0, 1] {
        for case in [
            "project", "stale", "current", "used", "timing", "reversed", "missing",
        ] {
            let mut request = roll(
                &before,
                "rejected",
                "left",
                "right",
                delta,
                (None, (delta != 0).then_some("right-crop")),
            );
            match case {
                "project" => request.project_id = ProjectId::new("other")?,
                "stale" => request.expected_revision = revision("right-registered"),
                "current" => request.new_revision = before.revision_id().clone(),
                "used" => {
                    request.new_revision = revision("right-registered");
                    let Command::RollSources { timing, .. } = &mut request.command else {
                        unreachable!()
                    };
                    timing.allocation = request.new_revision.clone();
                }
                "timing" => {
                    let Command::RollSources { timing, .. } = &mut request.command else {
                        unreachable!()
                    };
                    timing.allocation = revision("wrong");
                }
                "reversed" => {
                    let Command::RollSources { left, right, .. } = &mut request.command else {
                        unreachable!()
                    };
                    std::mem::swap(left, right);
                }
                "missing" => {
                    let Command::RollSources { right, .. } = &mut request.command else {
                        unreachable!()
                    };
                    *right = node("absent");
                }
                _ => unreachable!(),
            }
            let error = store.preview_source_roll(&request).unwrap_err();
            if let Some(code) = match case {
                "project" => Some("ProjectConflict"),
                "stale" => Some("RevisionConflict"),
                "current" => Some("InvalidCommand"),
                "used" => Some("RevisionReused"),
                _ => None,
            } {
                assert_eq!(error.code(), code, "{case}, delta {delta}");
            }
            assert!(store.commit(&request).is_err(), "{case}, delta {delta}");
            assert_eq!(stored(&database)?, rows, "{case}, delta {delta}");
        }
    }
    let late = roll(
        &before,
        "late",
        "left",
        "right",
        1,
        (None, Some("right-crop")),
    );
    assert!(store.preview_source_roll(&late)?.edit.is_some());
    // Even an empty structural child breaks literal adjacency. Retaining the
    // old pair IDs must not discover and author a replacement pair later.
    store.commit(&edit(
        &before,
        "separated",
        Command::Insert {
            parent: node("root"),
            index: 1,
            subtree: Subtree {
                root: node("empty"),
                nodes: BTreeMap::from([(node("empty"), BeatNode::sequence("Empty", vec![]))]),
                overrides: Default::default(),
                gap_overrides: Default::default(),
            },
        },
    ))?;
    let separated = store.snapshot()?;
    let rows = stored(&database)?;
    assert_eq!(
        store.preview_source_roll(&late).unwrap_err().code(),
        "RevisionConflict"
    );
    assert_eq!(store.commit(&late).unwrap_err().code(), "RevisionConflict");
    let fresh = roll(
        &separated,
        "late",
        "left",
        "right",
        1,
        (None, Some("right-crop")),
    );
    assert!(store.preview_source_roll(&fresh).is_err());
    assert!(store.commit(&fresh).is_err());
    assert_eq!(stored(&database)?, rows);
    store.validate()?;
    Ok(())
}

#[test]
fn roll_rechecks_each_receipt_and_original_with_the_other_side_valid() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = ready(scratch.path(), 10..13)?;
    let before = store.snapshot()?;
    let database = Connection::open(path.join("project.sqlite"))?;
    let clean_rows = stored(&database)?;
    for (asset, other) in [("left", "right"), ("right", "left")] {
        let qualification = before.assets()[&id(asset)]
            .source_qualification
            .as_ref()
            .unwrap();
        let (content, original_ref, receipt): (String, String, Vec<u8>) = database.query_row(
            "SELECT original_content_id,original_ref,snapshot FROM source_qualifications WHERE id=?1",
            [qualification.as_str()], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        let ownership: String = database.query_row(
            "SELECT record FROM original_media WHERE content_id=?1",
            [&content],
            |row| row.get(0),
        )?;
        let mut wrong_owner: serde_json::Value = serde_json::from_str(&ownership)?;
        wrong_owner["sha256"] = serde_json::to_value([0_u8; 32])?;
        let other_receipt = store.registered_source(before.revision_id(), &id(other))?;
        let other_file = path.join("Media/Originals").join(format!(
            "blake3-{}",
            other_receipt.original().content().digest()
        ));
        for case in ["missing", "receipt", "ownership", "asset"] {
            match case {
                "missing" => {
                    database.execute(
                        "DELETE FROM source_qualifications WHERE id=?1",
                        [qualification.as_str()],
                    )?;
                }
                "receipt" => {
                    database.execute(
                        "UPDATE source_qualifications SET snapshot=X'00' WHERE id=?1",
                        [qualification.as_str()],
                    )?;
                }
                "ownership" => {
                    database.execute(
                        "UPDATE original_media SET record=?1 WHERE content_id=?2",
                        rusqlite::params![wrong_owner.to_string(), content],
                    )?;
                }
                "asset" => {
                    let mut document = serde_json::to_value(&before)?;
                    document["assets"][asset]["content_hash"] = serde_json::json!("a".repeat(64));
                    database.execute(
                        "UPDATE revisions SET document=?1 WHERE id='ready'",
                        [document.to_string()],
                    )?;
                }
                _ => unreachable!(),
            }
            let rows = stored(&database)?;
            assert_eq!(
                store.registered_source(before.revision_id(), &id(other))?,
                other_receipt
            );
            assert!(
                other_file.is_file(),
                "unaffected retained Original stays present"
            );
            for delta in [0, 1] {
                let request = roll(
                    &before,
                    "rejected",
                    "left",
                    "right",
                    delta,
                    (None, (delta != 0).then_some("right-crop")),
                );
                assert!(
                    matches!(
                        store.preview_source_roll(&request),
                        Err(StoreError::SourceRegistration(_))
                    ),
                    "{asset} {case}, delta {delta}"
                );
                if delta != 0 {
                    assert!(
                        matches!(
                            store.preview(&request),
                            Err(StoreError::SourceRegistration(_))
                        ),
                        "{asset} {case}"
                    );
                    assert!(
                        matches!(
                            store.commit(&request),
                            Err(StoreError::SourceRegistration(_))
                        ),
                        "{asset} {case}"
                    );
                }
                assert_eq!(stored(&database)?, rows, "{asset} {case}");
            }
            database.execute("INSERT OR REPLACE INTO source_qualifications(id,original_content_id,original_ref,snapshot) VALUES(?1,?2,?3,?4)",
                rusqlite::params![qualification.as_str(), content, original_ref, receipt])?;
            database.execute(
                "UPDATE original_media SET record=?1 WHERE content_id=?2",
                rusqlite::params![ownership, content],
            )?;
            database.execute(
                "UPDATE revisions SET document=?1 WHERE id='ready'",
                [before.to_json()?],
            )?;
            assert_eq!(stored(&database)?, clean_rows);
        }
    }
    assert_eq!(store.snapshot()?, before);
    store.validate()?;
    Ok(())
}

#[test]
fn roll_failure_after_history_write_restores_both_sources_and_existing_redo() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = ready(scratch.path(), 10..13)?;
    let initial = store.snapshot()?;
    store.commit(&edit(
        &initial,
        "renamed",
        Command::Rename {
            node: node("right"),
            label: "Temporary label".into(),
        },
    ))?;
    store.undo(&revision("renamed"), revision("undo-rename"))?;
    let before = store.snapshot()?;
    let request = roll(
        &before,
        "rolled",
        "left",
        "right",
        -1,
        (Some("left-crop"), None),
    );
    let database = Connection::open(path.join("project.sqlite"))?;
    assert_eq!(
        database.query_row("SELECT count(*) FROM redo", [], |row| row.get::<_, i64>(0))?,
        1
    );
    let rows = stored(&database)?;
    let preview = store.preview_source_roll(&request)?.edit.unwrap();
    database.execute_batch(
        "CREATE TRIGGER fail_roll_cursor BEFORE UPDATE OF head_revision,cursor ON state
         WHEN NEW.head_revision='rolled' AND
              EXISTS(SELECT 1 FROM revisions WHERE id='rolled') AND
              EXISTS(SELECT 1 FROM history WHERE revision_id='rolled')
         BEGIN SELECT RAISE(ABORT, 'injected roll commit failure'); END;",
    )?;
    assert!(matches!(store.commit(&request),
        Err(StoreError::Database(rusqlite::Error::SqliteFailure(_, Some(message))))
        if message == "injected roll commit failure"));
    assert_eq!(stored(&database)?, rows);
    assert_eq!(store.snapshot()?, before);
    database.execute_batch("DROP TRIGGER fail_roll_cursor")?;
    // The exact failed command, including allocation and wrapper identities,
    // remains usable once the transient write fault is removed.
    assert_eq!(store.commit(&request)?.edit, preview);
    assert_eq!(store.snapshot()?, preview.forward.apply(&before)?);
    assert_eq!(
        database.query_row("SELECT count(*) FROM redo", [], |row| row.get::<_, i64>(0))?,
        0
    );
    store.validate()?;
    Ok(())
}
