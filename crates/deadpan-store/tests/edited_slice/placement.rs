use super::*;

#[derive(Clone, Copy)]
enum Destination {
    Interior,
    Replacement,
}

fn placement(
    document: &ProjectDocument,
    slice: &CapturedEditSlice,
    name: &str,
    destination: Destination,
) -> Result<CommandRequest> {
    let mut request = paste(document, slice, name, 1)?;
    let Command::SpliceSlice {
        parent,
        slice,
        identities,
        timing,
        ..
    } = request.command
    else {
        unreachable!()
    };
    let target = node("tail");
    let at = FrameDuration::new(1)?;
    let range = FrameRange::new(ProjectFrame(1), ProjectFrame(4))?;
    let required = match destination {
        Destination::Interior => {
            document
                .slice_splice_interior(&parent, &target, at, &slice)?
                .required_ids
        }
        Destination::Replacement => {
            document
                .slice_replacement(&parent, range, &slice)?
                .required_ids
        }
    };
    let split_identities = SplitIdentities {
        nodes: (0..required)
            .map(|i| node(&format!("{name}-split-{i}")))
            .collect(),
    };
    request.command = match destination {
        Destination::Interior => Command::SpliceSliceAt {
            parent,
            target,
            at,
            slice,
            identities,
            split_identities,
            timing,
        },
        Destination::Replacement => Command::ReplaceSlice {
            parent,
            range,
            slice,
            identities,
            split_identities,
            timing,
        },
    };
    Ok(request)
}

fn durable_placement(destination: Destination, expected_duration: i64) -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("placement.deadpan");
    let baseline = document()?;
    let slice = capture(&baseline)?;
    let mut store = ProjectStore::create(&path, &baseline)?;
    store.commit(&request(
        &baseline,
        "remove-source",
        Command::DeleteRipple {
            node: node("group"),
            timing: timing("remove-source"),
        },
    ))?;
    let before = store.snapshot()?;
    assert_eq!(before.duration()?.frames(), 5);
    let command = placement(&before, &slice, "placed", destination)?;
    let bytes = serde_json::to_vec(&command)?;
    drop(store);

    let command: CommandRequest = serde_json::from_slice(&bytes)?;
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let cells = authored(&path)?;
    let preview = store.preview(&command)?;
    assert_eq!(preview.duration_delta, expected_duration - 5);
    assert_eq!(authored(&path)?, cells);
    assert_eq!(store.commit(&command)?.edit, preview);
    let after = store.snapshot()?;
    assert_eq!(after.duration()?.frames(), expected_duration);
    assert_eq!(after.node_duration(&node("placed-node-0"))?.frames(), 14);
    assert!(after.marks().contains_key(&MarkId::new("placed-mark-0")?));
    store.validate()?;
    drop(store);

    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, after);
    store.undo(after.revision_id(), revision("undo-placement"))?;
    assert_authored(&store.snapshot()?, &before)?;
    let cells = authored(&path)?;
    assert!(store.preview(&command).is_err());
    assert!(store.commit(&command).is_err());
    assert_eq!(authored(&path)?, cells);
    drop(store);

    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    store.redo(&revision("undo-placement"), revision("redo-placement"))?;
    assert_authored(&store.snapshot()?, &after)?;
    store.validate()?;
    drop(store);
    ProjectStore::open(&path, AccessMode::ReadOnly)?.validate()?;
    let database = Connection::open(path.join("project.sqlite"))?;
    assert_eq!(
        database.query_row("SELECT count(*) FROM history", [], |row| row
            .get::<_, i64>(0))?,
        2,
        "source deletion plus exactly one placement"
    );
    assert_eq!(
        database.query_row("SELECT count(*) FROM revisions", [], |row| row
            .get::<_, i64>(0))?,
        5,
        "baseline, deletion, placement, Undo and Redo"
    );
    let saved: String = database.query_row(
        "SELECT request FROM history WHERE revision_id='placed'",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(serde_json::from_str::<CommandRequest>(&saved)?, command);
    Ok(())
}

#[test]
fn interior_slice_paste_is_one_durable_edit_after_source_deletion() -> Result {
    durable_placement(Destination::Interior, 19)
}

#[test]
fn slice_replacement_splits_both_endpoints_in_one_durable_edit() -> Result {
    durable_placement(Destination::Replacement, 16)
}

#[test]
fn invalid_split_and_import_pools_leave_placement_history_unchanged() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("invalid-placement.deadpan");
    let baseline = document()?;
    let slice = capture(&baseline)?;
    let mut store = ProjectStore::create(&path, &baseline)?;
    store.commit(&request(
        &baseline,
        "remove-source",
        Command::DeleteRipple {
            node: node("group"),
            timing: timing("remove-source"),
        },
    ))?;
    let baseline = store.snapshot()?;
    let cells = authored(&path)?;
    for destination in [Destination::Interior, Destination::Replacement] {
        for case in 0..4 {
            let mut invalid =
                placement(&baseline, &slice, &format!("invalid-{case}"), destination)?;
            let (identities, split, timing) = match &mut invalid.command {
                Command::SpliceSliceAt {
                    identities,
                    split_identities,
                    timing,
                    ..
                }
                | Command::ReplaceSlice {
                    identities,
                    split_identities,
                    timing,
                    ..
                } => (identities, split_identities, timing),
                _ => unreachable!(),
            };
            assert!(!split.nodes.is_empty());
            match case {
                0 => split.nodes.clear(),
                1 => split.nodes[0] = identities.authored.nodes[0].clone(),
                2 => split.nodes[0] = identities.aliases[0].clone(),
                _ => timing.ordinal = u32::MAX,
            }
            assert!(store.preview(&invalid).is_err());
            assert!(store.commit(&invalid).is_err());
            assert_eq!(store.snapshot()?, baseline);
            assert_eq!(authored(&path)?, cells);
        }
    }
    store.validate()?;
    Ok(())
}
