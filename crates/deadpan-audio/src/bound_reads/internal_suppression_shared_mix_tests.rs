//! Real checked public graph tests for E's non-separate SharedMix mask owner.
use super::*;
use crate::stages::shared_mix_mask_trace;
use crate::stages::{
    ReadBlock, point_suppression, shared_eq::InputSelection, shared_mix_mask_request,
};

fn compile(setup: Checked) -> Arc<RenderPlan> {
    Arc::new(RenderPlan::compile(&setup.document.expect("checked public fixture")).unwrap())
}

fn source_only() -> Checked {
    let rate = FrameRate::new(48_000, 1).unwrap();
    checked(
        vec![id("source")],
        BTreeMap::from([(id("source"), source(rate, 0, 64, 64))]),
    )
}

fn snapshot(block: &ReadBlock) -> Value {
    json!({
        "start": block.start.0,
        "f32_bits": block.samples.iter().map(|sample| sample.map(f32::to_bits)).collect::<Vec<_>>(),
        "dependencies": dependencies(&block.dependencies),
        "relative_depth": block.relative_depth,
        "suppressed": audio_mask(block.suppressed.as_slice(), block.suppressed.capacity()),
        "exhausted": audio_mask(block.exhausted.as_slice(), block.exhausted.capacity())
    })
}

struct Run {
    result: Result<ReadBlock, StageAudioError>,
    observer: Option<point_suppression::Observer>,
    live_after_controller_drop: u64,
    live_before_request: u64,
    active_after_request: u64,
    trace: Value,
}

fn read_shared(
    setup: Checked,
    frames: u32,
    legacy: bool,
    plan_work_seed: usize,
    trace_enabled: bool,
) -> Run {
    let plan = compile(setup);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let live_before_request = renderer.owned_suppression.live_frames();
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork {
        plan_work: plan_work_seed,
        ..ReadWork::default()
    });
    let control = WorkControl {
        cancelled: &cancelled,
        deadline: Instant::now() + Duration::from_secs(10),
        work: &work,
    };
    let mut provider = PublicProvider {
        prepared: cases::prepared(cases::Wav::Stereo48000),
        calls: 0,
        events: Vec::new(),
    };
    point_suppression::ops::trace::reset();
    if trace_enabled {
        shared_mix_mask_trace::enable();
    } else {
        shared_mix_mask_trace::disable();
    }
    let mut read = || {
        renderer.read_shared_input(
            &mut provider,
            AudioSample(0),
            frames,
            control,
            InputSelection::default(),
        )
    };
    let result = if legacy {
        shared_mix_mask_request::with_legacy_shared_mix(read)
    } else {
        read()
    };
    let active_after_request = renderer.active_frames;
    assert_eq!(work.borrow().sound_context_frames, 0);
    let observer = renderer.owned_suppression.observer();
    let trace = shared_mix_mask_trace::snapshot();
    drop(provider);
    drop(renderer);
    let live_after_controller_drop = observer.as_ref().map_or(0, |value| value.live_frames());
    shared_mix_mask_trace::disable();
    Run {
        result,
        observer,
        live_after_controller_drop,
        live_before_request,
        active_after_request,
        trace,
    }
}

fn active_mask_rows(trace: &Value) -> &Vec<Value> {
    assert_eq!(trace["truncated_rows"], false);
    assert_eq!(trace["truncated_events"], false);
    assert_eq!(trace["unfinished"], false);
    assert_eq!(trace["active_row"], Value::Null);
    trace["rows"].as_array().expect("trace rows")
}

struct SharedMixTraceSession;
impl Drop for SharedMixTraceSession {
    fn drop(&mut self) {
        shared_mix_mask_trace::disable();
        point_suppression::ops::trace::disable();
    }
}

fn accepted_destination(row: &Value) -> bool {
    row["events"].as_array().is_some_and(|events| {
        events
            .iter()
            .any(|event| event["kind"] == "reserve_after" && event["value"]["accepted"] == true)
    })
}

