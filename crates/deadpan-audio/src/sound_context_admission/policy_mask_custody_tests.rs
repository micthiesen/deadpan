//! Child of sound_context_admission::policy_mask_handoff, separate from primitive tests.
use super::*;
use crate::stages::point_suppression::{self, ops};
use deadpan_core::{
    BeatNode, ExactRatio, FrameDuration, FrameRate, HoldAudio, HoldRecipe, HoldVideo, NodeId,
    ProjectDocument,
};
use deadpan_plan::{
    AudioDefinitionSelector, AudioPointDomain, AudioQueryLimits, AudioRootPlacement, RenderPlan,
};
use std::collections::BTreeMap;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

const SHELL: u64 = 3;
// Pinned Storage<SignalSample> layout is 40 bytes / 8-byte frame, independently
// covered by point_suppression's layout tests; the final Vec is charged separately.
const STORAGE: u64 = 5;

fn fixture() -> Arc<RenderPlan> {
    let (base, _provider) = crate::stages::controlled_reads::fixture_document_at_rate(
        FrameRate::new(48_000, 1).unwrap(),
    );
    let id = |value: &str| NodeId::new(value).unwrap();
    let mut wire = serde_json::to_value(base).unwrap();
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (id("root"), BeatNode::sequence("Root", vec![id("quiet")])),
        (
            id("quiet"),
            BeatNode::hold(
                "Quiet",
                HoldRecipe {
                    picture_context: None,
                    duration: FrameDuration::new(64).unwrap(),
                    video: HoldVideo::Background,
                    audio: HoldAudio::Silence,
                },
            ),
        ),
    ]))
    .unwrap();
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    Arc::new(RenderPlan::compile(&document).unwrap())
}
fn point(plan: &RenderPlan) -> AudioPointDomain<'_> {
    let definition = plan
        .audio_definition(AudioDefinitionSelector::Node {
            node: NodeId::new("quiet").unwrap(),
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
fn control<'w>(work: &'w RefCell<ReadWork>, cancelled: &'w AtomicBool) -> WorkControl<'w> {
    WorkControl {
        cancelled,
        deadline: Instant::now() + Duration::from_secs(30),
        work,
    }
}
fn query<'w>(
    point: &AudioPointDomain<'_>,
    renderer: &mut StageAudio,
    control: WorkControl<'w>,
    empty: bool,
) -> deadpan_plan::AdmittedSignalPolicyQuery<Issuer<'w>, Lease<'w>> {
    point
        .policy_admitted(
            SignalSample(0)..SignalSample(if empty { 0 } else { 64 }),
            AudioQueryLimits::default(),
            &mut Admission::new(renderer, control),
        )
        .unwrap()
}
fn held_owner(renderer: &mut StageAudio, control: WorkControl<'_>) -> SignalSuppressionMask {
    let mut context = renderer.masks(control);
    let mut builder = ops::begin_ranges::<SignalSample, _>(&mut context, 1).unwrap();
    builder
        .push(SignalSample(-8)..SignalSample(-2), &context)
        .unwrap();
    builder.finish(&context).unwrap()
}

#[test]
fn actual_factory_native_handoff_keeps_exact_vec_and_outlives_all_request_owners() {
    let plan = fixture();
    let point = point(&plan);
    let (mask, observer) = {
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        let cancelled = AtomicBool::new(false);
        let work = RefCell::new(ReadWork::default());
        let control = control(&work, &cancelled);
        // Construction and consumption use different Admission values with the same issuer.
        let query = query(&point, &mut renderer, control, false);
        let pointer = query.view().suppressed.as_ptr();
        let values = query.view().suppressed.to_vec();
        assert_eq!(values, [SignalSample(0)..SignalSample(64)]);
        assert!(query.was_forwarded());
        assert!(work.borrow().sound_context_frames > 2);
        let mask = query
            .into_suppression(&mut Admission::new(&mut renderer, control))
            .unwrap();
        assert_eq!(mask.as_slice().as_ptr(), pointer);
        assert_eq!(mask.as_slice(), values);
        assert_eq!(mask.len(), 1);
        assert_eq!(mask.capacity(), 1);
        assert!(mask.is_owned());
        assert_eq!(work.borrow().sound_context_frames, 0);
        let observer = renderer.owned_suppression.observer().unwrap();
        assert_eq!(observer.live_frames(), SHELL + STORAGE + 2);
        (mask, observer)
    };
    assert_eq!(mask.as_slice(), [SignalSample(0)..SignalSample(64)]);
    assert_eq!(observer.live_frames(), SHELL + STORAGE + 2);
    drop(mask);
    assert_eq!(observer.live_frames(), SHELL);
}

#[derive(Clone, Copy)]
struct Observation {
    capacity: usize,
    lease_frames: u64,
    ledger_before: u64,
    ledger_prepared: u64,
    native_before: u64,
    native_prepared: u64,
    native_after_other_drop: u64,
}
// This test backend delegates all Plan accounting and native commit. For its
// positive prepare it calls the same native allocation/preflight primitives,
// allowing observations before a fresh final exclusive preflight. It does not
// claim to wrap Admission::prepare_signal_mask transparently.
struct Observing<'r, 'w> {
    inner: Admission<'r, 'w>,
    observation: Option<Observation>,
    other_owner: Option<SignalSuppressionMask>,
    cancel_after_preparation: bool,
}
impl<'w> AudioSoundContextAdmission for Observing<'_, 'w> {
    type Error = StageAudioError;
    type Issuer = Issuer<'w>;
    type Lease = Lease<'w>;
    fn issuer(&self) -> Self::Issuer {
        self.inner.issuer()
    }
    fn spend(&mut self, units: usize) -> Result<(), Self::Error> {
        self.inner.spend(units)
    }
    fn reserve(&mut self, layout: Layout) -> Result<Self::Lease, Self::Error> {
        self.inner.reserve(layout)
    }
    fn merge(&mut self, target: &mut Self::Lease, source: &mut Self::Lease) {
        self.inner.merge(target, source);
    }
}
impl<'w> AudioSignalPolicyMaskHandoff for Observing<'_, 'w> {
    type PreparedMask = PreparedPolicyMask<'w>;
    type Mask = SignalSuppressionMask;
    fn prepare_signal_mask(
        &mut self,
        source: SignalPolicyMaskRef<'_, Self::Issuer, Self::Lease>,
    ) -> Result<Self::PreparedMask, Self::Error> {
        if source.capacity() == 0 {
            return self.inner.prepare_signal_mask(source);
        }
        let control = self.inner.control;
        control.check()?;
        let current = self.inner.issuer();
        let capacity = source.capacity();
        let lease = source.lease().unwrap();
        let vector_frames = point_suppression::policy_handoff::vector_frames(capacity)?;
        // Observe the real borrowed source, never manufacture descriptor/lease fields.
        preflight_source_lease(
            control,
            source.issuer(),
            current,
            capacity,
            source.lease(),
            lease,
            vector_frames,
        )?;
        let ledger_before = control.work.borrow().sound_context_frames;
        let native_before = self.inner.controller.owned_suppression.live_frames();
        let destination = point_suppression::policy_handoff::prepare_policy_mask(
            self.inner.controller,
            capacity,
            control,
        )?;
        let ledger_prepared = control.work.borrow().sound_context_frames;
        let native_prepared = self.inner.controller.owned_suppression.live_frames();
        if let Some(other_owner) = self.other_owner.take() {
            // A genuine account-owned Vec+Box is released on another thread
            // after destination preparation and before the final preflight.
            std::thread::spawn(move || drop(other_owner))
                .join()
                .unwrap();
        }
        let native_after_other_drop = self.inner.controller.owned_suppression.live_frames();
        self.observation = Some(Observation {
            capacity,
            lease_frames: lease.frames(),
            ledger_before,
            ledger_prepared,
            native_before,
            native_prepared,
            native_after_other_drop,
        });
        if self.cancel_after_preparation {
            control.cancelled.store(true, Ordering::Relaxed);
        }
        control.check()?;
        assert_eq!(destination.vector_frames(), vector_frames);
        // Nothing observable or fallible may intervene after this exact helper.
        let remaining = preflight_source_lease(
            control,
            source.issuer(),
            current,
            capacity,
            source.lease(),
            lease,
            vector_frames,
        )?;
        Ok(PreparedPolicyMask::Transfer {
            destination,
            ledger: control.work,
            remaining,
        })
    }
    fn commit_signal_mask(
        &mut self,
        prepared: Self::PreparedMask,
        ranges: Vec<Range<SignalSample>>,
        source_lease: Option<&mut Self::Lease>,
    ) -> Self::Mask {
        self.inner
            .commit_signal_mask(prepared, ranges, source_lease)
    }
}

