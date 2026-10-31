//! Real retained Point masks through Audio relabeling and Original publication.
use super::*;
use crate::stages::point_suppression;
use deadpan_core::*;
use deadpan_plan::{AudioBoundDomain, AudioOwnerInput, AudioOwnerRetainedInput};
use serde_json::{Value, json};

fn node_id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn span_frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn quiet() -> BeatNode {
    BeatNode::hold(
        "Silence",
        HoldRecipe {
            picture_context: None,
            duration: span_frames(16),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    )
}
fn preserve_node(child: &str, input: i64, output: i64) -> BeatNode {
    BeatNode {
        framing: None,
        label: "Preserve".into(),
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            child: node_id(child),
            duration: span_frames(output),
            mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(input)).unwrap(),
            pitch: PitchPolicy::Preserve,
            purpose: RetimePurpose::Edit,
        },
        cutaways: Vec::new(),
        captions: Vec::new(),
    }
}
fn source_node() -> BeatNode {
    let audio = SourceAudio {
        asset: AssetId::new("media").unwrap(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: 0,
                time_base: SourceTimeBase::new(1, 48_000).unwrap(),
            },
            SourceTimestamp {
                ticks: 48,
                time_base: SourceTimeBase::new(1, 48_000).unwrap(),
            },
        )
        .unwrap(),
    };
    BeatNode {
        framing: None,
        label: "Source".into(),
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Source {
            source: SourceNode {
                edit_window: None,
                duration: span_frames(48),
                video: SourceVideo::Blank,
                video_mapping: SourceVideoMapping::FitBeat,
                audio_mapping: SourceAudioMapping::natural_rate(
                    audio.span,
                    FrameRate::new(48_000, 1).unwrap(),
                )
                .unwrap(),
                audio: Some(audio),
                audio_offset: AudioSample(0),
                link: LinkRelation::Independent,
            },
        },
        cutaways: Vec::new(),
        captions: Vec::new(),
    }
}
fn checked_document(
    base: &ProjectDocument,
    root: &str,
    mut nodes: BTreeMap<NodeId, BeatNode>,
) -> Result<ProjectDocument, StageAudioError> {
    nodes.insert(
        node_id("root"),
        BeatNode::sequence("Root", vec![node_id(root)]),
    );
    let mut wire = serde_json::to_value(base).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    ProjectDocument::from_json(&wire.to_string())
        .map_err(|error| StageAudioError::from(PlanError::from(error)))
}
fn retained_fixture() -> Result<(Arc<RenderPlan>, Provider), StageAudioError> {
    let (base, provider) = fixture_document_at_rate(FrameRate::new(48_000, 1).unwrap());
    let nodes = || {
        BTreeMap::from([
            (node_id("h"), quiet()),
            (node_id("a"), source_node()),
            (
                node_id("group"),
                BeatNode::sequence("Group", vec![node_id("h"), node_id("a")]),
            ),
            (node_id("b"), preserve_node("group", 64, 128)),
        ])
    };
    let mut old_nodes = nodes();
    old_nodes.insert(node_id("clock"), preserve_node("b", 128, 256));
    let historical = checked_document(&base, "clock", old_nodes)?;
    let current = checked_document(&base, "b", nodes())?;
    let state = AudioBindingState::new(
        vec![AudioTimingRecord {
            id: AudioTimingId {
                allocation: RevisionId::new("timing").unwrap(),
                ordinal: 0,
            },
            layout: FrozenAudioLayout::capture(&historical).map_err(PlanError::from)?,
        }],
        BTreeMap::from([(
            node_id("b"),
            OwnedAudioBinding {
                reanchors: Vec::new(),
                lattice: AudioPlacementTemplate {
                    reference_local_offset: ExactRatio::ZERO,
                    gap_after: None,
                    reference: AudioReferenceClock {
                        recipe: AudioRecipeKind::Node,
                        timing: AudioTimingId {
                            allocation: RevisionId::new("timing").unwrap(),
                            ordinal: 0,
                        },
                        root: AudioClockRoot::PreserveInputPointCeil {
                            stage: node_id("clock"),
                        },
                        physical: node_id("b"),
                    },
                    arguments: Vec::new(),
                    births: Vec::new(),
                },
                resume: None,
            },
        )]),
    )
    .map_err(PlanError::from)?;
    let mut wire = serde_json::to_value(current).unwrap();
    wire["audio_bindings"] = serde_json::to_value(state).unwrap();
    let document = ProjectDocument::from_json(&wire.to_string())
        .map_err(|error| StageAudioError::from(PlanError::from(error)))?;
    Ok((Arc::new(RenderPlan::compile(&document)?), provider))
}
fn owner_input(plan: &RenderPlan) -> Result<AudioOwnerInput<'_>, StageAudioError> {
    let owners = plan.audio_gain_owners(AudioSample(8)..AudioSample(9), Default::default())?;
    let owner = owners
        .spans()
        .iter()
        .flat_map(|span| span.owners())
        .find(|owner| owner.instance().node == node_id("b"))
        .ok_or(PlanError::InvalidPlan(
            "checked retained b owner not reached",
        ))?;
    Ok(plan.audio_owner_input(owner, AudioSample(8), Default::default())?)
}
fn retained(plan: &RenderPlan) -> Result<AudioOwnerRetainedInput<'_>, StageAudioError> {
    let retained = owner_input(plan)?
        .retained_at(AudioSample(8), Default::default())?
        .ok_or(PlanError::InvalidPlan(
            "checked owner has no genuine retained operand",
        ))?;
    if !matches!(retained.domain(), AudioBoundDomain::Point(_)) {
        return Err(PlanError::InvalidPlan("checked retained operand did not reach Point").into());
    }
    Ok(retained)
}
fn trace() -> Value {
    serde_json::to_value(point_suppression::ops::trace::snapshot()).unwrap()
}
struct Trace;
impl Trace {
    fn begin() -> Self {
        point_suppression::ops::trace::reset();
        Self
    }
}
impl Drop for Trace {
    fn drop(&mut self) {
        point_suppression::ops::trace::disable();
    }
}
fn payload(capacity: usize) -> u64 {
    assert_eq!(
        std::alloc::Layout::new::<Range<AudioSample>>(),
        std::alloc::Layout::new::<Range<deadpan_plan::ReferenceSample>>(),
    );
    point_suppression::payload_frames(capacity).unwrap()
}
fn bits(samples: &[[f32; 2]]) -> Vec<[u32; 2]> {
    samples
        .iter()
        .map(|sample| sample.map(f32::to_bits))
        .collect()
}
fn mask_json(value: &AudioSuppressionMask) -> Value {
    json!({
        "ranges":value.as_slice(),"len":value.len(),"capacity":value.capacity(),
        "layout_size":std::alloc::Layout::array::<Range<AudioSample>>(value.capacity()).unwrap().size(),
        "layout_align":std::alloc::Layout::array::<Range<AudioSample>>(value.capacity()).unwrap().align(),
    })
}
fn cache_resident_without_mask(block: &ReadBlock) -> u64 {
    let bytes = block.samples.capacity() * std::mem::size_of::<[f32; 2]>()
        + block.exhausted.capacity() * std::mem::size_of::<Range<AudioSample>>()
        + block.dependencies.len() * 1024
        + std::mem::size_of::<ReadBlock>()
        + 2 * std::mem::size_of::<usize>()
        + 1024;
    u64::try_from(bytes).unwrap().div_ceil(8)
}

