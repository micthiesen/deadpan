use std::collections::BTreeMap;
use std::ops::Range;

use deadpan_core::*;
use deadpan_plan::{
    AudioContent, AudioQueryLimits, AudioSignalContent, PlanError, RenderPlan, SignalSample,
    SilenceReason,
};

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn ratio(numerator: i128, denominator: i128) -> ExactRatio {
    ExactRatio::new(numerator, denominator).unwrap()
}

fn duration(frames: i64) -> FrameDuration {
    FrameDuration::new(frames).unwrap()
}

fn points(start: i64, end: i64) -> Range<SignalSample> {
    SignalSample(start)..SignalSample(end)
}

fn samples(start: i64, end: i64) -> Range<AudioSample> {
    AudioSample(start)..AudioSample(end)
}

fn audio(start: i64, end: i64) -> SourceAudio {
    let clock = SourceTimeBase::new(1, 48_000).unwrap();
    SourceAudio {
        asset: AssetId::new("media").unwrap(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: start,
                time_base: clock,
            },
            SourceTimestamp {
                ticks: end,
                time_base: clock,
            },
        )
        .unwrap(),
    }
}

fn source(frames: i64, start: i64, end: i64, mapping: SourceAudioMapping) -> BeatNode {
    BeatNode {
        label: "Source".into(),
        kind: NodeKind::Source {
            source: SourceNode {
                duration: duration(frames),
                video: SourceVideo::Blank,
                video_mapping: SourceVideoMapping::FitBeat,
                audio: Some(audio(start, end)),
                audio_mapping: mapping,
                audio_offset: AudioSample(0),
                link: LinkRelation::Independent,
            },
        },
    }
}

fn hold_recipe(frames: i64, audio: HoldAudio) -> HoldRecipe {
    HoldRecipe {
        duration: duration(frames),
        video: HoldVideo::Background,
        audio,
    }
}

fn hold(frames: i64) -> BeatNode {
    BeatNode::hold("Silence", hold_recipe(frames, HoldAudio::Silence))
}

fn retime(child: &str, frames: i64, start: i64, end: i64, pitch: PitchPolicy) -> BeatNode {
    BeatNode {
        label: "Retime".into(),
        kind: NodeKind::Retime {
            child: id(child),
            duration: duration(frames),
            mapping: FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap(),
            pitch,
        },
    }
}

fn iteration(ordinal: u32) -> IterationId {
    IterationId {
        allocation: RevisionId::new("plays").unwrap(),
        ordinal,
    }
}

fn repeat(child: &str, plays: u32, gap: i64) -> BeatNode {
    BeatNode {
        label: "Repeat".into(),
        kind: NodeKind::Repeat {
            child: id(child),
            iterations: IterationOrder::new(RevisionId::new("plays").unwrap(), plays).unwrap(),
            gap: (gap > 0).then(|| hold_recipe(gap, HoldAudio::Silence)),
        },
    }
}

fn document(
    rate: FrameRate,
    children: &[&str],
    nodes: impl IntoIterator<Item = (&'static str, BeatNode)>,
    overrides: BTreeMap<NodeId, PlayOverrides>,
) -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("project").unwrap(),
        RevisionId::new("revision").unwrap(),
        PresentationBasis {
            width: 1920,
            height: 1080,
            frame_rate: rate,
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut nodes: BTreeMap<_, _> = nodes
        .into_iter()
        .map(|(name, node)| (id(name), node))
        .collect();
    nodes.insert(
        id("root"),
        BeatNode::sequence("Root", children.iter().map(|name| id(name)).collect()),
    );
    let mut wire = serde_json::to_value(empty).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["overrides"] = serde_json::to_value(overrides).unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        AssetId::new("media").unwrap(),
        AssetRecord {
            label: "Original".into(),
            content_hash: "a".repeat(64),
            video: None,
            audio: Some(audio(-1_000_000, 1_000_000).span),
            still_image: true,
            frame_count: None,
            source_qualification: None,
        },
    )]))
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn rate() -> FrameRate {
    FrameRate::new(48_000, 1).unwrap()
}