#[test]
fn actual_factory_keeps_all_plan_guards_during_box_only_prepare_and_other_owner_drop() {
    let plan = fixture();
    let point = point(&plan);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let control = control(&work, &cancelled);
    let held = held_owner(&mut renderer, control);
    let observer = renderer.owned_suppression.observer().unwrap();
    let query = query(&point, &mut renderer, control, false);
    let pointer = query.view().suppressed.as_ptr();
    let source_len = query.view().suppressed.len();
    let ledger_before = work.borrow().sound_context_frames;
    let mut backend = Observing {
        inner: Admission::new(&mut renderer, control),
        observation: None,
        other_owner: Some(held),
        cancel_after_preparation: false,
    };
    let mask = query.into_suppression(&mut backend).unwrap();
    let observed = backend.observation.unwrap();
    assert_eq!(observed.capacity, 1);
    assert_eq!(observed.lease_frames, 2);
    assert_eq!(observed.ledger_before, ledger_before);
    assert!(ledger_before > observed.lease_frames);
    assert_eq!(observed.ledger_prepared, ledger_before);
    assert_eq!(observed.native_before, SHELL + STORAGE + 2);
    assert_eq!(observed.native_prepared, observed.native_before + STORAGE);
    assert_eq!(
        observed.native_after_other_drop,
        observed.native_prepared - STORAGE - 2
    );
    drop(backend);
    assert_eq!(mask.as_slice().as_ptr(), pointer);
    assert_eq!(mask.len(), source_len);
    assert_eq!(mask.capacity(), observed.capacity);
    assert_eq!(observer.live_frames(), SHELL + STORAGE + 2);
    assert_eq!(work.borrow().sound_context_frames, 0);
    drop(renderer);
    drop(mask);
    assert_eq!(observer.live_frames(), SHELL);
}

