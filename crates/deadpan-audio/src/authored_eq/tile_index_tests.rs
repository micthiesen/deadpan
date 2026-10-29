use super::*;
use crate::stages::authored_eq::eq_admission_fixture as fixture;
use std::sync::atomic::Ordering;

fn harness(maximum_resident_frames: u32) -> (StageAudio, AtomicBool, RefCell<ReadWork>) {
    let mut controller = StageAudio::new(fixture::fixture());
    controller.limits.maximum_resident_frames = maximum_resident_frames;
    (
        controller,
        AtomicBool::new(false),
        RefCell::new(ReadWork::default()),
    )
}

fn frames(capacity: usize) -> u64 {
    capacity_frames(Layout::array::<Entry>(capacity).unwrap()).unwrap()
}

fn empty_block() -> Arc<WideBlock> {
    // These helper tests exercise index storage only. Arc-shell and PCM
    // admission are covered by actual eq_tile publication tests.
    Arc::new(WideBlock {
        samples: Vec::new(),
        suppressed: Vec::new(),
        dependencies: Dependencies::new(),
        relative_depth: 1,
    })
}

fn seeded(keys: &[i64], capacity: usize) -> TileIndex {
    let mut entries = Vec::with_capacity(capacity);
    entries.extend(keys.iter().map(|&key| (key, empty_block())));
    assert_eq!(entries.capacity(), capacity);
    TileIndex {
        entries,
        capacity_frames: frames(capacity),
    }
}

fn keys(index: &TileIndex) -> Vec<i64> {
    index.entries().iter().map(|(key, _)| *key).collect()
}

fn insert<const METER_WORK: bool>(
    controller: &mut StageAudio,
    current: &mut TileIndex,
    key: i64,
    control: WorkControl<'_>,
) -> Result<(), StageAudioError> {
    let mut pending = prepare::<METER_WORK>(controller, current, key, control)?;
    let next_active = controller
        .active_frames
        .checked_sub(pending.retired_frames())
        .and_then(|value| value.checked_add(pending.new_frames()))
        .ok_or(StageAudioError::Range)?;
    pending.allocate()?;
    pending.prepay_moves::<METER_WORK>(control)?;
    pending.validate_current::<METER_WORK>(current, control)?;
    let transfer = sound_context_admission::preflight_tile_transfer(
        control,
        sound_context_admission::TileTransferLeases {
            index: pending.lease(),
            arc_shell: None,
        },
    )?;
    pending.publish(current, empty_block());
    controller.active_frames = next_active;
    transfer.commit(sound_context_admission::TileTransferLeasesMut {
        index: pending.lease_mut(),
        arc_shell: None,
    });
    Ok(())
}

#[test]
fn empty_then_spare_append_has_exact_new_work_and_actual_capacity() {
    let (mut controller, cancelled, work) = harness(64);
    let control = fixture::control(&cancelled, &work);
    let mut current = TileIndex::new();
    assert!(current.is_empty());
    let (result, fee) =
        with_fee_trace(|| insert::<true>(&mut controller, &mut current, -4, control));
    result.unwrap();
    assert_eq!(fee, 1, "empty search; one new entry write; no neighbors");
    assert_eq!(current.capacity(), 4);
    assert_eq!(current.capacity_frames(), frames(current.capacity()));
    assert_eq!(controller.active_frames, current.capacity_frames());
    assert_eq!(work.borrow().sound_context_frames, 0);
    for (key, expected_fee) in [(0, 3), (4, 3), (8, 4)] {
        let (result, fee) =
            with_fee_trace(|| insert::<true>(&mut controller, &mut current, key, control));
        result.unwrap();
        assert_eq!(fee, expected_fee);
        assert_eq!(current.capacity(), 4);
    }
    assert_eq!(keys(&current), [-4, 0, 4, 8]);
    assert_eq!(work.borrow().plan_work, 11);
}

