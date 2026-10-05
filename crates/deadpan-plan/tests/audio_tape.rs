use std::collections::BTreeMap;

use deadpan_core::*;
use deadpan_plan::{
    AudioContent, AudioDefinitionSelector, AudioQueryLimits, AudioSignalContent, AudioSignalTape,
    AudioSignalTapeRun, PlanError, RenderPlan, SignalSample,
};

#[path = "audio_tape/mix.rs"]
mod mix;

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn ratio(numerator: i128, denominator: i128) -> ExactRatio {
    ExactRatio::new(numerator, denominator).unwrap()
}

fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}

fn audio_span() -> SourceSpan {
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: 1_000_000,
            time_base,
        },
    )
    .unwrap()
}

fn source(length: i64) -> BeatNode {
    BeatNode {
        label: "Current source".into(),
        framing: None,
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Source {
            source: SourceNode {
                edit_window: None,
                duration: frames(length),
                video: SourceVideo::Blank,
                video_mapping: SourceVideoMapping::FitBeat,
                audio: Some(SourceAudio {
                    asset: AssetId::new("media").unwrap(),
                    span: audio_span(),
                }),
                audio_mapping: SourceAudioMapping::FitBeat,
                audio_offset: AudioSample(0),
                link: LinkRelation::Independent,
            },
        },
        cutaways: Vec::new(),
        captions: Vec::new(),
    }
}

fn hold(length: i64) -> BeatNode {
    BeatNode::hold(
        "Silent Hold",
        HoldRecipe {
            duration: frames(length),
            video: HoldVideo::Background,
            picture_context: None,
            audio: HoldAudio::Silence,
        },
    )
}

fn make_plan(rate: FrameRate, source_frames: i64) -> RenderPlan {
    let empty = ProjectDocument::new(
        ProjectId::new("tape-project").unwrap(),
        RevisionId::new("tape-revision").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: rate,
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut nodes = BTreeMap::from([
        (id("source"), source(source_frames)),
        (id("left"), hold(4)),
        (id("middle"), hold(4)),
        (id("right"), hold(4)),
        (id("partition-source"), source(4)),
        (
            id("partition"),
            BeatNode {
                label: "Retained input".into(),
                framing: None,
                audio_treatments: Default::default(),
                audio_editorial_edges: Default::default(),
                audio_edges: Default::default(),
                kind: NodeKind::Retime {
                    child: id("partition-source"),
                    duration: frames(2),
                    mapping: FrameRange::new(ProjectFrame(1), ProjectFrame(3)).unwrap(),
                    pitch: PitchPolicy::FollowSpeed,
                    purpose: RetimePurpose::Partition,
                },
                cutaways: Vec::new(),
                captions: Vec::new(),
            },
        ),
    ]);
    nodes.insert(
        id("root"),
        BeatNode::sequence(
            "Root",
            vec![
                id("source"),
                id("left"),
                id("middle"),
                id("right"),
                id("partition"),
            ],
        ),
    );
    let mut wire = serde_json::to_value(empty).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        AssetId::new("media").unwrap(),
        AssetRecord {
            label: "Original".into(),
            content_hash: "a".repeat(64),
            video: None,
            audio: Some(audio_span()),
            still_image: true,
            frame_count: None,
            source_qualification: None,
        },
    )]))
    .unwrap();
    RenderPlan::compile(&ProjectDocument::from_json(&wire.to_string()).unwrap()).unwrap()
}

fn signal<'plan>(plan: &'plan RenderPlan, node: &str) -> deadpan_plan::AudioSignal<'plan> {
    plan.audio_definition(AudioDefinitionSelector::Node { node: id(node) })
        .unwrap()
        .signal()
}

fn run<'plan>(
    destination: std::ops::Range<ExactRatio>,
    source: std::ops::Range<ExactRatio>,
    signal: deadpan_plan::AudioSignal<'plan>,
) -> AudioSignalTapeRun<'plan> {
    AudioSignalTapeRun::new(destination, source, signal)
}

fn limits(maximum_spans: usize, maximum_work: usize) -> AudioQueryLimits {
    AudioQueryLimits {
        maximum_spans,
        maximum_work,
    }
}

