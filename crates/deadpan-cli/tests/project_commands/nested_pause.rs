use super::*;

#[test]
fn nested_pause_dry_run_commit_and_durable_history_share_one_transaction() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = create(scratch.path())?;
    let path = package.to_str().unwrap();
    let input = scratch.path().join("command.json");
    let file = input.to_str().unwrap();
    let empty = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    fs::write(&input, request(&empty)?.to_string())?;
    success(&["command", path, "--json", file])?;
    for name in ["inner", "outer"] {
        let before = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
        fs::write(
            &input,
            json!({"protocol":1,"project_id":before.project_id(),
            "expected_revision":before.revision_id(),"new_revision":format!("group-{name}"),
            "command":Command::Group{parent:before.root().clone(),start:0,end:1,
                id:NodeId::new(name)?,label:name.into()}})
            .to_string(),
        )?;
        success(&["command", path, "--json", file])?;
    }
    let before = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    fs::write(
        &input,
        json!({"protocol":1,"project_id":before.project_id(),
        "expected_revision":before.revision_id(),"new_revision":"nested-pause",
        "command":{"command":"insert_time","at":17,
            "hold":{"duration":11,"video":{"type":"background"},"audio":{"type":"silence"}},
            "id":"pause","identities":{"nodes":["left","right","right-context"]},
            "timing":{"allocation":"nested-pause","ordinal":0}}})
        .to_string(),
    )?;
    let preview = success(&["command", path, "--json", file, "--dry-run"])?;
    assert_eq!(preview["edit"]["duration_delta"], 11);
    assert_eq!(
        ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?,
        before
    );
    let committed = success(&["command", path, "--json", file])?;
    assert_eq!(committed["outcome"]["edit"], preview["edit"]);
    let saved = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    assert_eq!(saved.duration()?.frames(), 56);
    assert_eq!(saved.nodes()[saved.root()], before.nodes()[before.root()]);
    assert_eq!(
        saved.nodes()[&NodeId::new("outer")?],
        before.nodes()[&NodeId::new("outer")?]
    );
    let deadpan_core::NodeKind::Sequence { children } = &saved.nodes()[&NodeId::new("inner")?].kind
    else {
        panic!()
    };
    assert_eq!(
        children.as_slice(),
        [
            NodeId::new("left")?,
            NodeId::new("pause")?,
            NodeId::new("right")?
        ]
    );
    success(&["inspect-plan", path, "--frame", "17"])?;
    assert!(
        !cli(&["command", path, "--json", file])?.status.success(),
        "stale command cannot duplicate the pause"
    );
    success(&["project", "undo", path, "--expected", "nested-pause"])?;
    let undone = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    assert_eq!(undone.nodes(), before.nodes());
    assert_eq!(undone.audio_bindings(), before.audio_bindings());
    assert_ne!(undone.revision_id(), before.revision_id());
    success(&[
        "project",
        "redo",
        path,
        "--expected",
        undone.revision_id().as_str(),
    ])?;
    let redone = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    assert_eq!(redone.nodes(), saved.nodes());
    assert_eq!(redone.audio_bindings(), saved.audio_bindings());
    assert_ne!(redone.revision_id(), saved.revision_id());
    ProjectStore::open(&package, AccessMode::ReadOnly)?.validate()?;
    Ok(())
}
