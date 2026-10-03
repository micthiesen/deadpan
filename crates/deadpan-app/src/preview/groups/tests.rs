use super::*;
use deadpan_core::{
    BeatNode, ColorPolicy, Command, CommandRequest, FrameDuration, FrameRate, HoldAudio,
    HoldRecipe, HoldVideo, PresentationBasis, ProjectId, SemanticVisualSelection, Subtree, apply,
};

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn fixture() -> ProjectDocument {
    let document = ProjectDocument::new(
        ProjectId::new("native-group").unwrap(),
        RevisionId::new("empty").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    let hold = || {
        BeatNode::hold(
            "picture",
            HoldRecipe {
                picture_context: None,
                duration: FrameDuration::new(2).unwrap(),
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
            },
        )
    };
    apply(
        &document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new("seed").unwrap(),
            command: Command::Insert {
                parent: node("root"),
                index: 0,
                subtree: Subtree {
                    root: node("outer"),
                    nodes: [
                        (
                            node("outer"),
                            BeatNode::sequence(
                                "outer",
                                vec![node("beat"), node("group"), node("empty")],
                            ),
                        ),
                        (node("beat"), hold()),
                        (
                            node("group"),
                            BeatNode::sequence("group", vec![node("inner")]),
                        ),
                        (node("inner"), hold()),
                        (node("empty"), BeatNode::sequence("empty", Vec::new())),
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
        parent: node("outer"),
        cursor: ProjectFrame(1),
        selected_child: selected.map(node),
        visual_selection: None,
    }
}

#[test]
fn group_intent_retains_the_name_and_explicit_child_including_empty() {
    let document = fixture();
    for selected in ["beat", "group", "empty"] {
        let label = "  réponse 🎬  ".to_owned();
        assert_eq!(
            instruction(&document, &context(Some(selected)), Some(label.clone())).unwrap(),
            SemanticInstruction::Group {
                selector: SemanticSelector::SelectedBeat,
                label
            }
        );
    }
    // A cursor inside the first beat cannot repair captured absence or a
    // selected descendant that is not a direct child of this scope.
    for selected in [None, Some("inner"), Some("missing")] {
        assert!(instruction(&document, &context(selected), Some("name".into())).is_err());
    }
    for label in ["\0".into(), "é".repeat(513)] {
        assert!(instruction(&document, &context(Some("beat")), Some(label)).is_err());
    }
}

#[test]
fn visual_group_takes_precedence_but_empty_visual_never_falls_back() {
    let document = fixture();
    for (anchor, head, extending) in [(0, 1, true), (1, 0, false), (0, 0, true), (0, 0, false)] {
        let mut context = context(Some("group"));
        context.visual_selection = Some(SemanticVisualSelection {
            anchor: ProjectFrame(anchor),
            head: ProjectFrame(head),
            extending,
        });
        let result = instruction(&document, &context, Some("range".into()));
        if anchor == head {
            assert!(result.is_err());
        } else {
            assert_eq!(
                result.unwrap(),
                SemanticInstruction::Group {
                    selector: SemanticSelector::VisualSelection,
                    label: "range".into()
                }
            );
            context.selected_child = None;
            assert!(instruction(&document, &context, Some("range".into())).is_ok());
        }
        assert!(instruction(&document, &context, None).is_err());
    }
}

#[test]
fn ungroup_requires_an_explicit_direct_sequence_and_refuses_visual() {
    let document = fixture();
    for selected in ["group", "empty"] {
        assert_eq!(
            instruction(&document, &context(Some(selected)), None).unwrap(),
            SemanticInstruction::Ungroup
        );
    }
    for selected in [None, Some("beat"), Some("inner"), Some("missing")] {
        assert!(instruction(&document, &context(selected), None).is_err());
    }
    let mut wrong_scope = context(Some("inner"));
    wrong_scope.parent = node("beat");
    assert!(instruction(&document, &wrong_scope, None).is_err());
}
