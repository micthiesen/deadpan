use super::*;
use crate::stages::authored_eq::eq_admission_fixture as fixture;
use std::cell::Cell;
use std::sync::atomic::Ordering;

fn harness(maximum_resident_frames: u32) -> (StageAudio, AtomicBool, RefCell<ReadWork>) {
    let plan = fixture::fixture();
    let mut controller = StageAudio::new(plan);
    controller.limits.maximum_resident_frames = maximum_resident_frames;
    (
        controller,
        AtomicBool::new(false),
        RefCell::new(ReadWork::default()),
    )
}

fn frames<T>(capacity: usize) -> u64 {
    capacity_frames(Layout::array::<T>(capacity).unwrap()).unwrap()
}

#[test]
fn initial_growth_reserves_before_allocating_and_publishes_one_value() {
    let (mut controller, cancelled, work) = harness(4);
    let control = fixture::control(&cancelled, &work);
    let mut current = Vec::<u64>::new();

    let mut pending = prepare(&mut controller, &current, 0, 8, control).unwrap();
    assert_eq!(pending.new_frames(), 4);
    assert_eq!(pending.retired_frames(), 0);
    assert_eq!(pending.capacity_frames_after(), 4);
    assert_eq!(work.borrow().sound_context_frames, 4);
    assert!(pending.value.is_none(), "prepare must not allocate the Vec");

    pending.allocate().unwrap();
    pending.validate_current(&current, 0).unwrap();
    pending.publish(&mut current, 41);
    assert_eq!(current, [41]);
    assert_eq!(current.capacity(), 4);

    let transfer = sound_context_admission::preflight_transfer(
        control,
        sound_context_admission::TransferLeases {
            capacity: pending.lease(),
            ..Default::default()
        },
    )
    .unwrap();
    transfer.commit(sound_context_admission::TransferLeasesMut {
        capacity: pending.lease_mut(),
        ..Default::default()
    });
    assert_eq!(pending.new_frames(), 0);
    assert_eq!(pending.capacity_frames_after(), 4);
    assert_eq!(work.borrow().sound_context_frames, 0);
}

#[test]
fn later_growth_accounts_old_and_new_capacity_until_publish() {
    let (mut controller, cancelled, work) = harness(12);
    let control = fixture::control(&cancelled, &work);
    let mut current = Vec::with_capacity(4);
    current.extend([10_u64, 20, 30, 40]);
    controller.active_frames = frames::<u64>(current.capacity());

    let mut pending = prepare(&mut controller, &current, 4, 8, control).unwrap();
    assert_eq!(pending.new_frames(), 8);
    assert_eq!(pending.retired_frames(), 4);
    assert_eq!(pending.capacity_frames_after(), 8);
    assert_eq!(work.borrow().sound_context_frames, 8);
    assert_eq!(u64::from(controller.limits.maximum_resident_frames), 12);

    pending.allocate().unwrap();
    pending.prepay_moves(control).unwrap();
    assert!(work.borrow().plan_work >= current.len());
    pending.validate_current(&current, 4).unwrap();
    pending.publish(&mut current, 50);
    assert_eq!(current, [10, 20, 30, 40, 50]);
    assert_eq!(current.capacity(), 8);
    assert_eq!(pending.capacity_frames_after(), 8);
    drop(pending);
    assert_eq!(work.borrow().sound_context_frames, 0);
}

#[test]
fn no_growth_uses_existing_spare_capacity_without_a_lease() {
    let (mut controller, cancelled, work) = harness(4);
    let control = fixture::control(&cancelled, &work);
    let mut current = Vec::with_capacity(4);
    current.extend([1_u64, 2]);
    controller.active_frames = frames::<u64>(4);
    let capacity = current.capacity();

    let mut pending = prepare(&mut controller, &current, 4, 4, control).unwrap();
    assert_eq!(pending.new_frames(), 0);
    assert_eq!(pending.retired_frames(), 0);
    assert_eq!(pending.capacity_frames_after(), 4);
    assert!(pending.lease().is_none());
    pending.allocate().unwrap();
    pending.validate_current(&current, 4).unwrap();
    pending.publish(&mut current, 3);
    assert_eq!(current, [1, 2, 3]);
    assert_eq!(current.capacity(), capacity);
    assert_eq!(work.borrow().sound_context_frames, 0);
}

#[test]
fn exact_overlap_budget_succeeds_and_one_frame_short_leaves_old_vec_intact() {
    let (mut exact_controller, cancelled, exact_work) = harness(12);
    let exact_control = fixture::control(&cancelled, &exact_work);
    let mut exact_vec = Vec::with_capacity(4);
    exact_vec.extend([1_u64, 2, 3, 4]);
    exact_controller.active_frames = 4;
    let mut exact = prepare(&mut exact_controller, &exact_vec, 4, 8, exact_control).unwrap();
    exact.allocate().unwrap();
    assert_eq!(
        exact_work.borrow().sound_context_frames + exact_controller.active_frames,
        12
    );
    drop(exact);
    assert_eq!(exact_work.borrow().sound_context_frames, 0);

    let (mut short_controller, cancelled, short_work) = harness(11);
    let control = fixture::control(&cancelled, &short_work);
    let mut current = Vec::with_capacity(4);
    current.extend([1_u64, 2, 3, 4]);
    short_controller.active_frames = 4;
    let before_capacity = current.capacity();
    let result = prepare(&mut short_controller, &current, 4, 8, control);
    assert!(matches!(result, Err(StageAudioError::Limit(_))));
    assert_eq!(current, [1, 2, 3, 4]);
    assert_eq!(current.capacity(), before_capacity);
    assert_eq!(short_work.borrow().sound_context_frames, 0);
}

