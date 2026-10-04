use std::{collections::BTreeMap, sync::Arc};

use deadpan_core::*;
use deadpan_plan::{
    AudioBoundaryRule, AudioDefinitionSelector, AudioProjectedRoot, AudioRootPlacement,
    AudioRoutedRoot, AudioRoutedRootInput, AudioRoutedSignal, AudioRoutedSignalInput,
    AudioSampleGrid, AudioSignalContent, AudioSignalTape, AudioSignalTapeRun, AudioSoundRoute,
    AudioSourceOccurrence, AudioSourceVoice, AudioSourceVoiceRecipe, AudioStage,
    AudioStageProjection, RenderPlan, SignalSample,
};

fn q(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}

fn id(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

fn definition(name: &str) -> AudioDefinitionSelector {
    AudioDefinitionSelector::Node { node: id(name) }
}

fn plan(rate: FrameRate) -> RenderPlan {
    plan_with_prefix(rate, 0)
}

fn plan_with_prefix(rate: FrameRate, prefix_frames: i64) -> RenderPlan {
    let mut wire = serde_json::to_value(
        ProjectDocument::new(
            ProjectId::new("routed-voice").unwrap(),
            RevisionId::new("revision").unwrap(),
            PresentationBasis {
                width: 16,
                height: 16,
                frame_rate: rate,
                color_policy: ColorPolicy::SdrRec709,
            },
            id("root"),
        )
        .unwrap(),
    )
    .unwrap();
    let mut children = Vec::new();
    if prefix_frames > 0 {
        children.push(id("prefix"));
    }
    children.push(id("repeat"));
    let mut nodes = BTreeMap::from([
        (id("root"), BeatNode::sequence("Root", children)),
        (
            id("repeat"),
            BeatNode {
                label: "Repeat".into(),
                framing: None,
                audio_treatments: Default::default(),
                audio_editorial_edges: Default::default(),
                audio_edges: Default::default(),
                kind: NodeKind::Repeat {
                    child: id("stage"),
                    iterations: IterationOrder::new(RevisionId::new("plays").unwrap(), 2).unwrap(),
                    gap: None,
                },
            },
        ),
        (
            id("stage"),
            BeatNode {
                label: "Preserve".into(),
                framing: None,
                audio_treatments: Default::default(),
                audio_editorial_edges: Default::default(),
                audio_edges: Default::default(),
                kind: NodeKind::Retime {
                    child: id("input"),
                    duration: FrameDuration::new(6).unwrap(),
                    mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(9)).unwrap(),
                    pitch: PitchPolicy::Preserve,
                    purpose: RetimePurpose::Edit,
                },
            },
        ),
        (
            id("input"),
            BeatNode::hold(
                "Input owner",
                HoldRecipe {
                    duration: FrameDuration::new(9).unwrap(),
                    video: HoldVideo::Background,
                    picture_context: None,
                    audio: HoldAudio::Silence,
                },
            ),
        ),
    ]);
    if prefix_frames > 0 {
        nodes.insert(
            id("prefix"),
            BeatNode::hold(
                "Prefix",
                HoldRecipe {
                    duration: FrameDuration::new(prefix_frames).unwrap(),
                    video: HoldVideo::Background,
                    picture_context: None,
                    audio: HoldAudio::Silence,
                },
            ),
        );
    }
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        AssetId::new("sound").unwrap(),
        AssetRecord {
            label: "Sound".into(),
            content_hash: "a".repeat(64),
            video: None,
            audio: Some(source().span),
            still_image: false,
            frame_count: None,
            source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
        },
    )]))
    .unwrap();
    RenderPlan::compile(&ProjectDocument::from_json(&wire.to_string()).unwrap()).unwrap()
}

fn source() -> SourceAudio {
    let time_base = SourceTimeBase::new(1, 44_100).unwrap();
    SourceAudio {
        asset: AssetId::new("sound").unwrap(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: 10,
                time_base,
            },
            SourceTimestamp {
                ticks: 44_110,
                time_base,
            },
        )
        .unwrap(),
    }
}

