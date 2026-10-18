use super::*;
use crate::{
    ExactRatio, Framing, FramingPose, FramingValue, SemanticZoomCenter,
    framing_zoom::{ZoomCurve, ZoomKind},
};

fn selected(cursor: i64, selection: Option<SemanticVisualSelection>) -> SemanticContext {
    SemanticContext {
        selected_child: Some(node("held")),
        visual_selection: selection,
        ..context("root", cursor)
    }
}

fn visual(anchor: i64, head: i64) -> Option<SemanticVisualSelection> {
    Some(SemanticVisualSelection::Time {
        anchor: ProjectFrame(anchor),
        head: ProjectFrame(head),
        extending: false,
    })
}

fn zoom(kind: ZoomKind, center: SemanticZoomCenter) -> SemanticInstruction {
    SemanticInstruction::Zoom { kind, center }
}

fn resolve(
    document: &ProjectDocument,
    context: &SemanticContext,
    instructions: Vec<SemanticInstruction>,
    bank: &BTreeMap<RegisterName, Arc<RegisterValue>>,
    resolver: impl FnMut(
        &ProjectDocument,
        &SemanticFramingSite,
    ) -> Result<SemanticFramingState, EditError>,
) -> Result<SemanticPlan, EditError> {
    plan_semantic_with_framing(
        document,
        context,
        &program(instructions),
        SemanticRegisterBank {
            entries: bank,
            version: 7,
        },
        revision("framing-outer"),
        |request| match request {
            SemanticAllocationRequest::ParameterEdit { step_index } => {
                Ok(SemanticAllocation::ParameterEdit {
                    new_revision: revision(&format!("framing-leaf-{step_index}")),
                })
            }
            other => allocate(other),
        },
        no_original,
        |_| Err(speech_unavailable()),
        |_, _| Err(pause_unavailable()),
        None,
        resolver,
    )
}

fn evaluate(document: &ProjectDocument, frame: i64) -> FramingPose {
    document.nodes()[&node("held")]
        .framing
        .as_ref()
        .unwrap()
        .evaluate(
            ExactRatio::new(i128::from(frame) * 2 + 1, 2).unwrap(),
            document.node_duration(&node("held")).unwrap(),
        )
        .unwrap()
}

#[test]
fn zoom_intent_rebinds_oriented_ranges_and_preserves_outside_pose_with_one_inverse() {
    let mut document = fixture(100);
    let before = FramingPose::new(
        ExactRatio::new(1, 4).unwrap(),
        ExactRatio::new(3, 4).unwrap(),
        ExactRatio::ONE,
    )
    .unwrap();
    document.nodes.get_mut(&node("held")).unwrap().framing =
        Some(Framing::static_pose(before).unwrap());
    let instruction = zoom(
        ZoomKind::Smash {
            scale: ExactRatio::integer(2),
        },
        SemanticZoomCenter::Keep,
    );
    for (anchor, head) in [(10, 20), (70, 40)] {
        let context = selected(head, visual(anchor, head));
        let planned = resolve(
            &document,
            &context,
            vec![instruction.clone()],
            &BTreeMap::new(),
            |staged, site| {
                assert_eq!(staged, &document);
                assert_eq!(site.node, node("held"));
                assert_eq!(site.frame, ProjectFrame(head));
                assert!(!site.original_center);
                Ok(SemanticFramingState {
                    duration: FrameDuration::new(100).unwrap(),
                    current: before,
                    original_center: None,
                })
            },
        )
        .unwrap();
        assert_eq!(planned.context, context);
        assert!(planned.register_writes.is_empty());
        let (start, end) = (anchor.min(head), anchor.max(head));
        assert_eq!(planned.trace[0].resolved_range, Some(range(start, end)));
        assert_eq!(evaluate(&planned.document, start - 1), before);
        assert_eq!(
            evaluate(&planned.document, start).scale,
            ExactRatio::integer(2)
        );
        assert_eq!(
            evaluate(&planned.document, end - 1).scale,
            ExactRatio::integer(2)
        );
        assert_eq!(evaluate(&planned.document, end), before);
        let request = planned.request.as_ref().unwrap();
        let replay = crate::replay_compound::<EditError>(&document, request, |_| Ok(())).unwrap();
        assert_eq!(replay.document, planned.document);
        assert_eq!(
            replay.edit.inverse.apply(&replay.document).unwrap(),
            document
        );
    }
}

