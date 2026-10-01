//! Move keeps current authored identities and is one durable transaction.
use super::*;

fn range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}

fn move_request(
    document: &ProjectDocument,
    name: &str,
    parent: &str,
    range: FrameRange,
    destination: MoveRangeDestination,
) -> Result<CommandRequest> {
    let plan = document.range_move(&node(parent), range, &destination)?;
    Ok(request(
        document,
        name,
        Command::MoveRange {
            source_revision: document.revision_id().clone(),
            source_parent: node(parent),
            range,
            destination,
            identities: SplitIdentities {
                nodes: (0..plan.required_ids)
                    .map(|i| node(&format!("{name}-split-{i}")))
                    .collect(),
            },
            timing: timing(name),
        },
    ))
}

fn seam(parent: &str, index: usize) -> MoveRangeDestination {
    MoveRangeDestination::Seam {
        parent: node(parent),
        index,
    }
}

fn rows(path: &Path) -> Result<(i64, i64)> {
    Ok(Connection::open(path.join("project.sqlite"))?.query_row(
        "SELECT (SELECT count(*) FROM revisions),(SELECT count(*) FROM history)",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?)
}

#[test]
fn move_dry_run_atomic_failure_commit_and_fresh_history_survive_reopen() -> Result {
    // Includes whole Repeat ownership, both directions, cross-parent moves and
    // one original Hold requiring three joint cuts. These are synthetic pictures.
    let cases = [
        ("root", range(2, 17), seam("root", 3), range(5, 20)),
        ("group", range(2, 9), seam("root", 3), range(13, 20)),
        ("root", range(17, 20), seam("group", 0), range(2, 5)),
        ("group", range(3, 7), seam("root", 3), range(16, 20)),
        (
            "group",
            range(3, 5),
            MoveRangeDestination::Interior {
                parent: node("group"),
                target: node("voice"),
                at: FrameDuration::new(6)?,
            },
            range(6, 8),
        ),
    ];
    for (source, selected, destination, inserted) in cases {
        let scratch = tempfile::tempdir()?;
        let path = scratch.path().join("move.deadpan");
        let before = document()?;
        let mut store = ProjectStore::create(&path, &before)?;
        let command = move_request(&before, "moved", source, selected, destination.clone())?;
        let plan = before.range_move(&node(source), selected, &destination)?;
        assert_eq!(plan.inserted, inserted);
        assert!(!plan.is_noop);
        let cells = authored(&path)?;
        let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
        let preview = reader.preview(&command)?;
        assert_eq!(preview.duration_delta, 0);
        assert_eq!(authored(&path)?, cells, "dry-run must write nothing");
        drop(reader);

        let database = Connection::open(path.join("project.sqlite"))?;
        database.execute_batch("CREATE TRIGGER fail_move_history BEFORE INSERT ON history BEGIN SELECT RAISE(FAIL,'forced MoveRange history failure'); END;")?;
        assert!(store.commit(&command).is_err());
        assert_eq!(store.snapshot()?, before);
        assert_eq!(
            authored(&path)?,
            cells,
            "revision and history roll back together"
        );
        database.execute_batch("DROP TRIGGER fail_move_history")?;
        assert_eq!(store.commit(&command)?.edit, preview);
        assert_eq!(rows(&path)?, (2, 1));
        let after = store.snapshot()?;
        assert_eq!(after.duration()?, before.duration()?);
        assert_eq!(
            after.nodes()[&node("repeat")],
            before.nodes()[&node("repeat")]
        );
        assert_eq!(after.nodes()[&node("echo")], before.nodes()[&node("echo")]);
        assert!(after.nodes().contains_key(&node("group")));
        assert_eq!(
            after.marks().keys().collect::<Vec<_>>(),
            before.marks().keys().collect::<Vec<_>>()
        );
        assert_eq!(after.assets(), before.assets());
        let saved: String =
            database.query_row("SELECT request FROM history", [], |row| row.get(0))?;
        assert_eq!(serde_json::from_str::<CommandRequest>(&saved)?, command);
        store.validate()?;
        drop(store);

        let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
        assert_eq!(store.snapshot()?, after);
        store.undo(after.revision_id(), revision("undo-move"))?;
        let undone = store.snapshot()?;
        assert_ne!(undone.revision_id(), before.revision_id());
        assert_authored(&undone, &before)?;
        let cells = authored(&path)?;
        assert!(store.preview(&command).is_err());
        assert!(store.commit(&command).is_err());
        assert_eq!(authored(&path)?, cells, "Undo must not revive a stale move");
        drop(store);

        let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
        store.redo(undone.revision_id(), revision("redo-move"))?;
        let redone = store.snapshot()?;
        assert_ne!(redone.revision_id(), after.revision_id());
        assert_ne!(redone.revision_id(), undone.revision_id());
        assert_authored(&redone, &after)?;
        assert_eq!(rows(&path)?, (4, 1));
        store.validate()?;
        drop(store);
        let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
        assert_eq!(reader.snapshot()?, redone);
        reader.validate()?;
    }
    Ok(())
}

#[test]
fn stale_source_stale_destination_and_bad_split_pool_cannot_write_history() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("stale-move.deadpan");
    let original = document()?;
    let mut store = ProjectStore::create(&path, &original)?;
    store.commit(&request(
        &original,
        "renamed",
        Command::Rename {
            node: node("tail"),
            label: "Current destination".into(),
        },
    ))?;
    let before = store.snapshot()?;
    let valid = move_request(&before, "moved", "group", range(3, 7), seam("root", 3))?;
    let cells = authored(&path)?;
    for case in 0..5 {
        let mut invalid = valid.clone();
        let Command::MoveRange {
            source_revision,
            identities,
            destination,
            ..
        } = &mut invalid.command
        else {
            unreachable!()
        };
        match case {
            0 => *source_revision = original.revision_id().clone(),
            1 => invalid.expected_revision = original.revision_id().clone(),
            2 => identities.nodes.clear(),
            3 => identities.nodes.push(node("tail")),
            _ => *destination = seam("root", usize::MAX),
        }
        assert!(
            store.preview(&invalid).is_err(),
            "preview accepted case {case}"
        );
        assert!(
            store.commit(&invalid).is_err(),
            "commit accepted case {case}"
        );
        assert_eq!(store.snapshot()?, before);
        assert_eq!(authored(&path)?, cells);
    }
    store.commit(&valid)?;
    assert_eq!(rows(&path)?, (3, 2));
    store.validate()?;
    Ok(())
}
