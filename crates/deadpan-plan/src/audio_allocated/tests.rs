use super::*;
use std::{collections::BTreeMap, ops::Range};

use crate::*;
use deadpan_core::*;

fn id(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}
fn q(value: i64) -> ExactRatio {
    ExactRatio::integer(value)
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}

fn empty_query(capacity: usize) -> AudioProcessingQuery<'static> {
    AudioProcessingQuery {
        project_id: ProjectId::new("p").unwrap(),
        revision_id: RevisionId::new("r").unwrap(),
        samples: AudioSample(0)..AudioSample(0),
        spans: Vec::with_capacity(capacity),
        lookup: Default::default(),
        work: 0,
    }
}

#[test]
fn processing_query_charges_spare_vector_capacity_and_identifier_arc_text() {
    let mut query = empty_query(0);
    let (empty, work) = query.allocated_bytes_bound(100).unwrap();
    query.spans.reserve_exact(19);
    let (reserved, reserved_work) = query.allocated_bytes_bound(100).unwrap();
    assert_eq!(
        reserved - empty,
        query.spans.capacity() * size_of::<AudioProcessingSpan<'_>>()
    );
    assert_eq!(
        work, reserved_work,
        "unused vector slots require no traversal"
    );
    let old_id = Layout::for_value(query.project_id.as_str());
    query.project_id = ProjectId::new("p".repeat(128)).unwrap();
    let new_id = Layout::for_value(query.project_id.as_str());
    let allocation = |payload| {
        Layout::new::<[usize; 2]>()
            .extend(payload)
            .unwrap()
            .0
            .pad_to_align()
            .size()
    };
    let (long_id, _) = query.allocated_bytes_bound(100).unwrap();
    assert_eq!(long_id - reserved, allocation(new_id) - allocation(old_id));
    assert!(long_id > reserved + 100);
}

#[test]
fn traversal_budget_and_byte_overflow_fail_truthfully() {
    let query = empty_query(3);
    let (bytes, work) = query.allocated_bytes_bound(100).unwrap();
    assert_eq!(query.allocated_bytes_bound(work).unwrap(), (bytes, work));
    for limit in [0, work - 1] {
        assert!(matches!(
            query.allocated_bytes_bound(limit),
            Err(PlanError::AudioQueryLimit(_))
        ));
    }
    let mut budget = HeapBudget {
        remaining: 1,
        bytes: usize::MAX,
        depth: 0,
    };
    assert!(matches!(
        budget.add(1),
        Err(PlanError::AudioQueryLimit("retained audio heap bytes"))
    ));
}

fn hold(length: i64) -> BeatNode {
    BeatNode::hold(
        "Input",
        HoldRecipe {
            duration: frames(length),
            video: HoldVideo::Background,
            picture_context: None,
            audio: HoldAudio::Silence,
        },
    )
}

fn preserve(child: &str, duration: i64, input: i64) -> BeatNode {
    let mut node = hold(duration);
    node.kind = NodeKind::Retime {
        child: id(child),
        duration: frames(duration),
        mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(input)).unwrap(),
        pitch: PitchPolicy::Preserve,
        purpose: RetimePurpose::Edit,
    };
    node
}

fn source() -> SourceAudio {
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    SourceAudio {
        asset: AssetId::new("catalog-sound").unwrap(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: 0,
                time_base,
            },
            SourceTimestamp {
                ticks: 8,
                time_base,
            },
        )
        .unwrap(),
    }
}

