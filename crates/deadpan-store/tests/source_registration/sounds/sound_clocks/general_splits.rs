//! Durable transaction verification for the general Split command, including
//! selective isolation. This deliberately does not claim decoded PCM equality.
use super::*;
use deadpan_core::{
    OccurrenceEdit, OccurrenceIdentities, RepeatInstance, RepeatSelectionIdentities,
};

#[derive(Clone, Copy, Debug)]
enum Shape {
    Source,
    Hold,
    Sequence,
    Repeat,
    Retime,
    Root,
}

fn attach_root_event(store: &mut ProjectStore) -> Result {
    let before = store.snapshot()?;
    let event = before.beat_sounds()[&node("owner")][&SoundId::new("overlay")?].clone();
    store.commit(&edit(
        &before,
        "root-event",
        Command::SetBeatSound {
            owner: before.root().clone(),
            id: SoundId::new("root-overlay")?,
            event,
        },
    ))?;
    Ok(())
}

fn capture_prefix(store: &mut ProjectStore) -> Result {
    let NodeKind::Hold { recipe } = hold(1, HoldAudio::Silence).kind else {
        unreachable!()
    };
    store.commit(&edit(
        &store.snapshot()?,
        "prefix",
        Command::InsertTime {
            at: ProjectFrame(0),
            hold: recipe,
            id: node("prefix"),
            identities: SplitIdentities::default(),
            timing: timing("prefix"),
        },
    ))?;
    Ok(())
}

fn repeat(
    store: &mut ProjectStore,
    parent: &str,
    child: &str,
    name: &str,
) -> Result<RepeatInstance> {
    let before = store.snapshot()?;
    store.commit(&edit(
        &before,
        name,
        Command::RepeatSelection {
            parent: node(parent),
            selection: SliceCaptureSelection::Child { node: node(child) },
            plays: 3,
            identities: RepeatSelectionIdentities {
                repeat: node(name),
                group: None,
                split: SplitIdentities::default(),
            },
            timing: timing(name),
        },
    ))?;
    let after = store.snapshot()?;
    let NodeKind::Repeat { iterations, .. } = &after.nodes()[&node(name)].kind else {
        panic!("expected the authored Repeat")
    };
    Ok(RepeatInstance {
        node: node(name),
        iteration: iterations.at(1).ok_or("missing second play")?,
    })
}

fn split_ids() -> SplitIdentities {
    SplitIdentities {
        nodes: (0..64).map(|index| node(&format!("cut-{index}"))).collect(),
    }
}

fn durable_split(path: &Path, mut store: ProjectStore, command: Command, selected: bool) -> Result {
    let before = store.snapshot()?;
    let request = edit(&before, "general-split", command);
    let rows = counts(path)?;
    let preview = store.preview(&request)?;
    assert_eq!(store.snapshot()?, before);
    assert_eq!(counts(path)?, rows);
    // Exercise SQLite rollback after the full in-memory sound transport was
    // prepared, not merely an early invalid-command refusal.
    let connection = Connection::open(path.join("project.sqlite"))?;
    connection.execute_batch(
        "CREATE TRIGGER reject_general_split BEFORE INSERT ON history
         BEGIN SELECT RAISE(FAIL,'forced general Split persistence failure'); END;",
    )?;
    assert!(store.commit(&request).is_err());
    assert_eq!(store.snapshot()?, before);
    assert_eq!(counts(path)?, rows);
    assert_eq!(store.history_availability()?, (true, false));
    connection.execute_batch("DROP TRIGGER reject_general_split")?;
    drop(connection);

    let result = store.commit(&request)?.edit;
    assert_eq!(result, preview);
    let after = store.snapshot()?;
    assert_eq!(after.duration()?, before.duration()?);
    assert_eq!(result.inverse.apply(&after)?, before);
    assert_eq!(ProjectDocument::from_json(&after.to_json()?)?, after);
    // A directly qualified Source can retain its original clock without an
    // introduced forest. Full history recomputation below proves the complete
    // transport representation appropriate to this primitive.
    assert!(after.beat_sounds().len() > before.beat_sounds().len());
    if selected {
        let proof = deadpan_core::derive_scoped_isolation(&before, &request, &after)?;
        assert!(!proof.record().steps().is_empty());
        // The ordinary/default branch and its event remain independently
        // authored. Isolating one presentation cannot rewrite them.
        assert_eq!(
            after.nodes()[&node("owner")],
            before.nodes()[&node("owner")]
        );
        assert_eq!(
            after.beat_sounds()[&node("owner")],
            before.beat_sounds()[&node("owner")]
        );
    }
    store.validate_full()?;
    drop(store);

    let mut store = ProjectStore::open(path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, after);
    assert_eq!(store.snapshot_at(before.revision_id())?, before);
    assert_eq!(store.snapshot_at(after.revision_id())?, after);
    store.undo(after.revision_id(), revision("undo-general-split"))?;
    assert_authored(&store.snapshot()?, &before)?;
    store.validate_full()?;
    drop(store);

    let mut store = ProjectStore::open(path, AccessMode::ReadWrite)?;
    assert_authored(&store.snapshot()?, &before)?;
    store.redo(
        &revision("undo-general-split"),
        revision("redo-general-split"),
    )?;
    let redone = store.snapshot()?;
    assert_authored(&redone, &after)?;
    store.validate_full()?;
    drop(store);

    let destination = path
        .parent()
        .ok_or("missing fixture parent")?
        .join("Portable-split.deadpan");
    let copied = deadpan_store::portable::copy_portable(path, &destination, &active())?;
    assert_eq!(copied.revision_id, redone.revision_id().as_str());
    let portable = ProjectStore::open(&destination, AccessMode::ReadOnly)?;
    assert_eq!(portable.snapshot()?, redone);
    assert_eq!(portable.snapshot_at(before.revision_id())?, before);
    assert_eq!(portable.snapshot_at(after.revision_id())?, after);
    portable.validate_full()?;
    Ok(())
}

