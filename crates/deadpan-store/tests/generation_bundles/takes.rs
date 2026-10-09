//! Snapshot restoration retains already admitted media and never revives workers.
use super::*;
use deadpan_store::takes::{Action, Request, TakeId, TakeName};

fn request(store: &ProjectStore, action: Action) -> Result<Request> {
    let catalog = store.take_catalog()?;
    Ok(Request {
        project_id: catalog.project_id,
        expected_revision: catalog.revision_id,
        expected_version: catalog.version,
        action,
    })
}

#[test]
fn takes_restore_accepted_media_exactly_and_retire_pending_generation() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("takes.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let acceptance = ready_for_acceptance(&mut store)?;
    store.accept_generation_bundle(
        &acceptance,
        &unchanged_relevance(&store, &acceptance.new_revision)?,
        media_limits(),
    )?;
    let accepted = store.snapshot()?;
    store.apply_take(&request(
        &store,
        Action::Create {
            id: TakeId::new("accepted")?,
            name: TakeName::new("Accepted picture")?,
        },
    )?)?;
    // Reverting selects the fallback, while the immutable snapshot keeps all
    // accepted provider/asset/input references and their original evidence.
    edit_reconciled(
        &mut store,
        "fallback",
        Command::RevertGeneratedHold {
            node: NodeId::new("hold")?,
        },
    )?;
    let fallback = store.snapshot()?;
    let pending = allocate_bridge(&mut store, "pending-after-take", 4)?;
    assert_eq!(pending.relevance, deadpan_jobs::Relevance::Current);
    let restore = request(
        &store,
        Action::Restore {
            id: TakeId::new("accepted")?,
            expected_snapshot: accepted.revision_id().clone(),
            new_revision: RevisionId::new("restored")?,
        },
    )?;
    let preview = store.preview_take(&restore)?;
    assert_eq!(store.current_generation_requests()?, vec![pending.clone()]);
    let outcome = store.apply_take(&restore)?;
    assert_eq!(
        outcome.commit.as_ref().unwrap().edit,
        preview.commit.unwrap().edit
    );
    assert!(outcome.commit.unwrap().generation_preparations.is_empty());
    assert!(store.current_generation_requests()?.is_empty());
    assert_eq!(
        store
            .generation_request(&pending.request_id)?
            .unwrap()
            .relevance,
        deadpan_jobs::Relevance::Stale
    );
    let mut expected = serde_json::to_value(&accepted)?;
    expected["revision_id"] = serde_json::to_value(RevisionId::new("restored")?)?;
    assert_eq!(serde_json::to_value(store.snapshot()?)?, expected);
    store.undo(
        &RevisionId::new("restored")?,
        RevisionId::new("undo-restored")?,
    )?;
    assert_eq!(store.snapshot()?.nodes(), fallback.nodes());
    store.redo(
        &RevisionId::new("undo-restored")?,
        RevisionId::new("redo-restored")?,
    )?;
    assert_eq!(store.snapshot()?.nodes(), accepted.nodes());
    assert!(store.current_generation_requests()?.is_empty());
    // No model, workspace or new candidate acceptance is needed to reopen.
    store.validate_full()?;
    drop(store);
    let reopened = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    assert_eq!(reopened.snapshot()?.nodes(), accepted.nodes());
    reopened.validate_full()?;
    for bytes in [NATIVE_BYTES, SAMPLED_BYTES, PROVENANCE_BYTES] {
        assert!(
            reopened
                .snapshot_generated_object(&object(bytes), media_limits())
                .is_ok()
        );
    }
    Ok(())
}
