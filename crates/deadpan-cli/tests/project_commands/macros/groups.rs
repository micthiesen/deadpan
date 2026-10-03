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
    invocation["operation"]["visual_selection"] = json!({"anchor":18,"head":3,"extending":true});
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
        failed["operation"]["visual_selection"] = json!({"anchor":0,"head":end,"extending":false});
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
