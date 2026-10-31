//! Proposal: real StageAudio work and residency gates, no decoder or provider.
#[path = "mutations_tests.rs"]
mod mutations;
use super::*;
use crate::stages::{ReadWork, WorkControl};
use deadpan_core::{
    AudioSample, ColorPolicy, FrameRate, NodeId, PresentationBasis, ProjectDocument, ProjectId,
    RevisionId,
};
use deadpan_plan::{RenderPlan, SignalSample};
use std::cell::{Cell, RefCell};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

const PHASES: usize = 7;
fn index(phase: MaskWorkPhase) -> usize {
    match phase {
        MaskWorkPhase::Account => 0,
        MaskWorkPhase::Scan => 1,
        MaskWorkPhase::Map => 2,
        MaskWorkPhase::Zero => 3,
        MaskWorkPhase::Mark => 4,
        MaskWorkPhase::AllocateVector => 5,
        MaskWorkPhase::AllocateStorage => 6,
    }
}

struct Context<'renderer, 'work> {
    renderer: &'renderer mut crate::StageAudio,
    control: WorkControl<'work>,
    accepted: Cell<[usize; PHASES]>,
    cancel_after: Option<MaskWorkPhase>,
}
impl MaskContext for Context<'_, '_> {
    fn check(&self) -> Result<(), StageAudioError> {
        self.control.check().map(|_| ())
    }

    fn spend(&self, phase: MaskWorkPhase, units: usize) -> Result<(), StageAudioError> {
        spend_work(self.control, phase, units)?;
        let mut accepted = self.accepted.get();
        accepted[index(phase)] += units;
        self.accepted.set(accepted);
        if self.cancel_after == Some(phase) {
            self.control.cancelled.store(true, Ordering::Relaxed);
        }
        Ok(())
    }

