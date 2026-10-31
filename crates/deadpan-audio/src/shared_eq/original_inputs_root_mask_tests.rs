//! Active checked Root/physical-Domain suppression destinations and cache pressure.
use super::*;
use deadpan_core::{HoldAudio, HoldRecipe, HoldVideo};

const HOLD_FRAMES: u32 = 64;

struct OpsTrace;
impl Drop for OpsTrace {
    fn drop(&mut self) {
        point_suppression::ops::trace::disable();
    }
}

fn quiet_root() -> (Arc<RenderPlan>, Provider) {
    let (document, provider) = fixture_document_at_rate(FrameRate::new(48_000, 1).unwrap());
    let root = deadpan_core::NodeId::new("root").unwrap();
    let quiet = deadpan_core::NodeId::new("quiet").unwrap();
    let mut wire = serde_json::to_value(document).unwrap();
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (root, BeatNode::sequence("Root", vec![quiet.clone()])),
        (
            quiet,
            BeatNode::hold(
                "Quiet",
                HoldRecipe {
                    picture_context: None,
                    duration: FrameDuration::new(i64::from(HOLD_FRAMES)).unwrap(),
                    video: HoldVideo::Background,
                    audio: HoldAudio::Silence,
                },
            ),
        ),
    ]))
    .unwrap();
    let document = deadpan_core::ProjectDocument::from_json(&wire.to_string()).unwrap();
    (Arc::new(RenderPlan::compile(&document).unwrap()), provider)
}

fn read_root(
    renderer: &mut StageAudio,
    provider: &mut Provider,
    start: AudioSample,
    cancelled: &AtomicBool,
) -> Result<TimeMappedBlock, StageAudioError> {
    renderer.read(
        provider,
        start,
        HOLD_FRAMES,
        Duration::from_secs(10),
        cancelled,
    )
}

fn assert_mask_outlives_controller(
    observer: &point_suppression::Observer,
    ranges: &AudioSuppressionMask,
    expected: Range<AudioSample>,
) {
    assert_eq!(ranges.as_slice(), &[expected]);
    assert!(ranges.is_owned());
    assert_eq!(ranges.unaccounted_capacity(), 0);
    assert_eq!(
        observer.live_frames(),
        point_suppression::shell_frames().unwrap()
            + point_suppression::payload_frames(ranges.capacity()).unwrap()
    );
}