#[test]
fn active_public_retained_point_maps_owned_signal_to_audio_with_both_owners_live() {
    let _trace = Trace::begin();
    let (plan, mut provider) = retained_fixture().unwrap();
    let retained = retained(&plan).unwrap();
    assert!(retained.belongs_to(&plan));
    let start = AudioSample(7);
    let legacy = crate::stages::internal_mask_request::with_legacy_internal_masks(|| {
        let mut old_renderer = StageAudio::new(Arc::clone(&plan));
        let (_, mut old_provider) = retained_fixture().unwrap();
        old_renderer
            .read_retained_owner_input(
                &mut old_provider,
                &retained,
                start,
                32,
                Duration::from_secs(10),
                &AtomicBool::new(false),
            )
            .unwrap()
    });
    assert!(!legacy.suppressed.is_owned());
    let legacy_pcm = bits(&legacy.samples);
    let legacy_policy = legacy.suppressed.as_slice().to_vec();
    drop(legacy);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let raw = {
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        renderer
            .read_bound_at(
                retained.bound(),
                &mut provider,
                retained.reference_at(start).unwrap(),
                (32, 0),
                control(&cancelled, &work),
            )
            .unwrap()
    };
    assert!(raw.suppressed.is_owned());
    assert!(!raw.suppressed.is_empty());
    let observer = renderer.owned_suppression.observer().unwrap();
    let shell = point_suppression::shell_frames().unwrap();
    let raw_payload = payload(raw.suppressed.capacity());
    assert_eq!(observer.live_frames(), shell + raw_payload);
    let raw_mask = serde_json::to_value(&raw.suppressed).unwrap();
    let expected = raw
        .suppressed
        .iter()
        .map(|range| AudioSample(start.0 + range.start.0)..AudioSample(start.0 + range.end.0))
        .collect::<Vec<_>>();
    let calls = provider.calls;
    point_suppression::ops::trace::reset();
    let mapped = renderer
        .read_retained_owner_input(
            &mut provider,
            &retained,
            start,
            32,
            Duration::from_secs(10),
            &AtomicBool::new(false),
        )
        .unwrap();
    assert!(provider.calls > calls);
    assert_eq!(mapped.start, start);
    assert!(mapped.suppressed.is_owned());
    assert_eq!(mapped.suppressed.unaccounted_capacity(), 0);
    assert_eq!(mapped.suppressed.as_slice(), expected.as_slice());
    assert_eq!(bits(&mapped.samples), legacy_pcm);
    assert_eq!(mapped.suppressed.as_slice(), legacy_policy.as_slice());
    let mapped_payload = payload(mapped.suppressed.capacity());
    assert_eq!(observer.live_frames(), shell + raw_payload + mapped_payload);
    assert_eq!(serde_json::to_value(&raw.suppressed).unwrap(), raw_mask);
    let receipt = trace();
    assert_eq!(receipt["overflow"], false);
    assert!(receipt["plan"]["accepted_map_units"].as_u64().unwrap() > 0);
    assert!(
        receipt["plan"]["accepted_payload_reservations"]
            .as_u64()
            .unwrap()
            >= 2
    );
    drop(raw);
    assert_eq!(observer.live_frames(), shell + mapped_payload);
    let expected_mask = mask_json(&mapped.suppressed);
    let expected_bits = bits(&mapped.samples);
    let expected_metadata = json!({
        "schema_version":mapped.schema_version,"stage":mapped.stage,"engine":mapped.engine,
        "processing_order":mapped.processing_order,"project_id":mapped.project_id,
        "revision_id":mapped.revision_id,"start":mapped.start,
    });
    drop(provider);
    drop(renderer);
    assert_eq!(observer.live_frames(), shell + mapped_payload);
    assert_eq!(mask_json(&mapped.suppressed), expected_mask);
    assert_eq!(bits(&mapped.samples), expected_bits);
    assert_eq!(
        json!({
            "schema_version":mapped.schema_version,"stage":mapped.stage,"engine":mapped.engine,
            "processing_order":mapped.processing_order,"project_id":mapped.project_id,
            "revision_id":mapped.revision_id,"start":mapped.start,
        }),
        expected_metadata
    );
    drop(mapped);
    assert_eq!(observer.live_frames(), shell);
}

