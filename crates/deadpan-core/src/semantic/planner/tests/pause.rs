use super::*;
use crate::{
    ExactRatio, Framing, FramingCurve, FramingPose, HoldVideo, PauseLength, PauseProvider,
};

fn plan_pauses(
    document: &ProjectDocument,
    context: SemanticContext,
    instructions: Vec<SemanticInstruction>,
) -> Result<SemanticPlan, EditError> {
    plan_semantic_with_speech(
        document,
        &context,
        &program(instructions),
        SemanticRegisterBank {
            entries: &BTreeMap::new(),
            version: 7,
        },
        revision("outer"),
        allocate,
        no_original,
        |_| Err(crate::speech_unavailable()),
        |_, _| {
            Ok(PauseProvider {
                video: HoldVideo::Background,
                picture_context: None,
            })
        },
    )
}

fn pause(frames: u32) -> SemanticInstruction {
    SemanticInstruction::InsertPause {
        length: PauseLength::Frames {
            frames: NonZeroU32::new(frames).unwrap(),
        },
    }
}

fn creep() -> SemanticInstruction {
    let close = FramingPose::new(
        ExactRatio::new(1, 2).unwrap(),
        ExactRatio::new(1, 2).unwrap(),
        ExactRatio::new(27, 20).unwrap(),
    )
    .unwrap();
    SemanticInstruction::SetFraming {
        framing: Some(Box::new(
            Framing::creep(FramingPose::identity(), close, FramingCurve::Smoothstep).unwrap(),
        )),
    }
}

#[test]
fn the_long_answer_inserts_a_pause_and_creeps_on_it_in_one_transaction() {
    let document = tree(&["a", "b"], vec![("a", hold(3)), ("b", hold(4))]);
    let planned = plan_pauses(&document, context("root", 3), vec![pause(5), creep()]).unwrap();
    assert_eq!(planned.document.duration().unwrap().frames(), 12);
    assert_eq!(
        planned.context.cursor,
        ProjectFrame(3),
        "the cursor stays at the pause"
    );
    assert_eq!(planned.context.selected_child, Some(node("pause-0")));
    let held = &planned.document.nodes()[&node("pause-0")];
    assert!(matches!(&held.kind, NodeKind::Hold { recipe } if recipe.duration.frames() == 5));
    assert!(held.framing.is_some(), "the creep frames the new pause");
    assert_eq!(planned.trace[0].resolved_range, Some(range(3, 8)));
    let replay =
        crate::replay_compound::<EditError>(&document, planned.request.as_ref().unwrap(), |_| {
            Ok(())
        })
        .unwrap();
    assert_eq!(replay.document, planned.document);
    assert_eq!(
        replay.edit.inverse.apply(&replay.document).unwrap(),
        document
    );
}

#[test]
fn a_pause_inside_a_beat_splits_it_and_milliseconds_round_once() {
    let document = tree(&["a"], vec![("a", hold(10))]);
    // 100 ms at the fixture's 30 fps is exactly three frames.
    let planned = plan_pauses(
        &document,
        context("root", 4),
        vec![SemanticInstruction::InsertPause {
            length: PauseLength::Milliseconds {
                milliseconds: NonZeroU32::new(100).unwrap(),
            },
        }],
    )
    .unwrap();
    assert_eq!(planned.document.duration().unwrap().frames(), 13);
    let refused = plan_pauses(
        &document,
        context("root", 4),
        vec![SemanticInstruction::InsertPause {
            length: PauseLength::Milliseconds {
                milliseconds: NonZeroU32::new(10).unwrap(),
            },
        }],
    )
    .unwrap_err();
    assert!(refused.message.contains("zero"), "{refused:?}");
    // Without a host resolver, plain planning explains the refusal.
    let unresolved = plan(
        &document,
        context("root", 4),
        vec![pause(2)],
        &BTreeMap::new(),
    )
    .unwrap_err();
    assert!(
        unresolved.message.contains("measured pictures"),
        "{unresolved:?}"
    );
}

#[test]
fn framing_needs_a_selected_direct_child() {
    let document = tree(&["a"], vec![("a", hold(3))]);
    let error = plan_pauses(&document, context("root", 0), vec![creep()]).unwrap_err();
    assert_eq!(error.code, EditErrorCode::SelectionUnavailable);
}

#[test]
fn the_long_answer_gag_is_one_editable_group_pinning_its_recipe() {
    let document = tree(&["a", "b"], vec![("a", hold(3)), ("b", hold(4))]);
    let recipe = crate::GagRecipe::LongAnswer {
        version: crate::GAG_RECIPE_VERSION,
        pause: PauseLength::Frames {
            frames: NonZeroU32::new(6).unwrap(),
        },
        scale: ExactRatio::new(27, 20).unwrap(),
    };
    let planned = plan_pauses(
        &document,
        context("root", 3),
        vec![SemanticInstruction::Gag { recipe }],
    )
    .unwrap();
    let group = planned.context.selected_child.clone().unwrap();
    let node = &planned.document.nodes()[&group];
    assert_eq!(node.label, recipe.label());
    let NodeKind::Sequence { children } = &node.kind else {
        panic!("the gag is a group")
    };
    assert_eq!(children, &[NodeId::new("pause-0").unwrap()]);
    assert!(planned.document.nodes()[&children[0]].framing.is_some());
    assert_eq!(planned.document.duration().unwrap().frames(), 13);
    let replay =
        crate::replay_compound::<EditError>(&document, planned.request.as_ref().unwrap(), |_| {
            Ok(())
        })
        .unwrap();
    assert_eq!(
        replay.edit.inverse.apply(&replay.document).unwrap(),
        document
    );
}

#[test]
fn the_long_answer_refuses_a_pause_hidden_in_a_nested_group() {
    let document = tree(
        &["g"],
        vec![
            ("g", BeatNode::sequence("group", vec![node("a"), node("b")])),
            ("a", hold(3)),
            ("b", hold(4)),
        ],
    );
    let recipe = crate::GagRecipe::LongAnswer {
        version: crate::GAG_RECIPE_VERSION,
        pause: PauseLength::Frames {
            frames: NonZeroU32::new(6).unwrap(),
        },
        scale: ExactRatio::new(27, 20).unwrap(),
    };
    let error = plan_pauses(
        &document,
        context("root", 3),
        vec![SemanticInstruction::Gag { recipe }],
    )
    .unwrap_err();
    assert!(error.message.contains("nested group"), "{error:?}");
    // Inside the group, the same gag frames and groups its own pause.
    let inside = plan_pauses(
        &document,
        context("g", 3),
        vec![SemanticInstruction::Gag { recipe }],
    )
    .unwrap();
    assert_eq!(inside.document.duration().unwrap().frames(), 13);
}
