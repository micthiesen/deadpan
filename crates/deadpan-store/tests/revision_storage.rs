//! Revisions are stored as keyframe snapshots plus forward patches, and pause
//! history records only the retained timing state it changes.

use std::collections::BTreeMap;
use std::error::Error;

use deadpan_core::{
    AudioTimingId, BeatNode, ColorPolicy, Command, CommandRequest, FrameDuration, FrameRate,
    HoldAudio, HoldRecipe, HoldVideo, NodeId, PresentationBasis, ProjectDocument, ProjectFrame,
    ProjectId, RevisionId, SplitIdentities, Subtree,
};
use deadpan_store::{AccessMode, ProjectStore};
use rusqlite::Connection;

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn hold(frames: i64) -> Result<HoldRecipe> {
    Ok(HoldRecipe {
        picture_context: None,
        duration: FrameDuration::new(frames)?,
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
    })
}

fn document() -> Result<ProjectDocument> {
    Ok(ProjectDocument::new(
        ProjectId::new("revision-storage")?,
        RevisionId::new("r0")?,
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1_001)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?)
}

fn request(document: &ProjectDocument, revision: &str, command: Command) -> Result<CommandRequest> {
    Ok(CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new(revision)?,
        command,
    })
}

fn beats(count: usize) -> Result<Command> {
    let group = NodeId::new("beats")?;
    let mut nodes = BTreeMap::new();
    let mut children = Vec::new();
    for index in 0..count {
        let id = NodeId::new(format!("b{index:04}"))?;
        nodes.insert(id.clone(), BeatNode::hold("Beat", hold(3)?));
        children.push(id);
    }
    nodes.insert(group.clone(), BeatNode::sequence("Beats", children));
    Ok(Command::Insert {
        parent: NodeId::new("root")?,
        index: 0,
        subtree: Subtree {
            root: group,
            nodes,
            overrides: Default::default(),
            gap_overrides: Default::default(),
        },
    })
}

fn pause(document: &ProjectDocument, revision: &str, at: i64) -> Result<CommandRequest> {
    let target = document.insert_time_target(ProjectFrame(at))?;
    let needed = target.split.map_or(0, |split| split.required_ids);
    request(
        document,
        revision,
        Command::InsertTime {
            at: ProjectFrame(at),
            hold: hold(2)?,
            id: NodeId::new(format!("{revision}-pause"))?,
            identities: SplitIdentities {
                nodes: (0..needed)
                    .map(|index| NodeId::new(format!("{revision}-split-{index}")))
                    .collect::<std::result::Result<_, _>>()?,
            },
            timing: AudioTimingId {
                allocation: RevisionId::new(revision)?,
                ordinal: 0,
            },
        },
    )
}

fn stored(path: &std::path::Path) -> Result<Vec<(String, String)>> {
    let database = Connection::open(path.join("project.sqlite"))?;
    let rows = database
        .prepare("SELECT id, document FROM revisions ORDER BY rowid")?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?;
    Ok(rows)
}

#[test]
fn elided_revisions_rebuild_exactly_through_edits_undo_redo_and_reopen() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("chain.deadpan");
    let mut store = ProjectStore::create(&path, &document()?)?;
    let mut expected = vec![store.snapshot()?];
    let current = store.snapshot()?;
    store.commit(&request(&current, "r1", beats(8)?)?)?;
    expected.push(store.snapshot()?);
    // Pauses, undo and redo produce every revision kind and enough revisions
    // to cross more than one keyframe interval.
    for index in 2..40 {
        let current = store.snapshot()?;
        let revision = format!("r{index}");
        match index % 5 {
            3 => {
                store.undo(current.revision_id(), RevisionId::new(&revision)?)?;
            }
            4 if index % 10 == 4 => {
                store.redo(current.revision_id(), RevisionId::new(&revision)?)?;
            }
            _ => {
                let total = current.duration()?.frames();
                store.commit(&pause(&current, &revision, (index as i64 * 7) % total)?)?;
            }
        }
        expected.push(store.snapshot()?);
    }
    let rows = stored(&path)?;
    assert_eq!(rows.len(), expected.len());
    // The newest revision, the initial one and every 16th keep documents.
    for (depth, (id, document)) in rows.iter().enumerate() {
        let keep = depth == 0 || depth == rows.len() - 1 || depth % 16 == 0;
        assert_eq!(document != "null", keep, "{id} at depth {depth}");
    }
    store.validate()?;
    for snapshot in &expected {
        assert_eq!(&store.snapshot_at(snapshot.revision_id())?, snapshot);
    }
    drop(store);
    for mode in [AccessMode::ReadOnly, AccessMode::ReadWrite] {
        let store = ProjectStore::open(&path, mode)?;
        store.validate()?;
        assert_eq!(&store.snapshot()?, expected.last().unwrap());
        for snapshot in &expected {
            assert_eq!(&store.snapshot_at(snapshot.revision_id())?, snapshot);
        }
    }
    Ok(())
}