fn fixture(label_bytes: usize) -> RenderPlan {
    let document = ProjectDocument::new(
        ProjectId::new("retained-heap").unwrap(),
        RevisionId::new("revision").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(48_000, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut repeat = hold(8);
    repeat.kind = NodeKind::Repeat {
        child: id("outer"),
        iterations: IterationOrder::new(RevisionId::new("play-allocation").unwrap(), 2).unwrap(),
        gap: None,
        escalation: None,
    };
    let mut leaf = hold(16);
    leaf.label = "x".repeat(label_bytes);
    let mut wire = serde_json::to_value(document).unwrap();
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (id("root"), BeatNode::sequence("Root", vec![id("repeat")])),
        (id("repeat"), repeat),
        (id("outer"), preserve("inner", 4, 8)),
        (id("inner"), preserve("leaf", 8, 16)),
        (id("leaf"), leaf),
    ]))
    .unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        source().asset,
        AssetRecord {
            label: "Qualified catalog".into(),
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

fn occurrence(plan: &RenderPlan) -> AudioSourceOccurrence<'_> {
    plan.source_voice_occurrence(
        InstancePath {
            node: id("leaf"),
            repeats: vec![RepeatInstance {
                node: id("repeat"),
                iteration: IterationId {
                    allocation: RevisionId::new("play-allocation").unwrap(),
                    ordinal: 1,
                },
            }],
        },
        AudioSourceVoiceRecipe {
            source: source(),
            mapping: SourceAudioMapping::natural_rate(
                source().span,
                plan.metadata().presentation_basis.frame_rate,
            )
            .unwrap(),
            offset: AudioSample(0),
        },
        Default::default(),
    )
    .unwrap()
}

#[test]
fn nested_preserve_occurrence_and_complete_route_have_owned_graph_lower_bounds() {
    let plan = fixture(1);
    let voice = occurrence(&plan);
    let (voice_bytes, voice_work) = voice.allocated_bytes_bound(100_000).unwrap();
    let query = voice
        .processing(voice.samples(), Default::default())
        .unwrap();
    assert!(matches!(
        query.spans[0].content,
        AudioSignalContent::ProjectedStage(_)
    ));
    let (query_bytes, query_work) = query.allocated_bytes_bound(100_000).unwrap();
    // Two independent Preserve projections must be owned, in addition to the
    // public query's vector and the occurrence's complete current gain clocks.
    assert!(
        voice_bytes
            >= 2 * size_of::<AudioStageProjection<'_>>()
                + std::mem::size_of_val(voice.gain_owners())
                + voice.instance().repeats.capacity() * size_of::<RepeatInstance>()
    );
    assert!(
        query_bytes
            >= query.spans.capacity() * size_of::<AudioProcessingSpan<'_>>()
                + 2 * size_of::<AudioStageProjection<'_>>()
    );
    for (limit, value) in [
        (voice_work - 1, voice.allocated_bytes_bound(voice_work - 1)),
        (query_work - 1, query.allocated_bytes_bound(query_work - 1)),
    ] {
        assert!(limit > 0 && matches!(value, Err(PlanError::AudioQueryLimit(_))));
    }
    let extent = voice.extent();
    let length = extent.end.checked_sub(extent.start).unwrap();
    let core = SoundRoute::identity(length).unwrap();
    let (core_bytes, _) = core.allocated_bytes_bound(1000).unwrap();
    let grid = AudioSampleGrid::new(
        q(0).checked_sub(extent.start).unwrap(),
        q(1),
        AudioBoundaryRule::RoundEven,
    )
    .unwrap();
    let route = AudioSoundRoute::<AudioSample>::new(core, vec![grid])
        .unwrap()
        .with_identity_anchor(voice.routing_anchor().unwrap())
        .unwrap();
    let routed = AudioRoutedRoot::occurrence(voice, route).unwrap();
    let (bytes, work) = routed.allocated_bytes_bound(100_000).unwrap();
    assert!(
        bytes
            >= voice_bytes
                + size_of::<AudioSourceOccurrence<'_>>()
                + size_of::<AudioSoundRoute<AudioSample>>()
                + core_bytes
    );
    assert_eq!(routed.allocated_bytes_bound(work).unwrap(), (bytes, work));
    assert!(matches!(
        routed.allocated_bytes_bound(work - 1),
        Err(PlanError::AudioQueryLimit(_))
    ));
}

#[test]
fn borrowed_plan_payload_is_excluded_from_query_accounting() {
    let small = fixture(1);
    let large = fixture(1024);
    let query = |plan: &RenderPlan| {
        plan.audio_processing(AudioSample(0)..AudioSample(1), Default::default())
            .unwrap()
            .allocated_bytes_bound(100_000)
            .unwrap()
    };
    assert_eq!(query(&small), query(&large));
}

