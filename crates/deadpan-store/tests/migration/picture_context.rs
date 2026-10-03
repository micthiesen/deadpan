use super::*;
use deadpan_core::*;
fn fixture(root: &Path) -> Result<PathBuf> {
    let package = current_fixture(
        root,
        include_str!("../fixtures/current-picture_context.json"),
        None,
    )?;
    pending_rename(
        &package,
        ProjectStore::open(&package, AccessMode::ReadOnly)?
            .snapshot()?
            .root()
            .clone(),
        "Retained picture redo",
    )?;
    Ok(package)
}

fn captured() -> CapturedFraming {
    CapturedFraming::capture(
        None,
        CapturedCanvas {
            width: 16,
            height: 16,
            fit: CapturedFit::Fill,
            layers: vec![Some(FramingPose::identity())],
        },
    )
    .unwrap()
}

#[test]
fn captured_picture_context_survives_current_history_and_pending_redo() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = fixture(scratch.path())?;
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let head = store.snapshot()?;
    assert!(head.nodes().values().any(|node| node.framing.is_some()));
    store.redo(head.revision_id(), RevisionId::new("v24-pending-redo")?)?;
    let redone = store.snapshot()?;
    store.undo(redone.revision_id(), RevisionId::new("v24-undo-redo")?)?;
    let before_capture = store.snapshot()?;
    let target = before_capture
        .nodes()
        .iter()
        .find(|(_, node)| matches!(node.kind, NodeKind::Hold { .. }))
        .unwrap()
        .0
        .clone();
    let req = CommandRequest {
        project_id: before_capture.project_id().clone(),
        expected_revision: before_capture.revision_id().clone(),
        new_revision: RevisionId::new("capture")?,
        command: Command::SetHoldPictureContext {
            node: target.clone(),
            context: Some(captured()),
        },
    };
    store.commit(&req)?;
    let after = store.snapshot()?;
    let NodeKind::Hold { recipe } = &after.nodes()[&target].kind else {
        unreachable!()
    };
    assert_eq!(recipe.picture_context, Some(captured()));
    assert_eq!(after.duration()?, before_capture.duration()?);
    assert_eq!(after.audio_bindings(), before_capture.audio_bindings());
    store.undo(after.revision_id(), RevisionId::new("undo-capture")?)?;
    store.redo(
        &RevisionId::new("undo-capture")?,
        RevisionId::new("redo-capture")?,
    )?;
    store.validate()?;
    let expected = store.snapshot()?;
    drop(store);
    assert_eq!(
        ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?,
        expected
    );
    Ok(())
}