#[test]
fn spare_prepend_middle_and_append_charge_only_actual_suffix_and_new_write() {
    for (key, expected_keys, expected_fee) in [
        (-8, [-8, -4, 0, 4], 7),
        (2, [-4, 0, 2, 4], 6),
        (8, [-4, 0, 4, 8], 4),
    ] {
        let (mut controller, cancelled, work) = harness(64);
        let control = fixture::control(&cancelled, &work);
        let mut current = seeded(&[-4, 0, 4], 4);
        controller.active_frames = current.capacity_frames();
        let before = current.capacity();
        let (result, fee) =
            with_fee_trace(|| insert::<true>(&mut controller, &mut current, key, control));
        result.unwrap();
        assert_eq!(fee, expected_fee);
        assert_eq!(work.borrow().plan_work, expected_fee);
        assert_eq!(keys(&current), expected_keys);
        assert_eq!(current.capacity(), before);
        assert_eq!(controller.active_frames, frames(before));
        assert_eq!(work.borrow().sound_context_frames, 0);
    }
}

#[test]
fn full_growth_moves_each_old_entry_once_then_retires_old_storage() {
    let (mut controller, cancelled, work) = harness(64);
    let control = fixture::control(&cancelled, &work);
    let mut current = seeded(&[-4, 0, 4, 8], 4);
    controller.active_frames = current.capacity_frames();
    let (result, fee) =
        with_fee_trace(|| insert::<true>(&mut controller, &mut current, 2, control));
    result.unwrap();
    assert_eq!(
        fee, 9,
        "two search + five entry writes/moves + two neighbors"
    );
    assert_eq!(keys(&current), [-4, 0, 2, 4, 8]);
    assert_eq!(current.capacity(), 8);
    assert_eq!(current.capacity_frames(), frames(8));
    assert_eq!(controller.active_frames, frames(8));
    assert_eq!(work.borrow().sound_context_frames, 0);
}

#[test]
fn paid_lookup_inspects_exact_keys_and_handles_negative_adjacent_and_extreme_keys() {
    let (_, cancelled, work) = harness(64);
    let control = fixture::control(&cancelled, &work);
    let current = seeded(&[i64::MIN, -1, 0, i64::MAX], 4);
    for (key, found, expected_fee) in [
        (0, true, 1),
        (-1, true, 2),
        (i64::MIN, true, 3),
        (i64::MAX, true, 2),
        (-2, false, 3),
        (1, false, 2),
    ] {
        let (result, fee) = with_fee_trace(|| current.get::<true>(key, control));
        assert_eq!(result.unwrap().is_some(), found);
        assert_eq!(fee, expected_fee);
    }
    let empty = TileIndex::new();
    let (result, fee) = with_fee_trace(|| empty.get::<true>(0, control));
    assert!(result.unwrap().is_none());
    assert_eq!(fee, 0);
}

#[test]
fn exact_old_plus_new_peak_succeeds_one_frame_short_preserves_current() {
    let overlap = frames(4) + frames(8);
    for (limit, accepted) in [(overlap, true), (overlap - 1, false)] {
        let (mut controller, cancelled, work) = harness(u32::try_from(limit).unwrap());
        let control = fixture::control(&cancelled, &work);
        let current = seeded(&[-4, 0, 4, 8], 4);
        controller.active_frames = frames(4);
        let prepared = prepare::<true>(&mut controller, &current, 2, control);
        if accepted {
            let mut pending = prepared.unwrap();
            assert_eq!(pending.new_frames(), frames(8));
            assert_eq!(pending.retired_frames(), frames(4));
            assert_eq!(pending.capacity_frames_after(), frames(8));
            assert!(pending.value.is_none());
            assert_eq!(
                controller.active_frames + work.borrow().sound_context_frames,
                overlap
            );
            pending.allocate().unwrap();
            assert_eq!(pending.value.as_ref().unwrap().capacity(), 8);
            pending.validate_current::<true>(&current, control).unwrap();
            drop(pending);
        } else {
            assert!(matches!(prepared, Err(StageAudioError::Limit(_))));
        }
        assert_eq!(keys(&current), [-4, 0, 4, 8]);
        assert_eq!(current.capacity(), 4);
        assert_eq!(controller.active_frames, frames(4));
        assert_eq!(work.borrow().sound_context_frames, 0);
    }
}

