use super::*;
use deadpan_core::{
    BeatNode, FrameDuration, HoldAudio, HoldRecipe, HoldVideo, ProjectFrame, SemanticContext,
    SemanticInstruction, SemanticProgram, SemanticRegisterBank, SemanticVisualSelection, Subtree,
    plan_semantic,
};
use std::collections::BTreeMap;

#[test]
fn explicit_empty_node_selection_survives_reopen_without_bank_or_history_writes() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let initial = store.snapshot()?;
    let node = |name: &str| NodeId::new(name).unwrap();
    store.commit(&CommandRequest {
        project_id: initial.project_id().clone(),
        expected_revision: initial.revision_id().clone(),
        new_revision: revision("node-fixture"),
        command: Command::Insert {
            parent: initial.root().clone(),
            index: 0,
            subtree: Subtree {
                root: node("group"),
                nodes: BTreeMap::from([
                    (
                        node("group"),
                        BeatNode::sequence(
                            "Group",
                            vec![node("empty-a"), node("empty-b"), node("hold")],
                        ),
                    ),
                    (node("empty-a"), BeatNode::sequence("Same label", vec![])),
                    (node("empty-b"), BeatNode::sequence("Same label", vec![])),
                    (
                        node("hold"),
                        BeatNode::hold(
                            "Hold",
                            HoldRecipe {
                                duration: FrameDuration::new(10)?,
                                picture_context: None,
                                video: HoldVideo::Background,
                                audio: HoldAudio::Silence,
                            },
                        ),
                    ),
                ]),
                overrides: Default::default(),
                gap_overrides: Default::default(),
            },
        },
    })?;
    let document = store.snapshot()?;
    let bank = store.registers()?;
    let before_counts = counts(&path)?;
    store.checkpoint()?;
    drop(store);
    let store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, document);
    assert_eq!(store.registers()?, bank);
    let context = SemanticContext {
        original: None,
        scoped: None,
        hold_duration: None,
        shared_hold_duration: None,
        mixed_hold_duration: None,
        selected_hold_duration: None,
        parent: document.root().clone(),
        cursor: ProjectFrame(10),
        selected_child: Some(node("group")),
        visual_selection: None,
    };
    for selected in ["empty-a", "empty-b"] {
        let plan = plan_semantic(
            &document,
            &context,
            &SemanticProgram::new(vec![SemanticInstruction::SelectNode {
                node: node(selected),
            }])?,
            SemanticRegisterBank {
                entries: &bank.entries,
                version: bank.version,
            },
            revision("selection-only-unused"),
            |_| unreachable!("selection allocates nothing"),
            |_, _| unreachable!("selection resolves no Original"),
        )?;
        assert!(plan.request.is_none());
        assert!(plan.register_writes.is_empty());
        assert_eq!(plan.document, document);
        assert_eq!(plan.context.parent, *document.root());
        assert_eq!(plan.context.cursor, ProjectFrame(0));
        assert_eq!(plan.context.selected_child, Some(node("group")));
        assert_eq!(
            plan.context.visual_selection,
            Some(SemanticVisualSelection::Node {
                node: node(selected)
            })
        );
        assert_eq!(plan.trace[0].resolved_parent, Some(node("group")));
        assert_eq!(
            plan.trace[0].resolved_selection,
            Some(deadpan_core::SliceCaptureSelection::Child {
                node: node(selected)
            })
        );
    }
    for missing in ["missing", "root"] {
        assert!(
            plan_semantic(
                &document,
                &context,
                &SemanticProgram::new(vec![SemanticInstruction::SelectNode {
                    node: node(missing)
                }])?,
                SemanticRegisterBank {
                    entries: &bank.entries,
                    version: bank.version
                },
                revision("refused-unused"),
                |_| unreachable!("failed selection allocates nothing"),
                |_, _| unreachable!("failed selection resolves no Original"),
            )
            .is_err()
        );
    }
    assert_eq!(store.snapshot()?, document);
    assert_eq!(store.registers()?, bank);
    assert_eq!(counts(&path)?, before_counts);
    let report = store.validate_report(true)?;
    assert_eq!(report.verified_by_receipt, 0);
    assert_eq!(report.replayed + 1, report.revisions);
    Ok(())
}