#[test]
fn occurrence_batch_includes_complete_voice_graphs_and_its_vector_storage() {
    let plan = fixture(1);
    let batch = plan
        .source_voice_occurrences(
            &id("leaf"),
            AudioSourceVoiceRecipe {
                source: source(),
                mapping: SourceAudioMapping::natural_rate(
                    source().span,
                    plan.metadata().presentation_basis.frame_rate,
                )
                .unwrap(),
                offset: AudioSample(0),
            },
            AudioSample(0)..AudioSample(8),
            Default::default(),
        )
        .unwrap();
    assert_eq!(batch.voices().len(), 2);
    let owned_voices = batch
        .voices()
        .iter()
        .map(|voice| voice.allocated_bytes_bound(100_000).unwrap().0)
        .sum::<usize>();
    let (bytes, work) = batch.allocated_bytes_bound(100_000).unwrap();
    assert!(bytes >= owned_voices + std::mem::size_of_val(batch.voices()));
    assert_eq!(batch.allocated_bytes_bound(work).unwrap(), (bytes, work));
    assert!(matches!(
        batch.allocated_bytes_bound(work - 1),
        Err(PlanError::AudioQueryLimit(_))
    ));
}

fn stage(signal: AudioSignal<'_>) -> AudioStage<'_> {
    let query = signal
        .query(
            SignalSample(0)..signal.sample_count().unwrap(),
            Default::default(),
        )
        .unwrap();
    query
        .spans
        .into_iter()
        .find_map(|span| match span.content {
            AudioSignalContent::Stage(stage) => Some(stage),
            _ => None,
        })
        .unwrap()
}

fn tape<'p>(
    plan: &'p RenderPlan,
    destination: Range<ExactRatio>,
    source: Range<ExactRatio>,
    signal: AudioSignal<'p>,
) -> AudioSignalTape<'p> {
    AudioSignalTape::new(
        plan,
        destination.clone(),
        vec![AudioSignalTapeRun::new(destination, source, signal)],
    )
    .unwrap()
}

#[test]
fn nested_mixed_projection_counts_every_shared_owning_edge_without_expanding_plays() {
    let plan = fixture(1);
    let outer = stage(plan.audio_signal());
    let inner = stage(outer.input_signal());
    let input = tape(&plan, q(0)..q(16), q(0)..q(16), inner.input_signal());
    let policy = tape(&plan, q(0)..q(8), q(0)..q(16), inner.input_signal());
    let child = AudioStageProjection::new_mixed_input(
        inner,
        AudioSignalMix::new(
            &plan,
            vec![input.clone(), input],
            vec![AudioMixGate::new(q(0)..q(16), vec![1])],
        )
        .unwrap(),
        policy,
        frames(8),
    )
    .unwrap();
    let intrinsic = || {
        AudioSignalTape::new(
            &plan,
            q(0)..q(8),
            vec![AudioSignalTapeRun::intrinsic(
                q(0)..q(8),
                q(0)..q(8),
                Arc::clone(&child),
            )],
        )
        .unwrap()
    };
    let policy = tape(&plan, q(0)..q(4), q(0)..q(8), outer.input_signal());
    let parent = AudioStageProjection::new_mixed_input(
        outer,
        AudioSignalMix::new(&plan, vec![intrinsic(), intrinsic()], Vec::new()).unwrap(),
        policy,
        frames(4),
    )
    .unwrap();
    let (child_bytes, _) = measure(&child, 100_000).unwrap();
    let (parent_bytes, work) = measure(&parent, 100_000).unwrap();
    assert!(parent_bytes >= child_bytes * 2 + size_of::<AudioStageProjection<'_>>());
    assert!(matches!(
        measure(&parent, work - 1),
        Err(PlanError::AudioQueryLimit(_))
    ));
    let root = AudioProjectedRoot::new(
        parent,
        AudioRootPlacement::new(q(0), q(1), q(0)..q(4)).unwrap(),
    )
    .unwrap();
    let route = AudioSoundRoute::<AudioSample>::new(
        SoundRoute::identity(q(4)).unwrap(),
        vec![AudioSampleGrid::new(q(0), q(1), AudioBoundaryRule::RoundEven).unwrap()],
    )
    .unwrap();
    let routed = AudioRoutedRoot::new(root, route).unwrap();
    assert!(routed.allocated_bytes_bound(100_000).unwrap().0 > parent_bytes);
}
