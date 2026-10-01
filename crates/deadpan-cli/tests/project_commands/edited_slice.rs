use super::*;
use deadpan_core::{
    AudioTimingId, CapturedEditSlice, FrameRange, OccurrenceIdentities, ProjectFrame, RevisionId,
    SlicePasteIdentities,
};

#[path = "edited_slice/placement.rs"]
mod placement;

fn authored(package: &Path) -> Result<Vec<String>> {
    let database = rusqlite::Connection::open(package.join("project.sqlite"))?;
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
    let mut expected = serde_json::to_value(expected)?;
    expected["revision_id"] = json!(actual.revision_id());
    assert_eq!(serde_json::to_value(actual)?, expected);
    Ok(())
}

#[test]
fn headless_slice_payload_survives_deleted_source_and_commits_one_reversible_edit() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = create(scratch.path())?;
    let path = package.to_str().unwrap();
    let input = scratch.path().join("slice-command.json");
    let file = input.to_str().unwrap();
    let empty = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let mut insert = request(&empty)?;
    insert["command"]["subtree"] = json!({
        "root":"group",
        "nodes":{
            "group":BeatNode::sequence("Group",vec![NodeId::new("voice")?]),
            "voice":BeatNode::hold("Synthetic pause",HoldRecipe {
                picture_context:None, duration:FrameDuration::new(7)?,
                video:HoldVideo::Background, audio:HoldAudio::Silence,
            }),
        },
    });
    fs::write(&input, insert.to_string())?;
    success(&["command", path, "--json", file])?;
    let source = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let cells = authored(&package)?;
    let slice = CapturedEditSlice::capture(
        &source,
        &NodeId::new("group")?,
        FrameRange::new(ProjectFrame(1), ProjectFrame(5))?,
        AudioTimingId {
            allocation: RevisionId::new("copy")?,
            ordinal: 0,
        },
    )?;
    assert_eq!(authored(&package)?, cells);
    fs::write(
        &input,
        json!({
            "protocol":1, "project_id":source.project_id(),
            "expected_revision":source.revision_id(), "new_revision":"removed",
            "command":Command::DeleteRipple {
                node:NodeId::new("group")?,
                timing:AudioTimingId {allocation:RevisionId::new("removed")?,ordinal:0},
            },
        })
        .to_string(),
    )?;
    success(&["command", path, "--json", file])?;
    let before = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    assert_eq!(before.duration()?.frames(), 0);
    assert!(!before.nodes().contains_key(&NodeId::new("voice")?));
    let count = slice.identity_requirements()?;
    assert_eq!(count.marks, 0);
    let command = Command::SpliceSlice {
        parent: before.root().clone(),
        index: 0,
        slice,
        identities: SlicePasteIdentities {
            authored: OccurrenceIdentities {
                nodes: (0..count.nodes)
                    .map(|i| NodeId::new(format!("paste-node-{i}")))
                    .collect::<std::result::Result<_, _>>()?,
                marks: Vec::new(),
            },
            aliases: (0..count.aliases)
                .map(|i| NodeId::new(format!("paste-alias-{i}")))
                .collect::<std::result::Result<_, _>>()?,
        },
        timing: AudioTimingId {
            allocation: RevisionId::new("pasted")?,
            ordinal: 0,
        },
    };
    let envelope = json!({"protocol":1,"project_id":before.project_id(),
        "expected_revision":before.revision_id(),"new_revision":"pasted","command":command});
    fs::write(&input, envelope.to_string())?;
    let cells = authored(&package)?;
    let preview = success(&["command", path, "--json", file, "--dry-run"])?;
    assert_eq!(preview["committed"], false);
    assert_eq!(preview["edit"]["duration_delta"], 4);
    assert_eq!(authored(&package)?, cells);
    let committed = success(&["command", path, "--json", file])?;
    assert_eq!(committed["committed"], true);
    assert_eq!(committed["outcome"]["edit"], preview["edit"]);
    let after = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    assert_eq!(after.duration()?.frames(), 4);
    assert_eq!(
        after.node_duration(&NodeId::new("paste-node-0")?)?.frames(),
        4
    );
    assert_eq!(after.children(after.root()).count(), 1);
    let cells = authored(&package)?;
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
        assert_eq!(authored(&package)?, cells);
    }
    // A structurally inconsistent captured window must fail at headless ingress,
    // before it can allocate a revision or alter the history cursor.
    let mut malformed = envelope.clone();
    malformed["expected_revision"] = json!(after.revision_id());
    malformed["new_revision"] = json!("malformed");
    malformed["command"]["slice"]["range"]["end"] = json!(6);
    fs::write(&input, malformed.to_string())?;
    assert!(!cli(&["command", path, "--json", file])?.status.success());
    assert_eq!(authored(&package)?, cells);
    success(&["project", "undo", path, "--expected", "pasted"])?;
    let undone = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    assert_authored(&undone, &before)?;
    fs::write(&input, envelope.to_string())?;
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
    let database = rusqlite::Connection::open(package.join("project.sqlite"))?;
    let stored: String = database.query_row(
        "SELECT request FROM history WHERE revision_id='pasted'",
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
        3
    );
    assert_eq!(
        database.query_row("SELECT count(*) FROM revisions", [], |row| row
            .get::<_, i64>(0))?,
        6
    );
    Ok(())
}