fn recipe(plan: &RenderPlan) -> AudioSourceVoiceRecipe {
    AudioSourceVoiceRecipe {
        source: source(),
        mapping: SourceAudioMapping::natural_rate(
            source().span,
            plan.metadata().presentation_basis.frame_rate,
        )
        .unwrap(),
        offset: AudioSample(-1),
    }
}

fn voice(plan: &RenderPlan) -> AudioSourceVoice<'_> {
    plan.audio_definition(definition("stage"))
        .unwrap()
        .signal()
        .source_voice(recipe(plan))
        .unwrap()
}

fn stages(plan: &RenderPlan) -> Vec<AudioStage<'_>> {
    let signal = plan.audio_signal();
    signal
        .query(
            SignalSample(0)..signal.sample_count().unwrap(),
            Default::default(),
        )
        .unwrap()
        .spans
        .into_iter()
        .map(|span| match span.content {
            AudioSignalContent::Stage(stage) => stage,
            _ => panic!("expected Preserve stage"),
        })
        .collect()
}

fn projection<'plan>(
    plan: &'plan RenderPlan,
    stage: AudioStage<'plan>,
) -> Arc<AudioStageProjection<'plan>> {
    let input = stage.input_signal();
    let support = input.support();
    let tape = AudioSignalTape::new(
        plan,
        support.clone(),
        vec![AudioSignalTapeRun::new(
            support.clone(),
            support.clone(),
            input.clone(),
        )],
    )
    .unwrap();
    let output = q(0, 1)..q(6, 1);
    let policy = AudioSignalTape::new(
        plan,
        output.clone(),
        vec![AudioSignalTapeRun::new(output, support, input)],
    )
    .unwrap();
    AudioStageProjection::new(stage, tape, policy, FrameDuration::new(6).unwrap()).unwrap()
}

fn point_route(
    extent: ExactRatio,
    origin: ExactRatio,
    step: ExactRatio,
) -> AudioSoundRoute<SignalSample> {
    AudioSoundRoute::<SignalSample>::new(
        SoundRoute::identity(extent).unwrap(),
        vec![AudioSampleGrid::new(origin, step, AudioBoundaryRule::PointCeil).unwrap()],
    )
    .unwrap()
}

fn root_route(
    extent: ExactRatio,
    origin: ExactRatio,
    step: ExactRatio,
) -> AudioSoundRoute<AudioSample> {
    AudioSoundRoute::<AudioSample>::new(
        SoundRoute::identity(extent).unwrap(),
        vec![AudioSampleGrid::new(origin, step, AudioBoundaryRule::RoundEven).unwrap()],
    )
    .unwrap()
}

fn occurrence(plan: &RenderPlan, ordinal: u32) -> AudioSourceOccurrence<'_> {
    let base = SourceTimeBase::new(1, 44_100).unwrap();
    let source = SourceAudio {
        asset: AssetId::new("sound").unwrap(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: 10,
                time_base: base,
            },
            SourceTimestamp {
                ticks: 8_830,
                time_base: base,
            },
        )
        .unwrap(),
    };
    let recipe = AudioSourceVoiceRecipe {
        mapping: SourceAudioMapping::natural_rate(
            source.span,
            plan.metadata().presentation_basis.frame_rate,
        )
        .unwrap(),
        source,
        offset: AudioSample(0),
    };
    plan.source_voice_occurrence(
        InstancePath {
            node: id("input"),
            repeats: vec![RepeatInstance {
                node: id("repeat"),
                iteration: IterationId {
                    allocation: RevisionId::new("plays").unwrap(),
                    ordinal,
                },
            }],
        },
        recipe,
        Default::default(),
    )
    .unwrap()
}

