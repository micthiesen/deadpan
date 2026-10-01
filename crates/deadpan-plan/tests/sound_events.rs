use deadpan_core::*;
use deadpan_plan::*;
use serde_json::json;

fn document() -> ProjectDocument {
    let document = ProjectDocument::new(
        ProjectId::new("sound-project").unwrap(),
        RevisionId::new("before").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(48_000, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root").unwrap(),
    )
    .unwrap();
    let mut wire = serde_json::to_value(document).unwrap();
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base: SourceTimeBase::new(1, 48_000).unwrap(),
        },
        SourceTimestamp {
            ticks: 100,
            time_base: SourceTimeBase::new(1, 48_000).unwrap(),
        },
    )
    .unwrap();
    wire["assets"] = json!({"sound":AssetRecord { label:"sound".into(), content_hash:"a".repeat(64), video:None, audio:Some(span), still_image:false, frame_count:None, source_qualification:Some(SourceQualificationId::new("b".repeat(64)).unwrap()) }});
    wire["assets"]["picture"] = json!(AssetRecord {
        label: "picture".into(),
        content_hash: "c".repeat(64),
        video: None,
        audio: None,
        still_image: true,
        frame_count: None,
        source_qualification: None
    });
    wire["nodes"]["root"] = serde_json::to_value(BeatNode::sequence(
        "root",
        vec![NodeId::new("blank").unwrap()],
    ))
    .unwrap();
    wire["nodes"]["blank"] = serde_json::to_value(BeatNode {
        label: "blank".into(),
        framing: None,
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Source {
            source: SourceNode {
                edit_window: None,
                duration: FrameDuration::new(64).unwrap(),
                video: SourceVideo::Still {
                    asset: AssetId::new("picture").unwrap(),
                },
                audio: None,
                link: LinkRelation::Independent,
                audio_offset: AudioSample(0),
                audio_mapping: SourceAudioMapping::FitBeat,
                video_mapping: SourceVideoMapping::FitBeat,
            },
        },
    })
    .unwrap();
    let event = SoundEvent {
        owner: NodeId::new("root").unwrap(),
        label: "fractional sound".into(),
        source: SourceAudio {
            asset: AssetId::new("sound").unwrap(),
            span: SourceSpan::new(
                SourceTimestamp {
                    ticks: 10,
                    ..span.start()
                },
                SourceTimestamp {
                    ticks: 42,
                    ..span.end()
                },
            )
            .unwrap(),
        },
        mapping: SourceAudioMapping::Placement {
            start: ExactRatio::new(5, 2).unwrap(),
            frames: ExactRatio::integer(32),
        },
        offset: AudioSample(0),
        gain_millidecibels: 0,
        start_edge: AudioEdgePolicy::Hard,
        end_edge: AudioEdgePolicy::Hard,
        overflow: SoundOverflowPolicy::Reject,
    };
    wire["sounds"] = json!({"event":event});
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

#[test]
fn root_sounds_use_round_even_boundaries_and_keep_exact_source_phase() {
    let doc = document();
    let plan = RenderPlan::compile(&doc).unwrap();
    let sound = plan.root_sound(&SoundId::new("event").unwrap()).unwrap();
    assert_eq!(sound.audible_samples(), AudioSample(2)..AudioSample(34));
    let query = sound
        .root_input_tape()
        .unwrap()
        .query(
            SignalSample(0)..SignalSample(64),
            AudioQueryLimits::default(),
        )
        .unwrap();
    let spans = query
        .spans
        .iter()
        .filter(|s| {
            matches!(
                s.content,
                AudioSignalContent::Leaf(AudioContent::Source { .. })
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].samples, SignalSample(2)..SignalSample(34));
    assert_eq!(
        spans[0].source_point(SignalSample(2)).unwrap().ticks,
        ExactRatio::new(19, 2).unwrap()
    );
    assert_eq!(
        spans[0].source_point(SignalSample(33)).unwrap().ticks,
        ExactRatio::new(81, 2).unwrap()
    );
    assert_eq!(plan.duration(), FrameDuration::new(64).unwrap());
    assert!(plan.root_sound(&SoundId::new("missing").unwrap()).is_err());
}

#[test]
fn root_sound_hold_policy_uses_current_project_clock_and_not_original_exhaustion() {
    let doc = document();
    let mut wire = serde_json::to_value(&doc).unwrap();
    wire["nodes"]["root"] = serde_json::to_value(BeatNode::sequence(
        "root",
        vec![
            NodeId::new("before").unwrap(),
            NodeId::new("hold").unwrap(),
            NodeId::new("after").unwrap(),
        ],
    ))
    .unwrap();
    let mut blank = doc.nodes()[&NodeId::new("blank").unwrap()].clone();
    let NodeKind::Source { source } = &mut blank.kind else {
        unreachable!()
    };
    source.duration = FrameDuration::new(16).unwrap();
    wire["nodes"].as_object_mut().unwrap().remove("blank");
    wire["nodes"]["before"] = serde_json::to_value(&blank).unwrap();
    let NodeKind::Source { source } = &mut blank.kind else {
        unreachable!()
    };
    source.duration = FrameDuration::new(32).unwrap();
    wire["nodes"]["after"] = serde_json::to_value(&blank).unwrap();
    wire["nodes"]["hold"] = serde_json::to_value(BeatNode::hold(
        "silent",
        HoldRecipe {
            duration: FrameDuration::new(16).unwrap(),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
            picture_context: None,
        },
    ))
    .unwrap();
    let plan =
        RenderPlan::compile(&ProjectDocument::from_json(&wire.to_string()).unwrap()).unwrap();
    let sound = plan.root_sound(&SoundId::new("event").unwrap()).unwrap();
    let policy = sound
        .root_input_tape()
        .unwrap()
        .policy(
            SignalSample(0)..SignalSample(64),
            AudioQueryLimits::default(),
        )
        .unwrap();
    assert!(
        !policy
            .suppressed
            .iter()
            .any(|r| r.start.0 <= 16 && r.end.0 >= 32)
    );
    let gated = sound
        .gate_fades(
            AudioSample(16)..AudioSample(32),
            AudioQueryLimits::default(),
        )
        .unwrap();
    assert!(gated.spans.iter().all(|span| span.length == 0));
    assert!(
        !policy
            .suppressed
            .iter()
            .any(|r| r.contains(&SignalSample(10)))
    );
    assert!(
        !policy
            .suppressed
            .iter()
            .any(|r| r.contains(&SignalSample(33)))
    );
}
