use super::*;
use deadpan_core::SplitIdentities;

#[test]
fn headless_interior_and_replacement_preview_commit_and_undo_as_one_command() -> Result {
    for replace in [false, true] {
        let scratch = tempfile::tempdir()?;
        let package = create(scratch.path())?;
        let path = package.to_str().unwrap();
        let input = scratch.path().join("placement.json");
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
        let before = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
        let cells = authored(&package)?;
        let parent = NodeId::new("group")?;
        let slice = CapturedEditSlice::capture(
            &before,
            &parent,
            FrameRange::new(ProjectFrame(1), ProjectFrame(5))?,
            AudioTimingId {
                allocation: RevisionId::new("copy")?,
                ordinal: 0,
            },
        )?;
        assert_eq!(authored(&package)?, cells);
        let count = slice.identity_requirements()?;
        assert_eq!(count.marks, 0);
        let identities = SlicePasteIdentities {
            authored: OccurrenceIdentities {
                nodes: (0..count.nodes)
                    .map(|i| NodeId::new(format!("placed-node-{i}")))
                    .collect::<std::result::Result<_, _>>()?,
                marks: Vec::new(),
            },
            aliases: (0..count.aliases)
                .map(|i| NodeId::new(format!("placed-alias-{i}")))
                .collect::<std::result::Result<_, _>>()?,
        };
        let target = NodeId::new("voice")?;
        let at = FrameDuration::new(3)?;
        let range = FrameRange::new(ProjectFrame(2), ProjectFrame(5))?;
        let split_count = if replace {
            before
                .slice_replacement(&parent, range, &slice)?
                .required_ids
        } else {
            before
                .slice_splice_interior(&parent, &target, at, &slice)?
                .required_ids
        };
        let split_identities = SplitIdentities {
            nodes: (0..split_count)
                .map(|i| NodeId::new(format!("placed-split-{i}")))
                .collect::<std::result::Result<_, _>>()?,
        };
        assert!(!split_identities.nodes.is_empty());
        let timing = AudioTimingId {
            allocation: RevisionId::new("placed")?,
            ordinal: 0,
        };
        let command = if replace {
            Command::ReplaceSlice {
                parent,
                range,
                slice,
                identities,
                split_identities,
                timing,
            }
        } else {
            Command::SpliceSliceAt {
                parent,
                target,
                at,
                slice,
                identities,
                split_identities,
                timing,
            }
        };
        let envelope = json!({"protocol":1,"project_id":before.project_id(),
            "expected_revision":before.revision_id(),"new_revision":"placed","command":command});
        let mut collision = envelope.clone();
        collision["command"]["split_identities"]["nodes"][0] =
            collision["command"]["identities"]["authored"]["nodes"][0].clone();
        fs::write(&input, collision.to_string())?;
        assert!(
            !cli(&["command", path, "--json", file, "--dry-run"])?
                .status
                .success()
        );
        assert!(!cli(&["command", path, "--json", file])?.status.success());
        assert_eq!(authored(&package)?, cells);
        fs::write(&input, envelope.to_string())?;
        let preview = success(&["command", path, "--json", file, "--dry-run"])?;
        assert_eq!(preview["committed"], false);
        assert_eq!(
            preview["edit"]["duration_delta"],
            if replace { 1 } else { 4 }
        );
        assert_eq!(authored(&package)?, cells);
        let committed = success(&["command", path, "--json", file])?;
        assert_eq!(committed["committed"], true);
        assert_eq!(committed["outcome"]["edit"], preview["edit"]);
        let after = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
        assert_eq!(after.duration()?.frames(), if replace { 8 } else { 11 });
        assert_eq!(
            after
                .node_duration(&NodeId::new("placed-node-0")?)?
                .frames(),
            4
        );
        let cells = authored(&package)?;
        let stale = cli(&["command", path, "--json", file])?;
        assert!(!stale.status.success());
        assert_eq!(
            serde_json::from_slice::<Value>(&stale.stderr)?["error"]["code"],
            "RevisionConflict"
        );
        assert_eq!(authored(&package)?, cells);
        success(&["project", "undo", path, "--expected", "placed"])?;
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
        let database = rusqlite::Connection::open(package.join("project.sqlite"))?;
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
        let saved: String = database.query_row(
            "SELECT request FROM history WHERE revision_id='placed'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(
            serde_json::from_str::<Value>(&saved)?["command"],
            serde_json::to_value(command)?
        );
    }
    Ok(())
}
