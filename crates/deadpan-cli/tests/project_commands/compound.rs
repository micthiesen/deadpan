use super::*;
use deadpan_core::{LeafEdit, ResolvedStep, ResolvedTransaction, RevisionId};

fn rows(package: &Path) -> Result<Vec<String>> {
    let database = rusqlite::Connection::open(package.join("project.sqlite"))?;
    let mut rows = Vec::new();
    for query in [
        "SELECT json_array(id,parent_id,kind,document) FROM revisions ORDER BY id",
        "SELECT json_array(id,parent_id,revision_id,request,edit) FROM history ORDER BY id",
        "SELECT json_array(singleton,head_revision,cursor,workflow) FROM state",
        "SELECT json_array(position,history_id) FROM redo ORDER BY position",
        "SELECT json_array(owner_revision,ordinal,step_revision,document) FROM transaction_steps ORDER BY owner_revision,ordinal",
        "SELECT json_array(singleton,version) FROM register_state",
        "SELECT json_array(name,content_id) FROM registers ORDER BY name",
    ] {
        rows.extend(
            database
                .prepare(query)?
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?,
        );
    }
    Ok(rows)
}

fn compound(missing: bool) -> Result<Command> {
    Ok(Command::Compound {
        transaction: ResolvedTransaction::new(
            0,
            BTreeMap::new(),
            vec![
                ResolvedStep::Edit {
                    edit: LeafEdit::new(
                        RevisionId::new("step-repeat")?,
                        Command::WrapRepeat {
                            node: NodeId::new("hold")?,
                            id: NodeId::new("twice")?,
                            plays: 2,
                            gap: None,
                            anchor_policy: Default::default(),
                        },
                    )?,
                },
                ResolvedStep::Edit {
                    edit: LeafEdit::new(
                        RevisionId::new("step-duration")?,
                        Command::SetHoldDuration {
                            node: NodeId::new(if missing { "missing" } else { "hold" })?,
                            duration: FrameDuration::new(17)?,
                        },
                    )?,
                },
            ],
        )?,
    })
}

#[test]
fn headless_compound_validates_all_steps_and_commits_one_reversible_edit() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = create(scratch.path())?;
    let path = package.to_str().unwrap();
    let input = scratch.path().join("compound.json");
    let file = input.to_str().unwrap();
    let empty = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    fs::write(&input, request(&empty)?.to_string())?;
    success(&["command", path, "--json", file])?;
    let before = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let mut envelope = json!({
        "protocol": 1, "project_id": before.project_id(),
        "expected_revision": before.revision_id(), "new_revision": "compound-saved",
        "command": compound(true)?,
    });
    let original_rows = rows(&package)?;
    fs::write(&input, envelope.to_string())?;
    assert!(!cli(&["command", path, "--json", file])?.status.success());
    assert_eq!(rows(&package)?, original_rows);
    envelope["command"] = serde_json::to_value(compound(false)?)?;
    fs::write(&input, envelope.to_string())?;
    let preview = success(&["command", path, "--json", file, "--dry-run"])?;
    assert_eq!(preview["committed"], false);
    assert_eq!(preview["edit"]["duration_delta"], -11);
    assert_eq!(rows(&package)?, original_rows);
    let saved = success(&["command", path, "--json", file])?;
    assert_eq!(saved["committed"], true);
    assert_eq!(saved["outcome"]["edit"], preview["edit"]);
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    let after = store.snapshot()?;
    assert_eq!(after.duration()?.frames(), 34);
    let step = RevisionId::new("step-repeat")?;
    assert!(store.snapshot_at(&step).is_err());
    assert!(store.capture_snapshot_at(&step).is_err());
    drop(store);
    let database = rusqlite::Connection::open(package.join("project.sqlite"))?;
    let cardinalities = || {
        database.query_row(
        "SELECT (SELECT count(*) FROM revisions), (SELECT count(*) FROM history), (SELECT count(*) FROM transaction_steps), (SELECT count(*) FROM transaction_steps WHERE document IS NOT NULL)",
        [], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?, row.get::<_, i64>(3)?)),
    )
    };
    assert_eq!(cardinalities()?, (3, 2, 2, 0));
    let saved_rows = rows(&package)?;
    assert!(!cli(&["command", path, "--json", file])?.status.success());
    assert_eq!(rows(&package)?, saved_rows);
    success(&["project", "undo", path, "--expected", "compound-saved"])?;
    let undone = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    assert_eq!(undone.nodes(), before.nodes());
    assert_eq!(cardinalities()?, (4, 2, 2, 0));
    envelope["expected_revision"] = json!(undone.revision_id());
    envelope["new_revision"] = json!("collision-after-undo");
    fs::write(&input, envelope.to_string())?;
    let undo_rows = rows(&package)?;
    assert!(!cli(&["command", path, "--json", file])?.status.success());
    assert_eq!(rows(&package)?, undo_rows);
    success(&[
        "project",
        "redo",
        path,
        "--expected",
        undone.revision_id().as_str(),
    ])?;
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    assert_eq!(store.snapshot()?.nodes(), after.nodes());
    assert_eq!(cardinalities()?, (5, 2, 2, 0));
    store.validate()?;
    Ok(())
}