#[test]
fn ntsc_fractional_split_keeps_common_point_phase_and_span_allocations() {
    let plan = make_plan(FrameRate::new(30_000, 1001).unwrap(), 4);
    let tape = AudioSignalTape::new(
        &plan,
        ExactRatio::ZERO..ratio(5, 2),
        vec![
            run(
                ExactRatio::ZERO..ratio(3, 2),
                ExactRatio::ZERO..ratio(3, 2),
                signal(&plan, "left"),
            ),
            run(
                ratio(3, 2)..ratio(5, 2),
                ExactRatio::ZERO..ExactRatio::ONE,
                signal(&plan, "right"),
            ),
        ],
    )
    .unwrap();
    assert_eq!(tape.sample_count().unwrap(), SignalSample(4004));

    let query = tape
        .query(
            SignalSample(0)..SignalSample(4004),
            AudioQueryLimits::default(),
        )
        .unwrap();
    assert_eq!(query.spans.len(), 2);
    assert_eq!(query.spans[0].samples, SignalSample(0)..SignalSample(2403));
    assert_eq!(
        query.spans[1].samples,
        SignalSample(2403)..SignalSample(4004)
    );
    // Keep the provider's physical allocation; the tape only clips requested
    // samples to its run window.
    assert_eq!(
        query.spans[1].allocated_samples,
        SignalSample(2403)..SignalSample(4004)
    );
    assert_eq!(query.spans[1].grid.frame_origin(), ExactRatio::ZERO);
    assert_eq!(
        query.spans[1]
            .sampling
            .local_at(SignalSample(2403))
            .unwrap(),
        ratio(3, 8008)
    );
    assert_eq!(
        query.spans[1].definition,
        Some(AudioDefinitionSelector::Node { node: id("right") })
    );
}

#[test]
fn positive_run_with_no_point_labels_is_retained_and_skipped() {
    let plan = make_plan(FrameRate::new(30, 1).unwrap(), 4);
    let middle_start = ratio(6401, 6400);
    let middle_end = ratio(6403, 6400);
    let tape = AudioSignalTape::new(
        &plan,
        ExactRatio::ZERO..ExactRatio::integer(2),
        vec![
            run(
                ExactRatio::ZERO..middle_start,
                ExactRatio::ZERO..middle_start,
                signal(&plan, "left"),
            ),
            run(
                middle_start..middle_end,
                ExactRatio::ZERO..ratio(1, 3200),
                signal(&plan, "middle"),
            ),
            run(
                middle_end..ExactRatio::integer(2),
                ExactRatio::ZERO..ExactRatio::integer(2).checked_sub(middle_end).unwrap(),
                signal(&plan, "right"),
            ),
        ],
    )
    .unwrap();
    assert_eq!(tape.sample_count().unwrap(), SignalSample(3200));
    let query = tape
        .query(
            SignalSample(0)..SignalSample(3200),
            AudioQueryLimits::default(),
        )
        .unwrap();
    assert_eq!(query.spans.len(), 2);
    assert_eq!(query.spans[0].samples, SignalSample(0)..SignalSample(1601));
    assert_eq!(
        query.spans[1].samples,
        SignalSample(1601)..SignalSample(3200)
    );
}

#[test]
fn requires_exact_coverage_and_rejects_foreign_provider_plans() {
    let plan = make_plan(FrameRate::new(30, 1).unwrap(), 4);
    let provider = signal(&plan, "left");
    assert!(matches!(
        AudioSignalTape::new(
            &plan,
            ExactRatio::ZERO..ExactRatio::integer(2),
            vec![run(
                ExactRatio::ZERO..ExactRatio::ONE,
                ExactRatio::ZERO..ExactRatio::ONE,
                provider.clone(),
            )],
        ),
        Err(PlanError::InvalidPlan(_))
    ));
    assert!(matches!(
        AudioSignalTape::new(
            &plan,
            ExactRatio::ZERO..ExactRatio::integer(2),
            vec![
                run(
                    ExactRatio::ZERO..ExactRatio::ONE,
                    ExactRatio::ZERO..ExactRatio::ONE,
                    provider.clone(),
                ),
                run(
                    ratio(1001, 1000)..ExactRatio::integer(2),
                    ExactRatio::ZERO..ratio(999, 1000),
                    provider.clone(),
                ),
            ],
        ),
        Err(PlanError::InvalidPlan(_))
    ));
    let other = make_plan(FrameRate::new(30, 1).unwrap(), 4);
    assert!(matches!(
        AudioSignalTape::new(
            &plan,
            ExactRatio::ZERO..ExactRatio::ONE,
            vec![run(
                ExactRatio::ZERO..ExactRatio::ONE,
                ExactRatio::ZERO..ExactRatio::ONE,
                signal(&other, "left"),
            )],
        ),
        Err(PlanError::InvalidPlan(_))
    ));
}

