use super::*;
use deadpan_core::{AudioTimingId, FrameRange, ProjectFrame, RevisionId, SplitIdentities};

fn authored(package: &Path) -> Result<Vec<String>> {
    let database = rusqlite::Connection::open(package.join("project.sqlite"))?;
    let mut result = Vec::new();
    for sql in [
        "SELECT json_array(id,parent_id,kind,document) FROM revisions ORDER BY id",
        "SELECT json_array(id,parent_id,revision_id,request,edit) FROM history ORDER BY id",
        "SELECT json_array(singleton,head_revision,cursor,workflow) FROM state",
        "SELECT json_array(position,history_id) FROM redo ORDER BY position",
    ] {
        result.extend(
            database
                .prepare(sql)?
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?,
        );
    }
    Ok(result)
}

fn assert_authored(actual: &ProjectDocument, expected: &ProjectDocument) -> Result {
    let mut expected = serde_json::to_value(expected)?;
    expected["revision_id"] = json!(actual.revision_id());
    assert_eq!(serde_json::to_value(actual)?, expected);
    Ok(())
}

#[test]
fn headless_range_delete_dry_run_commit_and_history_keep_the_captured_ids() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = create(scratch.path())?;
    let path = package.to_str().unwrap();
    let input = scratch.path().join("delete-range.json");
    let file = input.to_str().unwrap();
    let empty = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let mut insert = request(&empty)?;
    insert["command"]["subtree"] = json!({
        "root":"group",
        "nodes":{
            "group":BeatNode::sequence("Group",vec![NodeId::new("voice")?]),
            "voice":BeatNode::hold("Synthetic pause",HoldRecipe {picture_context:None,duration:FrameDuration::new(7)?,video:HoldVideo::Background,audio:HoldAudio::Silence}),
        },
    });
    fs::write(&input, insert.to_string())?;
    success(&["command", path, "--json", file])?;
    let before = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let parent = NodeId::new("group")?;
    let range = FrameRange::new(ProjectFrame(1), ProjectFrame(3))?;
    let required = before.range_deletion(&parent, range)?.required_ids;
    let command = Command::DeleteRange {
        parent,
        range,
        identities: SplitIdentities {
            nodes: (0..required)
                .map(|i| NodeId::new(format!("range-part-{i}")))
                .collect::<std::result::Result<_, _>>()?,
        },
        timing: AudioTimingId {
            allocation: RevisionId::new("range-deleted")?,
            ordinal: 0,
        },
    };
    let envelope = json!({"protocol":1,"project_id":before.project_id(),"expected_revision":before.revision_id(),"new_revision":"range-deleted","command":command});
    fs::write(&input, envelope.to_string())?;
    let cells = authored(&package)?;
    let preview = success(&["command", path, "--json", file, "--dry-run"])?;
    assert_eq!(preview["committed"], false);
    assert_eq!(preview["edit"]["duration_delta"], -2);
    assert!(preview["edit"]["forward"]["audio_bindings"].is_object());
    assert_eq!(authored(&package)?, cells);
    let committed = success(&["command", path, "--json", file])?;
    assert_eq!(committed["committed"], true);
    assert_eq!(committed["outcome"]["edit"], preview["edit"]);
    let after = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    assert_eq!(after.duration()?.frames(), 5);
    let durations: Vec<_> = after
        .children(&NodeId::new("group")?)
        .map(|id| after.node_duration(id).unwrap().frames())
        .collect();
    assert_eq!(durations, vec![1, 4]);
    let database = rusqlite::Connection::open(package.join("project.sqlite"))?;
    let stored: String = database.query_row(
        "SELECT request FROM history WHERE revision_id='range-deleted'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(
        serde_json::from_str::<Value>(&stored)?["command"],
        serde_json::to_value(command)?
    );
    assert_eq!(
        database.query_row("SELECT count(*) FROM history", [], |row| row
            .get::<_, i64>(0))?,
        2
    );
    assert_eq!(
        database.query_row("SELECT count(*) FROM revisions", [], |row| row
            .get::<_, i64>(0))?,
        3
    );
    let saved = authored(&package)?;
    for arguments in [
        vec!["command", path, "--json", file, "--dry-run"],
        vec!["command", path, "--json", file],
    ] {
        let failed = cli(&arguments)?;
        assert!(!failed.status.success());
        assert_eq!(
            serde_json::from_slice::<Value>(&failed.stderr)?["error"]["code"],
            "RevisionConflict"
        );
        assert_eq!(authored(&package)?, saved);
    }
    success(&["project", "undo", path, "--expected", "range-deleted"])?;
    let undone = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    assert_authored(&undone, &before)?;
    let cells = authored(&package)?;
    assert!(!cli(&["command", path, "--json", file])?.status.success());
    assert_eq!(authored(&package)?, cells);
    success(&[
        "project",
        "redo",
        path,
        "--expected",
        undone.revision_id().as_str(),
    ])?;
    let reopened = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    assert_authored(&reopened.snapshot()?, &after)?;
    reopened.validate()?;
    assert_eq!(
        database.query_row("SELECT count(*) FROM history", [], |row| row
            .get::<_, i64>(0))?,
        2
    );
    assert_eq!(
        database.query_row("SELECT count(*) FROM revisions", [], |row| row
            .get::<_, i64>(0))?,
        5
    );
    Ok(())
}