#[test]
fn zoom_resolves_after_earlier_duration_edits_and_nested_calls_on_the_staged_owner() {
    let document = fixture(100);
    let instruction = zoom(
        ZoomKind::Creep {
            from: None,
            to: ExactRatio::integer(2),
            curve: ZoomCurve::Linear,
        },
        SemanticZoomCenter::OriginalCenter,
    );
    let bank = BTreeMap::from([(name('z'), macro_value(vec![instruction]))]);
    let planned = resolve(
        &document,
        &selected(10, None),
        vec![
            SemanticInstruction::SetHoldDuration {
                length: crate::PauseLength::Frames {
                    frames: NonZeroU32::new(80).unwrap(),
                },
            },
            motion(true, 10),
            SemanticInstruction::BeginSelection,
            motion(true, 20),
            SemanticInstruction::FinishSelection,
            call('z', 1),
        ],
        &bank,
        |staged, site| {
            assert_eq!(staged.duration().unwrap().frames(), 80);
            assert_eq!(site.frame, ProjectFrame(30));
            assert!(site.original_center);
            Ok(SemanticFramingState {
                duration: FrameDuration::new(80).unwrap(),
                current: FramingPose::identity(),
                original_center: Some([
                    ExactRatio::new(1, 4).unwrap(),
                    ExactRatio::new(3, 4).unwrap(),
                ]),
            })
        },
    )
    .unwrap();
    let framing = planned.document.nodes()[&node("held")]
        .framing
        .as_ref()
        .unwrap();
    let FramingValue::Envelope { envelope } = &framing.value else {
        panic!()
    };
    assert_eq!(envelope.segments[0].end, ExactRatio::new(1, 8).unwrap());
    assert_eq!(envelope.segments[1].end, ExactRatio::new(3, 8).unwrap());
    assert_eq!(
        evaluate(&planned.document, 31).scale,
        ExactRatio::integer(2)
    );
    assert_eq!(
        evaluate(&planned.document, 79).center_x,
        ExactRatio::new(1, 4).unwrap()
    );
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
fn zoom_refuses_empty_cross_owner_ranges_and_missing_projected_center_without_a_leaf() {
    let document = tree(
        &["held", "other"],
        vec![("held", hold(10)), ("other", hold(10))],
    );
    let instruction = zoom(
        ZoomKind::Smash {
            scale: ExactRatio::integer(2),
        },
        SemanticZoomCenter::OriginalCenter,
    );
    for selection in [visual(4, 4), visual(5, 15)] {
        let error = resolve(
            &document,
            &selected(5, selection),
            vec![instruction.clone()],
            &BTreeMap::new(),
            |_, _| panic!("range refusal must precede geometry"),
        )
        .unwrap_err();
        assert_eq!(error.code, EditErrorCode::SelectionUnavailable);
    }
    let error = resolve(
        &document,
        &selected(5, visual(2, 8)),
        vec![instruction],
        &BTreeMap::new(),
        |_, _| {
            Ok(SemanticFramingState {
                duration: FrameDuration::new(20).unwrap(),
                current: FramingPose::identity(),
                original_center: None,
            })
        },
    )
    .unwrap_err();
    assert!(error.message.contains("clipped"));
    assert!(document.nodes()[&node("held")].framing.is_none());
}

#[test]
fn zoom_noop_has_no_history_and_late_path_refusal_keeps_input_and_bank_unchanged() {
    let document = fixture(20);
    let instruction = zoom(ZoomKind::Off, SemanticZoomCenter::Keep);
    let planned = resolve(
        &document,
        &selected(4, None),
        vec![instruction],
        &BTreeMap::new(),
        |_, _| {
            Ok(SemanticFramingState {
                duration: FrameDuration::new(20).unwrap(),
                current: FramingPose::identity(),
                original_center: None,
            })
        },
    )
    .unwrap();
    assert!(planned.request.is_none());
    assert_eq!(planned.document, document);
    let instruction = zoom(
        ZoomKind::Smash {
            scale: ExactRatio::integer(2),
        },
        SemanticZoomCenter::Keep,
    );
    let bank = BTreeMap::from([(name('a'), macro_value(vec![instruction.clone()]))]);
    let frozen = bank.clone();
    let error = resolve(
        &document,
        &selected(4, visual(2, 8)),
        vec![call('a', 1), instruction],
        &bank,
        |_, _| {
            Ok(SemanticFramingState {
                duration: FrameDuration::new(20).unwrap(),
                current: FramingPose::identity(),
                original_center: None,
            })
        },
    )
    .unwrap_err();
    assert!(error.message.contains("camera path"));
    assert_eq!(bank, frozen);
    assert!(document.nodes()[&node("held")].framing.is_none());
}

#[test]
fn zoom_ordinary_owner_remains_available_with_retained_nested_tape_pause_clocks() {
    let speed = |child, frames, end| {
        let mut stage = hold(frames);
        stage.kind = NodeKind::Retime {
            child: node(child),
            duration: FrameDuration::new(frames).unwrap(),
            mapping: range(0, end),
            pitch: crate::PitchPolicy::FollowSpeed,
            purpose: crate::RetimePurpose::Edit,
        };
        stage
    };
    let document = tree(
        &["outer-speed"],
        vec![
            ("outer-speed", speed("inner-speed", 160, 240)),
            ("inner-speed", speed("body-group", 240, 120)),
            ("body-group", BeatNode::sequence("Body", vec![node("body")])),
            ("body", hold(120)),
        ],
    );
    let paused = plan_semantic_with_speech(
        &document,
        &context("root", 17),
        &program(vec![SemanticInstruction::InsertPause {
            length: crate::PauseLength::Frames {
                frames: NonZeroU32::new(3).unwrap(),
            },
            black: false,
        }]),
        SemanticRegisterBank {
            entries: &BTreeMap::new(),
            version: 7,
        },
        revision("paused"),
        allocate,
        no_original,
        |_| Err(speech_unavailable()),
        |_, _| {
            Ok(PauseProvider {
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
                picture_context: None,
            })
        },
    )
    .unwrap()
    .document;
    assert_eq!(paused.duration().unwrap().frames(), 163);
    assert!(
        paused.node_duration(&node("inner-speed")).is_err(),
        "the fractional inner clock must not be rounded"
    );
    assert_eq!(
        scope_bounds(&paused, paused.root()).unwrap().1,
        ProjectFrame(163)
    );
    let context = SemanticContext {
        selected_child: Some(node("outer-speed")),
        visual_selection: visual(30, 60),
        ..context("root", 60)
    };
    let planned = resolve(
        &paused,
        &context,
        vec![zoom(
            ZoomKind::Smash {
                scale: ExactRatio::integer(2),
            },
            SemanticZoomCenter::Keep,
        )],
        &BTreeMap::new(),
        |_, site| {
            assert_eq!(site.frame, ProjectFrame(60));
            Ok(SemanticFramingState {
                duration: FrameDuration::new(163).unwrap(),
                current: FramingPose::identity(),
                original_center: None,
            })
        },
    )
    .unwrap();
    assert_eq!(planned.context, context);
    assert_eq!(planned.trace[0].resolved_range, Some(range(30, 60)));
    let replay =
        crate::replay_compound::<EditError>(&paused, planned.request.as_ref().unwrap(), |_| Ok(()))
            .unwrap();
    assert_eq!(replay.edit.inverse.apply(&replay.document).unwrap(), paused);
}
