//! Actual public Point outputs retain their destination storage after a read.
use super::*;
use crate::stages::point_suppression;
use deadpan_plan::{
    AudioDefinitionSelector, AudioPointDomain, AudioRootPlacement, ReferenceSample,
};

struct PointProvider {
    prepared: crate::PreparedSource,
    calls: usize,
}
impl AudioSourceProvider for PointProvider {
    fn source(
        &mut self,
        _: &ProjectId,
        _: &RevisionId,
        asset: &AssetId,
        cancelled: &AtomicBool,
    ) -> Result<&crate::PreparedSource, PreparationError> {
        assert_eq!(asset.as_str(), "media");
        check_cancel(cancelled)?;
        self.calls += 1;
        Ok(&self.prepared)
    }
}
fn setup(silent: bool) -> (Arc<RenderPlan>, PointProvider) {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let (document, _) = document(
        rate,
        &["a"],
        vec![(
            "a",
            if silent {
                hold(64)
            } else {
                source(rate, 0, 64, 64)
            },
        )],
    );
    (
        Arc::new(RenderPlan::compile(&document).unwrap()),
        PointProvider {
            prepared: cases::prepared(cases::Wav::Stereo48000),
            calls: 0,
        },
    )
}
fn domain(plan: &RenderPlan) -> AudioPointDomain<'_> {
    let definition = plan
        .audio_definition(AudioDefinitionSelector::Node { node: id("a") })
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
fn read(
    renderer: &mut StageAudio,
    provider: &mut PointProvider,
    domain: &AudioPointDomain<'_>,
) -> Result<PointDomainAudioBlock, StageAudioError> {
    renderer.read_point_domain(
        provider,
        domain,
        ReferenceSample(16),
        16,
        TIMEOUT,
        &AtomicBool::new(false),
    )
}

#[test]
fn public_point_mask_keeps_values_and_charge_beyond_request_and_controller() {
    let (plan, mut provider) = setup(true);
    let domain = domain(&plan);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let expected = point_suppression::with_legacy_point_suppression(|| {
        read(&mut renderer, &mut provider, &domain)
    })
    .unwrap();
    assert_eq!(renderer.owned_suppression.live_frames(), 0);
    let expected_json = serde_json::to_value(&expected).unwrap();
    let actual = read(&mut renderer, &mut provider, &domain).unwrap();
    assert_eq!(serde_json::to_value(&actual).unwrap(), expected_json);
    assert_eq!(
        actual.suppressed,
        vec![ReferenceSample(16)..ReferenceSample(32)]
    );
    let observer = renderer.owned_suppression.observer().unwrap();
    let held = observer.live_frames();
    assert_eq!(
        held,
        point_suppression::shell_frames().unwrap()
            + point_suppression::payload_frames(actual.suppressed.capacity()).unwrap()
    );
    assert_eq!(provider.calls, 0);
    drop(provider);
    drop(renderer);
    assert_eq!(observer.live_frames(), held);
    assert_eq!(serde_json::to_value(&actual).unwrap(), expected_json);
    drop(actual);
    assert_eq!(
        observer.live_frames(),
        point_suppression::shell_frames().unwrap()
    );
}

#[test]
fn public_point_source_empty_mask_creates_no_account() {
    let (plan, mut provider) = setup(false);
    let domain = domain(&plan);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let result = read(&mut renderer, &mut provider, &domain).unwrap();
    assert!(provider.calls > 0);
    assert!(result.suppressed.is_empty());
    assert_eq!(result.suppressed.capacity(), 0);
    assert_eq!(renderer.owned_suppression.live_frames(), 0);
    assert!(renderer.owned_suppression.observer().is_none());
}

#[test]
fn held_public_point_mask_constrains_later_request_until_its_storage_drops() {
    let (plan, mut provider) = setup(true);
    let domain = domain(&plan);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let initial = read(&mut renderer, &mut provider, &domain).unwrap();
    drop(initial);
    let base = renderer.owned_suppression.live_frames();
    assert_eq!(base, point_suppression::shell_frames().unwrap());
    let mut successful = None;
    for cap in u32::try_from(base).unwrap()..=512 {
        renderer.limits.maximum_resident_frames = cap;
        if let Ok(value) = read(&mut renderer, &mut provider, &domain) {
            successful = Some(value);
            break;
        }
    }
    let held = successful.expect("bounded SilentHold Point must fit an existing production cap");
    assert!(renderer.owned_suppression.live_frames() > base);
    assert!(matches!(
        read(&mut renderer, &mut provider, &domain),
        Err(StageAudioError::Limit(_))
    ));
    drop(held);
    assert_eq!(renderer.owned_suppression.live_frames(), base);
    let recovered = read(&mut renderer, &mut provider, &domain).unwrap();
    assert_eq!(
        recovered.suppressed,
        vec![ReferenceSample(16)..ReferenceSample(32)]
    );
    drop(recovered);
    assert_eq!(renderer.owned_suppression.live_frames(), base);
}