#[test]
fn checked_public_root_and_physical_domain_emission_match_c_and_escape_the_controller() {
    let (plan, mut provider) = quiet_root();
    let cancelled = AtomicBool::new(false);
    let expected_range = AudioSample(0)..AudioSample(i64::from(HOLD_FRAMES));

    let mut root_renderer = StageAudio::new(Arc::clone(&plan));
    let old_root = crate::stages::root_mask_request::with_legacy_root_masks(|| {
        read_root(
            &mut root_renderer,
            &mut provider,
            AudioSample(0),
            &cancelled,
        )
    })
    .unwrap();
    let root = read_root(
        &mut root_renderer,
        &mut provider,
        AudioSample(0),
        &cancelled,
    )
    .unwrap();
    assert_eq!(root.samples, old_root.samples);
    assert_eq!(root.suppressed.as_slice(), old_root.suppressed.as_slice());
    assert_eq!(root.suppressed.capacity(), old_root.suppressed.capacity());
    let root_observer = root_renderer.owned_suppression.observer().unwrap();
    assert_mask_outlives_controller(&root_observer, &root.suppressed, expected_range.clone());
    let root_bits = root
        .samples
        .iter()
        .map(|s| s.map(f32::to_bits))
        .collect::<Vec<_>>();
    drop(old_root);
    drop(provider);
    drop(root_renderer);
    assert_eq!(
        root_observer.live_frames(),
        point_suppression::shell_frames().unwrap()
            + point_suppression::payload_frames(root.suppressed.capacity()).unwrap()
    );
    assert_eq!(
        root.samples
            .iter()
            .map(|s| s.map(f32::to_bits))
            .collect::<Vec<_>>(),
        root_bits
    );
    assert_eq!(
        root.suppressed.as_slice(),
        std::slice::from_ref(&expected_range)
    );
    drop(root);
    assert_eq!(
        root_observer.live_frames(),
        point_suppression::shell_frames().unwrap()
    );

    let (plan, mut provider) = quiet_root();
    let domain = plan
        .audio_domain_at(AudioSample(0), Default::default())
        .unwrap();
    let mut domain_renderer = StageAudio::new(Arc::clone(&plan));
    let old_domain = crate::stages::root_mask_request::with_legacy_root_masks(|| {
        domain_renderer.read_domain(
            &mut provider,
            &domain,
            AudioSample(0),
            HOLD_FRAMES,
            Duration::from_secs(10),
            &cancelled,
        )
    })
    .unwrap();
    let physical = domain_renderer
        .read_domain(
            &mut provider,
            &domain,
            AudioSample(0),
            HOLD_FRAMES,
            Duration::from_secs(10),
            &cancelled,
        )
        .unwrap();
    assert_eq!(physical.samples, old_domain.samples);
    assert_eq!(
        physical.suppressed.as_slice(),
        old_domain.suppressed.as_slice()
    );
    // The public second merge keeps the funded allocation instead of copying len.
    assert_eq!(old_domain.suppressed.capacity(), 1);
    assert_eq!(physical.suppressed.capacity(), 2);
    let domain_observer = domain_renderer.owned_suppression.observer().unwrap();
    assert_mask_outlives_controller(&domain_observer, &physical.suppressed, expected_range);
    drop(old_domain);
    drop(provider);
    drop(domain_renderer);
    assert_eq!(
        domain_observer.live_frames(),
        point_suppression::shell_frames().unwrap()
            + point_suppression::payload_frames(physical.suppressed.capacity()).unwrap()
    );
    drop(physical);
    assert_eq!(
        domain_observer.live_frames(),
        point_suppression::shell_frames().unwrap()
    );
}

