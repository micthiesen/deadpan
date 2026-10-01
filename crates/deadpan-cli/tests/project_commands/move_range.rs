use super::*;
use deadpan_core::{
    AudioTimingId, CommandRequest, FrameRange, MoveRangeDestination, ProjectFrame, RevisionId,
    SplitIdentities,
};

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

fn hold(label: &str) -> Result<BeatNode> {
    Ok(BeatNode::hold(
        label,
        HoldRecipe {
            picture_context: None,
            duration: FrameDuration::new(10)?,
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    ))
}

#[test]
fn headless_move_range_is_one_durable_command_with_source_and_destination_revision_checks() -> Result
{
    for interior in [false, true] {
        let scratch = tempfile::tempdir()?;
        let package = create(scratch.path())?;
        let path = package.to_str().unwrap();
        let input = scratch.path().join("move.json");
        let file = input.to_str().unwrap();
        let empty = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
        let group = NodeId::new("group")?;
        let a = NodeId::new("a")?;
        let b = NodeId::new("b")?;
        let c = NodeId::new("c")?;
        let mut insert = request(&empty)?;
        insert["command"]["subtree"] = json!(Subtree {
            root: group.clone(),
            nodes: BTreeMap::from([
                (
                    group.clone(),
                    BeatNode::sequence("Group", vec![a.clone(), b.clone(), c.clone()])
                ),
                (a.clone(), hold("A")?),
                (b.clone(), hold("B")?),
                (c.clone(), hold("C")?),
            ]),
            overrides: Default::default(),
            gap_overrides: Default::default(),
        });
        fs::write(&input, insert.to_string())?;
        success(&["command", path, "--json", file])?;
        let before = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
        let range = if interior {
            FrameRange::new(ProjectFrame(2), ProjectFrame(4))?
        } else {
            FrameRange::new(ProjectFrame(0), ProjectFrame(10))?
        };
        let destination = if interior {
            MoveRangeDestination::Interior {
                parent: group.clone(),
                target: a.clone(),
                at: FrameDuration::new(8)?,
            }
        } else {
            MoveRangeDestination::Seam {
                parent: before.root().clone(),
                index: 1,
            }
        };
        let plan = before.range_move(&group, range, &destination)?;
        assert_eq!(
            plan.inserted,
            if interior {
                FrameRange::new(ProjectFrame(6), ProjectFrame(8))?
            } else {
                FrameRange::new(ProjectFrame(20), ProjectFrame(30))?
            }
        );
        let command = Command::MoveRange {
            source_revision: before.revision_id().clone(),
            source_parent: group.clone(),
            range,
            destination,
            identities: SplitIdentities {
                nodes: (0..plan.required_ids)
                    .map(|i| NodeId::new(format!("move-split-{i}")))
                    .collect::<std::result::Result<_, _>>()?,
            },
            timing: AudioTimingId {
                allocation: RevisionId::new("moved")?,
                ordinal: 0,
            },
        };
        let envelope = json!({"protocol":1, "project_id":before.project_id(), "expected_revision":before.revision_id(), "new_revision":"moved", "command":command});
        fs::write(&input, envelope.to_string())?;
        let cells = authored(&package)?;
        let preview = success(&["command", path, "--json", file, "--dry-run"])?;
        assert_eq!(preview["committed"], false);
        assert_eq!(preview["edit"]["duration_delta"], 0);
        assert_eq!(authored(&package)?, cells);
        let committed = success(&["command", path, "--json", file])?;
        assert_eq!(committed["committed"], true);
        assert_eq!(committed["outcome"]["edit"], preview["edit"]);
        let after = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
        assert_eq!(after.duration()?.frames(), 30);
        assert_eq!(after.nodes()[&b], before.nodes()[&b]);
        assert_eq!(after.nodes()[&c], before.nodes()[&c]);
        if !interior {
            assert_eq!(after.nodes()[&a], before.nodes()[&a]);
            assert_eq!(
                after.children(after.root()).cloned().collect::<Vec<_>>(),
                vec![group.clone(), a.clone()]
            );
            assert_eq!(
                after.children(&group).cloned().collect::<Vec<_>>(),
                vec![b, c]
            );
        }
        let database = rusqlite::Connection::open(package.join("project.sqlite"))?;
        let counts = || -> Result<(i64, i64)> {
            Ok(database.query_row(
                "SELECT (SELECT count(*) FROM revisions),(SELECT count(*) FROM history)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?)
        };
        assert_eq!(counts()?, (3, 2));
        let stored: String = database.query_row(
            "SELECT request FROM history WHERE revision_id='moved'",
            [],
            |row| row.get(0),
        )?;
        let stored: CommandRequest = serde_json::from_str(&stored)?;
        assert_eq!(stored.command, command);
        assert_eq!(stored.expected_revision, *before.revision_id());

        // Duplicate Enter/stale destination cannot create a second command.
        let cells = authored(&package)?;
        for dry in [true, false] {
            let mut args = vec!["command", path, "--json", file];
            if dry {
                args.push("--dry-run");
            }
            let failed = cli(&args)?;
            assert!(!failed.status.success());
            assert_eq!(
                serde_json::from_slice::<Value>(&failed.stderr)?["error"]["code"],
                "RevisionConflict"
            );
            assert_eq!(authored(&package)?, cells);
        }
        success(&["project", "undo", path, "--expected", "moved"])?;
        let undone = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
        assert_authored(&undone, &before)?;
        assert_ne!(undone.revision_id(), before.revision_id());

        // Refreshing only the destination envelope cannot authorize removing
        // whatever occupies the old source register's numeric coordinates.
        let mut stale_source = envelope.clone();
        stale_source["expected_revision"] = json!(undone.revision_id());
        stale_source["new_revision"] = json!("stale-source");
        stale_source["command"]["timing"]["allocation"] = json!("stale-source");
        fs::write(&input, stale_source.to_string())?;
        let cells = authored(&package)?;
        for dry in [true, false] {
            let mut args = vec!["command", path, "--json", file];
            if dry {
                args.push("--dry-run");
            }
            assert!(!cli(&args)?.status.success());
            assert_eq!(authored(&package)?, cells);
        }
        // The new command remains a closed grammar at headless ingress.
        stale_source["command"]["source_revision"] = json!(undone.revision_id());
        stale_source["command"]["destination"]["unexpected"] = json!(true);
        fs::write(&input, stale_source.to_string())?;
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
        let redone = reopened.snapshot()?;
        assert_authored(&redone, &after)?;
        assert_ne!(redone.revision_id(), after.revision_id());
        assert_ne!(redone.revision_id(), undone.revision_id());
        assert_eq!(counts()?, (5, 2));
        reopened.validate()?;
    }
    Ok(())
}