#[test]
fn point_mask_constructor_work_refusals_release_payload_without_refunds() {
    fn receipt() -> (Value, usize) {
        let value = serde_json::to_value(point_suppression::work_trace::snapshot()).unwrap();
        assert_eq!(value["overflow"], false);
        assert_eq!(value["constructor_calls"], 1);
        let accepted = [
            "accepted_account_allocation_units",
            "accepted_vector_allocation_units",
            "accepted_storage_allocation_units",
            "accepted_range_mapping_units",
            "accepted_residency_units",
        ]
        .into_iter()
        .map(|field| usize::try_from(value[field].as_u64().unwrap()).unwrap())
        .sum();
        (value, accepted)
    }
    let (plan, _) = setup(true);
    let domain = domain(&plan);
    let cancelled = AtomicBool::new(false);
    let ranges = || {
        vec![
            SignalSample(1)..SignalSample(2),
            SignalSample(3)..SignalSample(4),
        ]
    };
    let measured = RefCell::new(ReadWork::default());
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    point_suppression::work_trace::reset();
    let result = renderer
        .admit_point_suppression(ranges(), &domain, control(&cancelled, &measured))
        .unwrap();
    let full_units = measured.borrow().plan_work;
    assert_eq!(full_units, 6);
    let (trace, accepted) = receipt();
    assert_eq!(accepted, full_units);
    assert_eq!(trace["completed"], 1);
    assert_eq!(trace["failed"], 0);
    point_suppression::work_trace::disable();
    drop(result);
    for remaining in 0..=full_units {
        let work = RefCell::new(ReadWork::default());
        let prior = MAX_PLAN_WORK_PER_READ - remaining;
        work.borrow_mut().plan_work = prior;
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        point_suppression::work_trace::reset();
        let result =
            renderer.admit_point_suppression(ranges(), &domain, control(&cancelled, &work));
        if remaining == full_units {
            assert!(result.is_ok());
        } else {
            assert!(matches!(result, Err(StageAudioError::Limit(_))));
        }
        let accepted = work.borrow().plan_work;
        assert_eq!(accepted - prior, remaining);
        let (trace, receipt_units) = receipt();
        assert_eq!(receipt_units, accepted - prior);
        for (field, units) in [
            (
                "accepted_account_allocation_units",
                usize::from(remaining >= 1),
            ),
            (
                "accepted_vector_allocation_units",
                usize::from(remaining >= 2),
            ),
            ("accepted_residency_units", usize::from(remaining >= 3)),
            (
                "accepted_range_mapping_units",
                remaining.saturating_sub(3).min(2),
            ),
            (
                "accepted_storage_allocation_units",
                usize::from(remaining >= 6),
            ),
        ] {
            assert_eq!(
                trace[field], units,
                "remaining={remaining}, receipt={trace}"
            );
        }
        assert_eq!(trace["completed"], usize::from(remaining == full_units));
        assert_eq!(trace["failed"], usize::from(remaining != full_units));
        point_suppression::work_trace::disable();
        drop(result);
        if let Some(observer) = renderer.owned_suppression.observer() {
            let base = observer.live_frames();
            assert_eq!(base, point_suppression::shell_frames().unwrap());
            renderer.limits.maximum_resident_frames = u32::try_from(base).unwrap();
            let fresh = RefCell::new(ReadWork::default());
            assert!(
                renderer
                    .make_room(0, false, control(&cancelled, &fresh))
                    .is_ok()
            );
            assert!(matches!(
                renderer.make_room(1, false, control(&cancelled, &fresh)),
                Err(StageAudioError::Limit(_))
            ));
        }
        assert_eq!(work.borrow().plan_work, accepted);
        assert_eq!(work.borrow().sound_context_frames, 0);
    }
}

#[test]
fn failed_point_mapping_and_initial_stop_leave_no_payload_charge() {
    let (plan, _) = setup(true);
    let domain = domain(&plan);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let cancelled = AtomicBool::new(true);
    let work = RefCell::new(ReadWork::default());
    let result = renderer.admit_point_suppression(
        vec![SignalSample(1)..SignalSample(2)],
        &domain,
        control(&cancelled, &work),
    );
    assert!(result.unwrap_err().is_cancelled());
    assert_eq!(renderer.owned_suppression.live_frames(), 0);
    cancelled.store(false, Ordering::Relaxed);
    point_suppression::work_trace::reset();
    let result = renderer.admit_point_suppression(
        vec![
            SignalSample(1)..SignalSample(2),
            SignalSample(64)..SignalSample(65),
        ],
        &domain,
        control(&cancelled, &work),
    );
    assert!(matches!(
        result,
        Err(StageAudioError::Plan(PlanError::AudioRangeOutOfRange))
    ));
    let trace = serde_json::to_value(point_suppression::work_trace::snapshot()).unwrap();
    point_suppression::work_trace::disable();
    assert_eq!(trace["accepted_storage_allocation_units"], 0);
    assert_eq!(trace["accepted_range_mapping_units"], 2);
    assert_eq!(trace["failed"], 1);
    let observer = renderer.owned_suppression.observer().unwrap();
    let base = observer.live_frames();
    assert_eq!(base, point_suppression::shell_frames().unwrap());
    renderer.limits.maximum_resident_frames = u32::try_from(base).unwrap();
    assert!(
        renderer
            .make_room(0, false, control(&cancelled, &work))
            .is_ok()
    );
    assert!(matches!(
        renderer.make_room(1, false, control(&cancelled, &work)),
        Err(StageAudioError::Limit(_))
    ));
}

#[test]
fn public_point_masks_can_drop_concurrently_after_controller() {
    let (plan, mut provider) = setup(true);
    let domain = domain(&plan);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let first = read(&mut renderer, &mut provider, &domain).unwrap();
    let first_live = renderer.owned_suppression.live_frames();
    let second = read(&mut renderer, &mut provider, &domain).unwrap();
    let observer = renderer.owned_suppression.observer().unwrap();
    assert!(observer.live_frames() > first_live);
    drop(renderer);
    let barrier = Arc::new(std::sync::Barrier::new(3));
    let workers = [first, second].map(|block| {
        let barrier = Arc::clone(&barrier);
        std::thread::spawn(move || {
            barrier.wait();
            drop(block);
        })
    });
    barrier.wait();
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(
        observer.live_frames(),
        point_suppression::shell_frames().unwrap()
    );
}
