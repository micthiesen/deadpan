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

fn assert_ranged_follow_plan(before: &ProjectDocument, plan: &SemanticPlan, start: i64, end: i64) {
    let framing = plan.document.nodes()[&node("held")]
        .framing
        .as_ref()
        .unwrap();
    let duration = ExactRatio::integer(before.node_duration(&node("held")).unwrap().frames());
    let at = |local| framing.evaluate_with_follow_exact(local, duration).unwrap();
    if start > 0 {
        let prefix = at(ExactRatio::integer(start - 1));
        assert_eq!(prefix.follow.unwrap().1, ExactRatio::integer(2));
        assert_eq!(prefix.pose.scale, ExactRatio::new(1, 2).unwrap());
    }
    let middle = at(ExactRatio::new(i128::from(start + end), 2).unwrap());
    assert_eq!(middle.follow.unwrap().1, ExactRatio::integer(3));
    assert_eq!(middle.pose.scale, ExactRatio::new(3, 4).unwrap());
    let terminal = at(ExactRatio::integer(end));
    assert_eq!(terminal.follow.unwrap().1, ExactRatio::integer(4));
    assert_eq!(terminal.pose.scale, ExactRatio::ONE);
    assert_eq!(terminal.follow.unwrap().0.as_str(), "subject");
    assert_eq!(terminal.pose.center_x, ExactRatio::new(1, 3).unwrap());
    let mut expected = before.clone();
    expected.revision_id = plan.document.revision_id().clone();
    expected.nodes.get_mut(&node("held")).unwrap().framing = Some(framing.clone());
    assert_eq!(plan.document, expected);
    let replay =
        crate::replay_compound::<EditError>(before, plan.request.as_ref().unwrap(), |_| Ok(()))
            .unwrap();
    assert_eq!(replay.document, plan.document);
    assert_eq!(replay.edit.inverse.apply(&plan.document).unwrap(), *before);
    assert_eq!(plan.trace[0].resolved_range, Some(range(start, end)));
}