#[test]
fn duplicate_bad_accounting_and_unallocated_replacement_refuse_without_mutation() {
    let (mut controller, cancelled, work) = harness(64);
    let control = fixture::control(&cancelled, &work);
    let mut current = seeded(&[-4, 0, 4, 8], 4);
    controller.active_frames = current.capacity_frames();
    let (duplicate, fee) =
        with_fee_trace(|| prepare::<true>(&mut controller, &current, 4, control));
    assert!(matches!(duplicate, Err(StageAudioError::Range)));
    assert_eq!(fee, 1);
    current.capacity_frames -= 1;
    let (bad_counter, fee) =
        with_fee_trace(|| prepare::<true>(&mut controller, &current, 2, control));
    assert!(matches!(bad_counter, Err(StageAudioError::Range)));
    assert_eq!(fee, 0);
    current.capacity_frames += 1;
    let mut pending = prepare::<true>(&mut controller, &current, 2, control).unwrap();
    assert!(matches!(
        pending.validate_current::<true>(&current, control),
        Err(StageAudioError::Range)
    ));
    pending.allocate().unwrap();
    current.entries[1].0 = 3;
    assert!(matches!(
        pending.validate_current::<true>(&current, control),
        Err(StageAudioError::Range)
    ));
    current.entries[1].0 = 0;
    current.capacity_frames -= 1;
    assert!(matches!(
        pending.validate_current::<true>(&current, control),
        Err(StageAudioError::Range)
    ));
    current.capacity_frames += 1;
    current.entries.pop();
    assert!(matches!(
        pending.validate_current::<true>(&current, control),
        Err(StageAudioError::Range)
    ));
    assert_eq!(current.capacity(), 4);
    drop(pending);
    assert_eq!(work.borrow().sound_context_frames, 0);
}

#[test]
fn final_validation_rechecks_allocator_reported_replacement_capacity() {
    let (mut controller, cancelled, work) = harness(128);
    let control = fixture::control(&cancelled, &work);
    let current = seeded(&[-4, 0, 4, 8], 4);
    controller.active_frames = current.capacity_frames();
    let mut pending = prepare::<true>(&mut controller, &current, 2, control).unwrap();
    pending.allocate().unwrap();
    pending.value = Some(Vec::with_capacity(9));
    assert!(matches!(
        pending.validate_current::<true>(&current, control),
        Err(StageAudioError::Range)
    ));
    assert_eq!(keys(&current), [-4, 0, 4, 8]);
    drop(pending);
    assert_eq!(work.borrow().sound_context_frames, 0);
}

#[test]
fn every_work_prefix_refusal_retains_accepted_fees_and_leaves_old_index_usable() {
    // Required operations are two prepare comparisons, five moves/writes,
    // then one left and one right validation comparison. Refuse each boundary.
    for remaining in [0, 1, 2, 7, 8] {
        let (mut controller, cancelled, work) = harness(64);
        let control = fixture::control(&cancelled, &work);
        let mut current = seeded(&[-4, 0, 4, 8], 4);
        controller.active_frames = current.capacity_frames();
        let before = MAX_PLAN_WORK_PER_READ - remaining;
        work.borrow_mut().plan_work = before;
        let (result, accepted) =
            with_fee_trace(|| insert::<true>(&mut controller, &mut current, 2, control));
        assert!(matches!(
            result,
            Err(StageAudioError::Limit("plan work per read"))
        ));
        let expected = match remaining {
            0 => 0,
            1 => 1,
            2 => 2,
            7 => 7,
            8 => 8,
            _ => unreachable!(),
        };
        assert_eq!(accepted, expected);
        assert_eq!(work.borrow().plan_work, before + accepted);
        assert_eq!(keys(&current), [-4, 0, 4, 8]);
        assert_eq!(current.capacity(), 4);
        assert_eq!(controller.active_frames, frames(4));
        assert_eq!(work.borrow().sound_context_frames, 0);
        // Retry in a fresh request; never refund the previous request's work.
        let retry_work = RefCell::new(ReadWork::default());
        let retry = fixture::control(&cancelled, &retry_work);
        insert::<true>(&mut controller, &mut current, 2, retry).unwrap();
        assert_eq!(keys(&current), [-4, 0, 2, 4, 8]);
        assert_eq!(work.borrow().plan_work, before + accepted);
    }
}

