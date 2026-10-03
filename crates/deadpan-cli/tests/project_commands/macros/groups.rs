use super::*;
use deadpan_core::{
    Anchor, AnchorLossPolicy, BoundaryAnchor, CommandRequest, ExactRatio, InsertionBias, MarkId,
    MarkLossReason, MarkState, NodeKind,
};

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

fn seeded(root: &Path, input: &Path) -> Result<std::path::PathBuf> {
    let package = create(root)?;
    let initial = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    fs::write(input, request(&initial)?.to_string())?;
    success(&[
        "command",
        package.to_str().unwrap(),
        "--json",
        input.to_str().unwrap(),
    ])?;
    Ok(package)
}

#[test]
fn range_group_dry_run_late_failure_and_reopen_preserve_exact_bank_and_history() -> Result {
    let scratch = tempfile::tempdir()?;
    let input = scratch.path().join("groups.json");
    let package = seeded(scratch.path(), &input)?;
    let before = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let label = "  A phrase 🎵  ";
    let group = json!({"type":"group","selector":{"type":"visual_selection"},"label":label});
    invoke(
        &package,
        &input,
        &save(&before, 0, "a", json!([group.clone()])),
        false,
    )?;
    // The first two steps succeed, then the selected promoted Hold cannot ungroup.
    invoke(
        &package,
        &input,
        &save(
            &before,
            1,
            "b",
            json!([
                group, {"type":"ungroup"}, {"type":"ungroup"}
            ]),
        ),
        false,
    )?;
    let bank = ProjectStore::open(&package, AccessMode::ReadOnly)?.registers()?;
    for label in ["x".repeat(1025), "bad\0label".into()] {
        reject(
            &package,
            &input,
            &save(
                &before,
                bank.version,
                "z",
                json!([
                    {"type":"group","selector":{"type":"selected_beat"},"label":label}
                ]),
            ),
        )?;
    }
    let mut invocation = run(&before, bank.version, "a", 3);
    invocation["operation"]["visual_selection"] =
        json!({"type":"time","anchor":18,"head":3,"extending":true});
    let unchanged = state(&package)?;
    let preview = invoke(&package, &input, &invocation, true)?;
    assert_eq!(state(&package)?, unchanged);
    assert_eq!(preview["context"]["cursor"], 3);
    assert!(preview["context"]["visual_selection"].is_null());
    assert_eq!(
        preview["trace"][1]["resolved_range"],
        json!({"start":3,"end":18})
    );
    assert_eq!(
        preview["trace"][1]["resolved_selection"],
        json!({"type":"range","range":{"start":3,"end":18}})
    );
    assert_eq!(preview["edit"]["duration_delta"], 0);
    assert_eq!(preview["mark_changes"], json!([]));
    assert!(preview["committed_revision"].is_null());
    assert!(preview["committed_registers"].is_null());
    let mut failed = invocation.clone();
    failed["operation"]["register"] = json!("b");
    reject(&package, &input, &failed)?;
    let result = invoke(&package, &input, &invocation, false)?;
    assert_eq!(result["committed_revision"], "macro-cut");
    assert!(result["committed_registers"].is_null());
    assert_eq!(result["mark_changes"], preview["mark_changes"]);
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let after = writer.snapshot()?;
    let selected: NodeId = serde_json::from_value(result["context"]["selected_child"].clone())?;
    assert_eq!(after.nodes()[&selected].label, label);
    assert!(
        matches!(&after.nodes()[&selected].kind, NodeKind::Sequence { children } if children.len() == 1)
    );
    assert_eq!(after.duration()?, before.duration()?);
    assert_eq!(writer.registers()?, bank);
    writer.undo(after.revision_id(), RevisionId::new("group-undo")?)?;
    same_document(&writer.snapshot()?, &before)?;
    assert_eq!(writer.registers()?, bank);
    writer.checkpoint()?;
    drop(writer);
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    writer.redo(
        &RevisionId::new("group-undo")?,
        RevisionId::new("group-redo")?,
    )?;
    same_document(&writer.snapshot()?, &after)?;
    assert_eq!(writer.registers()?, bank);
    writer.validate()?;
    Ok(())
}