#[test]
fn ordinary_follow_creep_resolves_whole_and_ranged_owners_with_atomic_refusals() {
    let mut document = fixture(8);
    let tb = crate::SourceTimeBase::new(1, 30).unwrap();
    let span = crate::SourceSpan::new(
        crate::SourceTimestamp {
            ticks: 0,
            time_base: tb,
        },
        crate::SourceTimestamp {
            ticks: 100,
            time_base: tb,
        },
    )
    .unwrap();
    let asset = crate::AssetId::new("original").unwrap();
    document.assets.insert(
        asset.clone(),
        crate::AssetRecord {
            label: "Original".into(),
            content_hash: "a".repeat(64),
            video: Some(span),
            audio: None,
            still_image: false,
            frame_count: None,
            source_qualification: None,
        },
    );
    let target = crate::TargetId::new("subject").unwrap();
    document.targets.insert(
        target.clone(),
        crate::AttentionTarget {
            label: "Subject".into(),
            asset,
            span,
            region: crate::TargetPoint {
                center: [200_000, 300_000],
            }
            .into(),
            samples: vec![],
            corrections: vec![],
            provenance: None,
            display_aspect: None,
        },
    );
    let fixed = Framing {
        clock: crate::FramingClock::OwnerOutput,
        value: FramingValue::Follow {
            target,
            scale: ExactRatio::integer(2),
            fallback: FramingPose::new(
                ExactRatio::new(1, 3).unwrap(),
                ExactRatio::new(2, 7).unwrap(),
                ExactRatio::new(1, 2).unwrap(),
            )
            .unwrap(),
            scale_transition: None,
        },
    };
    document.nodes.get_mut(&node("held")).unwrap().framing = Some(fixed);
    document.validate().unwrap();
    let before = document.clone();
    let instruction = zoom(
        ZoomKind::Creep {
            from: None,
            to: ExactRatio::integer(4),
            curve: ZoomCurve::Linear,
        },
        SemanticZoomCenter::Keep,
    );
    let resolver = |doc: &ProjectDocument, _: &SemanticFramingSite| {
        Ok(SemanticFramingState {
            duration: doc.node_duration(&node("held")).unwrap(),
            current: FramingPose::identity(),
            original_center: None,
        })
    };
    let jumped = resolve(
        &document,
        &selected(3, visual(2, 6)),
        vec![zoom(
            ZoomKind::Creep {
                from: Some(ExactRatio::integer(3)),
                to: ExactRatio::integer(4),
                curve: ZoomCurve::Linear,
            },
            SemanticZoomCenter::Keep,
        )],
        &BTreeMap::new(),
        resolver,
    )
    .unwrap();
    let framing = jumped.document.nodes()[&node("held")]
        .framing
        .as_ref()
        .unwrap();
    for (frame, n, d) in [(1, 2, 1), (2, 3, 1), (4, 7, 2), (7, 4, 1)] {
        let state = framing
            .evaluate_with_follow_exact(ExactRatio::integer(frame), ExactRatio::integer(8))
            .unwrap();
        let expected = ExactRatio::new(n, d).unwrap();
        assert_eq!(state.follow.unwrap().1, expected);
        assert_eq!(
            state.pose.scale,
            expected.checked_div(ExactRatio::integer(4)).unwrap()
        );
    }
    let jumped_replay = crate::replay_compound::<EditError>(
        &document,
        jumped.request.as_ref().unwrap(),
        |_| Ok(()),
    )
    .unwrap();
    assert_eq!(jumped_replay.document, jumped.document);
    assert_eq!(
        jumped_replay.edit.inverse.apply(&jumped.document).unwrap(),
        document
    );
    let plan = resolve(
        &document,
        &selected(3, None),
        vec![instruction.clone()],
        &BTreeMap::new(),
        resolver,
    )
    .unwrap();
    let mut expected_node = before.nodes()[&node("held")].clone();
    expected_node.framing = plan.document.nodes()[&node("held")].framing.clone();
    assert_eq!(plan.document.nodes()[&node("held")], expected_node);
    assert_eq!(
        plan.document.duration().unwrap(),
        document.duration().unwrap()
    );
    let value = plan.document.nodes()[&node("held")]
        .framing
        .as_ref()
        .unwrap()
        .evaluate_with_follow_exact(ExactRatio::integer(2), ExactRatio::integer(8))
        .unwrap();
    assert_eq!(value.follow.unwrap().1, ExactRatio::new(5, 2).unwrap());
    assert_eq!(value.pose.scale, ExactRatio::new(5, 8).unwrap());
    let replay =
        crate::replay_compound::<EditError>(&document, plan.request.as_ref().unwrap(), |_| Ok(()))
            .unwrap();
    assert_eq!(replay.document, plan.document);
    assert_eq!(replay.edit.inverse.apply(&plan.document).unwrap(), before);
    for (anchor, head) in [(0, 8), (1, 7), (8, 0)] {
        let ranged = resolve(
            &document,
            &selected(3, visual(anchor, head)),
            vec![instruction.clone()],
            &BTreeMap::new(),
            resolver,
        )
        .unwrap();
        assert_ranged_follow_plan(&document, &ranged, anchor.min(head), anchor.max(head));
    }
    assert!(
        resolve(
            &plan.document,
            &selected(3, None),
            vec![instruction.clone()],
            &BTreeMap::new(),
            resolver
        )
        .is_err()
    );
    // The actual selected Sequence owns one continuous output clock across its
    // unequal children. Child structure/framing and the full inverse survive.
    let mut inside = hold(3);
    inside.framing = Some(Framing::static_pose(FramingPose::identity()).unwrap());
    document.nodes.insert(node("inside-a"), inside);
    document.nodes.insert(node("inside-b"), hold(5));
    document.nodes.get_mut(&node("held")).unwrap().kind = NodeKind::Sequence {
        children: vec![node("inside-a"), node("inside-b")],
    };
    document.validate().unwrap();
    let sequence = document.clone();
    let planned = resolve(
        &sequence,
        &selected(3, None),
        vec![instruction.clone()],
        &BTreeMap::new(),
        resolver,
    )
    .unwrap();
    for frame in 0..8 {
        let value = planned.document.nodes()[&node("held")]
            .framing
            .as_ref()
            .unwrap()
            .evaluate_with_follow_exact(
                ExactRatio::new(2 * frame + 1, 2).unwrap(),
                ExactRatio::integer(8),
            )
            .unwrap();
        assert_eq!(
            value.follow.unwrap().1,
            ExactRatio::new(17 + 2 * frame, 8).unwrap()
        );
        assert_eq!(
            value.pose.scale,
            ExactRatio::new(17 + 2 * frame, 32).unwrap()
        );
    }
    let replay =
        crate::replay_compound::<EditError>(&sequence, planned.request.as_ref().unwrap(), |_| {
            Ok(())
        })
        .unwrap();
    assert_eq!(replay.document, planned.document);
    assert_eq!(
        replay.edit.inverse.apply(&planned.document).unwrap(),
        sequence
    );
    let mut only_framing = sequence.clone();
    only_framing.revision_id = planned.document.revision_id().clone();
    only_framing.nodes.get_mut(&node("held")).unwrap().framing =
        planned.document.nodes()[&node("held")].framing.clone();
    assert_eq!(planned.document, only_framing);
    assert_eq!(
        planned.document.nodes()[&node("held")]
            .framing
            .as_ref()
            .unwrap()
            .clock,
        crate::FramingClock::OwnerOutput
    );
    for (anchor, head) in [(0, 8), (1, 7), (8, 0)] {
        let ranged = resolve(
            &sequence,
            &selected(3, visual(anchor, head)),
            vec![instruction.clone()],
            &BTreeMap::new(),
            resolver,
        )
        .unwrap();
        assert_ranged_follow_plan(&sequence, &ranged, anchor.min(head), anchor.max(head));
    }
    document.nodes.insert(
        node("inside"),
        BeatNode::sequence("Inside", vec![node("inside-a"), node("inside-b")]),
    );
    document.nodes.get_mut(&node("held")).unwrap().kind = NodeKind::Repeat {
        child: node("inside"),
        iterations: crate::IterationOrder::new(revision("retained-plays"), 3).unwrap(),
        gap: Some(match hold(2).kind {
            NodeKind::Hold { recipe } => recipe,
            _ => unreachable!(),
        }),
        escalation: Some(crate::RepeatEscalation {
            gain_step: crate::GainDb::UNITY,
            zoom: Some(crate::ZoomStep {
                step: ExactRatio::new(1, 8).unwrap(),
                progression: crate::ZoomProgression::Add,
                target: None,
            }),
        }),
    };
    document.validate().unwrap();
    assert_eq!(document.duration().unwrap().frames(), 28);
    let repeat = document.clone();
    let planned = resolve(
        &repeat,
        &selected(13, None),
        vec![instruction.clone()],
        &BTreeMap::new(),
        resolver,
    )
    .unwrap();
    let value = planned.document.nodes()[&node("held")]
        .framing
        .as_ref()
        .unwrap()
        .evaluate_with_follow_exact(ExactRatio::integer(14), ExactRatio::integer(28))
        .unwrap();
    assert_eq!(value.follow.unwrap().1, ExactRatio::integer(3));
    assert_eq!(value.pose.scale, ExactRatio::new(3, 4).unwrap());
    let replay =
        crate::replay_compound::<EditError>(&repeat, planned.request.as_ref().unwrap(), |_| Ok(()))
            .unwrap();
    assert_eq!(replay.document, planned.document);
    assert_eq!(
        replay.edit.inverse.apply(&planned.document).unwrap(),
        repeat
    );
    let mut only_framing = repeat.clone();
    only_framing.revision_id = planned.document.revision_id().clone();
    only_framing.nodes.get_mut(&node("held")).unwrap().framing =
        planned.document.nodes()[&node("held")].framing.clone();
    assert_eq!(planned.document, only_framing);
    for (anchor, head) in [(0, 28), (1, 27), (28, 0)] {
        let ranged = resolve(
            &repeat,
            &selected(13, visual(anchor, head)),
            vec![instruction.clone()],
            &BTreeMap::new(),
            resolver,
        )
        .unwrap();
        assert_ranged_follow_plan(&repeat, &ranged, anchor.min(head), anchor.max(head));
    }
    // An inner Repeat cannot acquire ordinary authority by omitting the
    // occurrence context and naming its containing Sequence directly.
    let mut nested = repeat.clone();
    nested.nodes.insert(
        node("container"),
        BeatNode::sequence("Container", vec![node("held")]),
    );
    let mut outer = BeatNode::sequence("Outer", vec![]);
    outer.kind = NodeKind::Repeat {
        child: node("container"),
        iterations: crate::IterationOrder::new(revision("outer-plays"), 2).unwrap(),
        gap: None,
        escalation: None,
    };
    nested.nodes.insert(node("outer"), outer);
    nested.nodes.get_mut(&node("root")).unwrap().kind = NodeKind::Sequence {
        children: vec![node("outer")],
    };
    nested.validate().unwrap();
    let context = SemanticContext {
        parent: node("container"),
        ..selected(13, None)
    };
    assert!(
        resolve(
            &nested,
            &context,
            vec![instruction.clone()],
            &BTreeMap::new(),
            resolver
        )
        .is_err()
    );
    // A selected Retime owns its output clock, including a nonzero input map
    // over an unrestricted Sequence child. No child or mapping is rewritten.
    document.nodes.get_mut(&node("held")).unwrap().kind = NodeKind::Retime {
        child: node("inside"),
        duration: FrameDuration::new(9).unwrap(),
        mapping: range(1, 7),
        pitch: crate::PitchPolicy::FollowSpeed,
        purpose: crate::RetimePurpose::Edit,
    };
    document.validate().unwrap();
    let retime = document.clone();
    let planned = resolve(
        &retime,
        &selected(3, None),
        vec![instruction.clone()],
        &BTreeMap::new(),
        resolver,
    )
    .unwrap();
    let value = planned.document.nodes()[&node("held")]
        .framing
        .as_ref()
        .unwrap()
        .evaluate_with_follow_exact(ExactRatio::new(9, 2).unwrap(), ExactRatio::integer(9))
        .unwrap();
    assert_eq!(value.follow.unwrap().1, ExactRatio::integer(3));
    assert_eq!(value.pose.scale, ExactRatio::new(3, 4).unwrap());
    let replay =
        crate::replay_compound::<EditError>(&retime, planned.request.as_ref().unwrap(), |_| Ok(()))
            .unwrap();
    assert_eq!(replay.document, planned.document);
    assert_eq!(
        replay.edit.inverse.apply(&planned.document).unwrap(),
        retime
    );
    let mut only_framing = retime.clone();
    only_framing.revision_id = planned.document.revision_id().clone();
    only_framing.nodes.get_mut(&node("held")).unwrap().framing =
        planned.document.nodes()[&node("held")].framing.clone();
    assert_eq!(planned.document, only_framing);
    for (anchor, head) in [(0, 9), (1, 8), (9, 0)] {
        let ranged = resolve(
            &retime,
            &selected(3, visual(anchor, head)),
            vec![instruction.clone()],
            &BTreeMap::new(),
            resolver,
        )
        .unwrap();
        assert_ranged_follow_plan(&retime, &ranged, anchor.min(head), anchor.max(head));
    }
    nested.nodes.get_mut(&node("held")).unwrap().kind = retime.nodes()[&node("held")].kind.clone();
    nested.validate().unwrap();
    assert!(
        resolve(
            &nested,
            &context,
            vec![instruction],
            &BTreeMap::new(),
            resolver
        )
        .is_err()
    );
    assert_eq!(before.nodes()[&node("held")].kind, expected_node.kind);
}