#[test]
fn point_grid_keeps_every_integer_position_in_fractional_source_support() {
    let placed = || {
        source(
            4,
            0,
            13,
            SourceAudioMapping::Placement {
                start: ratio(3, 5),
                frames: ratio(13, 5),
            },
        )
    };
    let plain = RenderPlan::compile(&document(
        rate(),
        &["source"],
        [("source", placed())],
        BTreeMap::new(),
    ))
    .unwrap();
    let root = plain
        .audio_processing(samples(0, 4), AudioQueryLimits::default())
        .unwrap();
    assert_eq!(root.spans.len(), 3);
    assert_eq!(root.spans[1].allocated_samples, samples(1, 3));
    let wrapped = RenderPlan::compile(&document(
        rate(),
        &["preserve"],
        [
            ("source", placed()),
            ("preserve", retime("source", 8, 0, 4, PitchPolicy::Preserve)),
        ],
        BTreeMap::new(),
    ))
    .unwrap();
    let root = wrapped
        .audio_processing(samples(0, 8), AudioQueryLimits::default())
        .unwrap();
    assert_eq!(root.spans.len(), 1);
    let AudioSignalContent::Stage(stage) = &root.spans[0].content else {
        panic!("expected Preserve stage")
    };
    let input = stage.input_signal();
    assert_eq!(input.sample_count().unwrap(), SignalSample(4));
    let query = input
        .query(points(0, 4), AudioQueryLimits::default())
        .unwrap();
    assert_eq!(query.spans.len(), 2);
    assert_eq!(query.spans[0].allocated_samples, points(0, 1));
    let source = &query.spans[1];
    // [.6,3.2) contains integer positions 1,2,3. Rounding the end to 3
    // would discard a required point of the stretcher's input signal.
    assert_eq!(source.allocated_samples, points(1, 4));
    assert_eq!(source.signal_extent, ratio(3, 5)..ratio(16, 5));
    assert_eq!(
        source.source_point(SignalSample(1)).unwrap().ticks,
        ExactRatio::integer(2)
    );
    assert_eq!(
        source.source_point(SignalSample(3)).unwrap().ticks,
        ExactRatio::integer(12)
    );
    assert_eq!(
        source
            .source_point_at_signal_frame(ratio(16, 5))
            .unwrap()
            .ticks,
        ExactRatio::integer(13)
    );
}

#[test]
fn virtual_input_retains_four_points_in_three_point_two_sample_extent() {
    let rate = FrameRate::new(15_000, 1).unwrap();
    let plan = RenderPlan::compile(&document(
        rate,
        &["preserve"],
        [
            ("source", source(1, 0, 16, SourceAudioMapping::FitBeat)),
            ("preserve", retime("source", 2, 0, 1, PitchPolicy::Preserve)),
        ],
        BTreeMap::new(),
    ))
    .unwrap();
    assert_eq!(plan.audio_duration().unwrap(), AudioSample(6));
    let query = plan
        .audio_processing(samples(0, 6), AudioQueryLimits::default())
        .unwrap();
    let AudioSignalContent::Stage(stage) = &query.spans[0].content else {
        panic!("expected stage")
    };
    let input = stage.input_signal();
    assert_eq!(input.support(), ExactRatio::ZERO..ExactRatio::ONE);
    assert_eq!(input.sample_count().unwrap(), SignalSample(4));
    let input = input
        .query(points(0, 4), AudioQueryLimits::default())
        .unwrap();
    assert_eq!(input.spans.len(), 1);
    assert_eq!(input.spans[0].allocated_samples, points(0, 4));
    assert_eq!(
        input.spans[0].transform.signal_at(SignalSample(3)).unwrap(),
        ratio(15, 16)
    );
    assert_eq!(
        input.spans[0].source_point(SignalSample(3)).unwrap().ticks,
        ExactRatio::integer(15)
    );
    assert!(
        stage
            .input_signal()
            .query(points(4, 5), AudioQueryLimits::default())
            .is_err()
    );
}

