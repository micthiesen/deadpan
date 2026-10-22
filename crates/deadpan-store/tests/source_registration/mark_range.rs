use super::*;
use deadpan_core::{
    Anchor, AnchorLossPolicy, BeatNode, BoundaryAnchor, ExactRatio, FrameDuration, HoldAudio,
    HoldRecipe, HoldVideo, InsertionBias, InstancePath, NamedMarkTarget, SemanticContext,
    SemanticInstruction, SemanticProgram, SemanticRegisterBank, SemanticVisualSelection, Subtree,
    plan_semantic,
};
use std::collections::BTreeMap;

fn mark(store: &mut ProjectStore, label: char, position: i64, next_revision: &str) -> Result {
    let document = store.snapshot()?;
    let mark_id = deadpan_core::MarkId::new(format!("native-mark-{label}"))?;
    store.commit(&CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(next_revision),
        command: Command::SetMark {
            id: mark_id,
            owner: NodeId::new("hold")?,
            label: label.to_string(),
            boundary: BoundaryAnchor {
                coordinate: Anchor::Occurrence {
                    instance: InstancePath {
                        node: NodeId::new("hold")?,
                        repeats: Vec::new(),
                    },
                    position: ExactRatio::integer(position),
                },
                bias: InsertionBias::Right,
            },
            loss_policy: AnchorLossPolicy::KeepUnresolved,
        },
    })?;
    Ok(())
}

#[test]
fn named_mark_range_selection_is_bank_neutral_and_rejects_reversed_endpoints() -> Result {
    let scratch = tempfile::tempdir()?;
    let (_path, mut store) = project(scratch.path())?;
    let empty = store.snapshot()?;
    store.commit(&CommandRequest {
        project_id: empty.project_id().clone(),
        expected_revision: empty.revision_id().clone(),
        new_revision: revision("hold"),
        command: Command::Insert {
            parent: empty.root().clone(),
            index: 0,
            subtree: Subtree {
                root: NodeId::new("hold")?,
                nodes: BTreeMap::from([(
                    NodeId::new("hold")?,
                    BeatNode::hold(
                        "Silence",
                        HoldRecipe {
                            picture_context: None,
                            duration: FrameDuration::new(45)?,
                            video: HoldVideo::Background,
                            audio: HoldAudio::Silence,
                        },
                    ),
                )]),
                overrides: Default::default(),
                gap_overrides: Default::default(),
            },
        },
    })?;
    mark(&mut store, 'a', 3, "mark-a")?;
    mark(&mut store, 'b', 10, "mark-b")?;

    let document = store.snapshot()?;
    let bank = store.registers()?;
    let context = SemanticContext {
        hold_duration: None,
        selected_hold_duration: None,
        mixed_hold_duration: None,
        shared_hold_duration: None,
        original: None,
        scoped: None,
        parent: document.root().clone(),
        cursor: deadpan_core::ProjectFrame(0),
        selected_child: None,
        visual_selection: None,
    };
    let start = NamedMarkTarget {
        id: deadpan_core::MarkId::new("native-mark-a")?,
        occurrence: None,
    };
    let end = NamedMarkTarget {
        id: deadpan_core::MarkId::new("native-mark-b")?,
        occurrence: None,
    };
    let selected = document.select_semantic_mark_range(&context, start.clone(), end.clone())?;
    assert_eq!(selected.cursor, deadpan_core::ProjectFrame(10));
    assert_eq!(
        selected.visual_selection,
        Some(SemanticVisualSelection::MarkRange {
            start: start.clone(),
            end: end.clone(),
        })
    );

    let plan = plan_semantic(
        &document,
        &context,
        &SemanticProgram::new(vec![SemanticInstruction::SelectMarkRange {
            start: start.clone(),
            end: end.clone(),
        }])?,
        SemanticRegisterBank {
            entries: &bank.entries,
            version: bank.version,
        },
        revision("selection-only"),
        |_| unreachable!("selection allocates no authored state"),
        |_, _| unreachable!("selection does not paste Original content"),
    )?;
    assert!(plan.request.is_none());
    assert_eq!(plan.context, selected);
    assert_eq!(store.snapshot()?, document);
    assert_eq!(store.registers()?, bank);

    let reversed = document.select_semantic_mark_range(&context, end.clone(), start.clone());
    assert!(
        reversed.is_err(),
        "reversed named endpoints must not be normalized"
    );
    let reversed_plan = plan_semantic(
        &document,
        &context,
        &SemanticProgram::new(vec![SemanticInstruction::SelectMarkRange {
            start: end,
            end: start,
        }])?,
        SemanticRegisterBank {
            entries: &bank.entries,
            version: bank.version,
        },
        revision("reversed-selection"),
        |_| unreachable!("failed selection allocates no authored state"),
        |_, _| unreachable!("failed selection does not paste Original content"),
    );
    assert!(reversed_plan.is_err());
    assert_eq!(store.snapshot()?, document);
    assert_eq!(store.registers()?, bank);
    Ok(())
}
