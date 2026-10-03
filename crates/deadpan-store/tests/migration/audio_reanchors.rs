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
    assert!(
        baseline
            .audio_bindings()
            .bindings()
            .values()
            .any(|binding| binding
                .resume
                .as_ref()
                .is_some_and(|resume| !resume.phase.terms.is_empty()))
    );
    assert!(
        baseline
            .audio_bindings()
            .bindings()
            .values()
            .all(|binding| binding.reanchors.is_empty())
    );
    store.redo(baseline.revision_id(), RevisionId::new("v26-pending-redo")?)?;
    let redone = store.snapshot()?;
    assert_eq!(
        redone.nodes()[redone.root()].label,
        "Retained selected-audio redo"
    );
    assert_eq!(redone.audio_bindings(), baseline.audio_bindings());
    store.undo(redone.revision_id(), RevisionId::new("v26-undo-redo")?)?;
    let undone = store.snapshot()?;
    assert_eq!(undone.nodes(), baseline.nodes());
    assert_eq!(undone.audio_bindings(), baseline.audio_bindings());
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

#[test]
fn modern_reanchor_intent_survives_durable_history_undo_redo_and_reopen() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path())?;
    let original = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let (owner, binding) = original.audio_bindings().bindings().iter().next().unwrap();
    let mut binding = binding.clone();
    binding.reanchors.push(AudioReanchorStep {
        anchor: Default::default(),
        placement: binding.lattice.clone(),
        window: None,
    });
    let mut wire = serde_json::to_value(&original)?;
    wire["audio_bindings"]["bindings"][owner.as_str()] = serde_json::to_value(&binding)?;
    let initial = ProjectDocument::from_json(&wire.to_string())?;
    let modern_path = scratch.path().join("modern-reanchors.deadpan");
    let mut store = ProjectStore::create(&modern_path, &initial)?;
    let request = CommandRequest {
        project_id: initial.project_id().clone(),
        expected_revision: initial.revision_id().clone(),
        new_revision: RevisionId::new("reanchor-rename")?,
        command: Command::Rename {
            node: initial.root().clone(),
            label: "Retained reanchors".into(),
        },
    };
    store.commit(&request)?;
    let renamed = store.snapshot()?;
    assert_eq!(renamed.audio_bindings(), initial.audio_bindings());
    store.undo(renamed.revision_id(), RevisionId::new("reanchor-undo")?)?;
    let undone = store.snapshot()?;
    assert_eq!(undone.nodes(), initial.nodes());
    assert_eq!(undone.audio_bindings().bindings()[owner], binding);
    assert_ne!(undone.revision_id(), initial.revision_id());
    drop(store);
    let mut store = ProjectStore::open(&modern_path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, undone);
    store.redo(undone.revision_id(), RevisionId::new("reanchor-redo")?)?;
    let redone = store.snapshot()?;
    assert_eq!(redone.nodes(), renamed.nodes());
    assert_eq!(redone.audio_bindings(), initial.audio_bindings());
    assert_ne!(redone.revision_id(), renamed.revision_id());
    store.validate()?;
    drop(store);
    assert_eq!(
        ProjectStore::open(&modern_path, AccessMode::ReadOnly)?.snapshot()?,
        redone
    );
    Ok(())
}