fn short_occurrence(plan: &RenderPlan, ordinal: u32) -> AudioSourceOccurrence<'_> {
    let mut owner = occurrence(plan, ordinal).instance().clone();
    // A sound on the Retime output bypasses its own Preserve processor, so
    // this selected recipe has a smaller audible interval than its owner.
    owner.node = id("stage");
    let base = SourceTimeBase::new(1, 44_100).unwrap();
    let source = SourceAudio {
        asset: AssetId::new("sound").unwrap(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: 10,
                time_base: base,
            },
            SourceTimestamp {
                ticks: 8_830,
                time_base: base,
            },
        )
        .unwrap(),
    };
    let natural = SourceAudioMapping::natural_rate(
        source.span,
        plan.metadata().presentation_basis.frame_rate,
    )
    .unwrap()
    .duration_frames(FrameDuration::ZERO)
    .unwrap();
    plan.source_voice_occurrence(
        owner,
        AudioSourceVoiceRecipe {
            source,
            mapping: SourceAudioMapping::SelectedPlacement {
                start: ExactRatio::ZERO,
                frames: natural,
                selection: ExactFrameRange {
                    start: ExactRatio::ZERO,
                    end: q(3, 1),
                },
            },
            offset: AudioSample(0),
        },
        Default::default(),
    )
    .unwrap()
}

#[test]
fn source_capture_retains_scope_identity_and_complete_recipe() {
    let plan = plan(FrameRate::new(48_000, 1).unwrap());
    let voice = voice(&plan);
    let identity = voice.identity();
    let routed =
        AudioRoutedSignal::source(voice.clone(), point_route(q(6, 1), q(0, 1), q(1, 1))).unwrap();
    assert!(routed.belongs_to(&plan));
    assert!(!routed.belongs_to(&plan.clone()));
    assert!(std::ptr::eq(routed.plan(), &plan));
    assert_eq!(routed.samples(), SignalSample(0)..SignalSample(6));
    let AudioRoutedSignalInput::Source(retained) = routed.input() else {
        panic!("expected source");
    };
    assert_eq!(retained.identity(), identity);
    assert_eq!(retained.source(), &source());
    assert_eq!(
        retained.input_signal().definition(),
        Some(&definition("stage"))
    );
    assert_eq!(retained.input_signal().support(), q(0, 1)..q(6, 1));
    assert_ne!(voice.identity(), self::voice(&plan).identity());
    let query = retained
        .input_signal()
        .query(routed.samples(), Default::default())
        .unwrap();
    let point = query.spans[0].source_point(SignalSample(0)).unwrap();
    assert_eq!(point.ticks, q(10, 1).checked_add(q(147, 160)).unwrap());
}

#[test]
fn source_capture_rejects_same_allocation_with_wrong_origin_spacing_or_extent() {
    let plan = plan(FrameRate::new(48_000, 1).unwrap());
    for route in [
        point_route(q(6, 1), q(1, 4), q(1, 1)),
        point_route(q(6, 1), q(0, 1), q(1001, 1000)),
        point_route(q(23, 4), q(0, 1), q(1, 1)),
    ] {
        assert_eq!(route.recipe_samples(), SignalSample(0)..SignalSample(6));
        assert!(AudioRoutedSignal::source(voice(&plan), route).is_err());
    }
    assert!(
        AudioSoundRoute::<SignalSample>::new(
            SoundRoute::identity(q(6, 1)).unwrap(),
            vec![AudioSampleGrid::new(q(0, 1), q(1, 1), AudioBoundaryRule::RoundEven).unwrap()],
        )
        .is_err()
    );
}

#[test]
fn cropped_or_constrained_source_capture_is_not_a_complete_recipe() {
    let plan = plan(FrameRate::new(48_000, 1).unwrap());
    let stage = stages(&plan).remove(0);
    for support in [q(0, 1)..q(6, 1), q(1, 1)..q(7, 1), q(0, 1)..q(9, 1)] {
        let selected = stage
            .child_signal(support.clone())
            .unwrap()
            .source_voice(recipe(&plan))
            .unwrap();
        let route = point_route(
            support.end.checked_sub(support.start).unwrap(),
            q(0, 1),
            q(1, 1),
        );
        assert!(AudioRoutedSignal::source(selected, route).is_err());
    }
}

