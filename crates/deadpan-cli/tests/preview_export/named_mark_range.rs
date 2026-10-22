//! Real retained Original and staged named-range edits through the public CLI.
use super::*;
use deadpan_core::{
    Anchor, AnchorIndex, AnchorLossPolicy, BoundaryAnchor, BoundarySelector, InsertionBias,
    InstancePath, MarkId, MediaRole, NamedMarkTarget, PauseLength, ResolvedSelectionKind,
    SelectionRequest, SemanticInstruction, SemanticProgram, SemanticSelector,
};
use std::num::NonZeroU32;

fn selector(first: char, last: char) -> Result<SemanticSelector> {
    let target = |letter| -> Result<NamedMarkTarget> {
        Ok(NamedMarkTarget {
            id: MarkId::new(format!("native-mark-{letter}"))?,
            occurrence: None,
        })
    };
    Ok(SemanticSelector::MarkRange {
        start: target(first)?,
        end: target(last)?,
    })
}

fn same_authored(actual: &ProjectDocument, expected: &ProjectDocument) -> Result {
    let mut expected = serde_json::to_value(expected)?;
    expected["revision_id"] = serde_json::to_value(actual.revision_id())?;
    assert_eq!(serde_json::to_value(actual)?, expected);
    Ok(())
}

fn assert_range(document: &ProjectDocument, first: char, last: char, from: i64, to: i64) -> Result {
    let SemanticSelector::MarkRange { start, end } = selector(first, last)? else {
        unreachable!("named range helper")
    };
    let selected = AnchorIndex::new(document)?.resolve(&SelectionRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        selector: BoundarySelector::MarkRange { start, end },
        role: MediaRole::Linked,
    })?;
    let ResolvedSelectionKind::Range { start, end, frames } = selected.selection else {
        return Err("named marks must resolve a range".into());
    };
    assert_eq!(start.exact_frame, ExactRatio::integer(from));
    assert_eq!(end.exact_frame, ExactRatio::integer(to));
    assert_eq!(
        frames,
        FrameRange::new(ProjectFrame(from), ProjectFrame(to))?
    );
    Ok(())
}