#[test]
fn general_split_all_primitive_owners_keep_atomic_registered_history_and_portable_proofs() -> Result
{
    for shape in [
        Shape::Source,
        Shape::Hold,
        Shape::Sequence,
        Shape::Repeat,
        Shape::Retime,
        Shape::Root,
    ] {
        (|| -> Result {
            let scratch = tempfile::tempdir()?;
            let (path, mut store) = setup_with_clock_owner(
                scratch.path(),
                !matches!(shape, Shape::Hold),
                false,
                matches!(shape, Shape::Retime),
            )?;
            let target = match shape {
                Shape::Source | Shape::Hold => node("owner"),
                Shape::Sequence => node("top"),
                Shape::Root => node("root"),
                Shape::Repeat => {
                    repeat(&mut store, "top", "owner", "inner-repeat")?;
                    node("inner-repeat")
                }
                Shape::Retime => node("retimed-owner"),
            };
            capture_prefix(&mut store)?;
            attach_root_event(&mut store)?;
            let split = Command::Split {
                node: target,
                at: frames(4),
                identities: split_ids(),
            };
            // Editing never cuts the root itself, and a root-owned sound has no
            // transport across that wrapper.
            if matches!(shape, Shape::Root) {
                let error = store
                    .commit(&edit(&store.snapshot()?, "split-root", split))
                    .unwrap_err()
                    .to_string();
                assert!(error.contains("root-owned beat sounds"), "{error}");
                return Ok(());
            }
            durable_split(&path, store, split, false)?;
            Ok(())
        })()
        .map_err(|error| format!("{shape:?}: {error}"))?;
    }
    Ok(())
}

#[test]
fn general_split_selected_nested_plays_keeps_default_and_durable_isolation_proofs() -> Result {
    for nested in [false, true] {
        let scratch = tempfile::tempdir()?;
        let (path, mut store) = setup_with_clock(scratch.path(), true, false)?;
        let inner = if nested {
            Some(repeat(&mut store, "top", "owner", "inner-repeat")?)
        } else {
            None
        };
        let outer = repeat(&mut store, "root", "top", "outer-repeat")?;
        capture_prefix(&mut store)?;
        attach_root_event(&mut store)?;
        let mut repeats = vec![outer];
        repeats.extend(inner);
        durable_split(
            &path,
            store,
            Command::EditOccurrence {
                instance: InstancePath {
                    node: node("owner"),
                    repeats,
                },
                edit: OccurrenceEdit::Split {
                    at: frames(4),
                    identities: split_ids(),
                },
                identities: OccurrenceIdentities {
                    nodes: (0..64)
                        .map(|index| node(&format!("isolation-{index}")))
                        .collect(),
                    marks: vec![],
                },
            },
            true,
        )
        .map_err(|error| format!("nested={nested}: {error}"))?;
    }
    Ok(())
}
