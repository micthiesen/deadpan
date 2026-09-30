use super::*;
use deadpan_core::{FrameRange, ProjectFrame, SplitIdentities};
use deadpan_store::source_registration::SourceMomentReplacementRequest;

fn request(
    store: &ProjectStore,
    name: &str,
    range: FrameRange,
) -> Result<SourceMomentReplacementRequest> {
    let base = paste(store, name)?;
    let target = store.snapshot()?.source_replacement(&base.parent, range)?;
    Ok(SourceMomentReplacementRequest {
        expected_revision: base.expected_revision,
        new_revision: base.new_revision,
        asset: base.asset,
        parent: base.parent,
        range,
        node: base.node,
        label: base.label,
        identities: SplitIdentities {
            nodes: (0..target.required_ids)
                .map(|index| NodeId::new(format!("replace-{index}")))
                .collect::<std::result::Result<_, _>>()?,
        },
        timing: base.timing,
        ordinals: base.ordinals,
    })
}

#[test]
fn replacement_admits_measured_source_and_persists_one_exact_undo_redo() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store, prepared) = ready(scratch.path())?;
    let before = store.snapshot()?;
    let counts_before = counts(&path)?;
    let range = FrameRange::new(ProjectFrame(1), ProjectFrame(3))?;
    let input = request(&store, "replacement", range)?;
    let preview = store.preview_prepared_source_replacement(&input, &prepared, &cancelled())?;
    assert_eq!(store.snapshot()?, before);
    assert_eq!(counts(&path)?, counts_before);
    let expected = preview.forward.apply(&before)?;
    let measured = derive_source_moment(
        prepared.receipt().snapshot().video().unwrap().index(),
        prepared.receipt().snapshot().audio(),
        input.ordinals.clone(),
        before.presentation_basis().frame_rate,
    )?;
    assert_eq!(
        expected.nodes()[&input.node].kind,
        NodeKind::Source {
            source: measured.source_node(input.asset.clone())
        }
    );
    store.commit_prepared_source_replacement(&input, &prepared, None, &cancelled())?;
    assert_eq!(store.snapshot()?, expected);
    assert_eq!(
        counts(&path)?,
        (counts_before.0 + 1, counts_before.1 + 1, counts_before.2)
    );
    store.undo(expected.revision_id(), revision("undo-replacement"))?;
    let undone = store.snapshot()?;
    let mut expected_undo = serde_json::to_value(&before)?;
    expected_undo["revision_id"] = serde_json::to_value(undone.revision_id())?;
    assert_eq!(serde_json::to_value(&undone)?, expected_undo);
    store.validate()?;
    drop(store);
    let mut reopened = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    reopened.redo(undone.revision_id(), revision("redo-replacement"))?;
    let redone = reopened.snapshot()?;
    let mut expected_redo = serde_json::to_value(&expected)?;
    expected_redo["revision_id"] = serde_json::to_value(redone.revision_id())?;
    assert_eq!(serde_json::to_value(&redone)?, expected_redo);
    reopened.validate()?;
    Ok(())
}

#[test]
fn stale_invalid_and_failed_replacement_never_saves_splits_or_removal() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store, prepared) = ready(scratch.path())?;
    let before = store.snapshot()?;
    let counts_before = counts(&path)?;
    let range = FrameRange::new(ProjectFrame(1), ProjectFrame(3))?;
    for case in ["stale", "parent", "range", "source", "identities", "timing"] {
        let mut input = request(&store, "rejected-replacement", range)?;
        match case {
            "stale" => input.expected_revision = revision("initial"),
            "parent" => input.parent = NodeId::new("full")?,
            "range" => input.range = FrameRange::new(ProjectFrame(0), ProjectFrame(i64::MAX))?,
            "source" => input.ordinals = 0..u64::MAX,
            "identities" => input.identities.nodes[0] = input.node.clone(),
            "timing" => input.timing.ordinal = u32::MAX,
            _ => unreachable!(),
        }
        assert!(
            store
                .preview_prepared_source_replacement(&input, &prepared, &cancelled())
                .is_err(),
            "{case}"
        );
        assert!(
            store
                .commit_prepared_source_replacement(&input, &prepared, None, &cancelled())
                .is_err(),
            "{case}"
        );
        assert_eq!(store.snapshot()?, before, "{case}");
        assert_eq!(counts(&path)?, counts_before, "{case}");
    }
    let input = request(&store, "replacement", range)?;
    let database = Connection::open(path.join("project.sqlite"))?;
    database.execute_batch(
        "CREATE TRIGGER fail_replacement_cursor BEFORE UPDATE OF head_revision,cursor ON state
         WHEN NEW.head_revision='replacement'
         BEGIN SELECT RAISE(ABORT, 'injected replacement failure'); END;",
    )?;
    assert!(matches!(
        store.commit_prepared_source_replacement(&input, &prepared, None, &cancelled()),
        Err(StoreError::Database(rusqlite::Error::SqliteFailure(_, Some(message))))
            if message == "injected replacement failure"
    ));
    assert_eq!(store.snapshot()?, before);
    assert_eq!(counts(&path)?, counts_before);
    database.execute_batch("DROP TRIGGER fail_replacement_cursor")?;
    store.commit_prepared_source_replacement(&input, &prepared, None, &cancelled())?;
    store.validate()?;
    Ok(())
}
