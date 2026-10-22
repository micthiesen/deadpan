//! Qualified Original A/V through one discontiguous editable union.
use super::*;
use deadpan_core::{
    Anchor, AnchorLossPolicy, BeatNode, BoundaryAnchor, InsertionBias, MarkId, SemanticInstruction,
    SemanticProgram, SemanticSelector, SliceCaptureSelection, SourceMoment, SourceStream,
    SourceTimeBase, Subtree,
};

fn same_authored(actual: &ProjectDocument, expected: &ProjectDocument) -> Result {
    let mut expected = serde_json::to_value(expected)?;
    expected["revision_id"] = json!(actual.revision_id());
    assert_eq!(serde_json::to_value(actual)?, expected);
    Ok(())
}

pub fn build(directory: &Path) -> Result<Fixture> {
    let mut project = Project::create(directory, "node-sets")?;
    project.shorten()?;
    // First separate the unselected suffix. The mark is then owned only by
    // the selected 18-frame prefix, before its public Split makes two bindings.
    project.split_root(18)?;
    let document = project.document()?;
    let (prefix, _) = root_child_at(&document, 0)?;
    let (host, _) = source_host(&document, &prefix)?;
    let mark_id = MarkId::new("node-sets-shared-cue")?;
    project.apply(|project, _, _| {
        Ok(Command::SetMark {
            id: mark_id.clone(),
            owner: host.clone(),
            label: "One shared Original cue".into(),
            boundary: BoundaryAnchor {
                coordinate: Anchor::Source {
                    asset: project.asset.clone(),
                    moment: SourceMoment::Timestamp {
                        stream: SourceStream::Video,
                        // Fixture recipe: CFR ordinal 24 has this exact PTS.
                        timestamp: SourceTimestamp {
                            ticks: 24,
                            time_base: SourceTimeBase::new(1001, 30_000)?,
                        },
                    },
                },
                bias: InsertionBias::Right,
            },
            loss_policy: AnchorLossPolicy::DeleteOwned,
        })
    })?;
    assert_eq!(project.document()?.marks()[&mark_id].binding_count(), 1);
    project.split_root(6)?;
    let split = project.document()?;
    assert_eq!(split.marks()[&mark_id].binding_count(), 2);
    let (left, _) = root_child_at(&split, 0)?;
    let (right, _) = root_child_at(&split, 6)?;
    let gap = node("node-sets-gap")?;
    project.apply(|project, document, revision| {
        let target = document.insert_time_target(ProjectFrame(6))?;
        let nodes = match target.split {
            Some(split) => project.fresh(split.required_ids)?,
            None => Vec::new(),
        };
        Ok(Command::InsertTime {
            at: ProjectFrame(6),
            hold: silent(2, HoldVideo::Background)?,
            id: gap.clone(),
            identities: SplitIdentities { nodes },
            timing: AudioTimingId {
                allocation: revision.clone(),
                ordinal: 0,
            },
        })
    })?;
    let left_parent = node("node-sets-left-parent")?;
    let right_parent = node("node-sets-right-parent")?;
    for (start, end, id, label) in [
        (0, 1, left_parent.clone(), "Left actual parent"),
        (2, 3, right_parent.clone(), "Right actual parent"),
    ] {
        project.apply(|_, document, _| {
            Ok(Command::Group {
                parent: document.root().clone(),
                start,
                end,
                id,
                label: label.into(),
            })
        })?;
    }
    let empty_a = node("node-sets-empty-a")?;
    let empty_b = node("node-sets-empty-b")?;
    for (parent, index, id, label) in [
        (left_parent.clone(), 0, empty_a.clone(), "Selected empty A"),
        (right_parent.clone(), 1, empty_b.clone(), "Selected empty B"),
    ] {
        project.apply(|_, _, _| {
            Ok(Command::Insert {
                parent,
                index,
                subtree: Subtree {
                    root: id.clone(),
                    nodes: std::collections::BTreeMap::from([(
                        id,
                        BeatNode::sequence(label, Vec::new()),
                    )]),
                    overrides: Default::default(),
                    gap_overrides: Default::default(),
                },
            })
        })?;
    }
    let before = project.document()?;
    assert_eq!(before.duration()?.frames(), 32);
    assert_eq!(before.marks()[&mark_id].binding_count(), 2);
    let canonical = [
        empty_a.clone(),
        left.clone(),
        right.clone(),
        empty_b.clone(),
    ];
    let requested = vec![
        empty_b.clone(),
        right.clone(),
        empty_a.clone(),
        left.clone(),
    ];
    let program = SemanticProgram::new(vec![
        SemanticInstruction::SelectNodes { nodes: requested },
        SemanticInstruction::Yank {
            selector: SemanticSelector::VisualSelection,
            register: RegisterName::new('a')?,
        },
        SemanticInstruction::Cut {
            selector: SemanticSelector::VisualSelection,
            register: RegisterName::new('b')?,
        },
        SemanticInstruction::ClearSelection,
        SemanticInstruction::Paste {
            register: RegisterName::new('b')?,
            before: false,
        },
    ])?;
    project.save_macro('m', serde_json::to_value(program.instructions())?)?;
    let output = project.macro_operation(
        json!({"type":"run","register":"m",
        "parent":before.root(),"cursor":31,"selected_child":right_parent,"count":1}),
        false,
    )?;
    assert!(
        output.status.success(),
        "node-union macro: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout)?;
    fs::write(
        project.directory.join("node-sets-macro.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    let expected_ranges = json!([
        {"start":0,"end":0},{"start":0,"end":6},{"start":8,"end":20},{"start":20,"end":20},
    ]);
    let trace = result["trace"].as_array().ok_or("semantic trace")?;
    for instruction in ["yank", "cut"] {
        let rows: Vec<_> = trace
            .iter()
            .filter(|row| row["instruction"]["type"] == instruction)
            .collect();
        assert_eq!(rows.len(), 1, "{instruction}: {trace:?}");
        let row = rows[0];
        assert!(row["resolved_range"].is_null());
        assert!(row["resolved_selection"].is_null());
        assert_eq!(row["resolved_ranges"], expected_ranges);
        assert_eq!(row["capture_set"]["parent"], json!(before.root()));
        let members = row["resolved_nodes"]
            .as_array()
            .ok_or("resolved union members")?;
        assert_eq!(
            members
                .iter()
                .map(|member| member["node"].clone())
                .collect::<Vec<_>>(),
            canonical.iter().map(|id| json!(id)).collect::<Vec<_>>()
        );
        assert_eq!(
            members
                .iter()
                .map(|member| member["parent"].clone())
                .collect::<Vec<_>>(),
            vec![
                json!(left_parent),
                json!(left_parent),
                json!(right_parent),
                json!(right_parent)
            ]
        );
    }
    let after = project.document()?;
    assert_eq!(after.duration()?.frames(), 32);
    assert_eq!(result["committed_revision"], json!(after.revision_id()));
    assert_eq!(after.nodes()[&gap], before.nodes()[&gap]);
    for id in &canonical {
        assert!(!after.nodes().contains_key(id));
    }
    assert!(!after.marks().contains_key(&mark_id));
    assert_eq!(
        after.marks().len(),
        1,
        "one copied logical mark, not one per member"
    );
    let pasted_mark = after.marks().values().next().ok_or("pasted mark")?;
    let original_mark = &before.marks()[&mark_id];
    assert_eq!(pasted_mark.binding_count(), 2);
    assert_eq!(pasted_mark.label, original_mark.label);
    assert_eq!(pasted_mark.loss_policy, original_mark.loss_policy);
    assert_eq!(pasted_mark.boundary.bias, original_mark.boundary.bias);
    for (copied, original) in pasted_mark.bindings().zip(original_mark.bindings()) {
        assert_eq!(copied.coordinate, original.coordinate);
        assert_eq!(copied.state, original.state);
        assert!(after.nodes().contains_key(&copied.owner));
    }
    let store = ProjectStore::open(&project.package, AccessMode::ReadOnly)?;
    let bank = store.registers()?;
    let mut captures = Vec::new();
    for register in ['a', 'b'] {
        let RegisterValue::EditedSet { set } = bank.entries[&RegisterName::new(register)?].as_ref()
        else {
            return Err(format!("register {register} did not retain a complete union").into());
        };
        assert_eq!(set.revision_id(), before.revision_id());
        assert_eq!(set.scope(), before.root());
        assert_eq!(set.duration().frames(), 18);
        assert_eq!(set.components().len(), 4);
        let source = store.capture_snapshot_at(set.revision_id())?;
        assert_eq!(source, before);
        set.validate_capture(&source)?;
        for (component, id) in set.components().iter().zip(&canonical) {
            assert_eq!(
                component.selection(),
                &SliceCaptureSelection::Child { node: id.clone() }
            );
            assert_eq!(component.revision_id(), before.revision_id());
            component.validate_capture(&source)?;
        }
        let wire = serde_json::to_value(set.as_ref())?;
        assert_eq!(wire["marks"].as_object().ok_or("union marks")?.len(), 1);
        assert_eq!(set.identity_requirements()?.marks, 1);
        assert!(
            !set.assets().is_empty(),
            "qualified Original asset is retained"
        );
        captures.push(json!({"register":register.to_string(),"capture":set}));
    }
    let history = store.validate_report(true)?;
    assert_eq!(history.verified_by_receipt, 0);
    assert_eq!(history.replayed + 1, history.revisions);
    drop(store);
    success(&[
        "project",
        "undo",
        project.path(),
        "--expected",
        after.revision_id().as_str(),
    ])?;
    let undone = project.document()?;
    same_authored(&undone, &before)?;
    assert_eq!(
        ProjectStore::open(&project.package, AccessMode::ReadOnly)?.registers()?,
        bank
    );
    success(&[
        "project",
        "redo",
        project.path(),
        "--expected",
        undone.revision_id().as_str(),
    ])?;
    let restored = project.document()?;
    same_authored(&restored, &after)?;
    let store = ProjectStore::open(&project.package, AccessMode::ReadOnly)?;
    assert_eq!(store.registers()?, bank);
    let restored_history = store.validate_report(true)?;
    assert_eq!(restored_history.verified_by_receipt, 0);
    assert_eq!(restored_history.replayed + 1, restored_history.revisions);
    fs::write(
        project.directory.join("node-sets-captures.json"),
        serde_json::to_vec_pretty(&json!({
            "captures":captures,"history_before_navigation":history,"history_after_redo":restored_history,
            "before_revision":before.revision_id(),"macro_revision":after.revision_id(),
            "render_revision":restored.revision_id(),"canonical_nodes":canonical,
        }))?,
    )?;
    drop(store);
    // Authored oracle: selected Original12..18 +18..30 concatenate at the
    // original left seam; the unselected gap stays 2f and suffix stays 30..42.
    let mut pictures: Vec<_> = (12..30).map(original).collect();
    pictures.extend([Expected::Background; 2]);
    pictures.extend((30..42).map(original));
    assert_eq!(pictures.len(), 32);
    project.finish(vec!["Discontiguous whole-node union"],
        pictures.into_iter().enumerate().map(|(frame,picture)|(frame as u64,picture)).collect(),
        vec!["Qualified retained cfr-bframes.mp4 A/V at 30000/1001. Public Source-clock SetMark followed by Split gives one logical mark with two selected physical bindings. Reverse-listed explicit nodes contain Original 12..18, Original 18..30 and two empty Sequence identities beneath different ordinary parents, separated by an unselected 2f Background gap. One public macro selects, copies, cuts, clears and seam-pastes the complete 18f union. The shared mark remains one copied logical mark with both bindings; the gap remains 2f and Original 30..42 stays intact. Full immutable-capture admission, chronological replay, public Undo/Redo and reopened register equality pass. All 32 pictures have an independent ordinal/background oracle; full encoded audio compares against canonical preview, without an independent PCM claim.".into()])
}