#[test]
fn root_half_sample_rounding_and_virtual_point_selection_use_different_probes() {
    let rate = FrameRate::new(96_000, 1).unwrap();
    let names = ["a", "b", "c", "d", "e", "f", "g", "h"];
    let doc = document(
        rate,
        &names,
        names.into_iter().map(|name| (name, hold(1))),
        BTreeMap::new(),
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    let root = plan
        .audio_processing(samples(0, 4), AudioQueryLimits::default())
        .unwrap();
    let points = plan
        .audio_signal()
        .query(points(0, 4), AudioQueryLimits::default())
        .unwrap();
    assert_eq!(
        root.spans
            .iter()
            .map(|span| span.instance.node.clone())
            .collect::<Vec<_>>(),
        ["b", "c", "f", "g"].map(id)
    );
    assert_eq!(
        points
            .spans
            .iter()
            .map(|span| span.instance.node.clone())
            .collect::<Vec<_>>(),
        ["a", "c", "e", "g"].map(id)
    );
    for (index, span) in points.spans.iter().enumerate() {
        assert_eq!(
            span.allocated_samples,
            self::points(index as i64, index as i64 + 1)
        );
    }
}

#[test]
fn selected_child_grid_keeps_fractional_origin_and_signed_source_placement() {
    let rate = FrameRate::new(15_000, 1).unwrap();
    let mut placed = source(
        5,
        100,
        200,
        SourceAudioMapping::Placement {
            start: ratio(1, 2),
            frames: ratio(125, 4),
        },
    );
    if let NodeKind::Source { source } = &mut placed.kind {
        source.audio_offset = AudioSample(-1);
    }
    let plan = RenderPlan::compile(&document(
        rate,
        &["lead", "preserve"],
        [
            ("lead", hold(2)),
            ("source", placed),
            ("preserve", retime("source", 2, 1, 4, PitchPolicy::Preserve)),
        ],
        BTreeMap::new(),
    ))
    .unwrap();
    let query = plan
        .audio_processing(samples(6, 13), AudioQueryLimits::default())
        .unwrap();
    assert_eq!(query.spans.len(), 1);
    let span = &query.spans[0];
    assert_eq!(
        span.transform.local_at(AudioSample(6)).unwrap(),
        ratio(-1, 8)
    );
    let AudioSignalContent::Stage(stage) = &span.content else {
        panic!("expected stage")
    };
    let descriptor = stage.descriptor();
    assert_eq!(descriptor.project_id, ProjectId::new("project").unwrap());
    assert_eq!(descriptor.revision_id, RevisionId::new("revision").unwrap());
    assert_eq!(descriptor.child, id("source"));
    assert_eq!(
        descriptor.selection,
        ExactRatio::integer(1)..ExactRatio::integer(4)
    );
    assert_eq!(descriptor.duration, duration(2));
    assert_eq!(descriptor.rate, ratio(3, 2));
    let input = stage.input_signal();
    assert_eq!(input.sample_count().unwrap(), SignalSample(10));
    let input = input
        .query(points(0, 10), AudioQueryLimits::default())
        .unwrap();
    let span = &input.spans[0];
    assert_eq!(span.transform.grid_origin, ExactRatio::integer(1));
    assert_eq!(
        span.source_point(SignalSample(0)).unwrap().ticks,
        ratio(513, 5)
    );
    assert_eq!(
        span.source_point(SignalSample(9)).unwrap().ticks,
        ratio(558, 5)
    );
    assert_eq!(
        span.source_point_at_signal_frame(ExactRatio::integer(4))
            .unwrap()
            .ticks,
        ratio(561, 5)
    );
}

#[test]
fn preserve_over_sequence_is_one_stage_with_continuous_input_across_cuts() {
    let plan = RenderPlan::compile(&document(
        rate(),
        &["preserve"],
        [
            ("a", source(4, 10, 14, SourceAudioMapping::FitBeat)),
            ("b", source(6, 50, 56, SourceAudioMapping::FitBeat)),
            (
                "sequence",
                BeatNode::sequence("Cuts", vec![id("a"), id("b")]),
            ),
            (
                "preserve",
                retime("sequence", 5, 0, 10, PitchPolicy::Preserve),
            ),
        ],
        BTreeMap::new(),
    ))
    .unwrap();
    let root = plan
        .audio_processing(samples(0, 5), AudioQueryLimits::default())
        .unwrap();
    assert_eq!(root.spans.len(), 1);
    let AudioSignalContent::Stage(stage) = &root.spans[0].content else {
        panic!("expected stage")
    };
    assert_eq!(stage.descriptor().instance.node, id("preserve"));
    assert_eq!(stage.descriptor().rate, ExactRatio::integer(2));
    let input = stage.input_signal();
    assert_eq!(input.sample_count().unwrap(), SignalSample(10));
    let query = input
        .query(points(0, 10), AudioQueryLimits::default())
        .unwrap();
    assert_eq!(query.spans.len(), 2);
    assert_eq!(query.spans[0].allocated_samples, points(0, 4));
    assert_eq!(query.spans[1].allocated_samples, points(4, 10));
    assert_eq!(
        query.spans[0].source_point(SignalSample(3)).unwrap().ticks,
        ExactRatio::integer(13)
    );
    assert_eq!(
        query.spans[1].source_point(SignalSample(4)).unwrap().ticks,
        ExactRatio::integer(50)
    );
    assert_eq!(
        query.spans[1].source_point(SignalSample(9)).unwrap().ticks,
        ExactRatio::integer(55)
    );
}

#[test]
fn outer_crop_does_not_shorten_an_inner_preserve_stages_canonical_history() {
    let nodes = || {
        [
            ("a", source(4, 10, 14, SourceAudioMapping::FitBeat)),
            ("b", source(6, 50, 56, SourceAudioMapping::FitBeat)),
            (
                "sequence",
                BeatNode::sequence("Cuts", vec![id("a"), id("b")]),
            ),
            (
                "inner",
                retime("sequence", 20, 0, 10, PitchPolicy::Preserve),
            ),
        ]
    };
    let full =
        RenderPlan::compile(&document(rate(), &["inner"], nodes(), BTreeMap::new())).unwrap();
    let mut cropped_nodes = nodes().to_vec();
    cropped_nodes.push((
        "outer",
        retime("inner", 4, 10, 18, PitchPolicy::FollowSpeed),
    ));
    let cropped = RenderPlan::compile(&document(
        rate(),
        &["outer"],
        cropped_nodes,
        BTreeMap::new(),
    ))
    .unwrap();
    let full = full
        .audio_processing(samples(0, 20), AudioQueryLimits::default())
        .unwrap();
    let crop = cropped
        .audio_processing(samples(2, 4), AudioQueryLimits::default())
        .unwrap();
    let AudioSignalContent::Stage(full_stage) = &full.spans[0].content else {
        panic!("expected full stage")
    };
    let AudioSignalContent::Stage(cropped_stage) = &crop.spans[0].content else {
        panic!("expected inner stage")
    };
    assert_eq!(full_stage.descriptor(), cropped_stage.descriptor());
    assert_eq!(
        cropped_stage.descriptor().selection,
        ExactRatio::ZERO..ExactRatio::integer(10)
    );
    assert_eq!(cropped_stage.descriptor().duration, duration(20));
    assert_eq!(
        crop.spans[0].transform.local_at(AudioSample(2)).unwrap(),
        ExactRatio::integer(14)
    );
    assert_eq!(crop.spans[0].retimes[0].node, id("outer"));
    assert_eq!(
        cropped_stage.input_signal().sample_count().unwrap(),
        SignalSample(10)
    );
    let history = cropped_stage
        .input_signal()
        .query(points(0, 4), AudioQueryLimits::default())
        .unwrap();
    assert_eq!(history.spans[0].instance.node, id("a"));
    assert_eq!(
        history.spans[0]
            .source_point(SignalSample(0))
            .unwrap()
            .ticks,
        ExactRatio::integer(10)
    );
}

#[test]
fn mixed_pitch_policies_keep_their_order_even_when_net_speed_is_unity() {
    let follow_then_preserve = RenderPlan::compile(&document(
        rate(),
        &["outer"],
        [
            ("source", source(16, 0, 16, SourceAudioMapping::FitBeat)),
            (
                "inner",
                retime("source", 8, 0, 16, PitchPolicy::FollowSpeed),
            ),
            ("outer", retime("inner", 16, 0, 8, PitchPolicy::Preserve)),
        ],
        BTreeMap::new(),
    ))
    .unwrap();
    let preserve_then_follow = RenderPlan::compile(&document(
        rate(),
        &["outer"],
        [
            ("source", source(16, 0, 16, SourceAudioMapping::FitBeat)),
            ("inner", retime("source", 8, 0, 16, PitchPolicy::Preserve)),
            ("outer", retime("inner", 16, 0, 8, PitchPolicy::FollowSpeed)),
        ],
        BTreeMap::new(),
    ))
    .unwrap();
    let first = follow_then_preserve
        .audio_processing(samples(0, 16), AudioQueryLimits::default())
        .unwrap();
    let second = preserve_then_follow
        .audio_processing(samples(0, 16), AudioQueryLimits::default())
        .unwrap();
    let AudioSignalContent::Stage(first_stage) = &first.spans[0].content else {
        panic!("expected outer Preserve")
    };
    let AudioSignalContent::Stage(second_stage) = &second.spans[0].content else {
        panic!("expected inner Preserve")
    };
    assert_eq!(first_stage.descriptor().instance.node, id("outer"));
    assert_eq!(first_stage.descriptor().rate, ratio(1, 2));
    assert!(first.spans[0].retimes.is_empty());
    let first_input = first_stage
        .input_signal()
        .query(points(0, 8), AudioQueryLimits::default())
        .unwrap();
    assert_eq!(
        first_input.spans[0].retimes[0].pitch,
        PitchPolicy::FollowSpeed
    );
    assert_eq!(
        first_input.spans[0]
            .source_point(SignalSample(2))
            .unwrap()
            .ticks,
        ExactRatio::integer(4)
    );
    assert_eq!(second_stage.descriptor().instance.node, id("inner"));
    assert_eq!(second_stage.descriptor().rate, ExactRatio::integer(2));
    assert_eq!(second.spans[0].retimes[0].pitch, PitchPolicy::FollowSpeed);
    assert_eq!(
        second.spans[0].transform.local_at(AudioSample(2)).unwrap(),
        ExactRatio::ONE
    );
    let second_input = second_stage
        .input_signal()
        .query(points(0, 16), AudioQueryLimits::default())
        .unwrap();
    assert!(second_input.spans[0].retimes.is_empty());
    assert_eq!(
        second_input.spans[0]
            .source_point(SignalSample(2))
            .unwrap()
            .ticks,
        ExactRatio::integer(2)
    );
}

#[test]
fn repeated_stages_and_sparse_overrides_retain_complete_occurrence_identity() {
    let overrides = BTreeMap::from([(
        id("repeat"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: iteration(1),
            root: id("alternate"),
        }])
        .unwrap(),
    )]);
    let doc = document(
        rate(),
        &["repeat"],
        [
            ("source", source(4, 0, 4, SourceAudioMapping::FitBeat)),
            ("normal", retime("source", 2, 0, 4, PitchPolicy::Preserve)),
            (
                "alternate-source",
                source(6, 100, 106, SourceAudioMapping::FitBeat),
            ),
            (
                "alternate",
                retime("alternate-source", 3, 0, 6, PitchPolicy::Preserve),
            ),
            ("repeat", repeat("normal", 3, 1)),
        ],
        overrides,
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    let query = plan
        .audio_processing(samples(0, 9), AudioQueryLimits::default())
        .unwrap();
    assert_eq!(query.spans.len(), 5);
    for (index, name, ordinal, child, start, end) in [
        (0, "normal", 0, "source", 0, 2),
        (2, "alternate", 1, "alternate-source", 3, 6),
        (4, "normal", 2, "source", 7, 9),
    ] {
        let span = &query.spans[index];
        assert_eq!(span.allocated_samples, samples(start, end));
        span.instance.validate(&doc).unwrap();
        let AudioSignalContent::Stage(stage) = &span.content else {
            panic!("expected stage")
        };
        let descriptor = stage.descriptor();
        assert_eq!(descriptor.instance.node, id(name));
        assert_eq!(descriptor.child, id(child));
        assert_eq!(
            descriptor.instance.repeats,
            [RepeatInstance {
                node: id("repeat"),
                iteration: iteration(ordinal)
            }]
        );
        let child = stage
            .input_signal()
            .query(points(0, 1), AudioQueryLimits::default())
            .unwrap();
        assert_eq!(child.spans[0].instance.repeats, descriptor.instance.repeats);
        child.spans[0].instance.validate(&doc).unwrap();
    }
    for (index, ordinal) in [(1, 0), (3, 1)] {
        assert_eq!(query.spans[index].instance.node, id("repeat"));
        assert_eq!(query.spans[index].gap_after, Some(iteration(ordinal)));
        assert!(query.spans[index].instance.repeats.is_empty());
    }
}