#[test]
fn projected_signal_preserves_arc_identity_occurrence_and_live_plan() {
    let plan = plan(FrameRate::new(48_000, 1).unwrap());
    let stages = stages(&plan);
    let first = projection(&plan, stages[0].clone());
    let second = projection(&plan, stages[1].clone());
    let routed =
        AudioRoutedSignal::projected(Arc::clone(&first), point_route(q(6, 1), q(0, 1), q(1, 1)))
            .unwrap();
    let AudioRoutedSignalInput::Projected(retained) = routed.input() else {
        panic!("expected projection");
    };
    assert!(Arc::ptr_eq(retained, &first));
    assert_ne!(retained.identity(), second.identity());
    assert_ne!(
        retained.stage().descriptor().instance,
        second.stage().descriptor().instance
    );
    assert!(routed.belongs_to(&plan));
    assert!(!routed.belongs_to(&plan.clone()));
    assert!(AudioRoutedSignal::projected(first, point_route(q(6, 1), q(1, 4), q(1, 1))).is_err());
}

#[test]
fn root_capture_preserves_fractional_signed_origins_and_old_sample_maps() {
    let plan = plan(FrameRate::new(48_000, 1).unwrap());
    let projection = projection(&plan, stages(&plan).remove(0));
    for origin in [q(1, 4), q(-9, 4), q(7, 2)] {
        let root = AudioProjectedRoot::new(
            Arc::clone(&projection),
            AudioRootPlacement::new(origin, q(1, 1), q(0, 1)..q(6, 1)).unwrap(),
        )
        .unwrap();
        let route = root_route(q(6, 1), q(0, 1).checked_sub(origin).unwrap(), q(1, 1));
        let routed = AudioRoutedRoot::new(root.clone(), route).unwrap();
        assert_eq!(routed.route().recipe_samples(), root.samples());
        assert!(routed.belongs_to(&plan));
        assert!(!routed.belongs_to(&plan.clone()));
        assert_eq!(routed.root().unwrap().sampling(), root.sampling());
        let sampled = routed
            .route()
            .query(routed.samples(), Default::default())
            .unwrap();
        let recipe = sampled.spans[0].sampling.unwrap();
        let old = recipe
            .local_at(routed.samples().start)
            .unwrap()
            .checked_sub(routed.route().recipe_grid().frame_origin())
            .unwrap()
            .checked_div(routed.route().recipe_grid().frames_per_sample())
            .unwrap();
        assert_eq!(old, ExactRatio::integer(root.samples().start.0));
    }
}

#[test]
fn root_capture_rejects_crops_resumes_and_equal_count_grid_substitutions() {
    let plan = plan(FrameRate::new(48_000, 1).unwrap());
    let projection = projection(&plan, stages(&plan).remove(0));
    let root = AudioProjectedRoot::new(
        Arc::clone(&projection),
        AudioRootPlacement::new(q(1, 4), q(1, 1), q(0, 1)..q(6, 1)).unwrap(),
    )
    .unwrap();
    for route in [
        root_route(q(6, 1), q(0, 1), q(1, 1)),
        root_route(q(6, 1), q(-1, 4), q(1001, 1000)),
        root_route(q(23, 4), q(-1, 4), q(1, 1)),
    ] {
        assert_eq!(route.recipe_samples(), root.samples());
        assert!(AudioRoutedRoot::new(root.clone(), route).is_err());
    }
    let cropped = root.crop(q(1, 4)..q(6, 1)).unwrap();
    assert_eq!(cropped.samples(), root.samples());
    assert!(AudioRoutedRoot::new(cropped, root_route(q(23, 4), q(-1, 4), q(1, 1))).is_err());
    let resumed = root.resume(AudioSample(1), root.extent()).unwrap();
    assert_eq!(resumed.samples(), root.samples());
    assert_eq!(resumed.extent(), root.extent());
    assert!(AudioRoutedRoot::new(resumed, root_route(q(6, 1), q(-1, 4), q(1, 1))).is_err());
    let partial = AudioProjectedRoot::new(
        projection,
        AudioRootPlacement::new(q(0, 1), q(1, 1), q(1, 1)..q(5, 1)).unwrap(),
    )
    .unwrap();
    assert!(AudioRoutedRoot::new(partial, root_route(q(4, 1), q(-1, 1), q(1, 1))).is_err());
    assert!(
        AudioSoundRoute::<AudioSample>::new(
            SoundRoute::identity(q(6, 1)).unwrap(),
            vec![AudioSampleGrid::new(q(-1, 4), q(1, 1), AudioBoundaryRule::PointCeil).unwrap()],
        )
        .is_err()
    );
}