#[test]
fn counted_ungroup_reports_final_mark_losses_and_restores_all_bindings_on_undo() -> Result {
    let scratch = tempfile::tempdir()?;
    let input = scratch.path().join("ungroup.json");
    let package = seeded(scratch.path(), &input)?;
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    for name in ["inner", "outer"] {
        let parent = writer.snapshot()?.root().clone();
        seed(
            &mut writer,
            Command::Group {
                parent,
                start: 0,
                end: 1,
                id: NodeId::new(name)?,
                label: name.into(),
            },
            &format!("seed-{name}"),
        )?;
    }
    let root = writer.snapshot()?.root().clone();
    for (id, owner, host, loss_policy) in [
        ("z-host", root, "outer", AnchorLossPolicy::KeepUnresolved),
        (
            "a-owned",
            NodeId::new("inner")?,
            "inner",
            AnchorLossPolicy::DeleteOwned,
        ),
    ] {
        seed(
            &mut writer,
            Command::SetMark {
                id: MarkId::new(id)?,
                owner,
                label: id.into(),
                boundary: BoundaryAnchor {
                    coordinate: Anchor::Local {
                        node: NodeId::new(host)?,
                        position: ExactRatio::new(3, 1)?,
                    },
                    bias: InsertionBias::Right,
                },
                loss_policy,
            },
            &format!("seed-{id}"),
        )?;
    }
    let before = writer.snapshot()?;
    drop(writer);
    invoke(
        &package,
        &input,
        &save(&before, 0, "a", json!([{"type":"ungroup"}])),
        false,
    )?;
    let bank = ProjectStore::open(&package, AccessMode::ReadOnly)?.registers()?;
    let mut invocation = run(&before, bank.version, "a", 40);
    invocation["operation"]["selected_child"] = json!("outer");
    invocation["operation"]["count"] = json!(2);
    for selected in [Value::Null, json!("inner"), json!("hold")] {
        let mut failed = invocation.clone();
        failed["operation"]["selected_child"] = selected;
        reject(&package, &input, &failed)?;
    }
    for end in [0, 4] {
        let mut failed = invocation.clone();
        failed["operation"]["visual_selection"] =
            json!({"type":"time","anchor":0,"head":end,"extending":false});
        reject(&package, &input, &failed)?;
    }
    let mut late = invocation.clone();
    late["operation"]["count"] = json!(3);
    reject(&package, &input, &late)?;
    let unchanged = state(&package)?;
    let preview = invoke(&package, &input, &invocation, true)?;
    assert_eq!(state(&package)?, unchanged);
    assert_eq!(preview["context"]["selected_child"], "hold");
    assert_eq!(preview["context"]["cursor"], 0);
    assert_eq!(
        preview["trace"][1]["resolved_selection"],
        json!({"type":"child","node":"outer"})
    );
    assert_eq!(
        preview["trace"][2]["resolved_selection"],
        json!({"type":"child","node":"inner"})
    );
    let expected_unresolved = MarkState::Unresolved {
        reason: MarkLossReason::HostMissing,
    };
    let changes = preview["mark_changes"].as_array().unwrap();
    assert_eq!(changes.len(), 2);
    assert_eq!(changes[0]["id"], "a-owned");
    assert_eq!(
        changes[0]["before"],
        serde_json::to_value(&before.marks()[&MarkId::new("a-owned")?])?
    );
    assert!(changes[0]["after"].is_null());
    assert_eq!(changes[1]["id"], "z-host");
    assert_eq!(
        changes[1]["after"]["state"],
        serde_json::to_value(&expected_unresolved)?
    );
    let result = invoke(&package, &input, &invocation, false)?;
    assert_eq!(result["mark_changes"], preview["mark_changes"]);
    assert!(result["committed_registers"].is_null());
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let after = writer.snapshot()?;
    assert_eq!(after.marks().len(), 1);
    assert_eq!(
        after.marks()[&MarkId::new("z-host")?].state,
        expected_unresolved
    );
    assert_eq!(writer.registers()?, bank);
    // Both wrappers and every original mark binding return in one Undo.
    writer.undo(after.revision_id(), RevisionId::new("ungroup-undo")?)?;
    same_document(&writer.snapshot()?, &before)?;
    writer.checkpoint()?;
    drop(writer);
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    writer.redo(
        &RevisionId::new("ungroup-undo")?,
        RevisionId::new("ungroup-redo")?,
    )?;
    same_document(&writer.snapshot()?, &after)?;
    assert_eq!(writer.registers()?, bank);
    writer.validate()?;
    Ok(())
}

