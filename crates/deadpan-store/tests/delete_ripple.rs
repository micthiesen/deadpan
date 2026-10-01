//! Both Delete grammars coexist in current-format history. These structural
//! fixtures test retained clocks and transactions, not decoded media.
use std::{collections::BTreeMap, error::Error, path::Path};

use deadpan_core::{
    AudioTimingId, BeatNode, ColorPolicy, Command, CommandRequest, FrameDuration, FrameRate,
    HoldAudio, HoldRecipe, HoldVideo, NodeId, PresentationBasis, ProjectDocument, ProjectId,
    RevisionId,
};
use deadpan_store::{AccessMode, DATABASE_SCHEMA_VERSION, ProjectStore};
use rusqlite::Connection;

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}

fn hold(frames: i64) -> BeatNode {
    BeatNode::hold(
        "Synthetic pause",
        HoldRecipe {
            duration: FrameDuration::new(frames).unwrap(),
            picture_context: None,
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    )
}

fn document() -> Result<ProjectDocument> {
    let initial = ProjectDocument::new(
        ProjectId::new("ripple-history")?,
        revision("baseline"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1_001)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )?;
    let mut wire = serde_json::to_value(initial)?;
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (
            node("root"),
            BeatNode::sequence("Root", vec![node("group"), node("tail")]),
        ),
        (
            node("group"),
            BeatNode::sequence("Group", vec![node("legacy"), node("cut"), node("suffix")]),
        ),
        (node("legacy"), hold(2)),
        (node("cut"), hold(1)),
        (node("suffix"), hold(4)),
        (node("tail"), hold(3)),
    ]))?;
    Ok(ProjectDocument::from_json(&wire.to_string())?)
}

fn request(document: &ProjectDocument, next: &str, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(next),
        command,
    }
}

fn assert_authored(actual: &ProjectDocument, expected: &ProjectDocument) -> Result {
    let mut expected = serde_json::to_value(expected)?;
    expected["revision_id"] = serde_json::to_value(actual.revision_id())?;
    assert_eq!(serde_json::to_value(actual)?, expected);
    Ok(())
}

fn history(path: &Path) -> Result<Vec<CommandRequest>> {
    let database = Connection::open(path.join("project.sqlite"))?;
    assert_eq!(DATABASE_SCHEMA_VERSION, 50);
    assert_eq!(
        database.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?,
        50
    );
    let encoded = database
        .prepare("SELECT request FROM history ORDER BY id")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(encoded
        .iter()
        .map(|request| serde_json::from_str(request))
        .collect::<std::result::Result<Vec<_>, _>>()?)
}

#[test]
fn current_history_replays_old_delete_and_ripple_delete_with_exact_undo_redo() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("mixed-delete.deadpan");
    let baseline = document()?;
    let mut store = ProjectStore::create(&path, &baseline)?;
    let old = request(
        &baseline,
        "old-delete",
        Command::Delete {
            node: node("legacy"),
        },
    );
    store.commit(&old)?;
    let legacy = store.snapshot()?;
    assert_eq!(legacy.duration()?.frames(), 8);
    assert!(legacy.audio_bindings().is_empty());
    let timing = AudioTimingId {
        allocation: revision("new-delete"),
        ordinal: 0,
    };
    let ripple = request(
        &legacy,
        "new-delete",
        Command::DeleteRipple {
            node: node("cut"),
            timing: timing.clone(),
        },
    );
    let stale = request(
        &legacy,
        "stale-delete",
        Command::DeleteRipple {
            node: node("tail"),
            timing: AudioTimingId {
                allocation: revision("stale-delete"),
                ordinal: 0,
            },
        },
    );
    store.commit(&ripple)?;
    let after = store.snapshot()?;
    assert_eq!(after.duration()?.frames(), 7);
    assert!(after.audio_bindings().timings().contains_key(&timing));
    assert!(!after.audio_bindings().bindings().contains_key(&node("cut")));
    for owner in ["suffix", "tail"] {
        assert_eq!(
            after.audio_bindings().bindings()[&node(owner)]
                .reanchors
                .len(),
            1
        );
    }
    assert!(store.commit(&stale).is_err());
    assert_eq!(store.snapshot()?, after);
    store.validate()?;
    assert_eq!(history(&path)?, vec![old.clone(), ripple.clone()]);
    drop(store);

    let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    reader.validate()?;
    assert_eq!(reader.snapshot()?, after);
    drop(reader);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    store.undo(after.revision_id(), revision("undo-new"))?;
    let undone = store.snapshot()?;
    assert_authored(&undone, &legacy)?;
    assert_ne!(undone.revision_id(), legacy.revision_id());
    assert!(store.commit(&stale).is_err());
    assert_eq!(store.snapshot()?, undone);
    assert_eq!(store.history_availability()?, (true, true));
    drop(store);

    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, undone);
    store.undo(undone.revision_id(), revision("undo-old"))?;
    assert_authored(&store.snapshot()?, &baseline)?;
    assert_eq!(store.history_availability()?, (false, true));
    store.redo(&revision("undo-old"), revision("redo-old"))?;
    assert_authored(&store.snapshot()?, &legacy)?;
    store.redo(&revision("redo-old"), revision("redo-new"))?;
    let redone = store.snapshot()?;
    assert_authored(&redone, &after)?;
    assert_eq!(store.history_availability()?, (true, false));
    assert_eq!(history(&path)?, vec![old, ripple]);
    store.validate()?;
    drop(store);

    let reopened = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert_eq!(reopened.snapshot()?, redone);
    reopened.validate()?;
    let database = Connection::open(path.join("project.sqlite"))?;
    assert_eq!(
        database.query_row("SELECT count(*) FROM revisions", [], |row| row
            .get::<_, i64>(0))?,
        7
    );
    Ok(())
}