fn insert(extent: i64, at: i64) -> SoundRippleMap {
    SoundRippleMap::new(
        q(i128::from(extent), 1),
        3,
        vec![
            SoundRippleNode::Keep {
                range: ExactFrameRange {
                    start: q(0, 1),
                    end: q(i128::from(at), 1),
                },
            },
            SoundRippleNode::Gap { duration: q(1, 1) },
            SoundRippleNode::Keep {
                range: ExactFrameRange {
                    start: q(i128::from(at), 1),
                    end: q(i128::from(extent), 1),
                },
            },
            SoundRippleNode::Sequence {
                parts: vec![0, 1, 2],
            },
        ],
    )
    .unwrap()
}

#[test]
fn routed_handles_keep_pointceil_and_roundeven_old_labels_distinct() {
    let plan = plan(FrameRate::new(30_000, 1001).unwrap());
    let route = SoundRoute::identity(q(6, 1))
        .unwrap()
        .ripple(insert(6, 1))
        .unwrap()
        .ripple(insert(7, 3))
        .unwrap();
    let point_grid =
        AudioSampleGrid::new(q(0, 1), q(5, 8008), AudioBoundaryRule::PointCeil).unwrap();
    let routed = AudioRoutedSignal::source(
        voice(&plan),
        AudioSoundRoute::<SignalSample>::new(route.clone(), vec![point_grid; 3]).unwrap(),
    )
    .unwrap();
    let start = point_grid.boundary(q(4, 1)).unwrap();
    let query = routed
        .route()
        .query(start..SignalSample(start.0 + 1), Default::default())
        .unwrap();
    let old = query.spans[0]
        .sampling
        .unwrap()
        .local_at(start)
        .unwrap()
        .checked_div(point_grid.frames_per_sample())
        .unwrap();
    assert_eq!(old, q(3203, 1));
    let root_grid =
        AudioSampleGrid::new(q(0, 1), q(5, 8008), AudioBoundaryRule::RoundEven).unwrap();
    let projected = projection(&plan, stages(&plan).remove(0));
    let root = AudioProjectedRoot::new(
        projected,
        AudioRootPlacement::new(q(0, 1), q(1, 1), q(0, 1)..q(6, 1)).unwrap(),
    )
    .unwrap();
    let routed = AudioRoutedRoot::new(
        root,
        AudioSoundRoute::<AudioSample>::new(route, vec![root_grid; 3]).unwrap(),
    )
    .unwrap();
    let start = root_grid.boundary(q(4, 1)).unwrap();
    let query = routed
        .route()
        .query(start..AudioSample(start.0 + 1), Default::default())
        .unwrap();
    let old = query.spans[0]
        .sampling
        .unwrap()
        .local_at(start)
        .unwrap()
        .checked_div(root_grid.frames_per_sample())
        .unwrap();
    assert_eq!(old, q(3204, 1));
}

#[test]
fn occurrence_route_keeps_complete_preserve_graph_and_absolute_ntsc_labels() {
    let plan = plan(FrameRate::new(30_000, 1001).unwrap());
    for ordinal in [0, 1] {
        let occurrence = occurrence(&plan, ordinal);
        let extent = occurrence.extent();
        let length = extent.end.checked_sub(extent.start).unwrap();
        let grid_origin = ExactRatio::ZERO.checked_sub(extent.start).unwrap();
        let grid =
            AudioSampleGrid::new(grid_origin, q(5, 8008), AudioBoundaryRule::RoundEven).unwrap();
        let route =
            AudioSoundRoute::<AudioSample>::new(SoundRoute::identity(length).unwrap(), vec![grid])
                .unwrap();
        let previous = occurrence
            .processing_projection()
            .expect("the owner occurrence is processed by Preserve")
            .clone();
        let retained = AudioRoutedRoot::occurrence(occurrence.clone(), route).unwrap();
        let AudioRoutedRootInput::Occurrence(captured) = retained.input() else {
            panic!("expected retained occurrence");
        };
        assert_eq!(retained.samples(), occurrence.samples());
        assert_eq!(retained.route().recipe_samples(), occurrence.samples());
        assert!(Arc::ptr_eq(
            captured.processing_projection().unwrap(),
            &previous
        ));
        assert!(retained.belongs_to(&plan));
        assert!(!retained.belongs_to(&plan.clone()));
    }
}