#[test]
fn object_visual_yank_and_exact_group_replacement_share_dry_run_and_commit() -> Result {
    let scratch = tempfile::tempdir()?;
    let input = scratch.path().join("group-objects.json");
    let package = seeded(scratch.path(), &input)?;
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let root = writer.snapshot()?.root().clone();
    seed(
        &mut writer,
        Command::Group {
            parent: root.clone(),
            start: 0,
            end: 1,
            id: NodeId::new("object-group")?,
            label: "Object group".into(),
        },
        "seed-object-group",
    )?;
    let before = writer.snapshot()?;
    drop(writer);
    invoke(
        &package,
        &input,
        &save(
            &before,
            0,
            "a",
            json!([
                {"type":"select_object","object":{"type":"inner_group"}},
                {"type":"yank","selector":{"type":"visual_selection"},"register":"b"},
            ]),
        ),
        false,
    )?;
    let mut copy = run(&before, 1, "a", 0);
    copy["operation"]["selected_child"] = json!("object-group");
    let mut untagged = copy.clone();
    untagged["operation"]["visual_selection"] = json!({"anchor":0,"head":1,"extending":false});
    assert!(Request::from_json(untagged.to_string().as_bytes()).is_err());
    let unchanged = state(&package)?;
    let preview = invoke(&package, &input, &copy, true)?;
    assert_eq!(state(&package)?, unchanged);
    assert_eq!(preview["context"]["parent"], json!(root));
    assert_eq!(
        preview["context"]["visual_selection"],
        json!({
            "type":"object","selection":{
                "kind":{"type":"inner_group"},"group":"object-group"
            },"extending":false
        })
    );
    assert_eq!(preview["trace"][2]["resolved_parent"], "object-group");
    assert_eq!(
        preview["trace"][2]["capture"]["scope"],
        json!(["object-group"])
    );
    assert_eq!(
        preview["trace"][2]["capture"]["scope_labels"],
        json!(["Object group"])
    );
    let copied = invoke(&package, &input, &copy, false)?;
    assert_eq!(copied["context"], preview["context"]);
    assert!(copied["committed_revision"].is_null());
    assert_eq!(copied["committed_registers"]["bank_version"], 2);
    let reader = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    assert_eq!(reader.snapshot()?, before);
    let bank = reader.registers()?;
    let RegisterValue::Edited { slice } = bank.entries[&RegisterName::new('b')?].as_ref() else {
        panic!("inner group object copy")
    };
    assert_eq!(slice.parent(), &NodeId::new("object-group")?);
    assert_eq!(
        slice.selection(),
        &deadpan_core::SliceCaptureSelection::Children {
            first: NodeId::new("hold")?,
            last: NodeId::new("hold")?,
        }
    );
    slice.validate_capture(&before)?;
    drop(reader);

    invoke(
        &package,
        &input,
        &save(
            &before,
            2,
            "c",
            json!([
                {"type":"select_object","object":{"type":"around_group"}},
                {"type":"replace_selection","register":"b"},
            ]),
        ),
        false,
    )?;
    let mut replace = run(&before, 3, "c", 0);
    replace["operation"]["selected_child"] = json!("object-group");
    replace["operation"]["new_revision"] = json!("replace-object-group");
    let unchanged = state(&package)?;
    let preview = invoke(&package, &input, &replace, true)?;
    assert_eq!(state(&package)?, unchanged);
    assert_eq!(preview["trace"][2]["resolved_parent"], json!(root));
    assert_eq!(
        preview["trace"][2]["resolved_selection"],
        json!({"type":"child","node":"object-group"})
    );
    assert!(preview["context"]["visual_selection"].is_null());
    assert_eq!(preview["context"]["parent"], json!(root));
    let result = invoke(&package, &input, &replace, false)?;
    // Separate dry-run and commit invocations allocate independent new owners.
    for field in ["parent", "cursor", "visual_selection"] {
        assert_eq!(result["context"][field], preview["context"][field]);
    }
    let preview_selected: NodeId =
        serde_json::from_value(preview["context"]["selected_child"].clone())?;
    assert!(!before.nodes().contains_key(&preview_selected));
    assert!(
        preview["edit"]["changed_ids"]
            .as_array()
            .unwrap()
            .contains(&json!(preview_selected))
    );
    assert_eq!(result["committed_revision"], "replace-object-group");
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let after = writer.snapshot()?;
    let selected: NodeId = serde_json::from_value(result["context"]["selected_child"].clone())?;
    assert!(!before.nodes().contains_key(&selected));
    assert_eq!(after.children(&root).collect::<Vec<_>>(), vec![&selected]);
    assert!(!after.nodes().contains_key(&NodeId::new("object-group")?));
    assert_eq!(after.duration()?, before.duration()?);
    assert_eq!(
        writer.registers()?.entries[&RegisterName::new('b')?],
        bank.entries[&RegisterName::new('b')?]
    );
    writer.undo(after.revision_id(), RevisionId::new("undo-object-replace")?)?;
    same_document(&writer.snapshot()?, &before)?;
    writer.validate()?;
    Ok(())
}

