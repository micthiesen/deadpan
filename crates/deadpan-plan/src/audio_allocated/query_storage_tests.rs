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
                ticks: 16,
                time_base,
            },
        )
        .unwrap(),
    }
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
pub(crate) fn fixture() -> RenderPlan {
    fixture_with_asset(source().asset)
}

pub(crate) fn fixture_with_asset(asset: AssetId) -> RenderPlan {
    let mut source = source();
    source.asset = asset;
    let rate = FrameRate::new(48_000, 1).unwrap();
    let document = ProjectDocument::new(
        ProjectId::new("query-storage").unwrap(),
        RevisionId::new("revision").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: rate,
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
    leaf.kind = NodeKind::Source {
        source: SourceNode {
            duration: frames(16),
            edit_window: None,
            video: SourceVideo::Blank,
            video_mapping: SourceVideoMapping::FitBeat,
            audio: Some(source.clone()),
            audio_mapping: SourceAudioMapping::natural_rate(source.span, rate).unwrap(),
            audio_offset: AudioSample(0),
            link: LinkRelation::Independent,
        },
    };
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
        source.asset.clone(),
        AssetRecord {
            label: "Qualified catalog".into(),
            content_hash: "a".repeat(64),
            video: None,
            audio: Some(source.span),
            still_image: false,
            frame_count: None,
            source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
        },
    )]))
    .unwrap();
    RenderPlan::compile(&ProjectDocument::from_json(&wire.to_string()).unwrap()).unwrap()
}

fn measurement(value: &impl OwnedHeap) -> (usize, usize) {
    let observed = measure_storage(value, 100_000);
    let work = observed.work();
    let (bytes, same_work) = observed.into_parts();
    assert_eq!(work, same_work);
    (bytes.unwrap(), work)
}

fn exact_and_short(value: &impl OwnedHeap) {
    let (bytes, work) = measurement(value);
    assert!(work > 0);
    let (result, actual) = measure_storage(value, work).into_parts();
    assert_eq!(result.unwrap(), bytes);
    assert_eq!(actual, work);
    for limit in [0, work - 1] {
        let observed = measure_storage(value, limit);
        assert!(observed.work() <= limit);
        let (result, _) = observed.into_parts();
        assert!(matches!(result, Err(PlanError::AudioQueryLimit(_))));
    }
}

#[test]
fn actual_root_query_products_measure_all_owned_storage_without_mutating_fields() {
    let plan = fixture();
    let range = AudioSample(0)..AudioSample(1);
    let flat = plan.audio(range.clone(), Default::default()).unwrap();
    let processing = plan
        .audio_processing(range.clone(), Default::default())
        .unwrap();
    let policy = plan
        .audio_policy(range.clone(), Default::default())
        .unwrap();
    let fades = plan.audio_fades(range, Default::default()).unwrap();
    assert!(!flat.spans.is_empty());
    assert!(flat.spans.iter().any(|span| !span.retimes.is_empty()));
    assert!(
        flat.spans
            .iter()
            .any(|span| !span.boundaries.start.is_empty())
    );
    assert!(
        fades
            .spans
            .iter()
            .any(|span| !span.start.is_empty() || !span.end.is_empty())
    );
    let before = (
        serde_json::to_value(&flat).unwrap(),
        serde_json::to_value(&processing).unwrap(),
        format!("{policy:?}"),
        serde_json::to_value(&fades).unwrap(),
    );
    exact_and_short(&flat);
    exact_and_short(&processing);
    exact_and_short(&policy);
    exact_and_short(&fades);
    assert_eq!(
        flat.measure_allocated_storage(100_000)
            .into_parts()
            .0
            .unwrap(),
        measurement(&flat).0
    );
    assert_eq!(
        processing
            .measure_allocated_storage(100_000)
            .into_parts()
            .0
            .unwrap(),
        measurement(&processing).0
    );
    assert_eq!(
        policy
            .measure_allocated_storage(100_000)
            .into_parts()
            .0
            .unwrap(),
        measurement(&policy).0
    );
    assert_eq!(
        fades
            .measure_allocated_storage(100_000)
            .into_parts()
            .0
            .unwrap(),
        measurement(&fades).0
    );
    assert_eq!(
        before,
        (
            serde_json::to_value(&flat).unwrap(),
            serde_json::to_value(&processing).unwrap(),
            format!("{policy:?}"),
            serde_json::to_value(&fades).unwrap(),
        )
    );
}

