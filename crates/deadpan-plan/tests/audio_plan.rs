use std::collections::BTreeMap;
use std::ops::Range;

use deadpan_core::*;
use deadpan_plan::{
    AudioBoundaryKind, AudioBoundaryOrigin, AudioContent, AudioQuery, AudioQueryLimits, AudioSpan,
    PlanError, RenderPlan, SilenceReason,
};
use proptest::prelude::*;
use proptest::test_runner::RngSeed;

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn duration(frames: i64) -> FrameDuration {
    FrameDuration::new(frames).unwrap()
}

fn ratio(numerator: i128, denominator: i128) -> ExactRatio {
    ExactRatio::new(numerator, denominator).unwrap()
}

fn samples(start: i64, end: i64) -> Range<AudioSample> {
    AudioSample(start)..AudioSample(end)
}

fn iteration(ordinal: u32) -> IterationId {
    IterationId {
        allocation: RevisionId::new("plays").unwrap(),
        ordinal,
    }
}

fn source_audio() -> SourceAudio {
    let clock = SourceTimeBase::new(1, 48_000).unwrap();
    SourceAudio {
        asset: AssetId::new("media").unwrap(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: -48_000,
                time_base: clock,
            },
            SourceTimestamp {
                ticks: 0,
                time_base: clock,
            },
        )
        .unwrap(),
    }
}

fn source(frames: i64, mapping: SourceAudioMapping, offset: i64) -> BeatNode {
    BeatNode {
        label: "Original audio".into(),
        kind: NodeKind::Source {
            source: SourceNode {
                duration: duration(frames),
                video: SourceVideo::Blank,
                video_mapping: SourceVideoMapping::FitBeat,
                audio: Some(source_audio()),
                audio_mapping: mapping,
                audio_offset: AudioSample(offset),
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
    children: Vec<NodeId>,
    mut nodes: BTreeMap<NodeId, BeatNode>,
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
    nodes.insert(id("root"), BeatNode::sequence("Root", children));
    let mut wire = serde_json::to_value(empty).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["overrides"] = serde_json::to_value(overrides).unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        AssetId::new("media").unwrap(),
        AssetRecord {
            label: "Original".into(),
            content_hash: "a".repeat(64),
            video: None,
            audio: Some(source_audio().span),
            still_image: true,
            frame_count: None,
            source_qualification: None,
        },
    )]))
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn one_sample_per_frame() -> FrameRate {
    FrameRate::new(48_000, 1).unwrap()
}

fn query(plan: &RenderPlan, start: i64, end: i64) -> AudioQuery {
    plan.audio(samples(start, end), AudioQueryLimits::default())
        .unwrap()
}

// Independent integer reference for nonnegative authored boundaries. Do not
// obtain expected allocations by calling the production frame/sample helpers.
fn rounded(numerator: i128, denominator: i128) -> i64 {
    let quotient = numerator / denominator;
    let twice_remainder = 2 * (numerator % denominator);
    i64::try_from(
        quotient
            + i128::from(
                twice_remainder > denominator
                    || (twice_remainder == denominator && quotient % 2 == 1),
            ),
    )
    .unwrap()
}

fn per_sample(query: AudioQuery) -> Vec<AudioSpan> {
    query
        .spans
        .into_iter()
        .flat_map(|span| {
            (span.samples.start.0..span.samples.end.0).map(move |sample| {
                let mut piece = span.clone();
                piece.samples = samples(sample, sample + 1);
                piece
            })
        })
        .collect()
}

fn boundary(
    node: &str,
    repeats: &[(&str, u32)],
    gap: Option<u32>,
    kind: AudioBoundaryKind,
) -> AudioBoundaryOrigin {
    AudioBoundaryOrigin {
        instance: InstancePath {
            node: id(node),
            repeats: repeats
                .iter()
                .map(|(node, play)| RepeatInstance {
                    node: id(node),
                    iteration: iteration(*play),
                })
                .collect(),
        },
        gap_after: gap.map(iteration),
        kind,
    }
}