#[test]
fn around_group_cut_from_inside_returns_outer_context_and_durable_copy() -> Result {
    let scratch = tempfile::tempdir()?;
    let input = scratch.path().join("around-cut.json");
    let package = seeded(scratch.path(), &input)?;
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let root = writer.snapshot()?.root().clone();
    seed(
        &mut writer,
        Command::Group {
            parent: root.clone(),
            start: 0,
            end: 1,
            id: NodeId::new("object-group")?,
            label: "Object group".into(),
        },
        "seed-around-cut",
    )?;
    let before = writer.snapshot()?;
    drop(writer);
    invoke(
        &package,
        &input,
        &save(
            &before,
            0,
            "a",
            json!([
                {"type":"cut","selector":{"type":"text_object","object":{"type":"around_group"}},"register":"b"}
            ]),
        ),
        false,
    )?;
    let mut cut = run(&before, 1, "a", 0);
    cut["operation"]["parent"] = json!("object-group");
    cut["operation"]["selected_child"] = json!("hold");
    let unchanged = state(&package)?;
    let preview = invoke(&package, &input, &cut, true)?;
    assert_eq!(state(&package)?, unchanged);
    assert_eq!(preview["trace"][1]["parent"], "object-group");
    assert_eq!(preview["trace"][1]["resolved_parent"], json!(root));
    assert_eq!(preview["trace"][1]["capture"]["scope"], json!([]));
    assert_eq!(preview["context"]["parent"], json!(root));
    assert!(preview["context"]["selected_child"].is_null());
    assert_eq!(preview["context"]["cursor"], 0);
    let result = invoke(&package, &input, &cut, false)?;
    assert_eq!(result["context"], preview["context"]);
    assert_eq!(result["committed_revision"], "macro-cut");
    assert_eq!(result["committed_registers"]["bank_version"], 2);
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let after = writer.snapshot()?;
    assert_eq!(after.duration()?.frames(), 0);
    let bank = writer.registers()?;
    let RegisterValue::Edited { slice } = bank.entries[&RegisterName::new('b')?].as_ref() else {
        panic!("around group cut copy")
    };
    assert_eq!(slice.parent(), &root);
    assert_eq!(
        slice.selection(),
        &deadpan_core::SliceCaptureSelection::Child {
            node: NodeId::new("object-group")?,
        }
    );
    slice.validate_capture(&before)?;
    writer.undo(after.revision_id(), RevisionId::new("undo-around-cut")?)?;
    same_document(&writer.snapshot()?, &before)?;
    assert_eq!(writer.registers()?, bank);
    writer.validate()?;
    Ok(())
}

#[test]
fn empty_inner_group_is_a_visual_object_but_cannot_be_yanked_as_an_empty_forest() -> Result {
    let scratch = tempfile::tempdir()?;
    let input = scratch.path().join("empty-group-object.json");
    let package = seeded(scratch.path(), &input)?;
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let parent = writer.snapshot()?.root().clone();
    seed(
        &mut writer,
        Command::Insert {
            parent: parent.clone(),
            index: 0,
            subtree: Subtree {
                root: NodeId::new("empty-object")?,
                nodes: BTreeMap::from([(
                    NodeId::new("empty-object")?,
                    BeatNode::sequence("Empty object", vec![]),
                )]),
                overrides: Default::default(),
                gap_overrides: Default::default(),
            },
        },
        "seed-empty-object",
    )?;
    let before = writer.snapshot()?;
    drop(writer);
    invoke(
        &package,
        &input,
        &save(
            &before,
            0,
            "a",
            json!([
                {"type":"select_object","object":{"type":"inner_group"}}
            ]),
        ),
        false,
    )?;
    let mut select = run(&before, 1, "a", 0);
    select["operation"]["selected_child"] = json!("empty-object");
    let unchanged = state(&package)?;
    let selected = invoke(&package, &input, &select, false)?;
    assert_eq!(state(&package)?, unchanged);
    assert!(selected["committed_revision"].is_null());
    assert!(selected["committed_registers"].is_null());
    assert_eq!(selected["context"]["parent"], json!(parent));
    assert_eq!(selected["context"]["cursor"], 0);
    assert_eq!(
        selected["context"]["visual_selection"],
        json!({
            "type":"object","selection":{
                "kind":{"type":"inner_group"},"group":"empty-object"
            },"extending":true
        })
    );
    invoke(
        &package,
        &input,
        &save(
            &before,
            1,
            "b",
            json!([
                {"type":"select_object","object":{"type":"inner_group"}},
                {"type":"yank","selector":{"type":"visual_selection"},"register":"c"}
            ]),
        ),
        false,
    )?;
    let mut yank = run(&before, 2, "b", 0);
    yank["operation"]["selected_child"] = json!("empty-object");
    let failed = reject(&package, &input, &yank)?;
    assert!(failed["error"].is_object());
    Ok(())
}