#[test]
fn actual_aggregate_514_projects_and_retains_owned_mask_across_three_chunks() {
    let _trace = TraceSession;
    let _shared_mix_trace = SharedMixTraceSession;
    let legacy = read_shared(aggregate(false), 514, true, 0, false);
    let actual = read_shared(aggregate(false), 514, false, 0, true);
    let old = legacy.result.as_ref().expect("E-only legacy request");
    let block = actual.result.as_ref().expect("active E request");
    let old_body = snapshot(old);
    let new_body = snapshot(block);
    assert_eq!(
        new_body, old_body,
        "E must preserve literal old outputs/capacity"
    );
    assert_eq!(actual.active_after_request, 0);
    assert_eq!(block.suppressed.len(), 257);
    assert_eq!(block.suppressed.capacity(), 257);
    assert_eq!(
        block.suppressed.as_slice(),
        (0..514_i64)
            .step_by(2)
            .map(|at| AudioSample(at)..AudioSample(at + 1))
            .collect::<Vec<_>>()
    );
    assert!(
        actual.live_after_controller_drop > actual.live_before_request,
        "the returned final mask must own separately charged payload"
    );

    let rows = active_mask_rows(&actual.trace);
    assert_eq!(actual.trace["truncated_rows"], false);
    assert_eq!(actual.trace["truncated_events"], false);
    assert_eq!(
        rows.len(),
        3,
        "record real 256/256/2 read_shared_program entries"
    );
    for (row, (start, frames)) in rows.iter().zip([(0, 256), (256, 256), (512, 2)]) {
        assert_eq!(row["start"], start);
        assert_eq!(row["source_len"], frames);
        assert!(row["source_capacity"].as_u64().unwrap() >= frames);
        assert!(
            row["active_frames_at_entry"].as_u64().unwrap()
                >= row["source_reservation_frames"].as_u64().unwrap()
        );
        assert!(
            accepted_destination(row),
            "actual target admission missing: {row}"
        );
        let events = row["events"].as_array().unwrap();
        for kind in ["reserve_before", "reserve_after"] {
            let event = events
                .iter()
                .find(|event| event["kind"] == kind)
                .expect("actual destination reserve boundary");
            assert!(
                event["value"]["active_frames"].as_u64().unwrap()
                    >= row["source_reservation_frames"].as_u64().unwrap()
            );
        }
        let destination_capacity = row["result"]["mask_capacity"].as_u64().unwrap() as usize;
        assert_eq!(row["destination_storage"]["capacity"], destination_capacity);
        assert_eq!(
            events
                .iter()
                .find(|event| {
                    event["kind"] == "reserve_after" && event["value"]["accepted"] == true
                })
                .unwrap()["value"]["requested_frames"],
            point_suppression::payload_frames(destination_capacity).unwrap()
        );
        assert_eq!(row["source_disposed"], true);
        assert_eq!(row["source_identity_preserved"], true);
        assert_eq!(row["result"]["owned"], true);
        assert_eq!(
            row["result"]["source_destination_allocations_distinct"],
            true
        );
        assert_eq!(row["destination_reservation"]["accepted"], true);
        assert_eq!(
            row["destination_reservation"]["frames"],
            point_suppression::payload_frames(destination_capacity).unwrap()
        );
        assert!(
            row["result"]["active_frames_after_disposal"]
                .as_u64()
                .unwrap()
                >= row["source_reservation_frames"].as_u64().unwrap()
        );
        assert_eq!(row["result"]["success"], true);
        assert_eq!(row["result"]["destination_identity_preserved"], true);
    }

    let held = actual.live_after_controller_drop;
    assert!(held > actual.live_before_request);
    let observer = actual.observer.as_ref().expect("live account observer");
    assert_eq!(observer.live_frames(), held);
    let payload = point_suppression::payload_frames(block.suppressed.capacity()).unwrap();
    drop(actual.result);
    assert_eq!(observer.live_frames(), held - payload);
}

