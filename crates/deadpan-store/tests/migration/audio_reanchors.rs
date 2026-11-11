use super::*;
use deadpan_core::*;
fn fixture(root: &Path) -> Result<PathBuf> {
    let package = current_fixture(
        root,
        include_str!("../fixtures/current-audio_reanchors.json"),
        None,
    )?;
    pending_rename(
        &package,
        ProjectStore::open(&package, AccessMode::ReadOnly)?
            .snapshot()?
            .root()
            .clone(),
        "Retained selected-audio redo",
    )?;
    Ok(package)
}

#[test]
fn selected_audio_phase_terms_survive_current_history_and_pending_redo() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path())?;
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let baseline = store.snapshot()?;
    assert!(baseline.nodes().values().any(|node| matches!(&node.kind,
        NodeKind::Source { source } if matches!(source.audio_mapping, SourceAudioMapping::SelectedPlacement { .. }))));
    store.redo(baseline.revision_id(), RevisionId::new("v26-pending-redo")?)?;
    let redone = store.snapshot()?;
    assert_eq!(
        redone.nodes()[redone.root()].label,
        "Retained selected-audio redo"
    );
    store.undo(redone.revision_id(), RevisionId::new("v26-undo-redo")?)?;
    let undone = store.snapshot()?;
    assert_eq!(undone.nodes(), baseline.nodes());
    assert_ne!(undone.revision_id(), baseline.revision_id());
    let request = CommandRequest {
        project_id: undone.project_id().clone(),
        expected_revision: undone.revision_id().clone(),
        new_revision: RevisionId::new("fresh-after-migration")?,
        command: Command::Rename {
            node: undone.root().clone(),
            label: "Current revision".into(),
        },
    };
    store.commit(&request)?;
    assert!(store.commit(&request).is_err());
    store.validate()?;
    let expected = store.snapshot()?;
    drop(store);
    assert_eq!(
        ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?,
        expected
    );
    assert!(ProjectStore::migrate(&package)?.backup.is_none());
    Ok(())
}
