use super::*;
use deadpan_core::SplitIdentities;
use deadpan_store::source_registration::SourceMomentInteriorInsertionRequest;

fn insertion(store: &ProjectStore, next: &str) -> Result<SourceMomentInteriorInsertionRequest> {
    let seam = paste(store, next)?;
    let target = NodeId::new("full")?;
    let at = FrameDuration::new(2)?;
    let destination = store
        .snapshot()?
        .source_splice_interior(&seam.parent, &target, at)?;
    Ok(SourceMomentInteriorInsertionRequest {
        expected_revision: seam.expected_revision,
        new_revision: seam.new_revision,
        asset: seam.asset,
        parent: seam.parent,
        target,
        at,
        node: seam.node,
        label: seam.label,
        identities: SplitIdentities {
            nodes: (0..destination.required_ids)
                .map(|index| NodeId::new(format!("split-{index}")))
                .collect::<std::result::Result<_, _>>()?,
        },
        timing: seam.timing,
        ordinals: seam.ordinals,
    })
}

#[test]
fn interior_preview_and_commit_share_one_exact_transaction_and_durable_undo() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store, prepared) = ready(scratch.path())?;
    let before = store.snapshot()?;
    let before_counts = counts(&path)?;
    let input = insertion(&store, "interior")?;
    let preview = store.preview_prepared_source_moment_interior(&input, &prepared, &cancelled())?;
    assert_eq!(counts(&path)?, before_counts);
    assert_eq!(store.snapshot()?, before);
    let expected = preview.forward.apply(&before)?;
    let NodeKind::Sequence { children } = &expected.nodes()[expected.root()].kind else {
        panic!("root must remain a Sequence");
    };
    assert_eq!(children.len(), 3);
    assert_eq!(children[1], input.node);
    let NodeKind::Source { source } = &expected.nodes()[&input.node].kind else {
        panic!("inserted moment must be a Source");
    };
    let measured = derive_source_moment(
        prepared.receipt().snapshot().video().unwrap().index(),
        prepared.receipt().snapshot().audio(),
        input.ordinals.clone(),
        before.presentation_basis().frame_rate,
    )?;
    assert_eq!(*source, measured.source_node(input.asset.clone()));
    assert_eq!(
        expected.duration()?.frames(),
        before.duration()?.frames() + source.duration.frames()
    );
    store.commit_prepared_source_moment_interior(&input, &prepared, None, &cancelled())?;
    assert_eq!(store.snapshot()?, expected);
    assert_eq!(
        counts(&path)?,
        (before_counts.0 + 1, before_counts.1 + 1, before_counts.2)
    );
    store.undo(expected.revision_id(), revision("undo-interior"))?;
    let undone = store.snapshot()?;
    assert_eq!(undone.nodes(), before.nodes());
    assert_eq!(undone.audio_bindings(), before.audio_bindings());
    store.validate()?;
    drop(store);
    let mut reopened = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    reopened.redo(undone.revision_id(), revision("redo-interior"))?;
    assert_eq!(reopened.snapshot()?.nodes(), expected.nodes());
    assert_eq!(
        reopened.snapshot()?.audio_bindings(),
        expected.audio_bindings()
    );
    reopened.validate()?;
    Ok(())
}

#[test]
fn interior_rejection_and_cursor_failure_never_publish_a_preliminary_split() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store, prepared) = ready(scratch.path())?;
    let before = store.snapshot()?;
    let before_counts = counts(&path)?;
    for case in [
        "stale",
        "target",
        "boundary",
        "source",
        "identities",
        "timing",
    ] {
        let mut input = insertion(&store, "rejected-interior")?;
        match case {
            "stale" => input.expected_revision = revision("initial"),
            "target" => input.target = input.parent.clone(),
            "boundary" => input.at = FrameDuration::ZERO,
            "source" => input.ordinals = 0..u64::MAX,
            "identities" => input.identities.nodes[0] = input.node.clone(),
            "timing" => input.timing.ordinal = u32::MAX,
            _ => unreachable!(),
        }
        assert!(
            store
                .preview_prepared_source_moment_interior(&input, &prepared, &cancelled())
                .is_err(),
            "{case}"
        );
        assert!(
            store
                .commit_prepared_source_moment_interior(&input, &prepared, None, &cancelled())
                .is_err(),
            "{case}"
        );
        assert_eq!(store.snapshot()?, before, "{case}");
        assert_eq!(counts(&path)?, before_counts, "{case}");
    }
    let input = insertion(&store, "interior")?;
    let database = Connection::open(path.join("project.sqlite"))?;
    database.execute_batch(
        "CREATE TRIGGER fail_interior_cursor BEFORE UPDATE OF head_revision,cursor ON state
         WHEN NEW.head_revision='interior'
         BEGIN SELECT RAISE(ABORT, 'injected interior commit failure'); END;",
    )?;
    assert!(matches!(
        store.commit_prepared_source_moment_interior(&input, &prepared, None, &cancelled()),
        Err(StoreError::Database(rusqlite::Error::SqliteFailure(_, Some(message))))
            if message == "injected interior commit failure"
    ));
    assert_eq!(store.snapshot()?, before);
    assert_eq!(counts(&path)?, before_counts);
    database.execute_batch("DROP TRIGGER fail_interior_cursor")?;
    store.commit_prepared_source_moment_interior(&input, &prepared, None, &cancelled())?;
    store.validate()?;
    Ok(())
}
