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
        black: false,
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
            black: false,
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
            black: false,
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
fn a_black_pause_needs_no_picture_resolver_and_keeps_its_wire_form() {
    let document = tree(&["a"], vec![("a", hold(10))]);
    let black = SemanticInstruction::InsertPause {
        length: PauseLength::Frames {
            frames: NonZeroU32::new(3).unwrap(),
        },
        black: true,
    };
    // Plain planning has no picture resolver; a black pause does not ask.
    let planned = plan(
        &document,
        context("root", 4),
        vec![black.clone()],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(planned.document.duration().unwrap().frames(), 13);
    let inserted = planned
        .document
        .nodes()
        .values()
        .find_map(|node| match &node.kind {
            NodeKind::Hold { recipe } if recipe.duration.frames() == 3 => Some(recipe.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(inserted.video, HoldVideo::Background);
    assert!(inserted.picture_context.is_none());
    // Freeze pauses keep their earlier wire form; black adds one field.
    let wire = serde_json::to_value(pause(2)).unwrap();
    assert!(!wire.to_string().contains("black"), "{wire}");
    let round: SemanticInstruction =
        serde_json::from_value(serde_json::to_value(&black).unwrap()).unwrap();
    assert_eq!(round, black);
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

fn frames_length(frames: u32) -> PauseLength {
    PauseLength::Frames {
        frames: NonZeroU32::new(frames).unwrap(),
    }
}

#[test]
fn one_more_time_repeats_with_each_gap_shorter_than_the_last() {
    let document = tree(&["a", "b"], vec![("a", hold(3)), ("b", hold(4))]);
    let recipe = crate::GagRecipe::OneMoreTime {
        version: crate::GAG_RECIPE_VERSION,
        plays: NonZeroU32::new(4).unwrap(),
        gap: frames_length(6),
        shorten: frames_length(2),
    };
    let mut start = context("root", 0);
    start.selected_child = Some(node("a"));
    let planned = plan_pauses(&document, start, vec![SemanticInstruction::Gag { recipe }]).unwrap();
    // Four 3-frame plays with 6, 4 and 2 frames between them, then b.
    assert_eq!(planned.document.duration().unwrap().frames(), 12 + 12 + 4);
    let group = planned.context.selected_child.clone().unwrap();
    assert_eq!(
        planned.document.nodes()[&group].label,
        "One More Time · v1 · 4 plays, gap 6f shortening by 2f"
    );
    let NodeKind::Sequence { children } = &planned.document.nodes()[&group].kind else {
        panic!("the gag is a group")
    };
    let NodeKind::Repeat {
        iterations, gap, ..
    } = &planned.document.nodes()[&children[0]].kind
    else {
        panic!("the group holds the Repeat")
    };
    assert_eq!(gap.as_ref().unwrap().duration.frames(), 6);
    let branches = &planned.document.gap_overrides()[&children[0]];
    for (position, frames) in [(1, 4), (2, 2)] {
        let hold = &branches.get(&iterations.at(position).unwrap()).unwrap();
        assert!(matches!(
            &planned.document.nodes()[*hold].kind,
            NodeKind::Hold { recipe } if recipe.duration.frames() == frames
                && recipe.audio == HoldAudio::Silence
        ));
    }
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
    // A gap that would shrink to nothing refuses the whole recipe.
    let too_short = crate::GagRecipe::OneMoreTime {
        version: crate::GAG_RECIPE_VERSION,
        plays: NonZeroU32::new(4).unwrap(),
        gap: frames_length(4),
        shorten: frames_length(2),
    };
    let mut start = context("root", 0);
    start.selected_child = Some(node("a"));
    assert!(
        plan_pauses(
            &document,
            start,
            vec![SemanticInstruction::Gag { recipe: too_short }]
        )
        .is_err()
    );
}

#[test]
fn repeat_gaps_set_the_default_and_refuse_more_gaps_than_plays_allow() {
    let document = tree(&["a", "b"], vec![("a", hold(3)), ("b", hold(4))]);
    let mut start = context("root", 0);
    start.selected_child = Some(node("a"));
    let wrap = SemanticInstruction::Repeat {
        selector: SemanticSelector::SelectedBeat,
        plays: NonZeroU32::new(3).unwrap(),
        escalation: None,
    };
    let gaps = |values: &[u32]| SemanticInstruction::SetRepeat {
        plays: None,
        gaps: Some(values.iter().map(|value| frames_length(*value)).collect()),
        escalation: None,
    };
    let planned = plan_pauses(&document, start.clone(), vec![wrap.clone(), gaps(&[5])]).unwrap();
    assert_eq!(planned.document.duration().unwrap().frames(), 9 + 10 + 4);
    assert_eq!(planned.trace[1].resolved_range, Some(range(0, 19)));
    let cleared = plan_pauses(
        &document,
        start.clone(),
        vec![wrap.clone(), gaps(&[5]), gaps(&[])],
    )
    .unwrap();
    assert_eq!(cleared.document.duration().unwrap().frames(), 9 + 4);
    let error = plan_pauses(
        &document,
        start.clone(),
        vec![wrap.clone(), gaps(&[3, 2, 1])],
    )
    .unwrap_err();
    assert!(error.message.contains("2 gaps"), "{error:?}");
    let error = plan_pauses(&document, start, vec![gaps(&[3])]).unwrap_err();
    assert_eq!(error.code, EditErrorCode::WrongNodeKind);
    let wire = serde_json::to_value(gaps(&[5, 3])).unwrap();
    assert_eq!(
        serde_json::from_value::<SemanticInstruction>(wire).unwrap(),
        gaps(&[5, 3])
    );
}

#[test]
fn nothing_happens_holds_room_tone_then_true_silence_in_one_group() {
    let mut document = tree(&["a", "b"], vec![("a", hold(3)), ("b", hold(4))]);
    let asset = crate::AssetId::new("original").unwrap();
    let time_base = crate::SourceTimeBase::new(1, 48_000).unwrap();
    let span = crate::SourceSpan::new(
        crate::SourceTimestamp {
            ticks: 0,
            time_base,
        },
        crate::SourceTimestamp {
            ticks: 96_000,
            time_base,
        },
    )
    .unwrap();
    document.assets.insert(
        asset.clone(),
        crate::AssetRecord {
            label: "Original".into(),
            content_hash: "a".repeat(64),
            audio: Some(span),
            video: None,
            frame_count: None,
            still_image: false,
            source_qualification: Some(crate::SourceQualificationId::new("b".repeat(64)).unwrap()),
        },
    );
    document.validate().unwrap();
    let register = name('t');
    let bank = BTreeMap::from([(
        register,
        Arc::new(RegisterValue::Original {
            revision: revision("base"),
            asset: asset.clone(),
            qualification: crate::SourceQualificationId::new("b".repeat(64)).unwrap(),
            ordinals: 0..30,
        }),
    )]);
    // The copied moment's picture starts a third of a frame into its audio:
    // 1600 samples per frame put In at 533⅓ and Out at 48533⅓ samples.
    let moment = SourceNode {
        duration: FrameDuration::new(30).unwrap(),
        edit_window: Some(
            crate::SourceEditWindow::new(ExactRatio::ZERO, ExactRatio::integer(30)).unwrap(),
        ),
        video: crate::SourceVideo::Blank,
        video_mapping: crate::SourceVideoMapping::FitBeat,
        audio: Some(crate::SourceAudio {
            asset: asset.clone(),
            span,
        }),
        audio_mapping: crate::SourceAudioMapping::Placement {
            start: ExactRatio::new(-1, 3).unwrap(),
            frames: ExactRatio::integer(60),
        },
        link: crate::LinkRelation::Independent,
        audio_offset: crate::AudioSample(0),
    };
    let recipe = crate::GagRecipe::NothingHappens {
        version: crate::GAG_RECIPE_VERSION,
        tone: frames_length(5),
        silence: frames_length(4),
        register,
    };
    let planned = plan_semantic_with_speech(
        &document,
        &context("root", 3),
        &program(vec![SemanticInstruction::Gag { recipe }]),
        SemanticRegisterBank {
            entries: &bank,
            version: 7,
        },
        revision("outer"),
        allocate,
        |_, _| Ok(moment.clone()),
        |_| Err(crate::speech_unavailable()),
        |_, _| {
            Ok(PauseProvider {
                video: HoldVideo::Background,
                picture_context: None,
            })
        },
    )
    .unwrap();
    assert_eq!(planned.document.duration().unwrap().frames(), 7 + 9);
    let group = planned.context.selected_child.clone().unwrap();
    assert_eq!(
        planned.document.nodes()[&group].label,
        "Nothing Happens · v1 · room tone 5f from register t, then 4f silence"
    );
    let NodeKind::Sequence { children } = &planned.document.nodes()[&group].kind else {
        panic!("the gag is a group")
    };
    let audio: Vec<_> = children
        .iter()
        .map(|child| match &planned.document.nodes()[child].kind {
            NodeKind::Hold { recipe } => (recipe.duration.frames(), recipe.audio.clone()),
            other => panic!("{other:?}"),
        })
        .collect();
    let expected = crate::SourceSpan::new(
        crate::SourceTimestamp {
            ticks: 534,
            time_base,
        },
        crate::SourceTimestamp {
            ticks: 48_533,
            time_base,
        },
    )
    .unwrap();
    assert_eq!(
        audio,
        [
            (
                5,
                HoldAudio::RoomTone {
                    source: crate::SourceAudio {
                        asset,
                        span: expected
                    }
                }
            ),
            (4, HoldAudio::Silence)
        ]
    );
    // The measured selected placement of a native copy yields the same range.
    let mut selected = moment.clone();
    selected.audio_mapping = crate::SourceAudioMapping::SelectedPlacement {
        start: ExactRatio::new(-1, 3).unwrap(),
        frames: ExactRatio::integer(60),
        selection: crate::ExactFrameRange {
            start: ExactRatio::ZERO,
            end: ExactRatio::integer(30),
        },
    };
    assert_eq!(
        crate::copied_moment_audio(&selected).unwrap().span,
        expected
    );
    selected.audio_offset = crate::AudioSample(1);
    assert!(crate::copied_moment_audio(&selected).is_err());
    selected.audio_offset = crate::AudioSample(0);
    // A narrower audible selection limits the range: [2, 10) frames is
    // samples [3200 + 533⅓, 16000 + 533⅓) → [3734, 16533).
    let crate::SourceAudioMapping::SelectedPlacement { selection, .. } =
        &mut selected.audio_mapping
    else {
        unreachable!()
    };
    *selection = crate::ExactFrameRange {
        start: ExactRatio::integer(2),
        end: ExactRatio::integer(10),
    };
    let narrowed = crate::copied_moment_audio(&selected).unwrap().span;
    assert_eq!(
        (narrowed.start().ticks, narrowed.end().ticks),
        (3734, 16533)
    );
    // A slipped Source keeps its window but its mapping starts elsewhere:
    // the heard samples follow the mapping, window [3, 13) at start 5/2.
    let mut slipped = moment.clone();
    slipped.edit_window = Some(
        crate::SourceEditWindow::new(ExactRatio::integer(3), ExactRatio::integer(13)).unwrap(),
    );
    slipped.audio_mapping = crate::SourceAudioMapping::Placement {
        start: ExactRatio::new(5, 2).unwrap(),
        frames: ExactRatio::integer(60),
    };
    let slipped_span = crate::copied_moment_audio(&slipped).unwrap().span;
    assert_eq!(
        (slipped_span.start().ticks, slipped_span.end().ticks),
        (800, 16800)
    );
    // Endpoints must be whole samples of a sample-rate time base.
    let mut coarse = moment.clone();
    let tenths = crate::SourceTimeBase::new(1001, 30_000).unwrap();
    coarse.audio = Some(crate::SourceAudio {
        asset: crate::AssetId::new("original").unwrap(),
        span: crate::SourceSpan::new(
            crate::SourceTimestamp {
                ticks: 0,
                time_base: tenths,
            },
            crate::SourceTimestamp {
                ticks: 60,
                time_base: tenths,
            },
        )
        .unwrap(),
    });
    assert!(crate::copied_moment_audio(&coarse).is_err());
    let request = planned.request.as_ref().unwrap();
    let Command::Compound { transaction } = &request.command else {
        panic!()
    };
    assert!(
        transaction.inputs().contains_key(&register),
        "the frozen bank records the register it read"
    );
    let replay = crate::replay_compound::<EditError>(&document, request, |_| Ok(())).unwrap();
    assert_eq!(replay.document, planned.document);
    assert_eq!(
        replay.edit.inverse.apply(&replay.document).unwrap(),
        document
    );
}

#[test]
fn one_repeat_change_wraps_or_sets_plays_gaps_and_escalation_together() {
    let document = tree(&["a", "b"], vec![("a", hold(3)), ("b", hold(4))]);
    let mut start = context("root", 0);
    start.selected_child = Some(node("a"));
    let escalation = crate::RepeatEscalation {
        gain_step: crate::GainDb::new(3_000).unwrap(),
        zoom: None,
    };
    let change =
        |plays: Option<u32>, gaps: Option<&[u32]>, escalation| SemanticInstruction::SetRepeat {
            plays: plays.map(|plays| NonZeroU32::new(plays).unwrap()),
            gaps: gaps.map(|gaps| gaps.iter().map(|value| frames_length(*value)).collect()),
            escalation,
        };
    // A plain beat is wrapped, then gapped and escalated: three leaves.
    let planned = plan_pauses(
        &document,
        start.clone(),
        vec![change(Some(3), Some(&[4]), Some(escalation))],
    )
    .unwrap();
    assert_eq!(planned.document.duration().unwrap().frames(), 9 + 8 + 4);
    let repeat = planned.context.selected_child.clone().unwrap();
    assert!(matches!(
        &planned.document.nodes()[&repeat].kind,
        NodeKind::Repeat { escalation: Some(value), gap: Some(gap), .. }
            if *value == escalation && gap.duration.frames() == 4
    ));
    assert_eq!(planned.trace[0].resolved_range, Some(range(0, 17)));
    let Command::Compound { transaction } = &planned.request.as_ref().unwrap().command else {
        panic!()
    };
    assert_eq!(transaction.steps().len(), 3);
    // On an existing Repeat, a new count and a removed step change in one
    // plan; the escalation is validated against the new count.
    let mut repeated = BeatNode::sequence("Repeat", vec![]);
    repeated.kind = NodeKind::Repeat {
        child: node("a"),
        iterations: crate::IterationOrder::new(revision("old-plays"), 3).unwrap(),
        gap: None,
        escalation: Some(escalation),
    };
    let existing = tree(
        &["r", "b"],
        vec![("r", repeated), ("a", hold(3)), ("b", hold(4))],
    );
    let mut on_repeat = context("root", 0);
    on_repeat.selected_child = Some(node("r"));
    let changed = plan_pauses(
        &existing,
        on_repeat.clone(),
        vec![change(
            Some(2),
            None,
            Some(crate::RepeatEscalation {
                gain_step: crate::GainDb::UNITY,
                zoom: None,
            }),
        )],
    )
    .unwrap();
    assert_eq!(changed.document.duration().unwrap().frames(), 6 + 4);
    assert!(matches!(
        &changed.document.nodes()[&node("r")].kind,
        NodeKind::Repeat {
            escalation: None,
            ..
        }
    ));
    // Nothing to change authors nothing.
    let same = plan_pauses(
        &existing,
        on_repeat,
        vec![change(Some(3), None, Some(escalation))],
    )
    .unwrap();
    assert!(same.request.is_none());
    assert!(
        SemanticProgram::new(vec![change(None, None, None)]).is_err(),
        "an empty change is refused"
    );
}

#[test]
fn restating_a_repeats_gaps_spends_no_revision() {
    let gap = HoldRecipe {
        duration: FrameDuration::new(4).unwrap(),
        picture_context: None,
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
    };
    let mut repeated = BeatNode::sequence("Repeat", vec![]);
    repeated.kind = NodeKind::Repeat {
        child: node("a"),
        iterations: crate::IterationOrder::new(revision("old-plays"), 3).unwrap(),
        gap: Some(gap),
        escalation: None,
    };
    let document = tree(&["r"], vec![("r", repeated), ("a", hold(3))]);
    let mut start = context("root", 0);
    start.selected_child = Some(node("r"));
    let same = plan_pauses(
        &document,
        start.clone(),
        vec![SemanticInstruction::SetRepeat {
            plays: Some(NonZeroU32::new(3).unwrap()),
            gaps: Some(vec![frames_length(4)]),
            escalation: None,
        }],
    )
    .unwrap();
    assert!(
        same.request.is_none(),
        "an unchanged gap set authors nothing"
    );
    assert_eq!(same.context.selected_child, Some(node("r")));
}
