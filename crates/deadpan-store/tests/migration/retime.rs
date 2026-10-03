use super::*;
use deadpan_core::*;
use serde_json::json;

fn request(document: &ProjectDocument, name: &str, command: Command) -> Result<CommandRequest> {
    Ok(CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new(name)?,
        command,
    })
}

fn wrap() -> Result<Command> {
    Ok(Command::WrapRetime {
        node: NodeId::new("first")?,
        id: NodeId::new("retime")?,
        duration: FrameDuration::new(8)?,
        pitch: PitchPolicy::Preserve,
    })
}

fn set() -> Result<Command> {
    Ok(Command::SetRetime {
        node: NodeId::new("retime")?,
        duration: FrameDuration::new(3)?,
        pitch: PitchPolicy::FollowSpeed,
    })
}

fn assert_authored_equal(actual: &ProjectDocument, expected: &ProjectDocument) -> Result {
    let mut expected = serde_json::to_value(expected)?;
    expected["revision_id"] = json!(actual.revision_id());
    assert_eq!(serde_json::to_value(actual)?, expected);
    Ok(())
}

#[test]
fn retime_wrap_update_and_navigation_are_durable_atomic_edits() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("modern-retime.deadpan");
    let initial = super::sequence_insert::nested_initial(true)?;
    let mut store = ProjectStore::create(&package, &initial)?;
    let wrapped_request = request(&initial, "wrap", wrap()?)?;
    let wrapped_preview = store.preview(&wrapped_request)?;
    assert_eq!(store.snapshot()?, initial);
    assert_eq!(store.commit(&wrapped_request)?.edit, wrapped_preview);
    let wrapped = store.snapshot()?;
    assert_eq!(wrapped.duration()?.frames(), 11);
    assert_eq!(wrapped_preview.inverse.apply(&wrapped)?, initial);
    drop(store);
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, wrapped);
    let changed_request = request(&wrapped, "set", set()?)?;
    let changed = store.commit(&changed_request)?.edit;
    let adjusted = store.snapshot()?;
    assert_eq!(adjusted.duration()?.frames(), 6);
    let NodeKind::Retime {
        mapping,
        pitch,
        duration,
        ..
    } = adjusted.nodes()[&NodeId::new("retime")?].kind
    else {
        panic!("retime wrapper missing");
    };
    assert_eq!(mapping, FrameRange::new(ProjectFrame(0), ProjectFrame(4))?);
    assert_eq!(pitch, PitchPolicy::FollowSpeed);
    assert_eq!(duration.frames(), 3);
    assert_eq!(changed.inverse.apply(&adjusted)?, wrapped);
    assert!(store.commit(&changed_request).is_err());
    assert_eq!(store.snapshot()?, adjusted);
    store.undo(adjusted.revision_id(), RevisionId::new("undo-set")?)?;
    assert_authored_equal(&store.snapshot()?, &wrapped)?;
    drop(store);
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let undone = store.snapshot()?;
    store.redo(undone.revision_id(), RevisionId::new("redo-set")?)?;
    assert_authored_equal(&store.snapshot()?, &adjusted)?;
    store.undo(
        &RevisionId::new("redo-set")?,
        RevisionId::new("undo-set-again")?,
    )?;
    store.undo(
        &RevisionId::new("undo-set-again")?,
        RevisionId::new("undo-wrap")?,
    )?;
    assert_authored_equal(&store.snapshot()?, &initial)?;
    drop(store);
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    store.redo(
        &RevisionId::new("undo-wrap")?,
        RevisionId::new("redo-wrap")?,
    )?;
    store.redo(
        &RevisionId::new("redo-wrap")?,
        RevisionId::new("redo-both")?,
    )?;
    assert_authored_equal(&store.snapshot()?, &adjusted)?;
    store.validate()?;
    drop(store);
    ProjectStore::open(&package, AccessMode::ReadOnly)?.validate()?;
    let database = Connection::open(package.join("project.sqlite"))?;
    assert_eq!(history_json(&database)?.len(), 2);
    assert_eq!(docs(&database)?.len(), 9);
    Ok(())
}