#[test]
fn actual_capacity_zero_factory_uses_no_account_or_storage() {
    let plan = fixture();
    let point = point(&plan);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let control = control(&work, &cancelled);
    let query = query(&point, &mut renderer, control, true);
    let pointer = query.view().suppressed.as_ptr();
    assert!(query.view().suppressed.is_empty());
    let before = work.borrow().plan_work;
    let mask = query
        .into_suppression(&mut Admission::new(&mut renderer, control))
        .unwrap();
    assert_eq!(mask.as_slice().as_ptr(), pointer);
    assert_eq!(mask.capacity(), 0);
    assert!(!mask.is_owned());
    assert_eq!(work.borrow().plan_work, before);
    assert_eq!(work.borrow().sound_context_frames, 0);
    assert!(renderer.owned_suppression.observer().is_none());
}

#[test]
fn actual_factory_entry_cancel_deadline_and_work_refusals_release_query_guards() {
    for refusal in 0..3 {
        let plan = fixture();
        let point = point(&plan);
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let initial = control(&work, &cancelled);
        let query = query(&point, &mut renderer, initial, false);
        let mut control = initial;
        match refusal {
            0 => cancelled.store(true, Ordering::Relaxed),
            1 => control.deadline = Instant::now() - Duration::from_secs(1),
            2 => {
                let remaining = crate::stages::MAX_PLAN_WORK_PER_READ - work.borrow().plan_work;
                control.spend_plan_work(remaining).unwrap();
            }
            _ => unreachable!(),
        }
        let result = query.into_suppression(&mut Admission::new(&mut renderer, control));
        match (refusal, &result) {
            (0, Err(error)) => assert!(error.is_cancelled()),
            (1, Err(StageAudioError::Timeout)) => {}
            (2, Err(StageAudioError::Limit("plan work per read"))) => {}
            _ => panic!("unexpected entry refusal"),
        }
        assert_eq!(work.borrow().sound_context_frames, 0);
        assert_eq!(renderer.owned_suppression.live_frames(), 0);
    }
}

#[test]
fn actual_factory_all_small_prepare_work_prefixes_and_exact_resident_ceiling() {
    // Measure actual accepted native preparation work, then sweep only that
    // short observed prefix. No canonical query fee or cap is rewritten.
    let plan = fixture();
    let point = point(&plan);
    let complete_units = {
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let control = control(&work, &cancelled);
        let query = query(&point, &mut renderer, control, false);
        let before = work.borrow().plan_work;
        let mask = query
            .into_suppression(&mut Admission::new(&mut renderer, control))
            .unwrap();
        let units = work.borrow().plan_work - before;
        assert!((1..=8).contains(&units), "bounded preparation prefix");
        drop(mask);
        units
    };
    for prefix in 0..complete_units {
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let control = control(&work, &cancelled);
        let query = query(&point, &mut renderer, control, false);
        let fill = crate::stages::MAX_PLAN_WORK_PER_READ - work.borrow().plan_work - prefix;
        control.spend_plan_work(fill).unwrap();
        assert!(
            query
                .into_suppression(&mut Admission::new(&mut renderer, control))
                .is_err()
        );
        assert_eq!(
            work.borrow().plan_work,
            crate::stages::MAX_PLAN_WORK_PER_READ
        );
        assert_eq!(work.borrow().sound_context_frames, 0);
        assert!(matches!(
            renderer.owned_suppression.live_frames(),
            0 | SHELL
        ));
    }
    for extra in [SHELL - 1, SHELL + STORAGE - 1, SHELL + STORAGE] {
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let control = control(&work, &cancelled);
        let query = query(&point, &mut renderer, control, false);
        let source_live = work.borrow().sound_context_frames;
        renderer.limits.maximum_resident_frames = u32::try_from(source_live + extra).unwrap();
        let result = query.into_suppression(&mut Admission::new(&mut renderer, control));
        assert_eq!(result.is_ok(), extra == SHELL + STORAGE);
        assert_eq!(work.borrow().sound_context_frames, 0);
        drop(result);
        assert!(matches!(
            renderer.owned_suppression.live_frames(),
            0 | SHELL
        ));
    }
}