    fn reserve(&mut self, frames: u64) -> Result<Charge, StageAudioError> {
        self.renderer.reserve_mask_storage(frames, self.control)
    }
}
fn renderer() -> crate::StageAudio {
    let document = ProjectDocument::new(
        ProjectId::new("mask-ops").unwrap(),
        RevisionId::new("revision").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(60, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root").unwrap(),
    )
    .unwrap();
    crate::StageAudio::new(Arc::new(RenderPlan::compile(&document).unwrap()))
}
fn control<'a>(work: &'a RefCell<ReadWork>, cancelled: &'a AtomicBool) -> WorkControl<'a> {
    WorkControl {
        cancelled,
        deadline: Instant::now() + Duration::from_secs(30),
        work,
    }
}
fn context<'r, 'w>(
    renderer: &'r mut crate::StageAudio,
    work: &'w RefCell<ReadWork>,
    cancelled: &'w AtomicBool,
) -> Context<'r, 'w> {
    Context {
        renderer,
        control: control(work, cancelled),
        accepted: Cell::new([0; PHASES]),
        cancel_after: None,
    }
}

#[test]
fn empty_destinations_allocate_no_account_and_spend_no_units() {
    let mut renderer = renderer();
    let work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let mut context = context(&mut renderer, &work, &cancelled);
    assert!(begin_flags(&mut context, 0).unwrap().as_slice().is_empty());
    let builder = begin_ranges::<SignalSample, _>(&mut context, 0).unwrap();
    assert!(builder.finish(&context).unwrap().is_empty());
    assert_eq!(work.borrow().plan_work, 0);
    assert_eq!(context.accepted.get(), [0; PHASES]);
    assert_eq!(renderer.owned_suppression.live_frames(), 0);
}

#[test]
fn funded_range_builder_rejects_growth_without_spending_or_allocating() {
    let mut renderer = renderer();
    let work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let mut context = context(&mut renderer, &work, &cancelled);
    let mut builder = begin_ranges::<SignalSample, _>(&mut context, 1).unwrap();
    builder
        .push(SignalSample(-7)..SignalSample(9), &context)
        .unwrap();
    let before = work.borrow().plan_work;
    assert!(matches!(
        builder.push(SignalSample(10)..SignalSample(12), &context),
        Err(StageAudioError::Limit("funded suppression range capacity"))
    ));
    assert_eq!(builder.len(), 1);
    assert_eq!(builder.capacity(), 1);
    assert_eq!(work.borrow().plan_work, before);
    let result = builder.finish(&context).unwrap();
    assert_eq!(result.as_slice(), [SignalSample(-7)..SignalSample(9)]);
    let observer = renderer.owned_suppression.observer().unwrap();
    assert_eq!(
        observer.live_frames(),
        super::super::shell_frames().unwrap()
            + super::super::payload_frames_for::<SignalSample>(1).unwrap()
    );
    drop(renderer);
    drop(result);
    assert_eq!(
        observer.live_frames(),
        super::super::shell_frames().unwrap()
    );
}

#[test]
fn rejected_range_and_storage_work_drop_pending_payload_before_its_charge() {
    for finish in [false, true] {
        let mut renderer = renderer();
        let work = RefCell::new(ReadWork::default());
        let cancelled = AtomicBool::new(false);
        let mut context = context(&mut renderer, &work, &cancelled);
        let mut builder = begin_ranges::<AudioSample, _>(&mut context, 1).unwrap();
        if finish {
            builder
                .push(AudioSample(1)..AudioSample(3), &context)
                .unwrap();
        }
        work.borrow_mut().plan_work = crate::stages::MAX_PLAN_WORK_PER_READ;
        if finish {
            assert!(matches!(
                builder.finish(&context),
                Err(StageAudioError::Limit(_))
            ));
        } else {
            assert!(matches!(
                builder.push(AudioSample(1)..AudioSample(3), &context),
                Err(StageAudioError::Limit(_))
            ));
            drop(builder);
        }
        assert_eq!(
            work.borrow().plan_work,
            crate::stages::MAX_PLAN_WORK_PER_READ
        );
        assert_eq!(
            renderer.owned_suppression.live_frames(),
            super::super::shell_frames().unwrap()
        );
    }
}

#[test]
fn flag_initialization_refusal_records_only_the_actual_written_prefix() {
    let mut renderer = renderer();
    let work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    // New account1, payload residency1, vector1, then one Zero1.
    let prior = crate::stages::MAX_PLAN_WORK_PER_READ - 4;
    work.borrow_mut().plan_work = prior;
    let mut context = context(&mut renderer, &work, &cancelled);
    assert!(matches!(
        begin_flags(&mut context, 3),
        Err(StageAudioError::Limit(_))
    ));
    assert_eq!(
        context.accepted.get()[index(MaskWorkPhase::AllocateVector)],
        1
    );
    assert_eq!(context.accepted.get()[index(MaskWorkPhase::Zero)], 1);
    assert_eq!(work.borrow().plan_work - prior, 4);
    assert_eq!(
        renderer.owned_suppression.live_frames(),
        super::super::shell_frames().unwrap()
    );
}

#[test]
fn final_check_disposes_box_vec_and_charge_after_accepted_storage_work() {
    let mut renderer = renderer();
    let work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let mut context = context(&mut renderer, &work, &cancelled);
    let mut builder = begin_ranges::<AudioSample, _>(&mut context, 1).unwrap();
    builder
        .push(AudioSample(1)..AudioSample(3), &context)
        .unwrap();
    context.cancel_after = Some(MaskWorkPhase::AllocateStorage);
    assert!(builder.finish(&context).unwrap_err().is_cancelled());
    assert_eq!(
        context.accepted.get()[index(MaskWorkPhase::AllocateStorage)],
        1
    );
    assert_eq!(
        renderer.owned_suppression.live_frames(),
        super::super::shell_frames().unwrap()
    );
}

#[test]
fn returned_flags_own_their_reservation_after_context_and_controller_drop() {
    let mut renderer = renderer();
    let work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let mut flags = {
        let mut context = context(&mut renderer, &work, &cancelled);
        begin_flags(&mut context, 9).unwrap()
    };
    flags.as_mut_slice()[8] = true;
    let observer = renderer.owned_suppression.observer().unwrap();
    assert_eq!(
        observer.live_frames(),
        super::super::shell_frames().unwrap() + 2
    );
    drop(renderer);
    assert_eq!(
        flags.as_slice(),
        [false, false, false, false, false, false, false, false, true]
    );
    drop(flags);
    assert_eq!(
        observer.live_frames(),
        super::super::shell_frames().unwrap()
    );
}

#[test]
fn actual_controller_quota_refuses_flags_before_vector_allocation() {
    let mut renderer = renderer();
    renderer.limits.maximum_resident_frames = 4;
    let work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let mut context = context(&mut renderer, &work, &cancelled);
    assert!(matches!(
        begin_flags(&mut context, 9),
        Err(StageAudioError::Limit(_))
    ));
    assert_eq!(
        context.accepted.get()[index(MaskWorkPhase::AllocateVector)],
        0
    );
    assert_eq!(
        renderer.owned_suppression.live_frames(),
        super::super::shell_frames().unwrap()
    );
}
#[path = "projections_tests.rs"]
mod projections;

#[path = "controlled_transfer_tests.rs"]
mod controlled_transfer;