#[test]
fn placement_edges_keep_their_original_side_in_adjacent_silence_and_query_crops() {
    use AudioBoundaryKind::*;
    let doc = document(
        one_sample_per_frame(),
        vec![id("source")],
        BTreeMap::from([(
            id("source"),
            source(
                10,
                SourceAudioMapping::Placement {
                    start: ratio(2, 1),
                    frames: ratio(6, 1),
                },
                0,
            ),
        )]),
        BTreeMap::new(),
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    let full = query(&plan, 0, 10);
    assert_eq!(full.spans.len(), 3);
    let placement_start = vec![boundary("source", &[], None, SourcePlacementStart)];
    let placement_end = vec![boundary("source", &[], None, SourcePlacementEnd)];
    assert_eq!(full.spans[0].boundaries.end, placement_start);
    assert_eq!(full.spans[1].boundaries.start, placement_start);
    assert_eq!(full.spans[1].boundaries.end, placement_end);
    assert_eq!(full.spans[2].boundaries.start, placement_end);
    assert_eq!(
        full.spans[0].boundaries.start,
        vec![
            boundary("root", &[], None, NodeStart),
            boundary("source", &[], None, NodeStart),
        ]
    );
    assert_eq!(
        full.spans[2].boundaries.end,
        vec![
            boundary("root", &[], None, NodeEnd),
            boundary("source", &[], None, NodeEnd),
        ]
    );
    for (start, end, index) in [(4, 5, 1), (9, 10, 2), (0, 1, 0), (2, 3, 1)] {
        let cropped = query(&plan, start, end);
        assert_eq!(cropped.spans[0].boundaries, full.spans[index].boundaries);
        assert_eq!(
            cropped.spans[0].allocated_samples,
            full.spans[index].allocated_samples
        );
    }
}

#[test]
fn nested_repeat_trim_and_gap_boundaries_keep_each_owners_own_occurrence() {
    use AudioBoundaryKind::*;
    let doc = document(
        one_sample_per_frame(),
        vec![id("outer")],
        BTreeMap::from([
            (id("source"), source(4, SourceAudioMapping::FitBeat, 0)),
            (id("inner"), repeat("source", 3, 1)),
            (id("cut"), retime("inner", 6, 6, 12, PitchPolicy::Preserve)),
            (id("outer"), repeat("cut", 2, 0)),
        ]),
        BTreeMap::new(),
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    let result = query(&plan, 6, 12);
    assert_eq!(result.spans.len(), 3);
    assert_eq!(result.spans[0].allocated_samples, samples(6, 9));
    assert_eq!(result.spans[1].allocated_samples, samples(9, 10));
    assert_eq!(result.spans[2].allocated_samples, samples(10, 12));
    assert_eq!(
        result.spans[0].boundaries.start,
        vec![boundary("cut", &[("outer", 1)], None, NodeStart)]
    );
    assert_eq!(
        result.spans[0].boundaries.end,
        vec![
            boundary("source", &[("outer", 1), ("inner", 1)], None, NodeEnd),
            boundary(
                "source",
                &[("outer", 1), ("inner", 1)],
                None,
                SourcePlacementEnd
            ),
        ]
    );
    assert_eq!(
        result.spans[1].boundaries.start,
        vec![boundary("inner", &[("outer", 1)], Some(1), RepeatGapStart)]
    );
    assert_eq!(
        result.spans[1].boundaries.end,
        vec![boundary("inner", &[("outer", 1)], Some(1), RepeatGapEnd)]
    );
    assert_eq!(
        result.spans[2].boundaries.end,
        vec![
            boundary("root", &[], None, NodeEnd),
            boundary("outer", &[], None, NodeEnd),
            boundary("cut", &[("outer", 1)], None, NodeEnd),
        ]
    );
    for span in &result.spans {
        for origin in span.boundaries.start.iter().chain(&span.boundaries.end) {
            origin.instance.validate(&doc).unwrap();
        }
    }
    assert_eq!(
        per_sample(result),
        (6..12)
            .flat_map(|at| per_sample(query(&plan, at, at + 1)))
            .collect::<Vec<_>>()
    );
}

#[test]
fn rounded_sample_equality_does_not_merge_different_exact_boundary_origins() {
    use AudioBoundaryKind::*;
    let doc = document(
        one_sample_per_frame(),
        vec![id("source")],
        BTreeMap::from([(
            id("source"),
            source(
                2,
                SourceAudioMapping::Placement {
                    start: ratio(1, 4),
                    frames: ratio(3, 2),
                },
                0,
            ),
        )]),
        BTreeMap::new(),
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    let result = query(&plan, 0, 2);
    assert_eq!(result.spans.len(), 1);
    let span = &result.spans[0];
    assert_eq!(span.allocated_samples, samples(0, 2));
    assert_eq!(span.project_extent, ratio(1, 4)..ratio(7, 4));
    assert_eq!(
        span.boundaries.start,
        vec![boundary("source", &[], None, SourcePlacementStart)]
    );
    assert_eq!(
        span.boundaries.end,
        vec![boundary("source", &[], None, SourcePlacementEnd)]
    );
}

#[test]
fn coincident_boundary_capture_is_charged_before_returning_metadata() {
    let doc = document(
        one_sample_per_frame(),
        vec![id("source")],
        BTreeMap::from([(id("source"), source(4, SourceAudioMapping::FitBeat, 0))]),
        BTreeMap::new(),
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    let limits = |maximum_work| AudioQueryLimits {
        maximum_spans: 1,
        maximum_work,
    };
    assert!(matches!(
        plan.audio(samples(1, 2), limits(8)),
        Err(PlanError::AudioQueryLimit("structural work"))
    ));
    let result = plan.audio(samples(1, 2), limits(9)).unwrap();
    assert_eq!(result.spans[0].boundaries.start.len(), 3);
    assert_eq!(result.spans[0].boundaries.end.len(), 3);
    assert_eq!(result.lookup.visited_nodes, 2);
}

#[test]
fn ntsc_sequence_boundaries_round_from_the_project_origin_without_drift() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let count = 1001;
    let children: Vec<_> = (0..count)
        .map(|index| id(&format!("beat-{index}")))
        .collect();
    let nodes = children
        .iter()
        .cloned()
        .map(|node| (node, hold(1)))
        .collect();
    let doc = document(rate, children.clone(), nodes, BTreeMap::new());
    let plan = RenderPlan::compile(&doc).unwrap();
    let end = rounded(i128::from(count) * 48_000 * 1001, 30_000);
    let result = query(&plan, 0, end);
    assert_eq!(result.project_id, *doc.project_id());
    assert_eq!(result.revision_id, *doc.revision_id());
    assert_eq!(result.spans.len(), children.len());
    assert_eq!(plan.audio_duration().unwrap(), AudioSample(end));
    for (index, span) in result.spans.iter().enumerate() {
        let frame = i64::try_from(index).unwrap();
        let expected = samples(
            rounded(i128::from(frame) * 48_000 * 1001, 30_000),
            rounded(i128::from(frame + 1) * 48_000 * 1001, 30_000),
        );
        assert_eq!(span.samples, expected);
        assert_eq!(span.allocated_samples, expected);
        assert_eq!(span.instance.node, children[index]);
        assert_eq!(
            span.project_extent,
            ExactRatio::integer(frame)..ExactRatio::integer(frame + 1)
        );
    }
    assert_ne!(end, rounded(48_000 * 1001, 30_000) * i64::from(count));
}

#[test]
fn signed_audio_placement_clips_to_the_host_without_refitting_or_losing_source_origin() {
    for (offset, expected_start, expected_end, expected_first_tick) in [
        (-8000, 0, 44_000, -44_000),
        (8000, 12_000, 60_000, -48_000),
        (104_000, 0, 0, 0),
        (-104_000, 0, 0, 0),
    ] {
        let doc = document(
            FrameRate::new(30, 1).unwrap(),
            vec![id("source")],
            BTreeMap::from([(
                id("source"),
                source(
                    60,
                    SourceAudioMapping::Placement {
                        start: ratio(5, 2),
                        frames: ExactRatio::integer(30),
                    },
                    offset,
                ),
            )]),
            BTreeMap::new(),
        );
        let plan = RenderPlan::compile(&doc).unwrap();
        let result = query(&plan, 0, 96_000);
        let audible: Vec<_> = result
            .spans
            .iter()
            .filter(|span| matches!(span.content, AudioContent::Source { .. }))
            .collect();
        if expected_end == 0 {
            assert!(audible.is_empty());
            assert_eq!(result.spans.len(), 1);
        } else {
            assert_eq!(audible.len(), 1);
            let span = audible[0];
            assert_eq!(
                span.allocated_samples,
                samples(expected_start, expected_end)
            );
            let point = span.source_point(AudioSample(expected_start)).unwrap();
            assert_eq!(point.ticks, ExactRatio::integer(expected_first_tick));
            assert_eq!(point.time_base, SourceTimeBase::new(1, 48_000).unwrap());
            let partial = query(&plan, expected_start + 17, expected_end - 13);
            assert_eq!(partial.spans.len(), 1);
            assert_eq!(partial.spans[0].allocated_samples, span.allocated_samples);
            assert_eq!(partial.spans[0].project_extent, span.project_extent);
            assert_eq!(
                partial.spans[0]
                    .source_point(AudioSample(expected_start + 17))
                    .unwrap(),
                span.source_point(AudioSample(expected_start + 17)).unwrap()
            );
        }
        let mut cursor = AudioSample(0);
        for span in result.spans {
            assert_eq!(span.samples.start, cursor);
            cursor = span.samples.end;
            if !matches!(span.content, AudioContent::Source { .. }) {
                assert_eq!(
                    span.content,
                    AudioContent::Silence {
                        reason: SilenceReason::OutsideSourcePlacement
                    }
                );
            }
        }
        assert_eq!(cursor, AudioSample(96_000));
    }
}

#[test]
fn nested_retimes_preserve_mixed_pitch_stages_and_exact_source_coordinates() {
    let doc = document(
        one_sample_per_frame(),
        vec![id("outer")],
        BTreeMap::from([
            (
                id("source"),
                source(
                    12,
                    SourceAudioMapping::Duration {
                        frames: ExactRatio::integer(6),
                    },
                    1,
                ),
            ),
            (
                id("inner"),
                retime("source", 12, 2, 10, PitchPolicy::Preserve),
            ),
            (
                id("outer"),
                retime("inner", 9, 3, 9, PitchPolicy::FollowSpeed),
            ),
        ]),
        BTreeMap::new(),
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    let result = query(&plan, 0, 9);
    assert_eq!(result.spans.len(), 2);
    let source = &result.spans[0];
    assert_eq!(source.samples, samples(0, 7));
    assert_eq!(source.project_extent, ExactRatio::ZERO..ratio(27, 4));
    assert_eq!(source.transform.project_origin, ExactRatio::integer(-9));
    assert_eq!(source.transform.project_frames_per_local_frame, ratio(9, 4));
    assert_eq!(source.retimes.len(), 2);
    assert_eq!(source.retimes[0].node, id("outer"));
    assert_eq!(source.retimes[0].pitch, PitchPolicy::FollowSpeed);
    assert_eq!(source.retimes[0].child_start, ExactRatio::integer(3));
    assert_eq!(source.retimes[0].child_frames_per_local_frame, ratio(2, 3));
    assert_eq!(source.retimes[1].node, id("inner"));
    assert_eq!(source.retimes[1].pitch, PitchPolicy::Preserve);
    assert_eq!(source.retimes[1].child_start, ExactRatio::integer(2));
    assert_eq!(source.retimes[1].child_frames_per_local_frame, ratio(2, 3));
    assert_eq!(
        source.source_point(AudioSample(0)).unwrap().ticks,
        ExactRatio::integer(-24_000)
    );
    assert_eq!(
        source.source_point(AudioSample(6)).unwrap().ticks,
        ratio(-8000, 3)
    );
    assert_eq!(result.spans[1].samples, samples(7, 9));
    assert_eq!(
        result.spans[1].content,
        AudioContent::Silence {
            reason: SilenceReason::OutsideSourcePlacement
        }
    );
}

#[test]
fn half_sample_ties_select_both_parities_and_skip_zero_sample_sequence_leaves() {
    let children: Vec<_> = (0..8).map(|index| id(&format!("leaf-{index}"))).collect();
    let mut nodes: BTreeMap<_, _> = children
        .iter()
        .cloned()
        .map(|node| (node, hold(1)))
        .collect();
    nodes.insert(id("sequence"), BeatNode::sequence("Eight leaves", children));
    nodes.insert(
        id("retime"),
        retime("sequence", 4, 0, 8, PitchPolicy::Preserve),
    );
    let doc = document(
        one_sample_per_frame(),
        vec![id("retime")],
        nodes,
        BTreeMap::new(),
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    let result = query(&plan, 0, 4);
    assert_eq!(result.spans.len(), 4);
    for (sample, (span, leaf)) in result.spans.iter().zip([1, 2, 5, 6]).enumerate() {
        let sample = i64::try_from(sample).unwrap();
        assert_eq!(span.instance.node, id(&format!("leaf-{leaf}")));
        assert_eq!(span.samples, samples(sample, sample + 1));
        assert_eq!(span.project_extent, ratio(leaf, 2)..ratio(leaf + 1, 2));
    }
}

#[test]
fn half_sample_repeat_gaps_are_assigned_once_and_have_no_trailing_gap() {
    let doc = document(
        one_sample_per_frame(),
        vec![id("retime")],
        BTreeMap::from([
            (id("source"), source(1, SourceAudioMapping::FitBeat, 0)),
            (id("repeat"), repeat("source", 4, 1)),
            (
                id("retime"),
                retime("repeat", 7, 0, 7, PitchPolicy::Preserve),
            ),
        ]),
        BTreeMap::new(),
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    let result = query(&plan, 0, 7);
    assert_eq!(result.spans.len(), 7);
    for (index, span) in result.spans.iter().enumerate() {
        let index = u32::try_from(index).unwrap();
        span.instance.validate(&doc).unwrap();
        if index % 2 == 0 {
            assert_eq!(span.instance.node, id("source"));
            assert_eq!(span.instance.repeats[0].iteration, iteration(index / 2));
            assert!(span.gap_after.is_none());
        } else {
            assert_eq!(span.instance.node, id("repeat"));
            assert!(span.instance.repeats.is_empty());
            assert_eq!(span.gap_after, Some(iteration(index / 2)));
            assert_eq!(
                span.content,
                AudioContent::Silence {
                    reason: SilenceReason::SilentHold
                }
            );
        }
    }
    // The trailing half-sample Hold disappears under ties-to-even rounding.
    // Both Repeat play and gap boundaries still select their allocated sample.
    let compressed = document(
        one_sample_per_frame(),
        vec![id("retime")],
        BTreeMap::from([
            (id("source"), source(1, SourceAudioMapping::FitBeat, 0)),
            (id("repeat"), repeat("source", 4, 1)),
            (id("trailing"), hold(1)),
            (
                id("sequence"),
                BeatNode::sequence("Sequence", vec![id("repeat"), id("trailing")]),
            ),
            (
                id("retime"),
                retime("sequence", 4, 0, 8, PitchPolicy::Preserve),
            ),
        ]),
        BTreeMap::new(),
    );
    let compressed = RenderPlan::compile(&compressed).unwrap();
    let compressed = query(&compressed, 0, 4);
    assert_eq!(compressed.spans.len(), 4);
    assert_eq!(compressed.spans[0].gap_after, Some(iteration(0)));
    assert_eq!(
        compressed.spans[1].instance.repeats[0].iteration,
        iteration(1)
    );
    assert_eq!(compressed.spans[2].gap_after, Some(iteration(2)));
    assert_eq!(compressed.spans[3].gap_after, None);
    assert_eq!(
        compressed.spans[3].instance.repeats[0].iteration,
        iteration(3)
    );
}

#[test]
fn hold_policies_and_absent_source_audio_remain_distinct() {
    let still = BeatNode {
        label: "Still".into(),
        kind: NodeKind::Source {
            source: SourceNode {
                duration: duration(2),
                video: SourceVideo::Still {
                    asset: AssetId::new("media").unwrap(),
                },
                video_mapping: SourceVideoMapping::FitBeat,
                audio: None,
                audio_mapping: SourceAudioMapping::FitBeat,
                audio_offset: AudioSample(0),
                link: LinkRelation::Independent,
            },
        },
    };
    let doc = document(
        one_sample_per_frame(),
        vec![id("still"), id("room"), id("tail"), id("silence")],
        BTreeMap::from([
            (id("still"), still),
            (
                id("room"),
                BeatNode::hold(
                    "Room",
                    hold_recipe(
                        5,
                        HoldAudio::RoomTone {
                            source: source_audio(),
                        },
                    ),
                ),
            ),
            (
                id("tail"),
                BeatNode::hold(
                    "Tail",
                    hold_recipe(
                        4,
                        HoldAudio::Tail {
                            source: source_audio(),
                            maximum: duration(3),
                        },
                    ),
                ),
            ),
            (id("silence"), hold(3)),
        ]),
        BTreeMap::new(),
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    let result = query(&plan, 0, 14);
    assert_eq!(
        result
            .spans
            .iter()
            .map(|span| span.content.clone())
            .collect::<Vec<_>>(),
        vec![
            AudioContent::Silence {
                reason: SilenceReason::NoSourceAudio
            },
            AudioContent::RoomTone {
                source: source_audio(),
                duration: duration(5)
            },
            AudioContent::Tail {
                source: source_audio(),
                maximum: duration(3)
            },
            AudioContent::Silence {
                reason: SilenceReason::SilentHold
            },
        ]
    );
    for span in result.spans {
        assert!(matches!(
            span.source_point(span.samples.start),
            Err(PlanError::NoSourceAudio)
        ));
    }
}

#[test]
fn billion_play_override_and_final_seek_stay_bounded() {
    let doc = document(
        one_sample_per_frame(),
        vec![id("repeat")],
        BTreeMap::from([
            (id("source"), source(1, SourceAudioMapping::FitBeat, 0)),
            (id("alternate"), source(3, SourceAudioMapping::FitBeat, 0)),
            (id("repeat"), repeat("source", 1_000_000_000, 1)),
        ]),
        BTreeMap::from([(
            id("repeat"),
            PlayOverrides::try_from(vec![PlayOverride {
                iteration: iteration(500_000_000),
                root: id("alternate"),
            }])
            .unwrap(),
        )]),
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    assert_eq!(plan.metadata().storage.repeat_segment_entries, 3);
    assert_eq!(plan.metadata().storage.authored_nodes, 4);
    assert_eq!(plan.audio_duration().unwrap(), AudioSample(2_000_000_001));
    assert!(matches!(
        plan.audio(
            samples(1_000_000_000, 1_000_000_001),
            AudioQueryLimits {
                maximum_spans: 1,
                maximum_work: 4,
            }
        ),
        Err(PlanError::AudioQueryLimit("structural work"))
    ));
    for (start, end, node, play) in [
        (1_000_000_000, 1_000_000_003, "alternate", 500_000_000),
        (1_000_000_004, 1_000_000_005, "source", 500_000_001),
        (2_000_000_000, 2_000_000_001, "source", 999_999_999),
    ] {
        let result = plan
            .audio(
                samples(start, end),
                AudioQueryLimits {
                    maximum_spans: 1,
                    // Includes copying each retained boundary's occurrence path.
                    maximum_work: 32,
                },
            )
            .unwrap();
        assert_eq!(result.spans.len(), 1);
        let span = &result.spans[0];
        assert_eq!(span.instance.node, id(node));
        assert_eq!(span.instance.repeats[0].iteration, iteration(play));
        span.instance.validate(&doc).unwrap();
        let origins = span.boundaries.start.iter().chain(&span.boundaries.end);
        let mut leaf_origins = 0;
        for origin in origins {
            origin.instance.validate(&doc).unwrap();
            if origin.instance.node == id(node) {
                assert_eq!(origin.instance, span.instance);
                leaf_origins += 1;
            }
        }
        assert_eq!(leaf_origins, 4);
        assert_eq!(span.allocated_samples, samples(start, end));
        assert!(span.gap_after.is_none());
        assert_eq!(result.lookup.visited_nodes, 3);
        assert!(result.lookup.iteration_run_comparisons <= 3);
    }
}

#[test]
fn invalid_ranges_and_budgets_fail_and_empty_queries_allocate_nothing() {
    let doc = document(
        one_sample_per_frame(),
        vec![id("one"), id("two")],
        BTreeMap::from([(id("one"), hold(1)), (id("two"), hold(1))]),
        BTreeMap::new(),
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    for range in [samples(-1, 0), samples(2, 1), samples(0, 3), samples(3, 3)] {
        assert!(matches!(
            plan.audio(range, AudioQueryLimits::default()),
            Err(PlanError::AudioRangeOutOfRange)
        ));
    }
    for limits in [
        AudioQueryLimits {
            maximum_spans: 0,
            maximum_work: 1,
        },
        AudioQueryLimits {
            maximum_spans: 4097,
            maximum_work: 1,
        },
        AudioQueryLimits {
            maximum_spans: 1,
            maximum_work: 0,
        },
        AudioQueryLimits {
            maximum_spans: 1,
            maximum_work: 65_537,
        },
    ] {
        assert!(matches!(
            plan.audio(samples(0, 0), limits),
            Err(PlanError::InvalidAudioLimits)
        ));
    }
    assert!(matches!(
        plan.audio(
            samples(0, 2),
            AudioQueryLimits {
                maximum_spans: 1,
                maximum_work: 100
            }
        ),
        Err(PlanError::AudioQueryLimit("span count"))
    ));
    assert!(matches!(
        plan.audio(
            samples(0, 1),
            AudioQueryLimits {
                maximum_spans: 1,
                maximum_work: 1
            }
        ),
        Err(PlanError::AudioQueryLimit("structural work"))
    ));
    for boundary in [0, 1, 2] {
        let empty = query(&plan, boundary, boundary);
        assert!(empty.spans.is_empty());
        assert_eq!(empty.lookup.visited_nodes, 0);
        assert_eq!(empty.lookup.sequence_comparisons, 0);
    }
    let empty = document(
        one_sample_per_frame(),
        Vec::new(),
        BTreeMap::new(),
        BTreeMap::new(),
    );
    let empty = RenderPlan::compile(&empty).unwrap();
    assert_eq!(empty.audio_duration().unwrap(), AudioSample(0));
    assert!(query(&empty, 0, 0).spans.is_empty());
}

#[test]
fn unrepresentable_sample_duration_and_nested_affines_fail_without_rounding() {
    let huge = document(
        FrameRate::new(1, 1).unwrap(),
        vec![id("hold")],
        BTreeMap::from([(id("hold"), hold(i64::MAX))]),
        BTreeMap::new(),
    );
    let plan = RenderPlan::compile(&huge).unwrap();
    assert!(matches!(
        plan.audio_duration(),
        Err(PlanError::Time(TimeError::Overflow))
    ));
    assert!(matches!(
        plan.audio(samples(0, 1), AudioQueryLimits::default()),
        Err(PlanError::Time(TimeError::Overflow))
    ));

    let mut nodes = BTreeMap::from([(
        id("source"),
        source(i64::MAX, SourceAudioMapping::FitBeat, 0),
    )]);
    let mut child = "source";
    for name in ["inner", "middle", "outer"] {
        nodes.insert(
            id(name),
            retime(child, i64::MAX, 2, i64::MAX - 1, PitchPolicy::Preserve),
        );
        child = name;
    }
    let nested = document(
        one_sample_per_frame(),
        vec![id("outer")],
        nodes,
        BTreeMap::new(),
    );
    let plan = RenderPlan::compile(&nested).unwrap();
    assert_eq!(plan.audio_duration().unwrap(), AudioSample(i64::MAX));
    assert!(matches!(
        plan.audio(samples(0, 1), AudioQueryLimits::default()),
        Err(PlanError::Time(TimeError::Overflow))
    ));
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 96,
        rng_seed: RngSeed::Fixed(0xdead_4800_3000_1001),
        ..ProptestConfig::default()
    })]

    #[test]
    fn retimed_sequence_matches_independent_boundary_reference_and_query_partitions(
        lengths in prop::collection::vec(0i64..8, 1..20),
        output in 1i64..65,
        partition in 0u32..65,
    ) {
        let total: i64 = lengths.iter().sum();
        prop_assume!(total > 0);
        let children: Vec<_> = (0..lengths.len()).map(|index| id(&format!("leaf-{index}"))).collect();
        let mut nodes: BTreeMap<_, _> = children.iter().cloned().zip(lengths.iter().copied()).map(|(node, frames)| {
            (node, if frames == 0 { BeatNode::sequence("Empty", vec![]) } else { hold(frames) })
        }).collect();
        nodes.insert(id("sequence"), BeatNode::sequence("Sequence", children.clone()));
        nodes.insert(id("retime"), retime("sequence", output, 0, total, PitchPolicy::FollowSpeed));
        let doc = document(one_sample_per_frame(), vec![id("retime")], nodes, BTreeMap::new());
        let plan = RenderPlan::compile(&doc).unwrap();
        let result = query(&plan, 0, output);
        let mut prefix = 0;
        let mut expected = Vec::new();
        for (node, frames) in children.iter().zip(lengths) {
            let start = rounded(i128::from(prefix) * i128::from(output), i128::from(total));
            prefix += frames;
            let end = rounded(i128::from(prefix) * i128::from(output), i128::from(total));
            if start < end {
                expected.push((node.clone(), samples(start, end)));
            }
        }
        let actual: Vec<_> = result.spans.iter().map(|span| (span.instance.node.clone(), span.allocated_samples.clone())).collect();
        prop_assert_eq!(actual, expected);
        let split = i64::from(partition) % (output + 1);
        let mut pieces = per_sample(query(&plan, 0, split));
        pieces.extend(per_sample(query(&plan, split, output)));
        prop_assert_eq!(pieces, per_sample(result));
    }

    #[test]
    fn retimed_repeat_override_matches_independently_expanded_intervals(
        plays in 1u32..16,
        base_frames in 1i64..5,
        alternate_frames in 1i64..7,
        gap_frames in 0i64..4,
        selected in 0u32..16,
        output in 1i64..65,
        partition in 0u32..65,
    ) {
        let selected = selected % plays;
        let total = i64::from(plays - 1) * (base_frames + gap_frames) + alternate_frames;
        let doc = document(one_sample_per_frame(), vec![id("retime")], BTreeMap::from([
            (id("source"), source(base_frames, SourceAudioMapping::FitBeat, 0)),
            (id("alternate"), source(alternate_frames, SourceAudioMapping::FitBeat, 0)),
            (id("repeat"), repeat("source", plays, gap_frames)),
            (id("retime"), retime("repeat", output, 0, total, PitchPolicy::Preserve)),
        ]), BTreeMap::from([(
            id("repeat"), PlayOverrides::try_from(vec![PlayOverride {
                iteration: iteration(selected), root: id("alternate"),
            }]).unwrap(),
        )]));
        let plan = RenderPlan::compile(&doc).unwrap();
        let result = query(&plan, 0, output);
        let mut expected = Vec::new();
        let mut prefix = 0;
        for play in 0..plays {
            let frames = if play == selected { alternate_frames } else { base_frames };
            let start = rounded(i128::from(prefix) * i128::from(output), i128::from(total));
            prefix += frames;
            let end = rounded(i128::from(prefix) * i128::from(output), i128::from(total));
            if start < end {
                expected.push((
                    id(if play == selected { "alternate" } else { "source" }),
                    None, Some(iteration(play)), samples(start, end),
                ));
            }
            if play + 1 < plays {
                let start = end;
                prefix += gap_frames;
                let end = rounded(i128::from(prefix) * i128::from(output), i128::from(total));
                if start < end {
                    expected.push((id("repeat"), Some(iteration(play)), None, samples(start, end)));
                }
            }
        }
        prop_assert_eq!(prefix, total);
        let actual: Vec<_> = result.spans.iter().map(|span| (
            span.instance.node.clone(), span.gap_after.clone(),
            span.instance.repeats.first().map(|instance| instance.iteration.clone()),
            span.allocated_samples.clone(),
        )).collect();
        prop_assert_eq!(actual, expected);
        for span in &result.spans {
            prop_assert!(span.instance.validate(&doc).is_ok());
        }
        let split = i64::from(partition) % (output + 1);
        let mut pieces = per_sample(query(&plan, 0, split));
        pieces.extend(per_sample(query(&plan, split, output)));
        prop_assert_eq!(pieces, per_sample(result));
    }
}