#[test]
fn active_retained_original_publication_excludes_owned_payload_and_respects_arc_pins() {
    let _trace = Trace::begin();
    let (plan, mut provider) = retained_fixture().unwrap();
    // This is the actual public retained handle's compatibility sharing factory.
    // Its ordinary metadata is setup, outside the owned-mask assertion boundary.
    let context = Context::Retained(retained(&plan).unwrap().into_shared());
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let mut graph = graph(&plan);
    let samples = AudioSample(7)..AudioSample(39);
    let first = renderer
        .shared_original_input(
            &mut provider,
            &mut graph,
            &context,
            samples.clone(),
            control,
            0,
        )
        .unwrap();
    let Read::Cached(first) = first else {
        panic!("genuine retained Point miss must publish the actual block")
    };
    assert!(provider.calls > 0);
    assert!(first.suppressed.is_owned());
    assert!(!first.suppressed.is_empty());
    assert_eq!(first.suppressed.unaccounted_capacity(), 0);
    assert_eq!(renderer.original_inputs.len(), 1);
    assert_eq!(graph.original_inputs.len, 1);
    let cached = renderer.original_inputs.values().next().unwrap();
    assert!(Arc::ptr_eq(&first, &cached.block));
    assert_eq!(Arc::strong_count(&first), 2);
    assert_eq!(cached.resident, resident(&first).unwrap());
    let expected_resident = cache_resident_without_mask(&first);
    assert_eq!(cached.resident, expected_resident);
    let mask_bytes = first.suppressed.capacity() * std::mem::size_of::<Range<AudioSample>>();
    assert_eq!(mask_bytes % 8, 0);
    assert!(mask_bytes > 0);
    let legacy_bytes = first.samples.capacity() * std::mem::size_of::<[f32; 2]>()
        + (first.suppressed.capacity() + first.exhausted.capacity())
            * std::mem::size_of::<Range<AudioSample>>()
        + first.dependencies.len() * 1024
        + std::mem::size_of::<ReadBlock>()
        + 2 * std::mem::size_of::<usize>()
        + 1024;
    let legacy_resident = u64::try_from(legacy_bytes).unwrap().div_ceil(8);
    assert_eq!(
        legacy_resident - resident(&first).unwrap(),
        u64::try_from(mask_bytes / 8).unwrap(),
    );
    let observer = renderer.owned_suppression.observer().unwrap();
    let shell = point_suppression::shell_frames().unwrap();
    let held = shell + payload(first.suppressed.capacity());
    assert_eq!(observer.live_frames(), held);
    let calls = provider.calls;
    let next = renderer.next_original_input;
    point_suppression::ops::trace::reset();
    let hit = renderer
        .shared_original_input(&mut provider, &mut graph, &context, samples, control, 0)
        .unwrap();
    let Read::Cached(hit) = hit else {
        panic!("same retained key must hit")
    };
    assert!(Arc::ptr_eq(&first, &hit));
    assert_eq!(Arc::strong_count(&first), 3);
    assert!(provider.calls > calls);
    assert_eq!(renderer.next_original_input, next);
    assert_eq!(observer.live_frames(), held);
    let receipt = trace();
    assert_eq!(receipt["overflow"], false);
    for lane in ["plan", "cache"] {
        for field in [
            "accepted_payload_reservations",
            "accepted_payload_frames",
            "accepted_vector_units",
            "accepted_storage_units",
            "accepted_map_units",
            "initialized_account_shells",
        ] {
            assert_eq!(receipt[lane][field], 0, "hit allocated mask: {receipt}");
        }
    }
    assert!(!renderer.evict_original_input(control).unwrap());
    assert_eq!(renderer.original_inputs.len(), 1);
    assert_eq!(observer.live_frames(), held);
    let total = renderer.cached_resident_frames()
        + renderer.active_frames
        + work.borrow().projected_resident_frames
        + work.borrow().sound_context_frames
        + held;
    assert!(total <= u64::from(renderer.limits.maximum_resident_frames));
    renderer.limits.maximum_resident_frames = u32::try_from(total).unwrap();
    assert!(
        renderer
            .original_prediction_reservation_fits(0, control)
            .unwrap()
    );
    renderer.limits.maximum_resident_frames -= 1;
    assert!(
        !renderer
            .original_prediction_reservation_fits(0, control)
            .unwrap()
    );
    drop(hit);
    drop(first);
    assert_eq!(observer.live_frames(), held);
    assert!(renderer.evict_original_input(control).unwrap());
    assert!(renderer.original_inputs.is_empty());
    assert_eq!(observer.live_frames(), shell);
    graph.original_inputs.clear(&mut renderer);
    assert_eq!(graph.original_inputs.len, 0);
    let prepared_resident: u64 = renderer
        .cache
        .iter()
        .map(|entry| entry.block.samples.resident_frames())
        .sum();
    assert!(prepared_resident > 0);
    assert_eq!(renderer.cached_resident_frames(), prepared_resident);
}

