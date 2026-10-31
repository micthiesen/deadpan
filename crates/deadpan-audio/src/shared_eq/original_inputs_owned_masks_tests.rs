//! Held public Point outputs participate in both Original cache admission paths.
use super::*;
use deadpan_core::{BeatNode, HoldAudio, HoldRecipe, HoldVideo};
use deadpan_plan::{AudioDefinitionSelector, AudioPointDomain, AudioRootPlacement};

#[path = "original_inputs_root_mask_tests.rs"]
mod root_mask_emission;

fn setup() -> (Arc<RenderPlan>, Provider) {
    let (document, provider) = fixture_document_at_rate(FrameRate::new(48_000, 1).unwrap());
    let mut wire = serde_json::to_value(document).unwrap();
    wire["nodes"]["quiet"] = serde_json::to_value(BeatNode::hold(
        "Silence",
        HoldRecipe {
            picture_context: None,
            duration: FrameDuration::new(64).unwrap(),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    ))
    .unwrap();
    wire["nodes"]["root"]["kind"]["children"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!("quiet"));
    let document = deadpan_core::ProjectDocument::from_json(&wire.to_string()).unwrap();
    (Arc::new(RenderPlan::compile(&document).unwrap()), provider)
}
fn point(plan: &RenderPlan) -> AudioPointDomain<'_> {
    let definition = plan
        .audio_definition(AudioDefinitionSelector::Node {
            node: deadpan_core::NodeId::new("quiet").unwrap(),
        })
        .unwrap();
    definition
        .in_point_clock(
            AudioRootPlacement::new(
                ExactRatio::ZERO,
                ExactRatio::ONE,
                ExactRatio::ZERO..definition.exact_duration(),
            )
            .unwrap(),
            ExactRatio::ZERO,
        )
        .unwrap()
}
fn mask(
    renderer: &mut StageAudio,
    provider: &mut Provider,
    point: &AudioPointDomain<'_>,
    cancelled: &AtomicBool,
) -> PointDomainAudioBlock {
    renderer
        .read_point_domain(
            provider,
            point,
            ReferenceSample(16),
            16,
            Duration::from_secs(10),
            cancelled,
        )
        .unwrap()
}
fn held(
    renderer: &mut StageAudio,
    provider: &mut Provider,
    point: &AudioPointDomain<'_>,
    cancelled: &AtomicBool,
) -> (PointDomainAudioBlock, u64) {
    let first = mask(renderer, provider, point, cancelled);
    let first_live = renderer.owned_suppression.live_frames();
    let second = mask(renderer, provider, point, cancelled);
    let payload = renderer.owned_suppression.live_frames() - first_live;
    assert!(payload > 0);
    drop(second);
    assert_eq!(renderer.owned_suppression.live_frames(), first_live);
    (first, first_live - payload)
}

#[test]
fn original_warm_prediction_respects_a_held_public_point_mask() {
    let (plan, mut provider) = setup();
    let point = point(&plan);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let cancelled = AtomicBool::new(false);
    let (held, base) = held(&mut renderer, &mut provider, &point, &cancelled);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let mut graph = graph(&plan);
    let original = renderer
        .shared_original_input(
            &mut provider,
            &mut graph,
            &Context::Root,
            AudioSample(0)..AudioSample(64),
            control,
            0,
        )
        .unwrap();
    let Read::Cached(original) = original else {
        panic!("actual WAV Original must enter the optional cache")
    };
    let identity = Arc::as_ptr(&original);
    drop(original);
    let cached = renderer.cached_resident_frames();
    assert!(cached > 0);
    assert_eq!(renderer.active_frames, 0);
    assert_eq!(work.borrow().sound_context_frames, 0);
    const ADDITIONAL: u64 = 37;
    renderer.limits.maximum_resident_frames = u32::try_from(cached + base + ADDITIONAL).unwrap();
    let calls = provider.calls;
    let cached_ids = renderer.original_inputs.keys().copied().collect::<Vec<_>>();
    let denied = renderer
        .try_cached_original_for_prediction(
            &mut provider,
            &mut graph,
            &Context::Root,
            (AudioSample(0)..AudioSample(64), ADDITIONAL),
            control,
            0,
        )
        .unwrap();
    assert!(denied.is_none());
    assert_eq!(provider.calls, calls);
    assert_eq!(
        renderer.original_inputs.keys().copied().collect::<Vec<_>>(),
        cached_ids
    );
    drop(held);
    assert_eq!(renderer.owned_suppression.live_frames(), base);
    let admitted = renderer
        .try_cached_original_for_prediction(
            &mut provider,
            &mut graph,
            &Context::Root,
            (AudioSample(0)..AudioSample(64), ADDITIONAL),
            control,
            0,
        )
        .unwrap()
        .unwrap();
    assert_eq!(Arc::as_ptr(&admitted), identity);
    assert!(provider.calls > calls);
    drop(admitted);
    graph.original_inputs.clear(&mut renderer);
}

#[test]
fn original_optional_publication_counts_the_public_mask_without_double_billing_cache() {
    let (plan, mut provider) = setup();
    let point = point(&plan);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let cancelled = AtomicBool::new(false);
    let (held, base) = held(&mut renderer, &mut provider, &point, &cancelled);
    let work = RefCell::new(ReadWork::default());
    let control = control(&cancelled, &work);
    let mut graph = graph(&plan);
    let samples = AudioSample(0)..AudioSample(64);
    let block = renderer
        .read_controlled(&mut provider, samples.start, 64, control, true, 0)
        .unwrap();
    let required = resident(&block).unwrap();
    renderer.limits.maximum_resident_frames = u32::try_from(required + base).unwrap();
    let key = LookupKey::new(&Context::Root, &samples, 0);
    let result = renderer
        .cache_original_input(
            &mut graph,
            &Context::Root,
            (samples.clone(), 0, key),
            block,
            control,
        )
        .unwrap();
    let Read::Fresh(block) = result else {
        panic!("held public mask must prevent optional retention")
    };
    assert!(renderer.original_inputs.is_empty());
    assert_eq!(renderer.cached_resident_frames(), 0);
    let expected = block.samples.clone();
    drop(held);
    assert_eq!(renderer.owned_suppression.live_frames(), base);
    let key = LookupKey::new(&Context::Root, &samples, 0);
    let result = renderer
        .cache_original_input(
            &mut graph,
            &Context::Root,
            (samples, 0, key),
            block,
            control,
        )
        .unwrap();
    let Read::Cached(block) = result else {
        panic!("released output charge must restore optional retention")
    };
    assert_eq!(block.samples, expected);
    assert_eq!(renderer.cached_resident_frames(), required);
    assert_eq!(renderer.owned_suppression.live_frames(), base);
    drop(block);
    graph.original_inputs.clear(&mut renderer);
    assert_eq!(renderer.cached_resident_frames(), 0);
    assert_eq!(renderer.owned_suppression.live_frames(), base);
}
