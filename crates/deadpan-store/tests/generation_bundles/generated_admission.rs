//! Uses the parent's admitted-bundle fixture and real acceptance transaction.
//! This proves retained provenance and history, not movie decoding or quality.
use super::*;

#[test]
fn existing_authored_artifact_resizes_reverts_and_navigates_durable_history() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("retained.deadpan");
    let mut store = ProjectStore::create(&path, &document()?)?;
    let input = ready_for_acceptance(&mut store)?;
    store.accept_generation_bundle(
        &input,
        &unchanged_relevance(&store, &input.new_revision)?,
        media_limits(),
    )?;
    let retained = store.snapshot()?;
    let node = NodeId::new("hold")?;
    let NodeKind::Hold { recipe } = &retained.nodes()[&node].kind else {
        panic!()
    };
    let HoldVideo::Generated { accepted } = &recipe.video else {
        panic!()
    };
    let artifact = accepted.artifact.clone();
    let origin = store
        .accepted_generation_origin(&artifact)?
        .expect("dedicated admission saves its origin");
    assert_eq!(origin.request_id(), &input.identity.request_id);
    assert_eq!(origin.accepted_revision(), &input.new_revision);
    store.validate_full()?;
    drop(store);

    // Exercise a preexisting accepted artifact after an ordinary reopen.
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, retained);
    let shorter_duration = FrameDuration::new(8)?;
    edit_reconciled(
        &mut store,
        "shorter",
        Command::SetHoldDuration {
            node: node.clone(),
            duration: shorter_duration,
        },
    )?;
    let shorter = store.snapshot()?;
    assert!(matches!(&shorter.nodes()[&node].kind,
        NodeKind::Hold { recipe } if recipe.duration == shorter_duration
            && matches!(&recipe.video, HoldVideo::Generated { accepted } if accepted.artifact == artifact)));
    let undo_shorter = RevisionId::new("undo-shorter")?;
    store.undo_reconciled(
        shorter.revision_id(),
        undo_shorter.clone(),
        &unchanged_relevance(&store, &undo_shorter)?,
    )?;
    assert_eq!(store.snapshot()?.nodes(), retained.nodes());
    let current = store.snapshot()?;
    let redo_shorter = RevisionId::new("redo-shorter")?;
    store.redo_reconciled(
        current.revision_id(),
        redo_shorter.clone(),
        &unchanged_relevance(&store, &redo_shorter)?,
    )?;
    assert_eq!(store.snapshot()?.nodes(), shorter.nodes());
    edit_reconciled(
        &mut store,
        "revert",
        Command::RevertGeneratedHold { node: node.clone() },
    )?;
    let reverted = store.snapshot()?;
    assert!(matches!(&reverted.nodes()[&node].kind,
        NodeKind::Hold { recipe } if recipe.video == HoldVideo::Background && recipe.duration == shorter_duration));
    let undo_revert = RevisionId::new("undo-revert")?;
    store.undo_reconciled(
        reverted.revision_id(),
        undo_revert.clone(),
        &unchanged_relevance(&store, &undo_revert)?,
    )?;
    assert_eq!(store.snapshot()?.nodes(), shorter.nodes());
    let current = store.snapshot()?;
    let redo_revert = RevisionId::new("redo-revert")?;
    store.redo_reconciled(
        current.revision_id(),
        redo_revert.clone(),
        &unchanged_relevance(&store, &redo_revert)?,
    )?;
    assert_eq!(store.snapshot()?.nodes(), reverted.nodes());
    let current = store.snapshot()?;
    let undo_again = RevisionId::new("undo-revert-again")?;
    store.undo_reconciled(
        current.revision_id(),
        undo_again.clone(),
        &unchanged_relevance(&store, &undo_again)?,
    )?;
    assert_eq!(store.snapshot()?.nodes(), shorter.nodes());
    store.validate_full()?;
    drop(store);
    let reopened = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert_eq!(reopened.snapshot()?.nodes(), shorter.nodes());
    assert_eq!(reopened.snapshot()?.assets(), retained.assets());
    assert_eq!(
        reopened.accepted_generation_origin(&artifact)?,
        Some(origin)
    );
    reopened.validate_full()?;
    Ok(())
}