#[test]
fn held_root_mask_changes_measured_whole_request_resident_minimum_and_recovers() {
    fn attempt(cap: u32) -> bool {
        let (plan, mut provider) = quiet_root();
        let cancelled = AtomicBool::new(false);
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        let original_default = renderer.limits.maximum_resident_frames;
        let held = read_root(&mut renderer, &mut provider, AudioSample(0), &cancelled).unwrap();
        let observer = renderer.owned_suppression.observer().unwrap();
        assert!(held.suppressed.is_owned());
        let held_live = observer.live_frames();
        renderer.limits.maximum_resident_frames = cap;
        let result = read_root(&mut renderer, &mut provider, AudioSample(0), &cancelled);
        assert_eq!(renderer.limits.maximum_resident_frames, cap);
        assert_eq!(
            original_default,
            StageLimits::default().maximum_resident_frames
        );
        assert_eq!(provider.calls, 0);
        match result {
            Ok(next) => {
                assert_eq!(next.suppressed.as_slice(), held.suppressed.as_slice());
                assert!(next.suppressed.is_owned());
                assert_eq!(
                    observer.live_frames(),
                    held_live
                        + point_suppression::payload_frames(next.suppressed.capacity()).unwrap()
                );
                drop(next);
                assert_eq!(observer.live_frames(), held_live);
                drop(held);
                assert_eq!(
                    observer.live_frames(),
                    point_suppression::shell_frames().unwrap()
                );
                true
            }
            Err(StageAudioError::Limit(_)) => {
                assert_eq!(
                    observer.live_frames(),
                    held_live,
                    "failed whole request must not retain partial mask storage"
                );
                drop(held);
                assert_eq!(
                    observer.live_frames(),
                    point_suppression::shell_frames().unwrap()
                );
                false
            }
            Err(error) => panic!("unexpected whole-request failure at cap {cap}: {error:?}"),
        }
    }

    let default_cap = StageLimits::default().maximum_resident_frames;
    assert!(
        attempt(default_cap),
        "actual held-mask request must fit the existing default"
    );
    let mut low = 0;
    let mut high = default_cap;
    while low < high {
        let middle = low + (high - low) / 2;
        if attempt(middle) {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    let minimum = low;
    assert!(minimum > 0);
    assert!(
        attempt(minimum),
        "measured whole-request minimum must succeed"
    );
    assert!(
        !attempt(minimum - 1),
        "one frame below measured whole-request minimum must refuse"
    );

    let (plan, mut provider) = quiet_root();
    let cancelled = AtomicBool::new(false);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let held = read_root(&mut renderer, &mut provider, AudioSample(0), &cancelled).unwrap();
    let observer = renderer.owned_suppression.observer().unwrap();
    renderer.limits.maximum_resident_frames = minimum;
    let next = read_root(&mut renderer, &mut provider, AudioSample(0), &cancelled).unwrap();
    drop(next);
    let held_live = observer.live_frames();
    renderer.limits.maximum_resident_frames = minimum - 1;
    let refused = read_root(&mut renderer, &mut provider, AudioSample(0), &cancelled);
    assert!(matches!(refused, Err(StageAudioError::Limit(_))));
    assert_eq!(observer.live_frames(), held_live);
    drop(held);
    let recovered = read_root(&mut renderer, &mut provider, AudioSample(0), &cancelled).unwrap();
    assert_eq!(
        recovered.suppressed.as_slice(),
        &[AudioSample(0)..AudioSample(64)]
    );
    drop(recovered);
    assert_eq!(
        observer.live_frames(),
        point_suppression::shell_frames().unwrap()
    );
}
#[test]
fn a_held_root_mask_limits_warm_original_prediction_until_released() {
    let (plan, mut provider) = setup();
    let quiet_start = AudioSample(plan.audio_duration().unwrap().0 - i64::from(HOLD_FRAMES));
    let cancelled = AtomicBool::new(false);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let held = read_root(&mut renderer, &mut provider, quiet_start, &cancelled).unwrap();
    assert_eq!(
        held.suppressed.as_slice(),
        &[quiet_start..AudioSample(quiet_start.0 + i64::from(HOLD_FRAMES))]
    );
    let payload = point_suppression::payload_frames(held.suppressed.capacity()).unwrap();
    let base = renderer.owned_suppression.live_frames() - payload;

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
        panic!("actual WAV Original input must warm the optional cache")
    };
    let identity = Arc::as_ptr(&original);
    drop(original);
    let cached = renderer.cached_resident_frames();
    assert!(cached > 0);
    renderer.limits.maximum_resident_frames = u32::try_from(cached + base + 37).unwrap();
    let calls = provider.calls;
    let keys = renderer.original_inputs.keys().copied().collect::<Vec<_>>();
    assert!(
        renderer
            .try_cached_original_for_prediction(
                &mut provider,
                &mut graph,
                &Context::Root,
                (AudioSample(0)..AudioSample(64), 37),
                control,
                0,
            )
            .unwrap()
            .is_none()
    );
    assert_eq!(provider.calls, calls);
    assert_eq!(
        renderer.original_inputs.keys().copied().collect::<Vec<_>>(),
        keys
    );

    drop(held);
    assert_eq!(renderer.owned_suppression.live_frames(), base);
    let admitted = renderer
        .try_cached_original_for_prediction(
            &mut provider,
            &mut graph,
            &Context::Root,
            (AudioSample(0)..AudioSample(64), 37),
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
fn root_legacy_replay_leaves_c_point_policy_active() {
    let (plan, mut provider) = setup();
    let domain = point(&plan);
    let cancelled = AtomicBool::new(false);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let _root_legacy = crate::stages::root_mask_request::with_legacy_root_masks(|| {
        assert!(!crate::stages::point_policy_request::legacy());
        let root_start = AudioSample(plan.audio_duration().unwrap().0 - i64::from(HOLD_FRAMES));
        read_root(&mut renderer, &mut provider, root_start, &cancelled).unwrap();
        let point = renderer
            .read_point_domain(
                &mut provider,
                &domain,
                ReferenceSample(16),
                16,
                Duration::from_secs(10),
                &cancelled,
            )
            .unwrap();
        assert!(point.suppressed.is_owned(), "C Point policy remains active");
        point
    });
}

#[test]
fn actual_root_work_prefixes_refuse_without_retaining_partial_mask_storage() {
    fn one_read(prior_work: usize) -> (Result<ReadBlock, StageAudioError>, usize, usize) {
        let (plan, mut provider) = quiet_root();
        let mut renderer = StageAudio::new(plan);
        let cancelled = AtomicBool::new(false);
        let work = RefCell::new(ReadWork::default());
        work.borrow_mut().plan_work = prior_work;
        let _trace = OpsTrace;
        point_suppression::ops::trace::reset();
        let result = renderer.read_controlled(
            &mut provider,
            AudioSample(0),
            HOLD_FRAMES,
            control(&cancelled, &work),
            false,
            0,
        );
        let delta = work.borrow().plan_work - prior_work;
        let trace = serde_json::to_value(point_suppression::ops::trace::snapshot()).unwrap();
        let native_units = [
            "accepted_account_units",
            "accepted_scan_units",
            "accepted_map_units",
            "accepted_zero_units",
            "accepted_mark_units",
            "accepted_vector_units",
            "accepted_storage_units",
            "accepted_host_residency_units",
        ]
        .into_iter()
        .map(|field| trace["plan"][field].as_u64().unwrap() as usize)
        .sum();
        assert_eq!(trace["overflow"], false);
        if result.is_err()
            && let Some(observer) = renderer.owned_suppression.observer()
        {
            assert_eq!(
                observer.live_frames(),
                point_suppression::shell_frames().unwrap()
            );
        }
        assert_eq!(provider.calls, 0);
        (result, delta, native_units)
    }

    let (measured, full_units, full_native_units) = one_read(0);
    let output = measured.unwrap();
    assert_eq!(
        output.suppressed.as_slice(),
        &[AudioSample(0)..AudioSample(64)]
    );
    assert!(output.suppressed.is_owned());
    assert!(full_units > 0);
    assert!(full_native_units > 0);
    drop(output);

    for remaining in 0..=full_units {
        let prior = MAX_PLAN_WORK_PER_READ - remaining;
        let (result, accepted, native_units) = one_read(prior);
        assert!(
            accepted <= remaining,
            "remaining={remaining}, accepted={accepted}"
        );
        if remaining == full_units {
            assert!(result.is_ok());
            assert_eq!(accepted, full_units);
        } else {
            assert!(
                matches!(
                    result,
                    Err(StageAudioError::Limit(_))
                        | Err(StageAudioError::Plan(
                            deadpan_plan::PlanError::AudioQueryLimit(_)
                        ))
                ),
                "remaining={remaining}, error={:?}",
                result.as_ref().err()
            );
            assert!(native_units <= full_native_units);
        }
        drop(result);
    }

    let (plan, mut provider) = quiet_root();
    let mut renderer = StageAudio::new(plan);
    let cancelled = AtomicBool::new(true);
    let work = RefCell::new(ReadWork::default());
    let stopped = renderer.read_controlled(
        &mut provider,
        AudioSample(0),
        HOLD_FRAMES,
        control(&cancelled, &work),
        false,
        0,
    );
    assert!(matches!(stopped, Err(error) if error.is_cancelled()));
    assert_eq!(provider.calls, 0);
    assert_eq!(work.borrow().plan_work, 0);
    assert_eq!(renderer.owned_suppression.live_frames(), 0);

    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let expired = WorkControl {
        cancelled: &cancelled,
        deadline: Instant::now() - Duration::from_millis(1),
        work: &work,
    };
    let stopped = renderer.read_controlled(
        &mut provider,
        AudioSample(0),
        HOLD_FRAMES,
        expired,
        false,
        0,
    );
    assert!(matches!(stopped, Err(StageAudioError::Timeout)));
    assert_eq!(provider.calls, 0);
    assert_eq!(work.borrow().plan_work, 0);
    assert_eq!(renderer.owned_suppression.live_frames(), 0);
}