#[test]
fn full_support_is_the_filter_crop_and_runs_do_not_crop_allocated_spans() {
    let plan = make_plan(FrameRate::new(30, 1).unwrap(), 4);
    let source = signal(&plan, "source");
    let tape = AudioSignalTape::new(
        &plan,
        ExactRatio::ZERO..ExactRatio::integer(2),
        vec![
            run(
                ExactRatio::ZERO..ExactRatio::ONE,
                ExactRatio::ZERO..ExactRatio::ONE,
                source.clone(),
            ),
            run(
                ExactRatio::ONE..ExactRatio::integer(2),
                ExactRatio::ONE..ExactRatio::integer(2),
                source,
            ),
        ],
    )
    .unwrap();
    let query = tape
        .query(
            SignalSample(0)..SignalSample(3200),
            AudioQueryLimits::default(),
        )
        .unwrap();
    assert_eq!(query.spans.len(), 2);
    for span in &query.spans {
        assert_eq!(span.allocated_samples, SignalSample(0)..SignalSample(3200));
        let AudioSignalContent::Leaf(AudioContent::Source { support, .. }) = &span.content else {
            panic!("expected live Source recipe")
        };
        assert_eq!(support.start.ticks, ExactRatio::ZERO);
        assert_eq!(support.end.ticks, ExactRatio::integer(500_000));
    }
    assert_eq!(query.spans[0].samples, SignalSample(0)..SignalSample(1600));
    assert_eq!(
        query.spans[1].samples,
        SignalSample(1600)..SignalSample(3200)
    );
}

#[test]
fn policy_keeps_per_provider_span_semantics_independent_of_run_order() {
    let plan = make_plan(FrameRate::new(30, 1).unwrap(), 4);
    let first = AudioSignalTape::new(
        &plan,
        ExactRatio::ZERO..ExactRatio::integer(2),
        vec![
            run(
                ExactRatio::ZERO..ExactRatio::ONE,
                ExactRatio::ZERO..ExactRatio::ONE,
                signal(&plan, "left"),
            ),
            run(
                ExactRatio::ONE..ExactRatio::integer(2),
                ExactRatio::ZERO..ExactRatio::ONE,
                signal(&plan, "source"),
            ),
        ],
    )
    .unwrap();
    let second = AudioSignalTape::new(
        &plan,
        ExactRatio::ZERO..ExactRatio::integer(2),
        vec![
            run(
                ExactRatio::ZERO..ExactRatio::ONE,
                ExactRatio::ZERO..ExactRatio::ONE,
                signal(&plan, "source"),
            ),
            run(
                ExactRatio::ONE..ExactRatio::integer(2),
                ExactRatio::ZERO..ExactRatio::ONE,
                signal(&plan, "left"),
            ),
        ],
    )
    .unwrap();
    let limits = limits(1, 512);
    assert!(
        first
            .policy(SignalSample(0)..SignalSample(3200), limits)
            .is_ok()
    );
    assert!(
        second
            .policy(SignalSample(0)..SignalSample(3200), limits)
            .is_ok()
    );
    assert!(
        first
            .query(SignalSample(0)..SignalSample(3200), limits)
            .is_err()
    );
}

#[test]
fn indexed_dispatch_comparisons_and_dispatch_use_the_shared_work_budget() {
    let plan = make_plan(FrameRate::new(30, 1).unwrap(), 4096);
    let source = signal(&plan, "source");
    let runs = (0..4096)
        .map(|frame| {
            let start = ExactRatio::integer(frame);
            let end = ExactRatio::integer(frame + 1);
            run(start..end, start..end, source.clone())
        })
        .collect();
    let tape =
        AudioSignalTape::new(&plan, ExactRatio::ZERO..ExactRatio::integer(4096), runs).unwrap();
    assert!(
        tape.query(SignalSample(0)..SignalSample(1), limits(1, 2))
            .is_err()
    );
    assert!(
        tape.policy(SignalSample(0)..SignalSample(1), limits(1, 4))
            .is_err()
    );
    assert!(
        tape.query(SignalSample(0)..SignalSample(1), limits(1, 64))
            .is_ok()
    );
    assert!(
        tape.policy(SignalSample(0)..SignalSample(1), limits(1, 64))
            .is_ok()
    );
}

#[test]
fn prior_partition_allocation_is_not_promoted_to_a_sampling_crop() {
    let plan = make_plan(FrameRate::new(30, 1).unwrap(), 4);
    let tape = AudioSignalTape::new(
        &plan,
        ExactRatio::ZERO..ExactRatio::integer(4),
        vec![
            run(
                ExactRatio::ZERO..ExactRatio::ONE,
                ExactRatio::ZERO..ExactRatio::ONE,
                signal(&plan, "partition"),
            ),
            run(
                ExactRatio::ONE..ExactRatio::integer(4),
                ExactRatio::ZERO..ExactRatio::integer(3),
                signal(&plan, "left"),
            ),
        ],
    )
    .unwrap();
    let query = tape
        .query(SignalSample(0)..SignalSample(1600), Default::default())
        .unwrap();
    let span = &query.spans[0];
    assert_eq!(span.allocated_samples, SignalSample(0)..SignalSample(3200));
    let AudioSignalContent::Leaf(AudioContent::Source { support, .. }) = &span.content else {
        panic!("expected retained Source support")
    };
    // The global input begins at child frame1. The old partition ends at
    // child3, but only allocates demand: context still reaches child4.
    assert_eq!(support.start.ticks, ExactRatio::integer(250_000));
    assert_eq!(support.end.ticks, ExactRatio::integer(1_000_000));
}
