use super::*;

use std::{collections::BTreeMap, ops::Range};

use deadpan_core::{
    AudioSample, BeatNode, EditErrorCode, EditTransaction, ExactFrameRange, ExactRatio,
    SourceAudioMapping, SourceEditWindow, SourceNode, SourceSlipClamp, SourceVideo, Subtree,
};
use deadpan_media::source_import_timing::derive_source_moment;

fn node(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}
fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}
fn edit(current: &ProjectDocument, next: &str, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: current.project_id().clone(),
        expected_revision: current.revision_id().clone(),
        new_revision: revision(next),
        command,
    }
}
fn slip(current: &ProjectDocument, next: &str, delta_frames: i64) -> CommandRequest {
    edit(
        current,
        next,
        Command::SlipSource {
            parent: node("root"),
            node: node("clip"),
            delta_frames,
        },
    )
}
fn source(document: &ProjectDocument) -> &SourceNode {
    let NodeKind::Source { source } = &document.nodes()[&node("clip")].kind else {
        panic!("expected retained Source")
    };
    source
}

fn ready(parent: &Path, ordinals: Range<u64>) -> Result<(PathBuf, ProjectStore)> {
    let (path, mut store) = project(parent)?;
    let original = retain(&mut store, "offset-bframes.mp4")?;
    let decoded = decode(&store, &original)?;
    let input = request(&store, &original, "registered", "camera", None)?;
    store.register_source(&input, &decoded, None, limits(), &active())?;
    let current = store.snapshot()?;
    let receipt = store.registered_source(current.revision_id(), &id("camera"))?;
    let timing = derive_source_moment(
        receipt.snapshot().video().unwrap().index(),
        receipt.snapshot().audio(),
        ordinals,
        current.presentation_basis().frame_rate,
    )?;
    let mut source = timing.source_node(id("camera"));
    source.audio_offset = AudioSample(17);
    // At 30000/1001 fps the independent 17-sample offset is exactly 85/8008
    // frames. Full measured audio covers this selected moment, so retain W in
    // effective output coordinates and subtract the residual once in the map.
    let offset = ratio(85, 8008);
    let window = source.edit_window.unwrap();
    let SourceAudioMapping::SelectedPlacement { start, frames, .. } = source.audio_mapping else {
        panic!("moment must retain measured audio context")
    };
    source.audio_mapping = SourceAudioMapping::SelectedPlacement {
        start,
        frames,
        selection: ExactFrameRange::new(
            window.start().checked_sub(offset)?,
            window.end().checked_sub(offset)?,
        )?,
    };
    store.commit(&edit(
        &current,
        "ready",
        Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: Subtree {
                root: node("clip"),
                nodes: BTreeMap::from([(
                    node("clip"),
                    BeatNode {
                        label: "Selected Original".into(),
                        framing: None,
                        audio_treatments: Default::default(),
                        audio_editorial_edges: Default::default(),
                        audio_edges: Default::default(),
                        kind: NodeKind::Source { source },
                        cutaways: Vec::new(),
                        captions: Vec::new(),
                    },
                )]),
                overrides: Default::default(),
                gap_overrides: Default::default(),
            },
        },
    ))?;
    Ok((path, store))
}