#[test]
fn cancellation_bad_accounting_and_bad_limits_fail_without_changing_old_vec() {
    let (mut controller, cancelled, work) = harness(32);
    let control = fixture::control(&cancelled, &work);
    let mut current = Vec::with_capacity(4);
    current.extend([7_u64, 8, 9, 10]);
    controller.active_frames = 4;
    let before_capacity = current.capacity();

    assert!(matches!(
        prepare(&mut controller, &current, 3, 8, control),
        Err(StageAudioError::Range)
    ));
    assert!(matches!(
        prepare(&mut controller, &current, 4, 4, control),
        Err(StageAudioError::Limit(_))
    ));
    cancelled.store(true, Ordering::Relaxed);
    assert!(prepare(&mut controller, &current, 4, 8, control).is_err());
    assert_eq!(current, [7, 8, 9, 10]);
    assert_eq!(current.capacity(), before_capacity);
    assert_eq!(work.borrow().sound_context_frames, 0);
}

#[test]
fn failed_final_validation_requires_allocation_and_leaves_old_vec_intact() {
    let (mut controller, cancelled, work) = harness(12);
    let control = fixture::control(&cancelled, &work);
    let mut current = Vec::with_capacity(4);
    current.extend([5_u64, 6, 7, 8]);
    controller.active_frames = 4;
    let before_capacity = current.capacity();

    let mut pending = prepare(&mut controller, &current, 4, 8, control).unwrap();
    assert!(
        matches!(
            pending.validate_current(&current, 4),
            Err(StageAudioError::Range)
        ),
        "growing publish must not be possible before allocate"
    );
    pending.allocate().unwrap();
    assert!(matches!(
        pending.validate_current(&current, 3),
        Err(StageAudioError::Range)
    ));
    assert_eq!(current, [5, 6, 7, 8]);
    assert_eq!(current.capacity(), before_capacity);
    drop(pending);
    assert_eq!(work.borrow().sound_context_frames, 0);
}

#[test]
fn cancellation_during_move_prepayment_leaves_old_vec_intact() {
    let (mut controller, cancelled, work) = harness(12);
    let control = fixture::control(&cancelled, &work);
    let mut current = Vec::with_capacity(4);
    current.extend([11_u64, 12, 13, 14]);
    controller.active_frames = 4;

    let mut pending = prepare(&mut controller, &current, 4, 8, control).unwrap();
    pending.allocate().unwrap();
    cancelled.store(true, Ordering::Relaxed);
    assert!(pending.prepay_moves(control).is_err());
    assert_eq!(current, [11, 12, 13, 14]);
    drop(pending);
    assert_eq!(work.borrow().sound_context_frames, 0);
}

struct DropProbe<'a> {
    work: &'a RefCell<ReadWork>,
    observed_frames: &'a Cell<u64>,
}

impl Drop for DropProbe<'_> {
    fn drop(&mut self) {
        self.observed_frames
            .set(self.work.borrow().sound_context_frames);
    }
}

#[test]
fn replacement_values_drop_before_their_capacity_lease() {
    let (mut controller, cancelled, work) = harness(32);
    let control = fixture::control(&cancelled, &work);
    let layout = Layout::array::<DropProbe<'_>>(1).unwrap();
    let target_frames = capacity_frames(layout).unwrap();
    let lease = {
        let mut admission = sound_context_admission::Admission::new(&mut controller, control);
        admission.reserve(layout).unwrap()
    };
    let observed_frames = Cell::new(0);
    let pending = PendingNodeCapacity {
        value: Some(vec![DropProbe {
            work: &work,
            observed_frames: &observed_frames,
        }]),
        old_len: 0,
        old_capacity: 0,
        old_layout: Layout::array::<DropProbe<'_>>(0).unwrap(),
        accounted_frames: 0,
        target_capacity: 1,
        target_layout: layout,
        target_frames,
        lease: Some(lease),
    };

    drop(pending);
    assert_eq!(observed_frames.get(), target_frames);
    assert_eq!(work.borrow().sound_context_frames, 0);
}

#[test]
fn zero_sized_node_shape_is_rejected() {
    let (mut controller, cancelled, work) = harness(32);
    let control = fixture::control(&cancelled, &work);
    let current = Vec::<()>::new();
    assert!(matches!(
        prepare(&mut controller, &current, 0, 8, control),
        Err(StageAudioError::Unsupported(_))
    ));
    assert_eq!(work.borrow().sound_context_frames, 0);
}
