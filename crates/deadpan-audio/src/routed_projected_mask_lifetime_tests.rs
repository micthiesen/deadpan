//! Warm request-memo boundaries for the actual projected Signal return.
//! These are post-preparation copy limits, not cold public-read minima.
use super::*;
use crate::stages::{ReadWork, WorkControl, point_suppression};
use std::cell::RefCell;
use std::time::Instant;

struct TraceGuard;
impl Drop for TraceGuard {
    fn drop(&mut self) {
        super::super::projected_mask_before_trace::take();
        point_suppression::ops::trace::disable();
    }
}

fn control<'a>(work: &'a RefCell<ReadWork>, cancelled: &'a AtomicBool) -> WorkControl<'a> {
    WorkControl {
        work,
        cancelled,
        deadline: Instant::now() + Duration::from_secs(30),
    }
}

fn source_baseline(renderer: &StageAudio, work: &RefCell<ReadWork>) -> u64 {
    let work = work.borrow();
    assert_eq!(renderer.cached_resident_frames(), 0);
    assert_eq!(renderer.active_frames, 0);
    assert_eq!(work.sound_context_frames, 0);
    assert_eq!(renderer.owned_suppression.live_frames(), 0);
    assert_eq!(work.projected_disk_bytes, 0);
    assert_eq!(work.projected_resident_frames, 768);
    work.projected_resident_frames
}

fn ops_snapshot() -> Value {
    let snapshot = serde_json::to_value(point_suppression::ops::trace::snapshot()).unwrap();
    assert_eq!(snapshot["overflow"], false);
    snapshot
}

#[test]
fn warm_projected_copy_exact_payload_limit_is_owned_before_routed_map_and_outlives_work() {
    crate::stages::read_signal_policy_request::with_legacy_read_signal_policy(
        warm_projected_copy_exact_payload_limit_is_owned_before_routed_map_and_outlives_work_historical,
    );
}
fn warm_projected_copy_exact_payload_limit_is_owned_before_routed_map_and_outlives_work_historical()
{
    let (plan, mut provider) = checked_plan_with_silent_hold();
    let projection = projected(&plan);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let control = control(&work, &cancelled);
    let prepared = renderer
        .prepare_projected(&projection, &mut provider, control, 1)
        .unwrap();
    let resident = source_baseline(&renderer, &work);
    let shell = point_suppression::shell_frames().unwrap();
    let payload = point_suppression::signal_payload_frames(1).unwrap();
    let limit = resident + shell + payload;
    assert!(limit <= u64::from(renderer.limits.maximum_resident_frames));
    renderer.limits.maximum_resident_frames = u32::try_from(limit).unwrap();
    let before_calls = provider.calls;
    super::super::projected_mask_before_trace::begin();
    point_suppression::ops::trace::reset();
    let _trace_guard = TraceGuard;

    let block =
        super::super::projected_tape_source_request::with_legacy_projected_tape_source(|| {
            renderer.read_projected_signal_controlled(
                &projection,
                &mut provider,
                SignalSample(512),
                256,
                control,
            )
        })
        .unwrap();
    let receipt = super::super::projected_mask_before_trace::take();
    assert_eq!(receipt.dropped, 0);
    assert_eq!(receipt.rows.len(), 1);
    let source = &receipt.rows[0];
    assert_eq!(source.source_len, 1);
    assert_eq!(source.source_capacity, 1);
    assert_eq!(source.source_ranges, [[615, 768]]);
    assert_eq!(source.account_live_frames, 0);
    assert_eq!(source.projected_resident_frames, resident);
    assert!(!source.source_ranges_truncated);
    assert!(block.suppressed.is_owned());
    assert_eq!(block.suppressed.capacity(), 1);
    assert_eq!(
        block.suppressed.as_slice(),
        [SignalSample(615)..SignalSample(768)]
    );
    assert!(source.allocation_is_distinct_from(&block.suppressed));
    assert!(provider.calls > before_calls);
    assert_eq!(block.dependencies, prepared.dependencies);
    assert_eq!(block.relative_depth, 1 + prepared.relative_depth);
    assert!(Arc::strong_count(&prepared) >= 2);
    let operations = ops_snapshot();
    assert_eq!(operations["plan"]["accepted_payload_reservations"], 1);
    assert_eq!(operations["plan"]["accepted_payload_frames"], payload);
    assert_eq!(operations["plan"]["initialized_account_frames"], shell);
    assert_eq!(operations["plan"]["accepted_vector_units"], 1);
    assert_eq!(operations["plan"]["accepted_map_units"], 1);
    assert_eq!(operations["plan"]["accepted_storage_units"], 1);
    assert_eq!(renderer.owned_suppression.live_frames(), shell + payload);
    let observer = renderer.owned_suppression.observer().unwrap();
    drop(prepared);
    drop(work);
    drop(provider);
    drop(renderer);
    assert_eq!(observer.live_frames(), shell + payload);
    assert_eq!(
        block.suppressed.as_slice(),
        [SignalSample(615)..SignalSample(768)]
    );
    assert_eq!(block.suppressed.capacity(), 1);
    drop(block);
    assert_eq!(observer.live_frames(), shell);
}