// Retain exact authored rows, cursor/redo and admission evidence. Failure tests
// must neither change an edit nor repair the deliberately invalid evidence.
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
fn linked_slip_commits_one_exact_edit_and_survives_fresh_history_and_reopen() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = ready(scratch.path(), 10..13)?;
    let before = store.snapshot()?;
    let database = Connection::open(path.join("project.sqlite"))?;
    let rows = stored(&database)?;
    let before_counts = counts(&path)?;
    // Use the serialized shared command ingress, not a store-specific write.
    let request: CommandRequest =
        serde_json::from_str(&serde_json::to_string(&slip(&before, "slipped", 2))?)?;
    let preview = store.preview_source_slip(&request)?;
    assert_eq!(preview.resolution.requested_delta_frames, 2);
    assert_eq!(preview.resolution.applied_delta_frames, 2);
    assert_eq!(preview.resolution.minimum_delta, ratio(-10, 1));
    assert_eq!(preview.resolution.maximum_delta, ratio(107, 1));
    assert_eq!(preview.resolution.minimum_delta_frames, -10);
    assert_eq!(preview.resolution.maximum_delta_frames, 107);
    assert_eq!(preview.resolution.clamp, None);
    assert_eq!(preview.resolution.asset, id("camera"));
    let transaction = preview.edit.unwrap();
    assert_eq!(store.preview(&request)?, transaction);
    assert_eq!(stored(&database)?, rows);
    let outcome = store.commit(&request)?;
    assert_eq!(outcome.edit, transaction);
    let after = store.snapshot()?;
    assert_eq!(after, transaction.forward.apply(&before)?);
    assert_eq!(transaction.inverse.apply(&after)?, before);
    assert_eq!(after.duration()?, before.duration()?);
    assert_eq!(after.assets(), before.assets());
    assert_eq!(after.audio_bindings(), before.audio_bindings());
    assert_eq!(source(&after).duration, source(&before).duration);
    assert_eq!(source(&after).video, source(&before).video);
    assert_eq!(source(&after).audio, source(&before).audio);
    assert_eq!(
        source(&after).edit_window,
        Some(SourceEditWindow::new(ratio(0, 1), ratio(3, 1))?)
    );
    assert_eq!(source(&after).audio_offset, AudioSample(17));
    assert_eq!(source(&after).video_mapping.start_frames(), ratio(-12, 1));
    assert_eq!(
        source(&after).audio_mapping,
        SourceAudioMapping::SelectedPlacement {
            start: ratio(-12652, 1001),
            frames: ratio(120760, 1001),
            selection: ExactFrameRange::new(ratio(-85, 8008), ratio(23939, 8008))?,
        }
    );
    let SourceVideo::Stream { span, .. } = source(&after).video else {
        panic!()
    };
    let selected = source(&after)
        .video_mapping
        .selection_in_source(span, source(&after).duration)?;
    assert_eq!(selected.start().ticks, ratio(72072, 1));
    assert_eq!(selected.end().ticks, ratio(75075, 1));
    assert_eq!(
        counts(&path)?,
        (before_counts.0 + 1, before_counts.1 + 1, before_counts.2)
    );
    let (wire, patch): (String, String) = database.query_row(
        "SELECT request,edit FROM history WHERE revision_id='slipped'",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    assert_eq!(serde_json::from_str::<CommandRequest>(&wire)?, request);
    assert_eq!(
        serde_json::from_str::<EditTransaction>(&patch)?,
        transaction
    );
    let receipt = store.registered_source(after.revision_id(), &id("camera"))?;
    store.validate()?;
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, after);
    store.undo(after.revision_id(), revision("undo-slip"))?;
    let undone = store.snapshot()?;
    same_content(&undone, &before)?;
    assert_ne!(undone.revision_id(), before.revision_id());
    assert!(store.preview_source_slip(&request).is_err());
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    store.redo(undone.revision_id(), revision("redo-slip"))?;
    let redone = store.snapshot()?;
    same_content(&redone, &after)?;
    assert_ne!(redone.revision_id(), after.revision_id());
    assert_ne!(redone.revision_id(), undone.revision_id());
    assert_eq!(store.snapshot_at(before.revision_id())?, before);
    assert_eq!(store.snapshot_at(after.revision_id())?, after);
    assert_eq!(
        store.registered_source(redone.revision_id(), &id("camera"))?,
        receipt
    );
    store.validate()?;
    Ok(())
}

