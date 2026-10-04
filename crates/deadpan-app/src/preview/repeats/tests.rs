use super::*;
use deadpan_core::{
    BeatNode, ColorPolicy, Command, CommandRequest, FrameDuration, FrameRate, HoldAudio,
    HoldRecipe, HoldVideo, IterationOrder, PresentationBasis, ProjectId, SemanticVisualSelection,
    Subtree, apply,
};

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn hold(label: &str) -> BeatNode {
    BeatNode::hold(
        label,
        HoldRecipe {
            picture_context: None,
            duration: FrameDuration::new(2).unwrap(),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    )
}
fn repeated(label: &str, child: &str) -> BeatNode {
    let mut beat = BeatNode::sequence(label, Vec::new());
    beat.kind = NodeKind::Repeat {
        child: node(child),
        iterations: IterationOrder::new(revision(label), 3).unwrap(),
        gap: None,
        escalation: None,
    };
    beat
}
fn fixture() -> ProjectDocument {
    let document = ProjectDocument::new(
        ProjectId::new("repeat-command-intent").unwrap(),
        revision("empty"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    apply(
        &document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: revision("seed"),
            command: Command::Insert {
                parent: node("root"),
                index: 0,
                subtree: Subtree {
                    root: node("group"),
                    nodes: [
                        (
                            node("group"),
                            BeatNode::sequence(
                                "group",
                                vec![node("repeat"), node("other"), node("empty"), node("nested")],
                            ),
                        ),
                        (node("repeat"), repeated("repeat", "body")),
                        (node("body"), hold("body")),
                        (node("other"), hold("other")),
                        (node("empty"), BeatNode::sequence("empty", Vec::new())),
                        (
                            node("nested"),
                            BeatNode::sequence("nested", vec![node("inner")]),
                        ),
                        (node("inner"), repeated("inner", "leaf")),
                        (node("leaf"), hold("leaf")),
                    ]
                    .into(),
                    overrides: Default::default(),
                    gap_overrides: Default::default(),
                },
            },
        },
    )
    .unwrap()
    .forward
    .apply(&document)
    .unwrap()
}
fn context(selected: Option<&str>) -> SemanticContext {
    SemanticContext {
        parent: node("group"),
        cursor: ProjectFrame(0),
        selected_child: selected.map(node),
        visual_selection: None,
    }
}

#[test]
fn repeat_command_resolves_exact_selected_kind_and_preserves_same_count_setter_intent() {
    let document = fixture();
    let plays = NonZeroU32::new(3).unwrap();
    assert_eq!(
        repeat_count_instruction(&document, &context(Some("repeat")), plays).unwrap(),
        SemanticInstruction::SetRepeatPlays { plays }
    );
    for selected in ["other", "empty", "nested"] {
        assert_eq!(
            repeat_count_instruction(&document, &context(Some(selected)), plays).unwrap(),
            SemanticInstruction::Repeat {
                selector: SemanticSelector::SelectedBeat,
                plays
            }
        );
    }
}

#[test]
fn repeat_command_cannot_fill_captured_absence_or_retarget_a_different_scope() {
    let document = fixture();
    let plays = NonZeroU32::new(2).unwrap();
    assert_eq!(
        repeat_count_instruction(&document, &context(None), plays).unwrap_err(),
        "Select a beat before repeating it."
    );
    for selected in ["inner", "body", "missing"] {
        assert_eq!(
            repeat_count_instruction(&document, &context(Some(selected)), plays).unwrap_err(),
            "The captured Repeat target is not a direct child of this group."
        );
    }
    let mut wrong_parent = context(Some("body"));
    wrong_parent.parent = node("repeat");
    assert_eq!(
        repeat_count_instruction(&document, &wrong_parent, plays).unwrap_err(),
        "Repeat commands need an ordinary Sequence scope."
    );
}

#[test]
fn repeat_command_refuses_active_finished_empty_and_reverse_visual_selections() {
    let document = fixture();
    let plays = NonZeroU32::new(2).unwrap();
    for selected in ["repeat", "other"] {
        for (anchor, head, extending) in [(0, 0, true), (0, 0, false), (0, 1, true), (1, 0, false)]
        {
            let mut context = context(Some(selected));
            context.visual_selection = Some(SemanticVisualSelection::Time {
                anchor: ProjectFrame(anchor),
                head: ProjectFrame(head),
                extending,
            });
            assert_eq!(
                repeat_count_instruction(&document, &context, plays).unwrap_err(),
                "Clear the Visual range before changing an existing Repeat count."
            );
        }
    }
}
