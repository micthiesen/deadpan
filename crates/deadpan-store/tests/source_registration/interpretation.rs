use super::*;
use deadpan_core::{SourceChromaLocation, SourceColorRange, SourceInterpretation};

fn choice() -> SourceInterpretation {
    SourceInterpretation {
        range: Some(SourceColorRange::Full),
        chroma_location: Some(SourceChromaLocation::Center),
    }
}

fn correction(store: &ProjectStore, next: &str) -> Result<CommandRequest> {
    let current = store.snapshot()?;
    Ok(CommandRequest {
        project_id: current.project_id().clone(),
        expected_revision: current.revision_id().clone(),
        new_revision: revision(next),
        command: Command::SetSourceInterpretation {
            asset: id("camera"),
            interpretation: Some(choice()),
        },
    })
}

#[test]
fn corrections_survive_history_and_reopen_without_replacing_receipts() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let original = retain(&mut store, "offset-bframes.mp4")?;
    let decoded = decode(&store, &original)?;
    store.register_source(
        &request(&store, &original, "register", "camera", Some("clip"))?,
        &decoded,
        None,
        limits(),
        &active(),
    )?;
    let before = store.snapshot()?;
    let receipt = store.registered_source(before.revision_id(), &id("camera"))?;
    let input = correction(&store, "corrected")?;
    let preview = store.preview(&input)?;
    store.commit(&input)?;
    let after = store.snapshot()?;
    assert_eq!(after, preview.forward.apply(&before)?);
    assert_eq!(after.assets(), before.assets());
    assert_eq!(after.nodes(), before.nodes());
    assert_eq!(after.source_interpretations()[&id("camera")], choice());
    assert_eq!(
        store.registered_source(after.revision_id(), &id("camera"))?,
        receipt
    );
    assert_eq!(counts(&path)?, (3, 2, 1));
    store.undo(after.revision_id(), revision("undo"))?;
    assert!(store.snapshot()?.source_interpretations().is_empty());
    store.redo(&revision("undo"), revision("redo"))?;
    assert_eq!(
        store.snapshot()?.source_interpretations(),
        after.source_interpretations()
    );
    store.validate()?;
    drop(store);
    let store = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert_eq!(
        store.snapshot()?.source_interpretations(),
        after.source_interpretations()
    );
    assert_eq!(
        store.registered_source(&revision("redo"), &id("camera"))?,
        receipt
    );
    store.validate()?;
    Ok(())
}

#[test]
fn unsupported_hdr_range_refuses_preview_and_commit_atomically() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let original = retain(&mut store, "hdr-pq-av.mp4")?;
    let decoded = decode(&store, &original)?;
    store.register_source(
        &request(&store, &original, "register", "camera", None)?,
        &decoded,
        None,
        limits(),
        &active(),
    )?;
    let before = store.snapshot()?;
    let count = counts(&path)?;
    let input = correction(&store, "invalid-full-hdr")?;
    assert!(matches!(
        store.preview(&input),
        Err(StoreError::SourceRegistration(_))
    ));
    assert!(matches!(
        store.commit(&input),
        Err(StoreError::SourceRegistration(_))
    ));
    assert_eq!(store.snapshot()?, before);
    assert_eq!(counts(&path)?, count);
    drop(store);
    ProjectStore::open(&path, AccessMode::ReadOnly)?.validate()?;
    Ok(())
}