pub fn build(directory: &Path) -> Result<Fixture> {
    let mut project = Project::create(directory, "named-mark-range")?;
    project.shorten()?;
    let at = ProjectFrame(6);
    let held = node("named-mark-range-held")?;
    project.apply(|project, document, revision| {
        let target = document.insert_time_target(at)?;
        let identities = match target.split {
            Some(split) => project.fresh(split.required_ids)?,
            None => Vec::new(),
        };
        Ok(Command::InsertTime {
            at,
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
    let (right, start) = root_child_at(&document, 8)?;
    assert_eq!(start, 8);
    let (host, offset) = source_host(&document, &right)?;
    for (letter, position, bias) in [
        ('a', 2, InsertionBias::Right),
        ('b', 5, InsertionBias::Left),
        ('c', 7, InsertionBias::Right),
        ('d', 10, InsertionBias::Left),
    ] {
        project.apply(|_, _, _| {
            Ok(Command::SetMark {
                id: MarkId::new(format!("native-mark-{letter}"))?,
                owner: host.clone(),
                label: letter.to_string(),
                boundary: BoundaryAnchor {
                    coordinate: Anchor::Occurrence {
                        instance: InstancePath {
                            node: host.clone(),
                            repeats: Vec::new(),
                        },
                        position: ExactRatio::integer(offset + position),
                    },
                    bias,
                },
                loss_policy: AnchorLossPolicy::DeleteOwned,
            })
        })?;
    }
    let before = project.document()?;
    assert_range(&before, 'a', 'b', 10, 13)?;
    assert_range(&before, 'c', 'd', 15, 18)?;
    let program = SemanticProgram::new(vec![
        SemanticInstruction::SetHoldDuration {
            length: PauseLength::Frames {
                frames: NonZeroU32::new(3).ok_or("positive Hold length")?,
            },
        },
        SemanticInstruction::Yank {
            selector: selector('a', 'b')?,
            register: RegisterName::new('a')?,
        },
        SemanticInstruction::Cut {
            selector: selector('a', 'b')?,
            register: RegisterName::new('b')?,
        },
        SemanticInstruction::Repeat {
            selector: selector('c', 'd')?,
            plays: NonZeroU32::new(2).ok_or("positive play count")?,
            escalation: None,
        },
    ])?;
    project.save_macro('m', serde_json::to_value(program.instructions())?)?;
    let output = project.macro_operation(
        json!({"type":"run","register":"m","parent":before.root(),
            "cursor":7,"selected_child":held,"count":1}),
        false,
    )?;
    assert!(
        output.status.success(),
        "named-range macro: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout)?;
    fs::write(
        project.directory.join("named-mark-range-macro.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    let trace = result["trace"].as_array().ok_or("semantic trace")?;
    for (instruction, range) in [
        ("yank", json!({"start":11,"end":14})),
        ("cut", json!({"start":11,"end":14})),
        ("repeat", json!({"start":13,"end":16})),
    ] {
        let rows: Vec<_> = trace
            .iter()
            .filter(|row| row["instruction"]["type"] == instruction)
            .collect();
        assert_eq!(rows.len(), 1, "{instruction}: {trace:?}");
        assert_eq!(rows[0]["resolved_range"], range, "{instruction}");
    }
    let after = project.document()?;
    assert_ne!(after.revision_id(), before.revision_id());
    assert_eq!(after.duration()?.frames(), 33);
    assert_eq!(result["committed_revision"], after.revision_id().as_str());
    let store = ProjectStore::open(&project.package, AccessMode::ReadOnly)?;
    let bank = store.registers()?;
    let range = FrameRange::new(ProjectFrame(11), ProjectFrame(14))?;
    let mut captures = Vec::new();
    for register in ['a', 'b'] {
        let RegisterValue::Edited { slice } = bank.entries[&RegisterName::new(register)?].as_ref()
        else {
            return Err(format!("register {register} is not an Edit capture").into());
        };
        let captured = store.capture_snapshot_at(slice.revision_id())?;
        slice.validate_capture(&captured)?;
        assert_range(&captured, 'a', 'b', 11, 14)?;
        assert_range(&captured, 'c', 'd', 16, 19)?;
        assert_eq!(captured.duration()?.frames(), 33);
        assert_ne!(captured.revision_id(), before.revision_id());
        assert_ne!(captured.revision_id(), after.revision_id());
        assert_eq!(slice.parent(), before.root());
        assert_eq!(slice.range(), range);
        assert_eq!(slice.duration().frames(), 3);
        assert_eq!(
            serde_json::to_value(slice)?["parts"],
            json!([{"root":right,"mapping":{"start":2,"end":5},"source_start":9}]),
        );
        captures.push(json!({"register":register.to_string(),
            "capture_revision":slice.revision_id(),"range":slice.range(),
            "capture":slice}));
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
        project.directory.join("named-mark-range-captures.json"),
        serde_json::to_vec_pretty(&json!({"captures":captures,
            "history_before_navigation":history,"history_after_redo":restored_history,
            "before_revision":before.revision_id(),"macro_revision":after.revision_id(),
            "render_revision":restored.revision_id()}))?,
    )?;
    drop(store);
    let mut pictures = Vec::new();
    pictures.extend((12..18).map(original));
    pictures.extend([Expected::Background; 3]);
    pictures.extend((18..20).map(original));
    pictures.extend((23..25).map(original));
    for _ in 0..2 {
        pictures.extend((25..28).map(original));
    }
    pictures.extend((28..42).map(original));
    assert_eq!(pictures.len(), 33);
    project.finish(
        vec!["Repeat operator", "Hold (insert time)"],
        pictures.into_iter().enumerate().map(|(frame, picture)| (frame as u64, picture)).collect(),
        vec!["Real retained cfr-bframes.mp4 A/V at 30000/1001. One public macro grows the silent Background Hold 2 to 3 frames, Yanks and Cuts marks a/b at re-resolved [11,14), then repeats marks c/d at re-resolved [13,16) twice. Both registers retain validated three-frame transaction-step captures. Complete chronological command replay passes before navigation and after public Undo/Redo; the bank remains intact. Every one of the 33 final pictures has an independent ordinal/background oracle. Encoded audio is compared against canonical preview by the shared verifier; this fixture does not claim an independent PCM oracle.".into()],
    )
}