#[test]
fn warm_projected_copy_one_frame_below_payload_limit_refuses_after_policy_and_cleans_up() {
    crate::stages::read_signal_policy_request::with_legacy_read_signal_policy(
        warm_projected_copy_one_frame_below_payload_limit_refuses_after_policy_and_cleans_up_historical,
    );
}
fn warm_projected_copy_one_frame_below_payload_limit_refuses_after_policy_and_cleans_up_historical()
{
    let (plan, mut provider) = checked_plan_with_silent_hold();
    let projection = projected(&plan);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let control = control(&work, &cancelled);
    let prepared = renderer
        .prepare_projected(&projection, &mut provider, control, 1)
        .unwrap();
    let resident = source_baseline(&renderer, &work);
    let shell = point_suppression::shell_frames().unwrap();
    let payload = point_suppression::signal_payload_frames(1).unwrap();
    renderer.limits.maximum_resident_frames =
        u32::try_from(resident + shell + payload - 1).unwrap();
    super::super::projected_mask_before_trace::begin();
    point_suppression::ops::trace::reset();
    let _trace_guard = TraceGuard;

    let result =
        super::super::projected_tape_source_request::with_legacy_projected_tape_source(|| {
            renderer.read_projected_signal_controlled(
                &projection,
                &mut provider,
                SignalSample(512),
                256,
                control,
            )
        });
    assert!(matches!(
        result,
        Err(StageAudioError::Limit("resident PCM or stage cache"))
    ));
    let receipt = super::super::projected_mask_before_trace::take();
    assert_eq!(receipt.dropped, 0);
    assert_eq!(receipt.rows.len(), 1);
    let source = &receipt.rows[0];
    assert_eq!(source.source_len, 1);
    assert_eq!(source.source_ranges, [[615, 768]]);
    assert_eq!(source.account_live_frames, 0);
    assert_eq!(source.projected_resident_frames, resident);
    let operations = ops_snapshot();
    assert_eq!(operations["plan"]["initialized_account_frames"], shell);
    assert_eq!(operations["plan"]["accepted_payload_reservations"], 0);
    assert_eq!(operations["plan"]["accepted_vector_units"], 0);
    assert_eq!(operations["plan"]["accepted_map_units"], 0);
    assert_eq!(operations["plan"]["accepted_storage_units"], 0);
    assert_eq!(renderer.active_frames, 0);
    assert_eq!(work.borrow().sound_context_frames, 0);
    assert_eq!(work.borrow().projected_resident_frames, resident);
    assert!(Arc::strong_count(&prepared) >= 2);
    assert_eq!(renderer.owned_suppression.live_frames(), shell);
    let observer = renderer.owned_suppression.observer().unwrap();
    drop(work);
    drop(provider);
    drop(renderer);
    assert_eq!(observer.live_frames(), shell);
    drop(prepared);
}

#[test]
fn warm_empty_projected_mask_needs_no_destination_above_existing_prepared_residency() {
    crate::stages::read_signal_policy_request::with_legacy_read_signal_policy(
        warm_empty_projected_mask_needs_no_destination_above_existing_prepared_residency_historical,
    );
}
fn warm_empty_projected_mask_needs_no_destination_above_existing_prepared_residency_historical() {
    let (plan, mut provider) = checked_plan_with_silent_hold();
    let projection = projected(&plan);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let control = control(&work, &cancelled);
    let prepared = renderer
        .prepare_projected(&projection, &mut provider, control, 1)
        .unwrap();
    let resident = source_baseline(&renderer, &work);
    renderer.limits.maximum_resident_frames = u32::try_from(resident).unwrap();
    super::super::projected_mask_before_trace::begin();
    point_suppression::ops::trace::reset();
    let _trace_guard = TraceGuard;

    let block =
        super::super::projected_tape_source_request::with_legacy_projected_tape_source(|| {
            renderer.read_projected_signal_controlled(
                &projection,
                &mut provider,
                SignalSample(0),
                256,
                control,
            )
        })
        .unwrap();
    let receipt = super::super::projected_mask_before_trace::take();
    assert_eq!(receipt.dropped, 0);
    assert_eq!(receipt.rows.len(), 1);
    assert_eq!(receipt.rows[0].source_len, 0);
    assert_eq!(receipt.rows[0].source_capacity, 0);
    assert_eq!(receipt.rows[0].account_live_frames, 0);
    assert!(block.suppressed.is_empty());
    assert!(!block.suppressed.is_owned());
    assert_eq!(block.suppressed.capacity(), 0);
    assert_eq!(renderer.owned_suppression.live_frames(), 0);
    assert!(renderer.owned_suppression.observer().is_none());
    let operations = ops_snapshot();
    for lane in ["plan", "cache"] {
        assert!(
            operations[lane]
                .as_object()
                .unwrap()
                .values()
                .all(|value| value.as_u64() == Some(0))
        );
    }
    assert!(Arc::strong_count(&prepared) >= 2);
}
