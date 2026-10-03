use super::*;
use deadpan_core::*;
fn fixture(root: &Path) -> Result<PathBuf> {
    let package = current_fixture(
        root,
        include_str!("../fixtures/current-gain.json"),
        Some(include_str!("../fixtures/current-gain-media.sql")),
    )?;
    Ok(package)
}

fn gain() -> AudioTreatments {
    AudioTreatments::from_clip_gain(
        ClipGain::new(GainDb::new(-6000).unwrap(), false, vec![], vec![]).unwrap(),
    )
}

#[test]
fn authored_gain_is_durable_reversible_and_revision_guarded_from_current_snapshot() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path())?;
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let before = store.snapshot()?;
    let request = CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: RevisionId::new("authored-gain")?,
        command: Command::SetAudioTreatments {
            node: before.root().clone(),
            treatments: gain(),
        },
    };
    let preview = store.preview(&request)?;
    assert_eq!(store.snapshot()?, before);
    store.commit(&request)?;
    let after = store.snapshot()?;
    assert_eq!(after, preview.forward.apply(&before)?);
    assert_eq!(after.duration()?, before.duration()?);
    assert_eq!(after.audio_bindings(), before.audio_bindings());
    assert_eq!(after.audio_lineage(), before.audio_lineage());
    assert!(store.commit(&request).is_err());
    store.undo(after.revision_id(), RevisionId::new("authored-gain-undo")?)?;
    let undone = store.snapshot()?;
    assert_eq!(undone.nodes(), before.nodes());
    assert_ne!(undone.revision_id(), before.revision_id());
    store.redo(undone.revision_id(), RevisionId::new("authored-gain-redo")?)?;
    assert_eq!(store.snapshot()?.nodes(), after.nodes());
    store.validate()?;
    drop(store);
    let reopened = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    assert_eq!(reopened.snapshot()?.nodes(), after.nodes());
    reopened.validate()?;
    Ok(())
}