#[test]
fn a_tampered_navigation_patch_is_rejected() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("tampered.deadpan");
    let mut store = ProjectStore::create(&path, &document()?)?;
    let current = store.snapshot()?;
    store.commit(&request(&current, "r1", beats(3)?)?)?;
    let current = store.snapshot()?;
    store.commit(&pause(&current, "r2", 4)?)?;
    store.undo(&RevisionId::new("r2")?, RevisionId::new("r3")?)?;
    store.redo(&RevisionId::new("r3")?, RevisionId::new("r4")?)?;
    store.undo(&RevisionId::new("r4")?, RevisionId::new("r5")?)?;
    store.validate()?;
    drop(store);
    let database = Connection::open(path.join("project.sqlite"))?;
    // r4 is elided; replace its redo patch with r3's undo patch.
    let r4: String =
        database.query_row("SELECT document FROM revisions WHERE id='r4'", [], |row| {
            row.get(0)
        })?;
    assert_eq!(r4, "null");
    database.execute(
        "UPDATE revision_patches SET patch=(SELECT patch FROM revision_patches WHERE revision_id='r3') WHERE revision_id='r4'",
        [],
    )?;
    drop(database);
    assert!(ProjectStore::open(&path, AccessMode::ReadOnly).is_err());
    Ok(())
}

#[test]
fn pause_history_records_only_changed_timing_state() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("pauses.deadpan");
    let mut store = ProjectStore::create(&path, &document()?)?;
    let current = store.snapshot()?;
    store.commit(&request(&current, "r1", beats(400)?)?)?;
    let mut sizes = Vec::new();
    for (index, at) in [(2, 7), (3, 600), (4, 31), (5, 902), (6, 13)] {
        let current = store.snapshot()?;
        let revision = format!("r{index}");
        store.commit(&pause(&current, &revision, at)?)?;
        let database = Connection::open(path.join("project.sqlite"))?;
        sizes.push(database.query_row(
            "SELECT length(edit) FROM history WHERE revision_id=?1",
            [revision.as_str()],
            |row| row.get::<_, i64>(0),
        )?);
    }
    let document = store.snapshot()?;
    // The first pause binds every Hold; later pauses add one small timing
    // table and a few bindings, never another copy of the project. Their
    // remaining bytes are the parent Sequence's child list in the node patch.
    assert!(sizes[0] > 300_000, "{sizes:?}");
    for size in &sizes[1..] {
        assert!(*size < 30_000, "{sizes:?}");
    }
    let timings = document.audio_bindings().timings();
    // An interior pause names two tables: lattices before its Split and
    // placements after it.
    assert!(timings.len() <= 10, "{}", timings.len());
    let largest = timings
        .values()
        .map(|layout| layout.nodes().len())
        .max()
        .unwrap();
    let smaller: Vec<_> = timings
        .values()
        .map(|layout| layout.nodes().len())
        .filter(|nodes| *nodes != largest)
        .collect();
    assert!(smaller.iter().all(|nodes| *nodes < 16), "{smaller:?}");
    Ok(())
}

#[test]
fn an_older_package_with_whole_state_binding_history_is_refused_unchanged() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("older.deadpan");
    let mut store = ProjectStore::create(&path, &document()?)?;
    let current = store.snapshot()?;
    store.commit(&request(&current, "r1", beats(3)?)?)?;
    let before = store.snapshot()?;
    store.commit(&pause(&before, "r2", 4)?)?;
    let after = store.snapshot()?;
    drop(store);
    // Rewrite the pause entry in the schema-62 form: complete binding states
    // on both sides of each patch, with no revision patch table.
    let database = Connection::open(path.join("project.sqlite"))?;
    let edit: String = database.query_row(
        "SELECT edit FROM history WHERE revision_id='r2'",
        [],
        |row| row.get(0),
    )?;
    let mut edit: serde_json::Value = serde_json::from_str(&edit)?;
    let states = [
        serde_json::to_value(before.audio_bindings())?,
        serde_json::to_value(after.audio_bindings())?,
    ];
    edit["forward"]["audio_bindings"] =
        serde_json::json!({"before": states[0], "after": states[1]});
    edit["inverse"]["audio_bindings"] =
        serde_json::json!({"before": states[1], "after": states[0]});
    database.execute(
        "UPDATE history SET edit=?1 WHERE revision_id='r2'",
        [edit.to_string()],
    )?;
    database.execute_batch("DROP TABLE revision_patches; PRAGMA user_version=62;")?;
    database.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    drop(database);
    let bytes = std::fs::read(path.join("project.sqlite"))?;
    for mode in [AccessMode::ReadOnly, AccessMode::ReadWrite] {
        assert!(matches!(
            ProjectStore::open(&path, mode),
            Err(deadpan_store::StoreError::UnsupportedSchema(62))
        ));
    }
    assert_eq!(std::fs::read(path.join("project.sqlite"))?, bytes);
    Ok(())
}