#[test]
fn compact_billion_play_stage_seek_obeys_structural_work_limits() {
    let overrides = BTreeMap::from([(
        id("repeat"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: iteration(500_000_000),
            root: id("alternate"),
        }])
        .unwrap(),
    )]);
    let plan = RenderPlan::compile(&document(
        rate(),
        &["repeat"],
        [
            ("source", source(2, 0, 2, SourceAudioMapping::FitBeat)),
            ("normal", retime("source", 1, 0, 2, PitchPolicy::Preserve)),
            (
                "alternate-source",
                source(6, 100, 106, SourceAudioMapping::FitBeat),
            ),
            (
                "alternate",
                retime("alternate-source", 3, 0, 6, PitchPolicy::Preserve),
            ),
            ("repeat", repeat("normal", 1_000_000_000, 1)),
        ],
        overrides,
    ))
    .unwrap();
    assert_eq!(plan.audio_duration().unwrap(), AudioSample(2_000_000_001));
    assert_eq!(plan.metadata().storage.repeat_segment_entries, 3);
    for (start, end, name, ordinal) in [
        (1_000_000_000, 1_000_000_003, "alternate", 500_000_000),
        (2_000_000_000, 2_000_000_001, "normal", 999_999_999),
    ] {
        let query = plan
            .audio_processing(
                samples(start, end),
                AudioQueryLimits {
                    maximum_spans: 1,
                    maximum_work: 12,
                },
            )
            .unwrap();
        assert_eq!(query.spans.len(), 1);
        let AudioSignalContent::Stage(stage) = &query.spans[0].content else {
            panic!("expected stage")
        };
        assert_eq!(stage.descriptor().instance.node, id(name));
        assert_eq!(
            stage.descriptor().instance.repeats[0].iteration,
            iteration(ordinal)
        );
        assert!(query.lookup.visited_nodes <= 3);
        assert!(query.lookup.iteration_run_comparisons <= 3);
        assert!(matches!(
            plan.audio_processing(
                samples(start, end),
                AudioQueryLimits {
                    maximum_spans: 1,
                    maximum_work: 1
                }
            ),
            Err(PlanError::AudioQueryLimit("structural work"))
        ));
    }
}