#[test]
fn actual_source_only_shared_mix_reaches_zero_emission_without_payload() {
    let _trace = TraceSession;
    let _shared_mix_trace = SharedMixTraceSession;
    let actual = read_shared(source_only(), 64, false, 0, true);
    let block = actual
        .result
        .as_ref()
        .expect("actual checked Source-only request");
    assert!(block.suppressed.is_empty());
    assert_eq!(actual.active_after_request, 0);
    let rows = active_mask_rows(&actual.trace);
    assert_eq!(actual.trace["truncated_rows"], false);
    assert_eq!(actual.trace["truncated_events"], false);
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row["source_len"], 64);
    assert_eq!(row["source_disposed"], true);
    assert_eq!(row["result"]["success"], true);
    assert_eq!(row["result"]["mask_len"], 0);
    assert_eq!(row["result"]["mask_capacity"], 0);
    assert_eq!(row["result"]["owned"], false);
    assert_eq!(
        row["result"]["source_destination_allocations_distinct"],
        false
    );
    assert_eq!(row.get("destination_storage"), None);
    assert_eq!(row.get("destination_reservation"), None);
}

#[test]
fn measured_native_work_prefix_reaches_partial_owned_emission_then_cleans_up() {
    let _trace = TraceSession;
    let _shared_mix_trace = SharedMixTraceSession;
    let ample = read_shared(aggregate(false), 514, false, 0, true);
    let first = &active_mask_rows(&ample.trace)[0];
    let entry_work = first["plan_work_at_entry"].as_u64().unwrap() as usize;
    let frame_count = first["source_len"].as_u64().unwrap() as usize;
    let allocation_units = first["work_by_phase"]["AllocateVector"]["accepted_units"]
        .as_u64()
        .unwrap() as usize;
    // Allow the actual count pass, all work in the measured real reserve, and one
    // emitted Hold entry (scan + push); the next existing debit must refuse.
    let events = first["events"].as_array().unwrap();
    let reserve_before = events
        .iter()
        .find(|event| event["kind"] == "reserve_before")
        .unwrap();
    let reserve_after = events
        .iter()
        .find(|event| event["kind"] == "reserve_after")
        .unwrap();
    let reserve_plan_work = reserve_after["value"]["plan_work"].as_u64().unwrap()
        - reserve_before["value"]["plan_work"].as_u64().unwrap();
    let reserve_cache_work = reserve_after["value"]["cache_plan_work"].as_u64().unwrap()
        - reserve_before["value"]["cache_plan_work"].as_u64().unwrap();
    let remaining_at_entry = frame_count
        + reserve_plan_work as usize
        + reserve_cache_work as usize
        + allocation_units
        + 2;
    let seed = super::MAX_PLAN_WORK_PER_READ
        .checked_sub(entry_work + remaining_at_entry)
        .expect("measured first producer fits inside the existing work limit");
    drop(ample.result);

    let failed = read_shared(aggregate(false), 514, false, seed, true);
    assert_eq!(failed.active_after_request, 0);
    let error = match failed.result.as_ref() {
        Err(error) => error,
        Ok(_) => panic!("measured producer prefix should refuse"),
    };
    assert!(matches!(error, StageAudioError::Limit(_)));
    let row = active_mask_rows(&failed.trace)
        .first()
        .expect("entered actual producer");
    assert!(
        accepted_destination(row),
        "destination was not genuinely admitted: {row}"
    );
    assert_eq!(row["work_by_phase"]["Scan"]["failed_calls"], 1);
    assert!(
        row["work_by_phase"]["Map"]["accepted_units"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert_eq!(row["source_disposed"], true);
    assert_eq!(row["source_identity_preserved"], true);
    assert_eq!(row["result"]["success"], false);
    // After controller unwind, only the initialized shared account shell may
    // remain; the pending destination and source grid have both been dropped.
    let observer = failed
        .observer
        .as_ref()
        .expect("admitted destination account");
    assert_eq!(
        observer.live_frames(),
        failed
            .live_before_request
            .max(point_suppression::shell_frames().unwrap())
    );
}