#[test]
fn zero_slip_preview_keeps_its_handle_reason_without_reserving_history() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = ready(scratch.path(), 0..3)?;
    let before = store.snapshot()?;
    let database = Connection::open(path.join("project.sqlite"))?;
    let rows = stored(&database)?;
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    for (delta, clamp) in [(0, None), (-1, Some(SourceSlipClamp::PictureStart))] {
        let request = slip(&before, "unused", delta);
        let preview = reader.preview_source_slip(&request)?;
        assert_eq!(preview.resolution.applied_delta_frames, 0);
        assert_eq!(preview.resolution.minimum_delta_frames, 0);
        assert_eq!(preview.resolution.clamp, clamp);
        assert_eq!(preview.resolution.before, preview.resolution.after);
        assert!(preview.edit.is_none());
        for result in [
            store.preview(&request),
            store.commit(&request).map(|outcome| outcome.edit),
        ] {
            assert!(matches!(result, Err(StoreError::Edit(error))
                if error.code == EditErrorCode::InvalidCommand
                && error.message == "source slip resolves to no change"));
        }
        assert_eq!(stored(&database)?, rows);
    }
    let positive = slip(&before, "unused", 1);
    assert!(reader.preview_source_slip(&positive)?.edit.is_some());
    let mut reader = reader;
    assert!(matches!(
        reader.commit(&positive),
        Err(StoreError::ReadOnly)
    ));
    store.commit(&positive)?;
    assert_eq!(store.snapshot()?.revision_id(), &revision("unused"));
    assert_eq!(counts(&path)?, (4, 3, 1));
    Ok(())
}

#[test]
fn slip_rechecks_receipts_assets_and_ownership_even_for_zero_preview() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = ready(scratch.path(), 10..13)?;
    let before = store.snapshot()?;
    let database = Connection::open(path.join("project.sqlite"))?;
    let (qualification, content, original_ref, receipt): (String, String, String, Vec<u8>) =
        database.query_row(
            "SELECT id,original_content_id,original_ref,snapshot FROM source_qualifications",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
    let ownership: String =
        database.query_row("SELECT record FROM original_media", [], |row| row.get(0))?;
    let mut wrong_owner: serde_json::Value = serde_json::from_str(&ownership)?;
    wrong_owner["sha256"] = serde_json::to_value([0_u8; 32])?;
    for case in ["missing", "receipt", "ownership", "asset"] {
        match case {
            "missing" => {
                database.execute("DELETE FROM source_qualifications", [])?;
            }
            "receipt" => {
                database.execute("UPDATE source_qualifications SET snapshot=X'00'", [])?;
            }
            "ownership" => {
                database.execute(
                    "UPDATE original_media SET record=?1",
                    [wrong_owner.to_string()],
                )?;
            }
            "asset" => {
                let mut document = serde_json::to_value(&before)?;
                document["assets"]["camera"]["content_hash"] = serde_json::json!("a".repeat(64));
                database.execute(
                    "UPDATE revisions SET document=?1 WHERE id='ready'",
                    [document.to_string()],
                )?;
            }
            _ => unreachable!(),
        }
        let rows = stored(&database)?;
        for delta in [0, 2] {
            let request = slip(&before, "rejected", delta);
            assert!(
                matches!(
                    store.preview_source_slip(&request),
                    Err(StoreError::SourceRegistration(_))
                ),
                "{case}, delta {delta}"
            );
            if delta != 0 {
                assert!(
                    matches!(
                        store.preview(&request),
                        Err(StoreError::SourceRegistration(_))
                    ),
                    "{case}"
                );
                assert!(
                    matches!(
                        store.commit(&request),
                        Err(StoreError::SourceRegistration(_))
                    ),
                    "{case}"
                );
            }
            assert_eq!(stored(&database)?, rows, "{case}");
        }
        database.execute("INSERT OR REPLACE INTO source_qualifications(id,original_content_id,original_ref,snapshot) VALUES(?1,?2,?3,?4)", rusqlite::params![qualification,content,original_ref,receipt])?;
        database.execute("UPDATE original_media SET record=?1", [&ownership])?;
        database.execute(
            "UPDATE revisions SET document=?1 WHERE id='ready'",
            [before.to_json()?],
        )?;
    }
    assert_eq!(store.snapshot()?, before);
    store.validate()?;
    Ok(())
}