#[test]
fn point_and_root_queries_are_stable_under_paging_and_out_of_order_seeks() {
    let plan = RenderPlan::compile(&document(
        rate(),
        &["lead", "outer"],
        [
            ("lead", hold(1)),
            ("a", source(2, 10, 12, SourceAudioMapping::FitBeat)),
            ("b", source(6, 50, 56, SourceAudioMapping::FitBeat)),
            ("preserve", retime("b", 3, 0, 6, PitchPolicy::Preserve)),
            ("tail", hold(1)),
            (
                "sequence",
                BeatNode::sequence("Mixed", vec![id("a"), id("preserve"), id("tail")]),
            ),
            (
                "outer",
                retime("sequence", 10, 0, 6, PitchPolicy::FollowSpeed),
            ),
        ],
        BTreeMap::new(),
    ))
    .unwrap();
    let signal = plan.audio_signal();
    let whole = signal
        .query(points(0, 11), AudioQueryLimits::default())
        .unwrap();
    for at in [10, 0, 4, 5, 9, 1, 7] {
        let mut expected = whole
            .spans
            .iter()
            .find(|span| span.samples.contains(&SignalSample(at)))
            .unwrap()
            .clone();
        expected.samples = points(at, at + 1);
        let query = signal
            .query(points(at, at + 1), AudioQueryLimits::default())
            .unwrap();
        assert_eq!(query.spans, [expected]);
    }
    let whole = plan
        .audio_processing(samples(0, 11), AudioQueryLimits::default())
        .unwrap();
    for (start, end) in [(0, 2), (2, 7), (7, 11)] {
        let page = plan
            .audio_processing(samples(start, end), AudioQueryLimits::default())
            .unwrap();
        for span in page.spans {
            let mut expected = whole
                .spans
                .iter()
                .find(|whole| whole.samples.contains(&span.samples.start))
                .unwrap()
                .clone();
            expected.samples = span.samples.clone();
            assert_eq!(span, expected);
        }
    }
}