#[test]
fn cancellation_before_lookup_and_move_payment_records_no_unaccepted_work() {
    let (mut controller, cancelled, work) = harness(64);
    let control = fixture::control(&cancelled, &work);
    let current = seeded(&[-4, 0, 4, 8], 4);
    controller.active_frames = current.capacity_frames();
    cancelled.store(true, Ordering::Relaxed);
    let (lookup, fee) = with_fee_trace(|| current.get::<true>(4, control));
    assert!(lookup.is_err());
    assert_eq!(fee, 0);
    cancelled.store(false, Ordering::Relaxed);
    let mut pending = prepare::<true>(&mut controller, &current, 2, control).unwrap();
    pending.allocate().unwrap();
    let spent = work.borrow().plan_work;
    cancelled.store(true, Ordering::Relaxed);
    let (moved, fee) = with_fee_trace(|| pending.prepay_moves::<true>(control));
    assert!(moved.is_err());
    assert_eq!(fee, 0);
    assert_eq!(work.borrow().plan_work, spent);
    assert_eq!(keys(&current), [-4, 0, 4, 8]);
    drop(pending);
    assert_eq!(work.borrow().sound_context_frames, 0);
}

#[test]
fn false_mode_keeps_actual_allocations_and_admission_but_adds_no_index_work() {
    let mut observed = Vec::new();
    for metered in [false, true] {
        let (mut controller, cancelled, work) = harness(64);
        let control = fixture::control(&cancelled, &work);
        let mut current = seeded(&[-4, 0, 4, 8], 4);
        controller.active_frames = current.capacity_frames();
        let (result, fee) = with_fee_trace(|| {
            if metered {
                insert::<true>(&mut controller, &mut current, 2, control)
            } else {
                insert::<false>(&mut controller, &mut current, 2, control)
            }
        });
        result.unwrap();
        assert_eq!(fee, if metered { 9 } else { 0 });
        assert_eq!(work.borrow().plan_work, fee);
        observed.push((
            keys(&current),
            current.capacity(),
            current.capacity_frames(),
            controller.active_frames,
        ));
        assert_eq!(work.borrow().sound_context_frames, 0);
    }
    assert_eq!(observed[0], observed[1]);
    let (mut controller, cancelled, work) = harness(64);
    let control = fixture::control(&cancelled, &work);
    let mut current = seeded(&[-4, 0, 4, 8], 4);
    controller.active_frames = current.capacity_frames();
    work.borrow_mut().plan_work = MAX_PLAN_WORK_PER_READ;
    let (result, fee) =
        with_fee_trace(|| insert::<false>(&mut controller, &mut current, 2, control));
    result.unwrap();
    assert_eq!(fee, 0);
    assert_eq!(work.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
}

#[test]
fn fee_trace_restores_outer_scope_and_counts_only_accepted_nested_spends() {
    let (_, cancelled, work) = harness(64);
    let control = fixture::control(&cancelled, &work);
    let current = seeded(&[-4, 0, 4, 8], 4);
    let ((inner_fee, result), outer_fee) = with_fee_trace(|| {
        current.get::<true>(4, control).unwrap();
        let (result, fee) = with_fee_trace(|| current.get::<true>(-4, control));
        (fee, result)
    });
    assert!(result.unwrap().is_some());
    assert_eq!(inner_fee, 3);
    assert_eq!(outer_fee, 4);
    let (_, next_fee) = with_fee_trace(|| current.get::<false>(4, control).unwrap());
    assert_eq!(next_fee, 0);
}

#[test]
fn fee_trace_unwind_restores_outer_scope_without_refunding_accepted_work() {
    let (_, cancelled, work) = harness(64);
    let control = fixture::control(&cancelled, &work);
    let current = seeded(&[-4, 0, 4, 8], 4);
    let (_, outer_fee) = with_fee_trace(|| {
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            with_fee_trace(|| {
                current.get::<true>(4, control).unwrap();
                panic!("test observer unwind");
            });
        }));
        assert!(panic.is_err());
        current.get::<true>(4, control).unwrap();
    });
    assert_eq!(outer_fee, 2);
    assert_eq!(work.borrow().plan_work, 2);
    let (_, next_fee) = with_fee_trace(|| current.get::<false>(4, control).unwrap());
    assert_eq!(next_fee, 0);
}

