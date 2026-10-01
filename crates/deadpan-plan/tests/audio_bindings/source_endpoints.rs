use super::*;

fn id(s: &str) -> NodeId {
    NodeId::new(s).unwrap()
}
fn frames(n: i64) -> FrameDuration {
    FrameDuration::new(n).unwrap()
}
fn crop(a: i64, b: i64) -> BeatNode {
    BeatNode {
        label: "Crop".into(),
        framing: None,
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            child: id("source"),
            duration: frames(b - a),
            mapping: FrameRange::new(ProjectFrame(a), ProjectFrame(b)).unwrap(),
            pitch: PitchPolicy::FollowSpeed,
            purpose: RetimePurpose::Partition,
        },
    }
}
#[test]
fn source_endpoint_anchor_projects_outside_final_crop_without_clamping() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let blank = ProjectDocument::new(
        ProjectId::new("endpoints").unwrap(),
        RevisionId::new("old").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: rate,
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: 48_000,
            time_base,
        },
    )
    .unwrap();
    let mut wire = serde_json::to_value(blank).unwrap();
    wire["nodes"]["root"] =
        serde_json::to_value(BeatNode::sequence("Root", vec![id("lead"), id("crop")])).unwrap();
    wire["nodes"]["lead"] = serde_json::to_value(BeatNode::hold(
        "Lead",
        HoldRecipe {
            picture_context: None,
            duration: frames(5),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    ))
    .unwrap();
    wire["nodes"]["crop"] = serde_json::to_value(crop(0, 1)).unwrap();
    wire["nodes"]["source"] = serde_json::to_value(BeatNode {
        label: "Source".into(),
        framing: None,
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Source {
            source: SourceNode {
                duration: frames(6),
                edit_window: None,
                video: SourceVideo::Blank,
                video_mapping: SourceVideoMapping::FitBeat,
                audio: Some(SourceAudio {
                    asset: AssetId::new("media").unwrap(),
                    span,
                }),
                audio_mapping: SourceAudioMapping::natural_rate(span, rate).unwrap(),
                audio_offset: AudioSample(7),
                link: LinkRelation::Independent,
            },
        },
    })
    .unwrap();
    wire["assets"]["media"] = serde_json::to_value(AssetRecord {
        label: "Audio".into(),
        content_hash: "a".repeat(64),
        video: None,
        audio: Some(span),
        still_image: false,
        frame_count: None,
        source_qualification: None,
    })
    .unwrap();
    let old = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let timing = AudioTimingId {
        allocation: RevisionId::new("capture").unwrap(),
        ordinal: 0,
    };
    let state = capture_unbound_audio_bindings(&old, timing).unwrap();
    let mut bindings = state.bindings().clone();
    let binding = bindings.get_mut(&id("source")).unwrap();
    binding
        .reanchors
        .push(AudioReanchorStep::for_source_endpoint(
            binding.lattice.clone(),
            AudioSourceEndpoint::End,
        ));
    let state = AudioBindingState::new(
        state
            .timings()
            .iter()
            .map(|(id, layout)| AudioTimingRecord {
                id: id.clone(),
                layout: layout.clone(),
            })
            .collect(),
        bindings,
    )
    .unwrap();
    wire["nodes"]["crop"] = serde_json::to_value(crop(2, 3)).unwrap();
    wire["audio_bindings"] = serde_json::to_value(state).unwrap();
    let current = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = RenderPlan::compile(&current).unwrap();
    let query = plan
        .audio_processing(
            AudioSample(8008)..AudioSample(8264),
            AudioQueryLimits::default(),
        )
        .unwrap();
    assert_eq!(query.spans.len(), 1);
    let span = &query.spans[0];
    assert_eq!(span.allocated_samples.start, AudioSample(8008));
    let AudioSignalContent::Bound(bound) = &span.content else {
        panic!("expected retained Source")
    };
    // Old End B(6)=9610, final virtual End B(4)=6406. End lies
    // before final crop [5,6), and must not be clamped to its first sample.
    assert_eq!(
        bound.reference_at_offset(0).unwrap(),
        ExactRatio::integer(11212)
    );
    assert_eq!(
        bound.reference_at_offset(255).unwrap(),
        ExactRatio::integer(11467)
    );
    assert!(
        plan.audio_processing(
            AudioSample(8008)..AudioSample(8009),
            AudioQueryLimits {
                maximum_work: 1,
                maximum_spans: 1
            }
        )
        .is_err()
    );
    assert!(FrozenAudioContext::capture(&current).is_err());
}
