use super::*;
use deadpan_core::*;
fn fixture(root: &Path) -> Result<PathBuf> {
    let package = current_fixture(
        root,
        include_str!("../fixtures/current-source_selection.json"),
        None,
    )?;
    pending_rename(&package, NodeId::new("source")?, "Retained pending rename")?;
    Ok(package)
}

fn selected() -> SourceAudioMapping {
    SourceAudioMapping::SelectedPlacement {
        start: ExactRatio::new(-2, 3).unwrap(),
        frames: ExactRatio::new(30000, 1001).unwrap(),
        selection: ExactFrameRange::new(ExactRatio::integer(2), ExactRatio::integer(8)).unwrap(),
    }
}

#[test]
fn selected_audio_preserves_captured_views_current_history_and_pending_redo() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path())?;
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let before_redo = store.snapshot()?;
    store.redo(
        before_redo.revision_id(),
        RevisionId::new("v25-pending-redo")?,
    )?;
    assert_eq!(
        store.snapshot()?.nodes()[&NodeId::new("source")?].label,
        "Retained pending rename"
    );
    store.undo(
        &RevisionId::new("v25-pending-redo")?,
        RevisionId::new("v25-undo-redo")?,
    )?;
    let baseline = store.snapshot()?;
    assert_eq!(baseline.nodes(), before_redo.nodes());
    let target = NodeId::new("source")?;
    let req = CommandRequest {
        project_id: baseline.project_id().clone(),
        expected_revision: baseline.revision_id().clone(),
        new_revision: RevisionId::new("selected")?,
        command: Command::SetSourceAudioMapping {
            node: target.clone(),
            mapping: selected(),
            offset: AudioSample(-31),
        },
    };
    store.commit(&req)?;
    let after = store.snapshot()?;
    let NodeKind::Source { source } = &after.nodes()[&target].kind else {
        unreachable!()
    };
    assert_eq!(source.audio_mapping, selected());
    assert_eq!(source.audio_offset, AudioSample(-31));
    assert_eq!(baseline.duration()?, after.duration()?);
    assert_eq!(baseline.audio_bindings(), after.audio_bindings());
    assert!(after.nodes().values().any(
        |node| matches!(&node.kind,NodeKind::Hold{recipe} if recipe.picture_context.is_some())
    ));
    assert!(after.nodes().values().any(|node|matches!(&node.kind,NodeKind::Repeat{gap:Some(recipe),..} if recipe.picture_context.is_some())));
    store.undo(after.revision_id(), RevisionId::new("undo-selection")?)?;
    assert_eq!(store.snapshot()?.nodes(), baseline.nodes());
    store.redo(
        &RevisionId::new("undo-selection")?,
        RevisionId::new("redo-selection")?,
    )?;
    assert_eq!(store.snapshot()?.nodes(), after.nodes());
    store.validate()?;
    let expected = store.snapshot()?;
    drop(store);
    assert_eq!(
        ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?,
        expected
    );
    Ok(())
}