#[test]
fn checked_geometric_growth_has_no_external_count_limit() {
    assert_eq!(growth_capacity(0, 1).unwrap(), 4);
    assert_eq!(growth_capacity(4, 4).unwrap(), 4);
    assert_eq!(growth_capacity(4, 5).unwrap(), 8);
    assert_eq!(growth_capacity(1024, 1025).unwrap(), 2048);
    assert!(matches!(
        growth_capacity(usize::MAX / 2 + 1, usize::MAX),
        Err(StageAudioError::Range)
    ));
}

#[test]
fn named_tile_handoff_checks_both_lanes_and_refuses_foreign_issuer() {
    let (mut controller, cancelled, work) = harness(64);
    let control = fixture::control(&cancelled, &work);
    let mut admission = sound_context_admission::Admission::new(&mut controller, control);
    let mut index = admission
        .reserve(Layout::array::<Entry>(4).unwrap())
        .unwrap();
    let mut shell = admission.reserve(Layout::new::<usize>()).unwrap();
    let total = index.frames() + shell.frames();
    assert_eq!(work.borrow().sound_context_frames, total);
    let foreign_work = RefCell::new(ReadWork::default());
    let foreign = fixture::control(&cancelled, &foreign_work);
    assert!(matches!(
        sound_context_admission::preflight_tile_transfer(
            foreign,
            sound_context_admission::TileTransferLeases {
                index: Some(&index),
                arc_shell: Some(&shell)
            },
        ),
        Err(StageAudioError::Plan(
            PlanError::AudioMetadataIssuerMismatch
        ))
    ));
    assert_eq!(work.borrow().sound_context_frames, total);
    let (mut foreign_controller, _, foreign_work) = harness(64);
    let foreign = fixture::control(&cancelled, &foreign_work);
    let mut foreign_admission =
        sound_context_admission::Admission::new(&mut foreign_controller, foreign);
    let foreign_shell = foreign_admission.reserve(Layout::new::<usize>()).unwrap();
    assert!(matches!(
        sound_context_admission::preflight_tile_transfer(
            control,
            sound_context_admission::TileTransferLeases {
                index: Some(&index),
                arc_shell: Some(&foreign_shell),
            },
        ),
        Err(StageAudioError::Plan(
            PlanError::AudioMetadataIssuerMismatch
        ))
    ));
    assert_eq!(work.borrow().sound_context_frames, total);
    work.borrow_mut().sound_context_frames = total - 1;
    assert!(matches!(
        sound_context_admission::preflight_tile_transfer(
            control,
            sound_context_admission::TileTransferLeases {
                index: Some(&index),
                arc_shell: Some(&shell)
            },
        ),
        Err(StageAudioError::Range)
    ));
    work.borrow_mut().sound_context_frames = total;
    let transfer = sound_context_admission::preflight_tile_transfer(
        control,
        sound_context_admission::TileTransferLeases {
            index: Some(&index),
            arc_shell: Some(&shell),
        },
    )
    .unwrap();
    transfer.commit(sound_context_admission::TileTransferLeasesMut {
        index: Some(&mut index),
        arc_shell: Some(&mut shell),
    });
    assert_eq!(index.frames(), 0);
    assert_eq!(shell.frames(), 0);
    assert_eq!(work.borrow().sound_context_frames, 0);
}