#[test]
fn occurrence_route_rejects_wrong_extent_origin_spacing_and_allocation() {
    let plan = plan(FrameRate::new(30_000, 1001).unwrap());
    let occurrence = occurrence(&plan, 1);
    let extent = occurrence.extent();
    let length = extent.end.checked_sub(extent.start).unwrap();
    let origin = ExactRatio::ZERO.checked_sub(extent.start).unwrap();
    let valid_step = q(5, 8008);
    let cases = [
        root_route(length.checked_add(q(1, 4)).unwrap(), origin, valid_step),
        root_route(length, origin.checked_add(q(1, 4)).unwrap(), valid_step),
        root_route(length, origin, q(5, 8007)),
    ];
    for route in cases {
        assert!(AudioRoutedRoot::occurrence(occurrence.clone(), route).is_err());
    }
}

#[test]
fn occurrence_route_normalizes_exact_round_even_half_sample_boundaries() {
    // This rate gives exactly 1601.5 samples per project frame.
    let rate = FrameRate::new(96_000, 3_203).unwrap();
    let plan = plan_with_prefix(rate, 1);
    let step = q(2, 3_203);
    let expected = [
        (0, 1, AudioSample(1_602), AudioSample(11_210)),
        (1, 7, AudioSample(11_210), AudioSample(20_820)),
    ];

    for (ordinal, start_frame, expected_start, expected_end) in expected {
        let occurrence = occurrence(&plan, ordinal);
        let extent = occurrence.extent();
        let length = extent.end.checked_sub(extent.start).unwrap();
        assert_eq!(extent.start, ExactRatio::integer(start_frame));
        assert_eq!(occurrence.samples(), expected_start..expected_end);

        // At frame 1 the half tie rounds up; at frame 7 it rounds down.
        let route_origin = ExactRatio::ZERO.checked_sub(extent.start).unwrap();
        let route = root_route(length, route_origin, step);
        let retained = AudioRoutedRoot::occurrence(occurrence, route).unwrap();
        assert_eq!(retained.route().recipe_grid().frame_origin(), route_origin);
        assert_eq!(
            retained
                .route()
                .recipe_grid()
                .boundary(ExactRatio::ZERO)
                .unwrap(),
            expected_start
        );
        assert_eq!(
            retained.route().recipe_samples(),
            expected_start..expected_end
        );
    }
}

#[test]
fn occurrence_routed_gates_validate_clock_history_and_apply_current_hold_mask() {
    let rate = FrameRate::new(30_000, 1_001).unwrap();
    let original_plan = plan_with_prefix(rate, 0);
    let moved_plan = plan_with_prefix(rate, 1);
    let returned_plan = plan_with_prefix(rate, 0);
    let original = short_occurrence(&original_plan, 0);
    let moved = short_occurrence(&moved_plan, 0);
    let returned = short_occurrence(&returned_plan, 0);
    assert!(original.processing_projection().is_none());
    assert_ne!(original.audible_extent(), original.extent());
    let placements = [original.extent(), moved.extent(), returned.extent()];
    let result = returned
        .routed_gate_fades(
            &original,
            &placements,
            AudioEdgePolicy::Automatic,
            AudioEdgePolicy::Automatic,
            returned.samples(),
            Default::default(),
        )
        .unwrap();
    assert_eq!(
        result.spans.first().unwrap().samples.start,
        returned.samples().start
    );
    assert_eq!(
        result.spans.last().unwrap().samples.end,
        returned.samples().end
    );
    // The live picture branch contains a silent Hold through the Retime.
    // Historical phase transport never grants permission through that gate.
    assert!(result.spans.iter().all(|span| span.length == 0));

    let mut wrong_extent = placements.to_vec();
    wrong_extent[1] = q(1, 1)..q(8, 1);
    assert!(
        returned
            .routed_gate_fades(
                &original,
                &wrong_extent,
                AudioEdgePolicy::Automatic,
                AudioEdgePolicy::Automatic,
                returned.samples(),
                Default::default(),
            )
            .is_err()
    );
}