#[test]
fn cancellation_after_real_box_preparation_releases_both_owners() {
    let plan = fixture();
    let point = point(&plan);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let control = control(&work, &cancelled);
    let query = query(&point, &mut renderer, control, false);
    let mut backend = Observing {
        inner: Admission::new(&mut renderer, control),
        observation: None,
        other_owner: None,
        cancel_after_preparation: true,
    };
    assert!(query.into_suppression(&mut backend).is_err());
    let observed = backend.observation.unwrap();
    assert_eq!(observed.ledger_prepared, observed.ledger_before);
    assert_eq!(observed.native_prepared, SHELL + STORAGE);
    drop(backend);
    assert_eq!(work.borrow().sound_context_frames, 0);
    assert_eq!(renderer.owned_suppression.live_frames(), SHELL);
}

#[test]
fn positive_capacity_empty_uses_real_lease_and_retains_its_exact_empty_vec() {
    // Lower native primitive, not a factory-produced len0/cap2 descriptor.
    let plan = fixture();
    let mut renderer = StageAudio::new(plan);
    let work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let control = control(&work, &cancelled);
    let mut lease = Admission::new(&mut renderer, control)
        .reserve(Layout::array::<Range<SignalSample>>(2).unwrap())
        .unwrap();
    let ranges = Vec::with_capacity(2);
    let pointer = ranges.as_ptr();
    let capacity = ranges.capacity();
    let destination =
        point_suppression::policy_handoff::prepare_policy_mask(&mut renderer, capacity, control)
            .unwrap();
    assert_eq!(work.borrow().sound_context_frames, 4);
    assert_eq!(renderer.owned_suppression.live_frames(), SHELL + STORAGE);
    let remaining = preflight_source_lease(
        control,
        issuer(control),
        issuer(control),
        capacity,
        Some(&lease),
        &lease,
        point_suppression::policy_handoff::vector_frames(capacity).unwrap(),
    )
    .unwrap();
    let mask = Admission::new(&mut renderer, control).commit_signal_mask(
        PreparedPolicyMask::Transfer {
            destination,
            ledger: &work,
            remaining,
        },
        ranges,
        Some(&mut lease),
    );
    assert_eq!(mask.as_slice().as_ptr(), pointer);
    assert!(mask.is_empty());
    assert_eq!(mask.capacity(), 2);
    assert!(mask.is_owned());
    assert_eq!(lease.frames(), 0);
    assert_eq!(work.borrow().sound_context_frames, 0);
    let observer = renderer.owned_suppression.observer().unwrap();
    assert_eq!(observer.live_frames(), SHELL + STORAGE + 4);
    drop(renderer);
    drop(mask);
    assert_eq!(observer.live_frames(), SHELL);
}

#[test]
fn actual_factory_foreign_ledger_rejects_equal_sized_native_custody() {
    let plan = fixture();
    let point = point(&plan);
    let mut source_renderer = StageAudio::new(Arc::clone(&plan));
    let mut destination_renderer = StageAudio::new(Arc::clone(&plan));
    let source_work = RefCell::new(ReadWork::default());
    let destination_work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let source_control = control(&source_work, &cancelled);
    let destination_control = control(&destination_work, &cancelled);
    let source = query(&point, &mut source_renderer, source_control, false);
    let destination = query(
        &point,
        &mut destination_renderer,
        destination_control,
        false,
    );
    let destination_live = destination_work.borrow().sound_context_frames;
    assert_eq!(source_work.borrow().sound_context_frames, destination_live);
    let result = source.into_suppression(&mut Admission::new(
        &mut destination_renderer,
        destination_control,
    ));
    assert!(matches!(
        result,
        Err(StageAudioError::Plan(
            PlanError::AudioMetadataIssuerMismatch
        ))
    ));
    assert_eq!(source_work.borrow().sound_context_frames, 0);
    assert_eq!(
        destination_work.borrow().sound_context_frames,
        destination_live
    );
    assert_eq!(source_renderer.owned_suppression.live_frames(), 0);
    assert_eq!(destination_renderer.owned_suppression.live_frames(), 0);
    drop(destination);
    assert_eq!(destination_work.borrow().sound_context_frames, 0);
}
