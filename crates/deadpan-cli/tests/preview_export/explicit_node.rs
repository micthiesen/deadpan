//! Whole authored identities through nested ordinary Sequences and real A/V.
use super::*;
use deadpan_core::{
    Anchor, AnchorLossPolicy, BoundaryAnchor, InsertionBias, InstancePath, MarkId, PauseLength,
    SemanticInstruction, SemanticProgram, SemanticSelector, SliceCaptureSelection,
};
use std::num::NonZeroU32;

fn same_authored(actual: &ProjectDocument, expected: &ProjectDocument) -> Result {
    let mut expected = serde_json::to_value(expected)?;
    expected["revision_id"] = serde_json::to_value(actual.revision_id())?;
    assert_eq!(serde_json::to_value(actual)?, expected);
    Ok(())
}

pub fn build(directory: &Path) -> Result<Fixture> {
    let mut project = Project::create(directory, "explicit-node")?;
    project.shorten()?;
    project.split_root(6)?;
    project.split_root(10)?;
    project.split_root(18)?;
    let held = node("explicit-node-held")?;
    project.apply(|project, document, revision| {
        let target = document.insert_time_target(ProjectFrame(6))?;
        let identities = match target.split {
            Some(split) => project.fresh(split.required_ids)?,
            None => Vec::new(),
        };
        Ok(Command::InsertTime {
            at: ProjectFrame(6),
            hold: silent(2, HoldVideo::Background)?,
            id: held.clone(),
            identities: SplitIdentities { nodes: identities },
            timing: AudioTimingId {
                allocation: revision.clone(),
                ordinal: 0,
            },
        })
    })?;
    let document = project.document()?;
    assert_eq!(document.duration()?.frames(), 32);
    let (cut, _) = root_child_at(&document, 8)?;
    let (repeated, _) = root_child_at(&document, 12)?;
    let group = node("explicit-node-group")?;
    project.apply(|_, document, _| {
        Ok(Command::Group {
            parent: document.root().clone(),
            start: 2,
            end: 4,
            id: group.clone(),
            label: "Explicit node targets".into(),
        })
    })?;
    let document = project.document()?;
    let (host, offset) = source_host(&document, &cut)?;
    let mark = MarkId::new("explicit-node-owned-mark")?;
    project.apply(|_, _, _| {
        Ok(Command::SetMark {
            id: mark.clone(),
            owner: host.clone(),
            label: "Owned by captured node".into(),
            boundary: BoundaryAnchor {
                coordinate: Anchor::Occurrence {
                    instance: InstancePath {
                        node: host.clone(),
                        repeats: Vec::new(),
                    },
                    position: ExactRatio::integer(offset + 1),
                },
                bias: InsertionBias::Right,
            },
            loss_policy: AnchorLossPolicy::DeleteOwned,
        })
    })?;
    let before = project.document()?;
    assert_eq!(
        before.sequence_children(&group, &cut, &cut)?.range,
        FrameRange::new(ProjectFrame(8), ProjectFrame(12))?
    );
    assert_eq!(
        before
            .sequence_children(&group, &repeated, &repeated)?
            .range,
        FrameRange::new(ProjectFrame(12), ProjectFrame(20))?
    );
    let program = SemanticProgram::new(vec![
        SemanticInstruction::SetHoldDuration {
            length: PauseLength::Frames {
                frames: NonZeroU32::new(3).ok_or("positive Hold length")?,
            },
        },
        SemanticInstruction::Yank {
            selector: SemanticSelector::Node { node: cut.clone() },
            register: RegisterName::new('a')?,
        },
        SemanticInstruction::Cut {
            selector: SemanticSelector::Node { node: cut.clone() },
            register: RegisterName::new('b')?,
        },
        SemanticInstruction::Repeat {
            selector: SemanticSelector::Node {
                node: repeated.clone(),
            },
            plays: NonZeroU32::new(2).ok_or("positive play count")?,
            escalation: None,
        },
    ])?;
    project.save_macro('m', serde_json::to_value(program.instructions())?)?;
    let output = project.macro_operation(
        json!({"type":"run","register":"m",
        "parent":before.root(),"cursor":31,"selected_child":held,"count":1}),
        false,
    )?;
    assert!(
        output.status.success(),
        "explicit-node macro: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout)?;
    fs::write(
        project.directory.join("explicit-node-macro.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    let trace = result["trace"].as_array().ok_or("semantic trace")?;
    for (instruction, target, range) in [
        ("yank", &cut, json!({"start":9,"end":13})),
        ("cut", &cut, json!({"start":9,"end":13})),
        ("repeat", &repeated, json!({"start":9,"end":17})),
    ] {
        let rows: Vec<_> = trace
            .iter()
            .filter(|row| row["instruction"]["type"] == instruction)
            .collect();
        assert_eq!(rows.len(), 1, "{instruction}: {trace:?}");
        assert_eq!(rows[0]["resolved_range"], range);
        assert_eq!(
            rows[0]["resolved_selection"],
            json!({"type":"child","node":target})
        );
    }
    let after = project.document()?;
    assert_eq!(after.duration()?.frames(), 37);
    assert_eq!(result["committed_revision"], after.revision_id().as_str());
    assert!(!after.nodes().contains_key(&cut));
    let store = ProjectStore::open(&project.package, AccessMode::ReadOnly)?;
    let bank = store.registers()?;
    let mut captures = Vec::new();
    for register in ['a', 'b'] {
        let RegisterValue::Edited { slice } = bank.entries[&RegisterName::new(register)?].as_ref()
        else {
            return Err(format!("register {register} is not a whole Child capture").into());
        };
        let staged = store.capture_snapshot_at(slice.revision_id())?;
        slice.validate_capture(&staged)?;
        assert_eq!(staged.duration()?.frames(), 33);
        assert_eq!(slice.parent(), &group);
        assert_eq!(
            slice.selection(),
            &SliceCaptureSelection::Child { node: cut.clone() }
        );
        assert_eq!(
            slice.range(),
            FrameRange::new(ProjectFrame(9), ProjectFrame(13))?
        );
        assert_eq!(slice.duration().frames(), 4);
        assert_ne!(slice.revision_id(), before.revision_id());
        assert_ne!(slice.revision_id(), after.revision_id());
        let wire = serde_json::to_value(slice)?;
        assert_eq!(
            wire["parts"],
            json!([{"root":cut,"mapping":{"start":0,"end":4},"source_start":9}])
        );
        assert!(
            !wire["assets"]
                .as_object()
                .ok_or("closed assets")?
                .is_empty()
        );
        assert!(
            wire["marks"]
                .as_object()
                .ok_or("closed marks")?
                .contains_key(mark.as_str())
        );
        captures.push(json!({"register":register.to_string(),"capture_revision":slice.revision_id(),"capture":slice}));
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
        project.directory.join("explicit-node-captures.json"),
        serde_json::to_vec_pretty(&json!({
            "captures":captures,"history_before_navigation":history,"history_after_redo":restored_history,
            "before_revision":before.revision_id(),"macro_revision":after.revision_id(),
            "render_revision":restored.revision_id(),"cut_node":cut,"repeated_node":repeated,"actual_parent":group,
        }))?,
    )?;
    drop(store);
    // Literal provenance from the authored operations, independent of any plan:
    // remove Original18..22, repeat Original22..30, keep Original12..18 and30..42.
    let mut pictures: Vec<_> = (12..18).map(original).collect();
    pictures.extend([Expected::Background; 3]);
    for _ in 0..2 {
        pictures.extend((22..30).map(original));
    }
    pictures.extend((30..42).map(original));
    assert_eq!(pictures.len(), 37);
    project.finish(vec!["Repeat operator", "Hold (insert time)"],
        pictures.into_iter().enumerate().map(|(frame, picture)| (frame as u64, picture)).collect(),
        vec!["Real retained cfr-bframes.mp4 A/V at 30000/1001. A public macro grows a direct silent Hold 2 to 3 frames, then resolves explicit whole authored IDs beneath an ordinary nested Group while the cursor and selected beat target other content. It Yanks and Cuts Original18..22 at [9,13), then repeats the distinct Original22..30 node at its new [9,17) twice. Both closed Child captures retain their actual Group parent, qualified asset and owned mark. Full command-history replay, public Undo/Redo and register persistence are checked. All 37 pictures have independent ordinal/background expectations. Encoded audio is compared against canonical preview; no independent PCM oracle is claimed.".into()])
}