#[test]
fn hold_policies_and_repeat_gap_metadata_remain_explicit_in_signal_queries() {
    let mut absent = source(1, 0, 1, SourceAudioMapping::FitBeat);
    if let NodeKind::Source { source } = &mut absent.kind {
        source.audio = None;
        source.video = SourceVideo::Still {
            asset: AssetId::new("media").unwrap(),
        };
    }
    let mut repeated = repeat("source", 2, 1);
    if let NodeKind::Repeat { gap, .. } = &mut repeated.kind {
        *gap = Some(hold_recipe(
            1,
            HoldAudio::RoomTone {
                source: audio(100, 200),
            },
        ));
    }
    let plan = RenderPlan::compile(&document(
        rate(),
        &["absent", "silence", "room", "tail", "repeat"],
        [
            ("absent", absent),
            ("silence", hold(2)),
            (
                "room",
                BeatNode::hold(
                    "Room",
                    hold_recipe(
                        3,
                        HoldAudio::RoomTone {
                            source: audio(100, 200),
                        },
                    ),
                ),
            ),
            (
                "tail",
                BeatNode::hold(
                    "Tail",
                    hold_recipe(
                        4,
                        HoldAudio::Tail {
                            source: audio(200, 400),
                            maximum: duration(2),
                        },
                    ),
                ),
            ),
            ("source", source(2, 500, 502, SourceAudioMapping::FitBeat)),
            ("repeat", repeated),
        ],
        BTreeMap::new(),
    ))
    .unwrap();
    let query = plan
        .audio_signal()
        .query(points(0, 15), AudioQueryLimits::default())
        .unwrap();
    assert_eq!(query.spans.len(), 7);
    assert_eq!(
        query.spans[0].content,
        AudioSignalContent::Leaf(AudioContent::Silence {
            reason: SilenceReason::NoSourceAudio
        })
    );
    assert_eq!(
        query.spans[1].content,
        AudioSignalContent::Leaf(AudioContent::Silence {
            reason: SilenceReason::SilentHold
        })
    );
    assert_eq!(
        query.spans[2].content,
        AudioSignalContent::Leaf(AudioContent::RoomTone {
            source: audio(100, 200),
            duration: duration(3)
        })
    );
    assert_eq!(
        query.spans[3].content,
        AudioSignalContent::Leaf(AudioContent::Tail {
            source: audio(200, 400),
            maximum: duration(2)
        })
    );
    assert_eq!(
        query.spans[5].content,
        AudioSignalContent::Leaf(AudioContent::RoomTone {
            source: audio(100, 200),
            duration: duration(1)
        })
    );
    assert_eq!(query.spans[5].allocated_samples, points(12, 13));
    assert_eq!(query.spans[5].gap_after, Some(iteration(0)));
    assert_eq!(query.spans[5].instance.node, id("repeat"));
    assert_eq!(query.spans[6].allocated_samples, points(13, 15));
    assert_eq!(query.spans[6].instance.repeats[0].iteration, iteration(1));
    assert!(query.spans[6].gap_after.is_none());
    assert!(matches!(
        query.spans[2].source_point(SignalSample(3)),
        Err(PlanError::NoSourceAudio)
    ));
}