#[test]
fn slip_revision_guards_cover_preview_noop_and_late_commit() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = ready(scratch.path(), 10..13)?;
    let before = store.snapshot()?;
    let database = Connection::open(path.join("project.sqlite"))?;
    for delta in [0, 2] {
        for case in ["project", "stale", "current", "used"] {
            let mut request = slip(&before, "rejected", delta);
            match case {
                "project" => request.project_id = ProjectId::new("other")?,
                "stale" => request.expected_revision = revision("registered"),
                "current" => request.new_revision = before.revision_id().clone(),
                "used" => request.new_revision = revision("registered"),
                _ => unreachable!(),
            }
            let rows = stored(&database)?;
            let code = match case {
                "project" => "ProjectConflict",
                "stale" => "RevisionConflict",
                "current" => "InvalidCommand",
                "used" => "RevisionReused",
                _ => unreachable!(),
            };
            assert_eq!(
                store.preview_source_slip(&request).unwrap_err().code(),
                code,
                "{case}"
            );
            if case != "used" {
                assert_eq!(
                    deadpan_core::apply(&before, &request)
                        .unwrap_err()
                        .code
                        .as_str(),
                    code
                );
            }
            assert!(store.commit(&request).is_err(), "{case}");
            assert_eq!(stored(&database)?, rows, "{case}");
        }
    }
    let late = slip(&before, "late", 2);
    assert!(store.preview_source_slip(&late)?.edit.is_some());
    store.commit(&edit(
        &before,
        "renamed",
        Command::Rename {
            node: node("clip"),
            label: "Changed target revision".into(),
        },
    ))?;
    let rows = stored(&database)?;
    assert!(
        matches!(store.preview_source_slip(&late), Err(StoreError::Edit(error)) if error.code == EditErrorCode::RevisionConflict)
    );
    assert!(
        matches!(store.commit(&late), Err(StoreError::Edit(error)) if error.code == EditErrorCode::RevisionConflict)
    );
    assert_eq!(stored(&database)?, rows);
    Ok(())
}

#[test]
fn slip_rolls_back_the_complete_write_and_does_not_require_current_media_bytes() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = ready(scratch.path(), 10..13)?;
    let before = store.snapshot()?;
    let request = slip(&before, "slipped", 2);
    let database = Connection::open(path.join("project.sqlite"))?;
    let rows = stored(&database)?;
    database.execute_batch(
        "CREATE TRIGGER fail_slip_cursor BEFORE UPDATE OF head_revision,cursor ON state
        WHEN NEW.head_revision='slipped' AND
             EXISTS(SELECT 1 FROM revisions WHERE id='slipped') AND
             EXISTS(SELECT 1 FROM history WHERE revision_id='slipped')
        BEGIN SELECT RAISE(ABORT, 'injected slip commit failure'); END;",
    )?;
    assert!(matches!(store.commit(&request),
        Err(StoreError::Database(rusqlite::Error::SqliteFailure(_, Some(message))))
        if message == "injected slip commit failure"));
    assert_eq!(stored(&database)?, rows);
    database.execute_batch("DROP TRIGGER fail_slip_cursor")?;

    // Remove only this test package's private managed copy. Stored qualification
    // still authorizes editing; rendering retains its separate byte checks.
    let receipt = store.registered_source(before.revision_id(), &id("camera"))?;
    fs::remove_file(
        path.join("Media/Originals")
            .join(format!("blake3-{}", receipt.original().content().digest())),
    )?;
    let preview = store.preview_source_slip(&request)?.edit.unwrap();
    assert_eq!(store.commit(&request)?.edit, preview);
    assert_eq!(store.snapshot()?, preview.forward.apply(&before)?);
    store.validate()?;
    Ok(())
}
