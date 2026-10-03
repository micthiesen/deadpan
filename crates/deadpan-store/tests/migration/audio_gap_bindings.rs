use super::*;
use deadpan_core::*;
fn fixture(root: &Path) -> Result<PathBuf> {
    let package = current_fixture(
        root,
        include_str!("../fixtures/current-audio_gap_bindings.json"),
        None,
    )?;
    pending_rename(
        &package,
        ProjectStore::open(&package, AccessMode::ReadOnly)?
            .snapshot()?
            .root()
            .clone(),
        "Retained reanchor redo",
    )?;
    Ok(package)
}

#[test]
fn reanchors_and_phase_terms_survive_current_history_and_pending_redo() -> Result {
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
            .any(|binding| binding.reanchors.len() > 1)
    );
    assert!(baseline.audio_bindings().gap_bindings().is_empty());
    store.redo(baseline.revision_id(), RevisionId::new("v27-pending-redo")?)?;
    let redone = store.snapshot()?;
    assert_eq!(
        redone.nodes()[redone.root()].label,
        "Retained reanchor redo"
    );
    assert_eq!(redone.audio_bindings(), baseline.audio_bindings());
    store.undo(redone.revision_id(), RevisionId::new("v27-undo-redo")?)?;
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
fn gap_bindings_survive_durable_commands_undo_redo_and_reopen() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path())?;
    let mut document = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    assert!(document.audio_bindings().gap_bindings().is_empty());
    let state = capture_unbound_audio_bindings(
        &document,
        AudioTimingId {
            allocation: RevisionId::new("gap-fixture-capture")?,
            ordinal: 0,
        },
    )?;
    assert_eq!(state.gap_bindings().len(), 1);
    let mut wire = serde_json::to_value(&document)?;
    wire["audio_bindings"] = serde_json::to_value(&state)?;
    document = ProjectDocument::from_json(&wire.to_string())?;
    let modern_path = scratch.path().join("modern-gaps.deadpan");
    let mut store = ProjectStore::create(&modern_path, &document)?;
    let gap = match &document.nodes()[&NodeId::new("old-gap-owner")?].kind {
        NodeKind::Repeat { gap, .. } => gap.clone(),
        _ => unreachable!(),
    };
    store.commit(&CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new("gap-growth")?,
        command: Command::SetRepeat {
            node: NodeId::new("old-gap-owner")?,
            plays: 4,
            gap,
        },
    })?;
    let grown = store.snapshot()?;
    assert_eq!(
        grown.audio_bindings().gap_bindings(),
        document.audio_bindings().gap_bindings()
    );
    store.undo(grown.revision_id(), RevisionId::new("gap-undo")?)?;
    let undone = store.snapshot()?;
    assert_eq!(undone.nodes(), document.nodes());
    assert_eq!(undone.audio_bindings(), document.audio_bindings());
    assert_ne!(undone.revision_id(), document.revision_id());
    drop(store);
    let mut store = ProjectStore::open(&modern_path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, undone);
    store.redo(undone.revision_id(), RevisionId::new("gap-redo")?)?;
    let redone = store.snapshot()?;
    assert_eq!(redone.nodes(), grown.nodes());
    assert_eq!(redone.audio_bindings(), grown.audio_bindings());
    assert_ne!(redone.revision_id(), grown.revision_id());
    store.validate()?;
    drop(store);
    assert_eq!(
        ProjectStore::open(&modern_path, AccessMode::ReadOnly)?.snapshot()?,
        redone
    );
    Ok(())
}
