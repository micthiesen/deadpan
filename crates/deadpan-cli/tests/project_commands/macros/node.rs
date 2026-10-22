use super::*;
use deadpan_core::{CommandRequest, FrameRange, ProjectFrame, SliceCaptureSelection};

fn node(value: &str) -> Result<NodeId> {
    Ok(NodeId::new(value)?)
}

fn seed(store: &mut ProjectStore, command: Command, revision: &str) -> Result {
    let before = store.snapshot()?;
    store.commit(&CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: RevisionId::new(revision)?,
        command,
    })?;
    Ok(())
}

fn hold(label: &str, frames: i64) -> Result<BeatNode> {
    Ok(BeatNode::hold(
        label,
        HoldRecipe {
            picture_context: None,
            duration: FrameDuration::new(frames)?,
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    ))
}

#[test]
fn explicit_node_named_calls_reresolve_staged_children_and_keep_durable_closed_captures() -> Result
{
    let scratch = tempfile::tempdir()?;
    let package = create(scratch.path())?;
    let input = scratch.path().join("explicit-node.json");
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let root = writer.snapshot()?.root().clone();
    for (index, id, beat) in [(0, "lead", hold("Lead", 3)?), (1, "tail", hold("Tail", 3)?)] {
        seed(
            &mut writer,
            Command::Insert {
                parent: root.clone(),
                index,
                subtree: Subtree {
                    root: node(id)?,
                    nodes: BTreeMap::from([(node(id)?, beat)]),
                    overrides: Default::default(),
                    gap_overrides: Default::default(),
                },
            },
            &format!("seed-{id}"),
        )?;
    }
    seed(
        &mut writer,
        Command::Insert {
            parent: root.clone(),
            index: 1,
            subtree: Subtree {
                root: node("group")?,
                nodes: BTreeMap::from([
                    (
                        node("group")?,
                        BeatNode::sequence(
                            "Group",
                            vec![
                                node("empty-a")?,
                                node("target")?,
                                node("repeat-me")?,
                                node("empty-b")?,
                            ],
                        ),
                    ),
                    (node("empty-a")?, BeatNode::sequence("Empty A", vec![])),
                    (node("target")?, hold("Whole target", 4)?),
                    (node("repeat-me")?, hold("Distinct repeated target", 6)?),
                    (node("empty-b")?, BeatNode::sequence("Empty B", vec![])),
                ]),
                overrides: Default::default(),
                gap_overrides: Default::default(),
            },
        },
        "seed-group",
    )?;
    let before = writer.snapshot()?;
    assert_eq!(before.duration()?.frames(), 16);
    drop(writer);
    let body = json!([
        {"type":"set_hold_duration","length":{"unit":"frames","frames":5}},
        {"type":"yank","selector":{"type":"node","node":"target"},"register":"c"},
        {"type":"cut","selector":{"type":"node","node":"target"},"register":"d"},
        {"type":"repeat","selector":{"type":"node","node":"repeat-me"},"plays":2}
    ]);
    invoke(&package, &input, &save(&before, 0, "a", body), false)?;
    invoke(
        &package,
        &input,
        &save(
            &before,
            1,
            "b",
            json!([
                {"type":"call","register":"a","count":1}
            ]),
        ),
        false,
    )?;
    let bank = ProjectStore::open(&package, AccessMode::ReadOnly)?.registers()?;
    let mut invocation = run(&before, bank.version, "b", 15);
    invocation["operation"]["selected_child"] = json!("lead");
    invocation["operation"]["new_revision"] = json!("explicit-node-edit");
    let unchanged = state(&package)?;
    let preview = invoke(&package, &input, &invocation, true)?;
    assert_eq!(state(&package)?, unchanged);
    assert!(preview["committed_revision"].is_null());
    assert!(preview["committed_registers"].is_null());
    let trace = preview["trace"].as_array().ok_or("nested call trace")?;
    for (instruction, selected, range) in [
        ("yank", "target", json!({"start":5,"end":9})),
        ("cut", "target", json!({"start":5,"end":9})),
        ("repeat", "repeat-me", json!({"start":5,"end":11})),
    ] {
        let rows: Vec<_> = trace
            .iter()
            .filter(|row| row["instruction"]["type"] == instruction)
            .collect();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["resolved_range"], range);
        assert_eq!(
            rows[0]["resolved_selection"],
            json!({"type":"child","node":selected})
        );
    }
    let result = invoke(&package, &input, &invocation, false)?;
    assert_eq!(result["committed_revision"], "explicit-node-edit");
    let writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let after = writer.snapshot()?;
    assert_eq!(after.duration()?.frames(), 20);
    let bank = writer.registers()?;
    for register in ['c', 'd'] {
        let RegisterValue::Edited { slice } = bank.entries[&RegisterName::new(register)?].as_ref()
        else {
            panic!("whole node capture")
        };
        assert_eq!(slice.parent(), &node("group")?);
        assert_eq!(
            slice.selection(),
            &SliceCaptureSelection::Child {
                node: node("target")?
            }
        );
        assert_eq!(
            slice.range(),
            FrameRange::new(ProjectFrame(5), ProjectFrame(9))?
        );
        let staged = writer.capture_snapshot_at(slice.revision_id())?;
        slice.validate_capture(&staged)?;
        assert_eq!(staged.duration()?.frames(), 18);
        assert_ne!(staged.revision_id(), before.revision_id());
        assert_ne!(staged.revision_id(), after.revision_id());
    }
    let history = writer.validate_report(true)?;
    assert_eq!(history.verified_by_receipt, 0);
    assert_eq!(history.replayed + 1, history.revisions);
    writer.checkpoint()?;
    drop(writer);
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    assert_eq!(writer.snapshot()?, after);
    assert_eq!(writer.registers()?, bank);
    writer.undo(after.revision_id(), RevisionId::new("node-undo")?)?;
    same_document(&writer.snapshot()?, &before)?;
    assert_eq!(writer.registers()?, bank);
    writer.checkpoint()?;
    drop(writer);
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    writer.redo(
        &RevisionId::new("node-undo")?,
        RevisionId::new("node-redo")?,
    )?;
    same_document(&writer.snapshot()?, &after)?;
    assert_eq!(writer.registers()?, bank);
    let restored = writer.snapshot()?;
    drop(writer);
    for (register, suffix) in [
        (
            "f",
            json!({"type":"yank","selector":{"type":"node","node":"missing-node"},"register":"e"}),
        ),
        ("g", json!({"type":"call","register":"z","count":1})),
    ] {
        let version = ProjectStore::open(&package, AccessMode::ReadOnly)?
            .registers()?
            .version;
        invoke(
            &package,
            &input,
            &save(
                &restored,
                version,
                register,
                json!([
                    {"type":"cut","selector":{"type":"node","node":"empty-a"},"register":"e"}, suffix
                ]),
            ),
            false,
        )?;
        let version = ProjectStore::open(&package, AccessMode::ReadOnly)?
            .registers()?
            .version;
        reject(&package, &input, &run(&restored, version, register, 19))?;
    }
    let version = ProjectStore::open(&package, AccessMode::ReadOnly)?
        .registers()?
        .version;
    invoke(
        &package,
        &input,
        &save(
            &restored,
            version,
            "s",
            json!([
                {"type":"select_node","node":"empty-a"}
            ]),
        ),
        false,
    )?;
    let version = ProjectStore::open(&package, AccessMode::ReadOnly)?
        .registers()?
        .version;
    let unchanged = state(&package)?;
    let selected = invoke(&package, &input, &run(&restored, version, "s", 19), false)?;
    assert_eq!(state(&package)?, unchanged);
    assert_eq!(
        selected["context"]["visual_selection"],
        json!({"type":"node","node":"empty-a"})
    );
    assert!(selected["committed_revision"].is_null());
    assert!(selected["committed_registers"].is_null());
    invoke(
        &package,
        &input,
        &save(
            &restored,
            version,
            "e",
            json!([
                {"type":"yank","selector":{"type":"node","node":"empty-a"},"register":"h"},
                {"type":"yank","selector":{"type":"node","node":"empty-b"},"register":"i"}
            ]),
        ),
        false,
    )?;
    let version = ProjectStore::open(&package, AccessMode::ReadOnly)?
        .registers()?
        .version;
    let copied = invoke(&package, &input, &run(&restored, version, "e", 19), false)?;
    assert!(copied["committed_revision"].is_null());
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    assert_eq!(store.snapshot()?, restored);
    let bank = store.registers()?;
    for (register, id, boundary) in [('h', "empty-a", 5), ('i', "empty-b", 17)] {
        let RegisterValue::Edited { slice } = bank.entries[&RegisterName::new(register)?].as_ref()
        else {
            panic!("distinct empty node capture")
        };
        assert_eq!(
            slice.selection(),
            &SliceCaptureSelection::Child { node: node(id)? }
        );
        assert_eq!(slice.parent(), &node("group")?);
        assert_eq!(
            slice.range(),
            FrameRange::new(ProjectFrame(boundary), ProjectFrame(boundary))?
        );
        slice.validate_capture(&restored)?;
    }
    store.validate()?;
    Ok(())
}