#[test]
fn active_original_clear_and_controller_drop_preserve_externally_pinned_owned_mask() {
    let _trace = Trace::begin();
    let (plan, mut provider) = retained_fixture().unwrap();
    let context = Context::Retained(retained(&plan).unwrap().into_shared());
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let mut graph = graph(&plan);
    let read = renderer
        .shared_original_input(
            &mut provider,
            &mut graph,
            &context,
            AudioSample(7)..AudioSample(39),
            control,
            0,
        )
        .unwrap();
    let Read::Cached(read) = read else {
        panic!("real retained block must be cached")
    };
    assert!(read.suppressed.is_owned());
    assert!(!read.suppressed.is_empty());
    assert_eq!(Arc::strong_count(&read), 2);
    let observer = renderer.owned_suppression.observer().unwrap();
    let shell = point_suppression::shell_frames().unwrap();
    let held = shell + payload(read.suppressed.capacity());
    assert_eq!(observer.live_frames(), held);
    let expected_bits = bits(&read.samples);
    let expected_mask = mask_json(&read.suppressed);
    graph.original_inputs.clear(&mut renderer);
    assert_eq!(Arc::strong_count(&read), 1);
    assert!(renderer.original_inputs.is_empty());
    let prepared_resident: u64 = renderer
        .cache
        .iter()
        .map(|entry| entry.block.samples.resident_frames())
        .sum();
    assert!(prepared_resident > 0);
    assert_eq!(renderer.cached_resident_frames(), prepared_resident);
    assert_eq!(graph.original_inputs.len, 0);
    assert_eq!(observer.live_frames(), held);
    drop(graph);
    drop(context);
    drop(work);
    drop(provider);
    drop(renderer);
    assert_eq!(observer.live_frames(), held);
    assert_eq!(bits(&read.samples), expected_bits);
    assert_eq!(mask_json(&read.suppressed), expected_mask);
    assert_eq!(read.suppressed.unaccounted_capacity(), 0);
    drop(read);
    assert_eq!(observer.live_frames(), shell);
}