#[test]
fn flattened_boundaries_retimes_and_fade_origins_charge_actual_spare_capacity() {
    let plan = fixture();
    let mut flat = plan
        .audio(AudioSample(0)..AudioSample(1), Default::default())
        .unwrap();
    let (base, work) = measurement(&flat);
    let old_spans = flat.spans.capacity();
    flat.spans.reserve_exact(7);
    let span_storage = (flat.spans.capacity() - old_spans) * size_of::<AudioSpan>();
    let span = &mut flat.spans[0];
    let old_origins = span.boundaries.start.capacity();
    let old_retimes = span.retimes.capacity();
    span.boundaries.start.reserve_exact(11);
    span.retimes.reserve_exact(13);
    let expected = span_storage
        + (span.boundaries.start.capacity() - old_origins) * size_of::<AudioBoundaryOrigin>()
        + (span.retimes.capacity() - old_retimes) * size_of::<AudioRetimeStage>();
    let (reserved, reserved_work) = measurement(&flat);
    assert_eq!(reserved - base, expected);
    assert_eq!(reserved_work, work);

    let mut fades = plan
        .audio_fades(AudioSample(0)..AudioSample(1), Default::default())
        .unwrap();
    let (base, work) = measurement(&fades);
    let old_spans = fades.spans.capacity();
    fades.spans.reserve_exact(7);
    let span_storage = (fades.spans.capacity() - old_spans) * size_of::<AudioFadeSpan>();
    let span = fades
        .spans
        .iter_mut()
        .find(|span| !span.start.is_empty() || !span.end.is_empty())
        .unwrap();
    let edges = if !span.start.is_empty() {
        &mut span.start
    } else {
        &mut span.end
    };
    let old_edges = edges.capacity();
    edges.reserve_exact(9);
    let edge_storage = (edges.capacity() - old_edges) * size_of::<AudioFadeEdge>();
    let edge = &mut edges[0];
    assert!(!edge.origins.is_empty());
    let old = edge.origins.capacity();
    edge.origins.reserve_exact(17);
    let expected = span_storage
        + edge_storage
        + (edge.origins.capacity() - old) * size_of::<AudioBoundaryOrigin>();
    let (reserved, reserved_work) = measurement(&fades);
    assert_eq!(reserved - base, expected);
    assert_eq!(reserved_work, work);
}

#[test]
fn policy_sample_labels_have_complete_scalar_storage_and_shared_identity_bounds() {
    let content = AudioContent::RoomTone {
        source: source(),
        duration: frames(16),
    };
    let mut policy = AudioPolicyQuery::<AudioSample> {
        suppressed: Vec::with_capacity(7),
        contents: vec![content.clone(), content],
        lookup: Default::default(),
        work: 99,
    };
    policy.suppressed.push(AudioSample(0)..AudioSample(1));
    let (two, work) = measurement(&policy);
    let removed = policy.contents.pop().unwrap();
    let (one, _) = measurement(&policy);
    let asset_allocation = Layout::new::<[usize; 2]>()
        .extend(Layout::for_value(source().asset.as_str()))
        .unwrap()
        .0
        .pad_to_align()
        .size();
    assert_eq!(
        two - one,
        asset_allocation,
        "shared identity charged at both owning edges"
    );
    assert!(matches!(removed, AudioContent::RoomTone { .. }));
    let old = policy.suppressed.capacity();
    let old_contents = policy.contents.capacity();
    let base = measurement(&policy);
    policy.suppressed.reserve_exact(19);
    policy.contents.reserve_exact(17);
    let reserved = measurement(&policy);
    assert_eq!(
        reserved.0 - base.0,
        (policy.suppressed.capacity() - old) * size_of::<Range<AudioSample>>()
            + (policy.contents.capacity() - old_contents) * size_of::<AudioContent>()
    );
    assert_eq!(reserved.1, base.1);
    assert!(work > base.1);
    exact_and_short(&policy);
    let signal = AudioPolicyQuery::<SignalSample> {
        suppressed: vec![SignalSample(-3)..SignalSample(4)],
        contents: policy.contents,
        lookup: Default::default(),
        work: 0,
    };
    exact_and_short(&signal);
    assert!(
        signal
            .measure_allocated_storage(1000)
            .into_parts()
            .0
            .unwrap()
            >= size_of::<Range<SignalSample>>()
    );
}

