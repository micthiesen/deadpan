//! Synthetic structural fixtures exercise atomic history, not media validity.
use deadpan_core::*;
use deadpan_store::{AccessMode, DATABASE_SCHEMA_VERSION, ProjectStore};
use rusqlite::Connection;
use std::{collections::BTreeMap, error::Error, path::Path};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;
fn node(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}
fn revision(name: &str) -> RevisionId {
    RevisionId::new(name).unwrap()
}
fn hold(duration: i64) -> BeatNode {
    BeatNode::hold(
        "Synthetic pause",
        HoldRecipe {
            picture_context: None,
            duration: FrameDuration::new(duration).unwrap(),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    )
}

fn document() -> Result<ProjectDocument> {
    let empty = ProjectDocument::new(
        ProjectId::new("range-delete-history")?,
        revision("baseline"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1001)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )?;
    let mut wire = serde_json::to_value(empty)?;
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (
            node("root"),
            BeatNode::sequence("Root", vec![node("legacy"), node("group"), node("tail")]),
        ),
        (node("legacy"), hold(2)),
        (
            node("group"),
            BeatNode::sequence("Group", vec![node("voice")]),
        ),
        (node("voice"), hold(7)),
        (node("tail"), hold(3)),
    ]))?;
    Ok(ProjectDocument::from_json(&wire.to_string())?)
}

fn request(document: &ProjectDocument, name: &str, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(name),
        command,
    }
}

fn range_request(document: &ProjectDocument, name: &str) -> Result<CommandRequest> {
    let range = FrameRange::new(ProjectFrame(1), ProjectFrame(3))?;
    let target = document.range_deletion(&node("group"), range)?;
    assert!(target.required_ids > 0);
    Ok(request(
        document,
        name,
        Command::DeleteRange {
            parent: node("group"),
            range,
            identities: SplitIdentities {
                nodes: (0..target.required_ids)
                    .map(|i| node(&format!("{name}-split-{i}")))
                    .collect(),
            },
            timing: AudioTimingId {
                allocation: revision(name),
                ordinal: 0,
            },
        },
    ))
}

fn authored(path: &Path) -> Result<Vec<String>> {
    let database = Connection::open(path.join("project.sqlite"))?;
    let mut rows = Vec::new();
    for sql in [
        "SELECT json_array(id,parent_id,kind,document) FROM revisions ORDER BY id",
        "SELECT json_array(id,parent_id,revision_id,request,edit) FROM history ORDER BY id",
        "SELECT json_array(singleton,head_revision,cursor,workflow) FROM state",
        "SELECT json_array(position,history_id) FROM redo ORDER BY position",
    ] {
        rows.extend(
            database
                .prepare(sql)?
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?,
        );
    }
    Ok(rows)
}

fn assert_authored(actual: &ProjectDocument, expected: &ProjectDocument) -> Result {
    let mut value = serde_json::to_value(expected)?;
    value["revision_id"] = serde_json::to_value(actual.revision_id())?;
    assert_eq!(serde_json::to_value(actual)?, value);
    Ok(())
}

#[test]
fn range_delete_previews_once_and_replays_mixed_history_after_reopen() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("range.deadpan");
    let baseline = document()?;
    let mut store = ProjectStore::create(&path, &baseline)?;
    let legacy = request(
        &baseline,
        "legacy-delete",
        Command::Delete {
            node: node("legacy"),
        },
    );
    store.commit(&legacy)?;
    let before = store.snapshot()?;
    assert_eq!(before.duration()?.frames(), 10);
    assert!(before.audio_bindings().is_empty());
    let removal = range_request(&before, "range-delete")?;
    let stale = range_request(&before, "stale-delete")?;
    let cells = authored(&path)?;
    // Preview also works through a read-only open while the writer is held.
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    let preview = reader.preview(&removal)?;
    assert_eq!(preview.duration_delta, -2);
    assert_eq!(authored(&path)?, cells);
    drop(reader);
    let outcome = store.commit(&removal)?;
    assert_eq!(outcome.edit, preview);
    let after = store.snapshot()?;
    assert_eq!(after.duration()?.frames(), 8);
    let durations: Vec<_> = after
        .children(&node("group"))
        .map(|id| after.node_duration(id).unwrap().frames())
        .collect();
    assert_eq!(durations, vec![1, 4]);
    assert!(!after.audio_bindings().is_empty());
    let committed = authored(&path)?;
    assert!(store.commit(&stale).is_err());
    assert_eq!(authored(&path)?, committed);
    store.validate()?;
    drop(store);

    let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert_eq!(reader.snapshot()?, after);
    reader.validate()?;
    drop(reader);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    store.undo(after.revision_id(), revision("undo-range"))?;
    let undone = store.snapshot()?;
    assert_authored(&undone, &before)?;
    let cells = authored(&path)?;
    assert!(store.commit(&stale).is_err());
    assert_eq!(authored(&path)?, cells);
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, undone);
    store.undo(undone.revision_id(), revision("undo-legacy"))?;
    assert_authored(&store.snapshot()?, &baseline)?;
    store.redo(&revision("undo-legacy"), revision("redo-legacy"))?;
    assert_authored(&store.snapshot()?, &before)?;
    store.redo(&revision("redo-legacy"), revision("redo-range"))?;
    let redone = store.snapshot()?;
    assert_authored(&redone, &after)?;
    assert_eq!(store.history_availability()?, (true, false));
    store.validate()?;
    drop(store);
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert_eq!(reader.snapshot()?, redone);
    reader.validate()?;
    let database = Connection::open(path.join("project.sqlite"))?;
    assert_eq!(DATABASE_SCHEMA_VERSION, 55);
    assert_eq!(
        database.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
        55
    );
    assert_eq!(
        database.query_row("SELECT count(*) FROM revisions", [], |row| row
            .get::<_, i64>(0))?,
        7
    );
    let stored = database
        .prepare("SELECT request FROM history ORDER BY id")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let stored: Vec<CommandRequest> = stored
        .iter()
        .map(|body| serde_json::from_str(body))
        .collect::<std::result::Result<_, _>>()?;
    assert_eq!(stored, vec![legacy, removal]);
    Ok(())
}

#[test]
fn invalid_range_split_ids_and_timing_fail_without_partial_history() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("invalid.deadpan");
    let initial = document()?;
    let mut store = ProjectStore::create(&path, &initial)?;
    store.commit(&request(
        &initial,
        "legacy",
        Command::Delete {
            node: node("legacy"),
        },
    ))?;
    let before = store.snapshot()?;
    let cells = authored(&path)?;
    for case in 0..3 {
        let mut invalid = range_request(&before, &format!("invalid-{case}"))?;
        let Command::DeleteRange {
            identities, timing, ..
        } = &mut invalid.command
        else {
            unreachable!()
        };
        match case {
            0 => identities.nodes.clear(),
            1 => identities.nodes[0] = node("tail"),
            _ => timing.ordinal = u32::MAX,
        }
        assert!(store.preview(&invalid).is_err());
        assert!(store.commit(&invalid).is_err());
        assert_eq!(store.snapshot()?, before);
        assert_eq!(authored(&path)?, cells);
    }
    store.validate()?;
    Ok(())
}
