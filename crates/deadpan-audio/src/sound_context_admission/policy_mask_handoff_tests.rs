//! Real admission-backed source leases; private helper tests, not full custody.
use super::*;
use crate::stages::controlled_reads::fixture_document_at_rate;
use deadpan_core::FrameRate;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

fn renderer() -> StageAudio {
    let (document, _provider) = fixture_document_at_rate(FrameRate::new(48_000, 1).unwrap());
    StageAudio::new(Arc::new(RenderPlan::compile(&document).unwrap()))
}

fn control<'work>(
    work: &'work RefCell<ReadWork>,
    cancelled: &'work AtomicBool,
) -> WorkControl<'work> {
    WorkControl {
        cancelled,
        deadline: Instant::now() + Duration::from_secs(30),
        work,
    }
}

// Source bytes are physically destroyed before their real reservation releases.
struct Source<'work> {
    ranges: Vec<Range<SignalSample>>,
    lease: Lease<'work>,
}
fn source<'work>(
    renderer: &mut StageAudio,
    control: WorkControl<'work>,
    capacity: usize,
) -> Source<'work> {
    let layout = Layout::array::<Range<SignalSample>>(capacity).unwrap();
    let lease = Admission::new(renderer, control).reserve(layout).unwrap();
    let mut ranges = Vec::with_capacity(capacity);
    assert_eq!(ranges.capacity(), capacity);
    if capacity > 0 {
        ranges.push(SignalSample(-2)..SignalSample(3));
    }
    Source { ranges, lease }
}

#[test]
fn actual_reserved_source_rejects_shared_and_exclusive_refs_before_any_native_credit() {
    let mut renderer = renderer();
    let work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let control = control(&work, &cancelled);
    let source = source(&mut renderer, control, 2);
    let capacity = source.ranges.capacity();
    let frames = crate::stages::point_suppression::policy_handoff::vector_frames(capacity).unwrap();
    assert_eq!(source.lease.frames(), frames);
    assert_eq!(work.borrow().sound_context_frames, frames);
    let source_pointer = source.ranges.as_ptr();
    let account_before = renderer.owned_suppression.live_frames();
    let work_before = work.borrow().plan_work;
    let current_issuer = Admission::new(&mut renderer, control).issuer();

    let shared = work.borrow();
    assert!(matches!(
        preflight_source_lease(
            control,
            issuer(control),
            current_issuer,
            capacity,
            Some(&source.lease),
            &source.lease,
            frames,
        ),
        Err(StageAudioError::Range)
    ));
    assert_eq!(shared.sound_context_frames, frames);
    assert_eq!(source.lease.frames(), frames);
    assert_eq!(source.ranges.as_ptr(), source_pointer);
    assert_eq!(source.ranges.len(), 1);
    assert_eq!(source.ranges.capacity(), capacity);
    assert_eq!(renderer.owned_suppression.live_frames(), account_before);
    drop(shared);

    let exclusive = work.borrow_mut();
    assert!(matches!(
        preflight_source_lease(
            control,
            issuer(control),
            current_issuer,
            capacity,
            Some(&source.lease),
            &source.lease,
            frames,
        ),
        Err(StageAudioError::Range)
    ));
    assert_eq!(exclusive.sound_context_frames, frames);
    assert_eq!(source.lease.frames(), frames);
    drop(exclusive);

    assert_eq!(
        preflight_source_lease(
            control,
            issuer(control),
            current_issuer,
            capacity,
            Some(&source.lease),
            &source.lease,
            frames,
        )
        .unwrap(),
        0
    );
    assert_eq!(work.borrow().sound_context_frames, frames);
    assert_eq!(work.borrow().plan_work, work_before);
    assert_eq!(renderer.owned_suppression.live_frames(), account_before);
    assert_eq!(
        source.ranges.as_slice(),
        [SignalSample(-2)..SignalSample(3)]
    );

    // Both conflicting Refs are gone before the existing real Lease destructor.
    drop(source);
    assert_eq!(work.borrow().sound_context_frames, 0);
}

#[test]
fn actual_equal_frame_foreign_and_replaced_leases_fail_exact_preflight_identity() {
    let mut renderer = renderer();
    let left_work = RefCell::new(ReadWork::default());
    let right_work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let left = control(&left_work, &cancelled);
    let right = control(&right_work, &cancelled);
    let original = source(&mut renderer, left, 2);
    let foreign = source(&mut renderer, right, 2);
    let replacement = source(&mut renderer, left, 2);
    let oversized = source(&mut renderer, left, 3);
    let capacity = original.ranges.capacity();
    let frames = crate::stages::point_suppression::policy_handoff::vector_frames(capacity).unwrap();
    assert_eq!(foreign.lease.frames(), frames);
    assert_eq!(replacement.lease.frames(), frames);
    assert!(oversized.lease.frames() > frames);
    let left_before = left_work.borrow().sound_context_frames;
    let right_before = right_work.borrow().sound_context_frames;
    let native_before = renderer.owned_suppression.live_frames();
    let current_issuer = Admission::new(&mut renderer, left).issuer();

    assert_eq!(
        preflight_source_lease(
            left,
            issuer(left),
            current_issuer,
            capacity,
            Some(&original.lease),
            &original.lease,
            frames,
        )
        .unwrap(),
        left_before - frames
    );
    assert!(matches!(
        preflight_source_lease(
            left,
            issuer(right),
            current_issuer,
            capacity,
            Some(&foreign.lease),
            &foreign.lease,
            frames,
        ),
        Err(StageAudioError::Plan(
            PlanError::AudioMetadataIssuerMismatch
        ))
    ));
    // Deliberately inconsistent private-helper metadata tests the ledger check
    // independently; this is not a constructible sealed Plan descriptor.
    assert!(matches!(
        preflight_source_lease(
            left,
            issuer(left),
            current_issuer,
            capacity,
            Some(&foreign.lease),
            &foreign.lease,
            frames,
        ),
        Err(StageAudioError::Plan(
            PlanError::AudioMetadataIssuerMismatch
        ))
    ));
    assert!(matches!(
        preflight_source_lease(
            left,
            issuer(left),
            current_issuer,
            capacity,
            Some(&replacement.lease),
            &original.lease,
            frames,
        ),
        Err(StageAudioError::Plan(
            PlanError::AudioMetadataIssuerMismatch
        ))
    ));
    assert!(matches!(
        preflight_source_lease(
            left,
            issuer(left),
            current_issuer,
            capacity,
            Some(&oversized.lease),
            &oversized.lease,
            frames,
        ),
        Err(StageAudioError::Plan(
            PlanError::AudioMetadataIssuerMismatch
        ))
    ));
    assert_eq!(left_work.borrow().sound_context_frames, left_before);
    assert_eq!(right_work.borrow().sound_context_frames, right_before);
    assert_eq!(renderer.owned_suppression.live_frames(), native_before);
    assert_eq!(original.lease.frames(), frames);
    assert_eq!(foreign.lease.frames(), frames);

    drop(original);
    drop(foreign);
    drop(replacement);
    drop(oversized);
    assert_eq!(left_work.borrow().sound_context_frames, 0);
    assert_eq!(right_work.borrow().sound_context_frames, 0);
}