#[test]
fn projected_processing_observation_keeps_legacy_bounds_and_every_failure_debit() {
    let plan = fixture();
    let voice = plan
        .source_voice_occurrence(
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
        .unwrap();
    let query = voice
        .processing(voice.samples(), Default::default())
        .unwrap();
    assert!(matches!(
        query.spans[0].content,
        AudioSignalContent::ProjectedStage(_)
    ));
    let legacy = query.allocated_bytes_bound(100_000).unwrap();
    let (result, work) = query.measure_allocated_storage(100_000).into_parts();
    assert_eq!((result.unwrap(), work), legacy);
    for limit in 0..=work {
        let (result, spent) = query.measure_allocated_storage(limit).into_parts();
        assert!(spent <= limit);
        assert_eq!(result.is_ok(), limit == work);
    }
    assert!(
        legacy.0
            >= query.spans.capacity() * size_of::<AudioProcessingSpan<'_>>()
                + 2 * size_of::<AudioStageProjection<'_>>()
    );
}

#[test]
fn failed_core_route_inspection_keeps_its_actual_partial_debits() {
    let route = SoundRoute::identity(q(16)).unwrap();
    let core_work = route.allocated_bytes_bound(100).unwrap().1;
    let sound = AudioSoundRoute::<AudioSample>::new(
        route.clone(),
        vec![AudioSampleGrid::new(q(0), q(1), AudioBoundaryRule::RoundEven).unwrap()],
    )
    .unwrap();
    for core_limit in 0..core_work {
        let (core_result, core_spent) = route.measure_allocated_storage(core_limit).into_parts();
        assert!(core_result.is_err());
        let (result, spent) = measure_storage(&sound, core_limit + 1).into_parts();
        assert!(matches!(
            result,
            Err(PlanError::AudioQueryLimit("retained sound route heap"))
        ));
        assert_eq!(
            spent,
            core_spent + 1,
            "outer visit plus delegated successful debits"
        );
    }
}

#[test]
fn byte_and_depth_refusals_report_actual_successful_debits() {
    struct Overflow;
    impl OwnedHeap for Overflow {
        fn visit_heap(&self, budget: &mut HeapBudget) -> Result<(), PlanError> {
            budget.add(usize::MAX)?;
            budget.spend(3)?;
            budget.add(1)
        }
    }
    let (result, work) = measure_storage(&Overflow, 10).into_parts();
    assert!(matches!(
        result,
        Err(PlanError::AudioQueryLimit("retained audio heap bytes"))
    ));
    assert_eq!(work, 4);

    struct Deep(Option<Box<Deep>>);
    impl OwnedHeap for Deep {
        fn visit_heap(&self, budget: &mut HeapBudget) -> Result<(), PlanError> {
            budget.visit(&self.0)
        }
    }
    let deep = (0..MAX_DOCUMENT_DEPTH).fold(Deep(None), |child, _| Deep(Some(Box::new(child))));
    let (result, work) = measure_storage(&deep, 1000).into_parts();
    assert!(matches!(
        result,
        Err(PlanError::AudioQueryLimit("retained audio heap depth"))
    ));
    assert_eq!(work, MAX_DOCUMENT_DEPTH + 1);
}