#[test]
fn room_tone_retains_intrinsic_duration_through_hold_and_repeat_gap_crops() {
    let mut repeated = repeat("silence", 3, 5);
    if let NodeKind::Repeat { gap, .. } = &mut repeated.kind {
        *gap = Some(hold_recipe(
            5,
            HoldAudio::RoomTone {
                source: audio(100, 200),
            },
        ));
    }
    let plan = RenderPlan::compile(&document(
        rate(),
        &["cropped-room", "cropped-repeat"],
        [
            (
                "room",
                BeatNode::hold(
                    "Room",
                    hold_recipe(
                        12,
                        HoldAudio::RoomTone {
                            source: audio(100, 200),
                        },
                    ),
                ),
            ),
            (
                "cropped-room",
                retime("room", 3, 4, 7, PitchPolicy::FollowSpeed),
            ),
            ("silence", hold(2)),
            ("repeat", repeated),
            (
                "cropped-repeat",
                retime("repeat", 9, 3, 12, PitchPolicy::FollowSpeed),
            ),
        ],
        BTreeMap::new(),
    ))
    .unwrap();
    // The root owns only three frames of the twelve-frame Hold. Its Repeat
    // selection starts inside gap 0 and ends inside gap 1. Query paging then
    // removes one more frame from either side without changing any identity.
    let root = plan
        .audio(samples(1, 11), AudioQueryLimits::default())
        .unwrap();
    let processing = plan
        .audio_processing(samples(1, 11), AudioQueryLimits::default())
        .unwrap();
    let signal = plan
        .audio_signal()
        .query(points(1, 11), AudioQueryLimits::default())
        .unwrap();
    assert_eq!(root.spans.len(), 4);
    assert_eq!(processing.spans.len(), 4);
    assert_eq!(signal.spans.len(), 4);
    for (index, intrinsic_duration, expected_node, gap, allocated, local_start) in [
        (0, 12, "room", None, 0..3, 5),
        (1, 5, "repeat", Some(iteration(0)), 3..7, 1),
        (3, 5, "repeat", Some(iteration(1)), 9..12, 0),
    ] {
        let expected = AudioContent::RoomTone {
            source: audio(100, 200),
            duration: duration(intrinsic_duration),
        };
        let root = &root.spans[index];
        let processing = &processing.spans[index];
        let signal = &signal.spans[index];
        assert_eq!(root.content, expected);
        assert_eq!(
            processing.content,
            AudioSignalContent::Leaf(expected.clone())
        );
        assert_eq!(signal.content, AudioSignalContent::Leaf(expected));
        assert_eq!(
            root.allocated_samples,
            samples(allocated.start, allocated.end)
        );
        assert_eq!(processing.allocated_samples, root.allocated_samples);
        assert_eq!(
            signal.allocated_samples,
            points(allocated.start, allocated.end)
        );
        assert_eq!(root.instance.node, id(expected_node));
        assert_eq!(processing.instance, root.instance);
        assert_eq!(signal.instance, root.instance);
        assert_eq!(root.gap_after, gap);
        assert_eq!(processing.gap_after, gap);
        assert_eq!(signal.gap_after, gap);
        assert_eq!(
            root.transform.local_at(root.samples.start).unwrap(),
            ratio(local_start, 1)
        );
        assert_eq!(
            signal.transform.local_at(signal.samples.start).unwrap(),
            ratio(local_start, 1)
        );
    }
}

#[test]
fn empty_invalid_and_budgeted_signal_queries_fail_without_partial_results() {
    let plan = RenderPlan::compile(&document(
        rate(),
        &["a", "b"],
        [("a", hold(1)), ("b", hold(1))],
        BTreeMap::new(),
    ))
    .unwrap();
    let signal = plan.audio_signal();
    for range in [points(-1, 0), points(2, 1), points(0, 3), points(3, 3)] {
        assert!(matches!(
            signal.query(range, AudioQueryLimits::default()),
            Err(PlanError::AudioRangeOutOfRange)
        ));
    }
    for limits in [
        AudioQueryLimits {
            maximum_spans: 0,
            maximum_work: 1,
        },
        AudioQueryLimits {
            maximum_spans: 1,
            maximum_work: 0,
        },
        AudioQueryLimits {
            maximum_spans: 4097,
            maximum_work: 1,
        },
        AudioQueryLimits {
            maximum_spans: 1,
            maximum_work: 65_537,
        },
    ] {
        assert!(matches!(
            signal.query(points(0, 1), limits),
            Err(PlanError::InvalidAudioLimits)
        ));
    }
    assert!(matches!(
        signal.query(
            points(0, 2),
            AudioQueryLimits {
                maximum_spans: 1,
                maximum_work: 100
            }
        ),
        Err(PlanError::AudioQueryLimit("span count"))
    ));
    let empty = signal
        .query(points(2, 2), AudioQueryLimits::default())
        .unwrap();
    assert!(empty.spans.is_empty());
    assert_eq!(empty.lookup.visited_nodes, 0);
}

#[test]
fn stage_output_policy_grid_retains_silent_holds_with_no_input_grid_sample() {
    let rate = FrameRate::new(96_000, 1).unwrap();
    let plan = RenderPlan::compile(&document(
        rate,
        &["repeat"],
        [
            ("a", source(1, 10, 11, SourceAudioMapping::FitBeat)),
            ("silent", hold(1)),
            ("b", source(1, 20, 21, SourceAudioMapping::FitBeat)),
            (
                "sequence",
                BeatNode::sequence("Short hold", vec![id("a"), id("silent"), id("b")]),
            ),
            (
                "preserve",
                retime("sequence", 6, 0, 3, PitchPolicy::Preserve),
            ),
            ("repeat", repeat("preserve", 2, 0)),
        ],
        BTreeMap::new(),
    ))
    .unwrap();
    let root = plan
        .audio_processing(samples(3, 6), AudioQueryLimits::default())
        .unwrap();
    let AudioSignalContent::Stage(stage) = &root.spans[0].content else {
        panic!("expected stage")
    };
    assert_eq!(
        stage.descriptor().instance.repeats,
        [RepeatInstance {
            node: id("repeat"),
            iteration: iteration(1),
        }]
    );
    let input = stage
        .input_signal()
        .query(points(0, 2), AudioQueryLimits::default())
        .unwrap();
    // The input Hold occupies [.5,1) sample positions, so this particular
    // canonical input grid has no point inside it.
    assert_eq!(input.spans.len(), 2);
    assert_eq!(input.spans[0].instance.node, id("a"));
    assert_eq!(input.spans[1].instance.node, id("b"));
    assert!(
        input
            .spans
            .iter()
            .all(|span| { span.instance.repeats == stage.descriptor().instance.repeats })
    );
    let output = stage.output_signal();
    assert_eq!(output.support(), ExactRatio::ZERO..ExactRatio::integer(6));
    assert_eq!(output.sample_count().unwrap(), SignalSample(3));
    let opaque = output
        .query(points(0, 3), AudioQueryLimits::default())
        .unwrap();
    assert_eq!(opaque.spans.len(), 1);
    assert!(matches!(
        opaque.spans[0].content,
        AudioSignalContent::Stage(_)
    ));
    let policy = output
        .query_flattened(points(0, 3), AudioQueryLimits::default())
        .unwrap();
    assert_eq!(policy.spans.len(), 3);
    assert_eq!(policy.spans[1].allocated_samples, points(1, 2));
    assert_eq!(policy.spans[1].instance.node, id("silent"));
    assert_eq!(
        policy.spans[1].instance.repeats,
        stage.descriptor().instance.repeats
    );
    assert_eq!(
        policy.spans[1].content,
        AudioSignalContent::Leaf(AudioContent::Silence {
            reason: SilenceReason::SilentHold,
        })
    );
    assert_eq!(policy.spans[1].retimes[0].node, id("preserve"));
    assert_eq!(policy.spans[1].retimes[0].pitch, PitchPolicy::Preserve);
    assert_eq!(
        output
            .query_flattened(points(1, 2), AudioQueryLimits::default())
            .unwrap()
            .spans,
        [policy.spans[1].clone()]
    );
}

#[test]
fn nested_preserve_stages_remain_distinct_and_unity_preserve_is_transparent() {
    let plan = RenderPlan::compile(&document(
        rate(),
        &["outer"],
        [
            ("source", source(6, 100, 106, SourceAudioMapping::FitBeat)),
            ("inner", retime("source", 3, 0, 6, PitchPolicy::Preserve)),
            ("outer", retime("inner", 6, 0, 3, PitchPolicy::Preserve)),
        ],
        BTreeMap::new(),
    ))
    .unwrap();
    let root = plan
        .audio_processing(samples(0, 6), AudioQueryLimits::default())
        .unwrap();
    let AudioSignalContent::Stage(outer) = &root.spans[0].content else {
        panic!("expected outer stage")
    };
    let input = outer
        .input_signal()
        .query(points(0, 3), AudioQueryLimits::default())
        .unwrap();
    let AudioSignalContent::Stage(inner) = &input.spans[0].content else {
        panic!("expected independent inner stage")
    };
    assert_eq!(outer.descriptor().rate, ratio(1, 2));
    assert_eq!(inner.descriptor().rate, ExactRatio::integer(2));
    assert_eq!(inner.descriptor().duration, duration(3));
    assert_eq!(
        inner.input_signal().sample_count().unwrap(),
        SignalSample(6)
    );
    let flattened = outer
        .output_signal()
        .query_flattened(points(0, 6), AudioQueryLimits::default())
        .unwrap();
    assert_eq!(flattened.spans[0].retimes.len(), 2);
    assert_eq!(flattened.spans[0].retimes[0].node, id("outer"));
    assert_eq!(flattened.spans[0].retimes[1].node, id("inner"));
    assert_eq!(
        flattened.spans[0]
            .source_point(SignalSample(2))
            .unwrap()
            .ticks,
        ExactRatio::integer(102)
    );

    let unity = RenderPlan::compile(&document(
        rate(),
        &["unity"],
        [
            ("source", source(6, 100, 106, SourceAudioMapping::FitBeat)),
            ("unity", retime("source", 6, 0, 6, PitchPolicy::Preserve)),
        ],
        BTreeMap::new(),
    ))
    .unwrap();
    let unity = unity
        .audio_processing(samples(0, 6), AudioQueryLimits::default())
        .unwrap();
    assert!(matches!(
        unity.spans[0].content,
        AudioSignalContent::Leaf(_)
    ));
    assert_eq!(unity.spans[0].retimes[0].node, id("unity"));
    assert_eq!(unity.spans[0].retimes[0].pitch, PitchPolicy::Preserve);
    assert_eq!(
        unity.spans[0].source_point(AudioSample(2)).unwrap().ticks,
        ExactRatio::integer(102)
    );
}
