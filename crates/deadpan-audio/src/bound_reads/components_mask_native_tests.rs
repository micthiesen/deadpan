//! Native receipts for checked public requests through Components' real mask producer.
use super::*;
use crate::stages::{
    ReadBlock, components_mask_request, components_mask_trace, point_suppression,
    shared_eq::InputSelection,
};
use deadpan_core::{
    AudioEdgePolicy, AudioTreatmentStage, AudioTreatments, ClipGain, EqualizerRecipe, GainClock,
    GainCurve, GainDb, GainEnvelope, GainRange, GainSegment, InstancePath, Saturation, SoundEvent,
    SoundHoldAllowances, SoundHoldIssuer, SoundOverflowPolicy, SourceAudioMapping, TailEffect,
    TailSends,
};
use std::time::{Duration, Instant};

fn checked_wet_aggregate() -> Checked {
    let mut setup = aggregate(false);
    let mut wire = setup.wire.clone();
    let mut nodes: BTreeMap<NodeId, BeatNode> =
        serde_json::from_value(wire["nodes"].clone()).unwrap();
    nodes.get_mut(&id("root")).unwrap().audio_treatments = AudioTreatments::default()
        .with_wet_sends(
            TailSends::default().with_level(TailEffect::Reverb, Some(GainDb::new(-6000).unwrap())),
        );
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    setup.document = ProjectDocument::from_json(&wire.to_string());
    setup.wire = wire;
    setup
}

fn checked_gain_aggregate() -> Checked {
    let mut setup = aggregate(false);
    let gain = ClipGain::new(
        GainDb::UNITY,
        false,
        vec![
            GainEnvelope::new(
                GainClock::OwnerOutput,
                GainRange::new(ExactRatio::ZERO, ExactRatio::integer(514)).unwrap(),
                GainDb::UNITY,
                vec![
                    GainSegment::new(
                        ExactRatio::integer(514),
                        GainDb::new(-1200).unwrap(),
                        GainCurve::Linear,
                    )
                    .unwrap(),
                ],
            )
            .unwrap(),
        ],
        vec![],
    )
    .unwrap();
    let saturation = Saturation::new(GainDb::new(9000).unwrap()).unwrap();
    let treatments = AudioTreatments::with_equalizer_stages(
        vec![
            AudioTreatmentStage::Saturation,
            AudioTreatmentStage::ClipGain,
        ],
        Some(gain),
        Some(saturation),
        None,
        None,
    )
    .unwrap();
    let mut wire = setup.wire.clone();
    let mut nodes: BTreeMap<NodeId, BeatNode> =
        serde_json::from_value(wire["nodes"].clone()).unwrap();
    nodes.get_mut(&id("root")).unwrap().audio_treatments = treatments;
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    setup.document = ProjectDocument::from_json(&wire.to_string());
    setup.wire = wire;
    setup
}

fn checked_wet_source_only() -> Checked {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let mut setup = checked(
        vec![id("source")],
        BTreeMap::from([(id("source"), source(rate, 0, 64, 64))]),
    );
    let mut wire = setup.wire.clone();
    let mut nodes: BTreeMap<NodeId, BeatNode> =
        serde_json::from_value(wire["nodes"].clone()).unwrap();
    nodes.get_mut(&id("root")).unwrap().audio_treatments = AudioTreatments::default()
        .with_wet_sends(
            TailSends::default().with_level(TailEffect::Reverb, Some(GainDb::new(-6000).unwrap())),
        );
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    setup.document = ProjectDocument::from_json(&wire.to_string());
    setup.wire = wire;
    setup
}

fn checked_dynamic_sounds() -> Checked {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let mut setup = checked(
        vec![id("a"), id("pause"), id("tail")],
        BTreeMap::from([
            (id("a"), source(rate, 0, 4096, 4096)),
            (id("pause"), hold(4096)),
            (id("tail"), hold(4096)),
        ]),
    );
    let mut wire = setup.wire.clone();
    wire["assets"]["media"]["source_qualification"] = serde_json::json!("b".repeat(64));
    wire["assets"]["media"]["still_image"] = serde_json::json!(false);
    let audio = match source(rate, 0, 8192, 8192).kind {
        NodeKind::Source { source } => source.audio.unwrap(),
        _ => unreachable!("public source helper"),
    };
    let event = SoundEvent {
        owner: id("root"),
        label: "Independent".into(),
        mapping: SourceAudioMapping::natural_rate(audio.span, rate).unwrap(),
        source: audio,
        offset: AudioSample(0),
        treatments: AudioTreatments::default(),
        start_edge: AudioEdgePolicy::Hard,
        end_edge: AudioEdgePolicy::Hard,
        overflow: SoundOverflowPolicy::Reject,
    };
    wire["sounds"] = serde_json::json!({"allowed": event.clone(), "forbidden": event});
    wire["sound_allowances"] = serde_json::json!({
        "allowed": SoundHoldAllowances::try_from(vec![SoundHoldIssuer::Node {
            instance: InstancePath { node: id("pause"), repeats: vec![] }
        }]).unwrap()
    });

    let recipe = EqualizerRecipe::new(
        [6000, -6000, 3000].map(|value| GainDb::new(value).unwrap()),
        None,
        None,
    )
    .unwrap();
    let saturation = Saturation::new(GainDb::new(9000).unwrap()).unwrap();
    let gain = ClipGain::new(
        GainDb::UNITY,
        false,
        vec![
            GainEnvelope::new(
                GainClock::OwnerOutput,
                GainRange::new(ExactRatio::ZERO, ExactRatio::integer(12288)).unwrap(),
                GainDb::UNITY,
                vec![
                    GainSegment::new(
                        ExactRatio::integer(12288),
                        GainDb::new(-1200).unwrap(),
                        GainCurve::Linear,
                    )
                    .unwrap(),
                ],
            )
            .unwrap(),
        ],
        vec![],
    )
    .unwrap();
    let treatments = AudioTreatments::with_equalizer_stages(
        vec![
            AudioTreatmentStage::Saturation,
            AudioTreatmentStage::Equalizer,
            AudioTreatmentStage::ClipGain,
        ],
        Some(gain),
        Some(saturation),
        None,
        Some(recipe),
    )
    .unwrap();
    let mut nodes: BTreeMap<NodeId, BeatNode> =
        serde_json::from_value(wire["nodes"].clone()).unwrap();
    nodes.get_mut(&id("root")).unwrap().audio_treatments = treatments;
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    setup.document = ProjectDocument::from_json(&wire.to_string());
    setup.wire = wire;
    setup
}

struct Run {
    body: Value,
    result: Result<ReadBlock, StageAudioError>,
    observer: Option<point_suppression::Observer>,
    live_before: u64,
    live_after_controller_drop: u64,
    active_after_request: u64,
    sound_context_after_request: u64,
    provider_calls: usize,
    work: Value,
    trace: Value,
}

struct TraceGuard;
impl Drop for TraceGuard {
    fn drop(&mut self) {
        components_mask_trace::disable();
        point_suppression::ops::trace::disable();
    }
}

fn run(
    setup: Checked,
    start: i64,
    frames: u32,
    legacy: bool,
    trace: bool,
    plan_work_seed: usize,
) -> Run {
    let plan = Arc::new(RenderPlan::compile(&setup.document.expect("checked graph")).unwrap());
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let live_before = renderer.owned_suppression.live_frames();
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
    components_mask_trace::reset_if_enabled();
    point_suppression::ops::trace::reset();
    if trace {
        components_mask_trace::enable();
    } else {
        components_mask_trace::disable();
    }
    let _trace_guard = TraceGuard;
    let mut read = || {
        renderer.read_shared_input(
            &mut provider,
            AudioSample(start),
            frames,
            control,
            InputSelection::default(),
        )
    };
    let result = if legacy {
        components_mask_request::with_legacy_components_masks(&mut read)
    } else {
        read()
    };
    let observer = renderer.owned_suppression.observer();
    let body = match &result {
        Ok(block) => json!({
            "status": "Ok",
            "start": block.start.0,
            "pcm": block.samples.iter().map(|sample| sample.map(f32::to_bits)).collect::<Vec<_>>(),
            "suppressed": block.suppressed.as_slice().iter().map(|range| [range.start.0, range.end.0]).collect::<Vec<_>>(),
            "mask_capacity": block.suppressed.capacity(),
            "sample_capacity": block.samples.capacity(),
            "exhausted_capacity": block.exhausted.capacity(),
            "dependencies": dependencies(&block.dependencies),
            "relative_depth": block.relative_depth,
            "exhausted": block.exhausted.as_slice().iter().map(|range| [range.start.0, range.end.0]).collect::<Vec<_>>()
        }),
        Err(error) => json!({"status": "Error", "error": format!("{error:?}")}),
    };
    let provider_calls = provider.calls;
    let active_after_request = renderer.active_frames;
    let sound_context_after_request = work.borrow().sound_context_frames;
    let work_value = {
        let work = work.borrow();
        json!({
            "plan_work": work.plan_work,
            "cache_plan_work": work.cache_plan_work,
            "sound_context_frames": work.sound_context_frames
        })
    };
    drop(provider);
    drop(renderer);
    let live_after_controller_drop = observer
        .as_ref()
        .map_or(0, |observer| observer.live_frames());
    let trace_value = components_mask_trace::snapshot();
    components_mask_trace::disable();
    Run {
        body,
        result,
        observer,
        live_before,
        live_after_controller_drop,
        active_after_request,
        sound_context_after_request,
        provider_calls,
        work: work_value,
        trace: trace_value,
    }
}

fn rows(trace: &Value) -> &[Value] {
    assert_eq!(trace["unfinished"], false);
    assert_eq!(
        trace["truncated_rows"], false,
        "observed {} Components requests, dropped {} rows",
        trace["attempted_rows"], trace["dropped_rows"]
    );
    let rows = trace["rows"].as_array().expect("Components rows");
    assert!(rows.iter().all(|row| row["events_truncated"] != true));
    rows
}
fn root_row(rows: &[Value], start: i64) -> &Value {
    let mut matches = rows
        .iter()
        .filter(|row| row["program_depth"] == 0 && row["start"] == start);
    let root = matches.next().expect("actual depth-zero request row");
    assert!(
        matches.next().is_none(),
        "depth-zero request identity is unique"
    );
    root
}
fn has_driver(row: &Value, expected: &str) -> bool {
    row["driver_kinds"]
        .as_array()
        .is_some_and(|kinds| kinds.iter().any(|kind| kind == expected))
}
fn inline_words(row: &Value) -> Vec<u64> {
    row["inline_words_latest"]
        .as_array()
        .expect("actual inline mark words at projection")
        .iter()
        .map(|word| word.as_u64().expect("u64 mark word"))
        .collect()
}

fn assert_inline_source(row: &Value, expect_marks: bool, expect_counted: bool) -> usize {
    assert_eq!(row["source_storage"], "inline_bitset");
    assert_eq!(row["source_heap_allocation"], false);
    assert_eq!(row["source_heap_charge_frames"], 0);
    assert_eq!(row["source_frames"], row["requested_frames"]);
    assert!(row["requested_frames"].as_u64().unwrap() > 0);
    assert!(
        row["requested_frames"].as_u64().unwrap() <= u64::from(crate::stages::MAX_OUTPUT_FRAMES)
    );
    assert_eq!(row["inline_source_scope_closed"], true);

    let initial: Vec<u64> = row["inline_words_initial"]
        .as_array()
        .expect("initialized fixed word array")
        .iter()
        .map(|word| word.as_u64().expect("u64 initial word"))
        .collect();
    assert_eq!(
        initial.len(),
        row["inline_word_count"].as_u64().unwrap() as usize
    );
    assert_eq!(
        initial.len(),
        (crate::stages::MAX_OUTPUT_FRAMES as usize).div_ceil(64)
    );
    assert!(initial.iter().all(|word| *word == 0));
    assert_eq!(
        row["inline_bytes"].as_u64().unwrap() as usize,
        initial.len() * std::mem::size_of::<u64>()
    );

    let words = inline_words(row);
    assert_eq!(words.len(), initial.len());
    let projected_marks: usize = words.iter().map(|word| word.count_ones() as usize).sum();
    if expect_counted {
        assert_eq!(
            row["raw_mark_count"].as_u64().unwrap() as usize,
            projected_marks
        );
    } else {
        assert!(
            row["raw_mark_count"].is_null(),
            "a pre-projection refusal has no counted mark field"
        );
    }
    assert_eq!(
        row["marks"]["written_count"].as_u64().unwrap_or(0) as usize,
        projected_marks,
        "projected bitset cardinality must equal successful Mark writes"
    );
    assert_eq!(projected_marks > 0, expect_marks);
    projected_marks
}
fn assert_successful_producer(run: &Run, expected_driver: &str, start: i64) {
    let rows = rows(&run.trace);
    assert!(!rows.is_empty());
    let root = root_row(rows, start);
    assert!(root["parent_row_id"].is_null());
    assert!(root["result"]["success"] == true);
    assert!(rows.iter().all(|row| row["source_init"]["success"] == true));
    assert!(rows.iter().any(|row| row["result"]["success"] == true));
    assert!(
        rows.iter()
            .all(|row| row["inline_source_scope_closed"] == true)
    );
    assert!(
        rows.iter()
            .all(|row| row["source_heap_allocation"] == false)
    );
    let raw_mark_count = assert_inline_source(root, true, true);
    assert!(root["source_reservation_frames"].as_u64().unwrap() > 0);
    assert!(
        has_driver(root, expected_driver),
        "the identified depth-zero request itself must select the expected driver"
    );
    assert!(root["marks"]["written_count"].as_u64().unwrap_or(0) > 0);
    assert!(root["result"]["mask_capacity"].as_u64().unwrap_or(0) > 0);
    assert_eq!(root["result"]["owned"], true);
    assert_eq!(root["result"]["destination_identity_preserved"], true);
    assert_eq!(root["destination_reservation"]["accepted"], true);
    assert!(
        root["destination_reservation"]["active_frames"]
            .as_u64()
            .unwrap()
            >= root["source_reservation_frames"].as_u64().unwrap()
    );
    assert_eq!(
        root["destination_reservation"]["frames"].as_u64().unwrap(),
        crate::stages::test_audio_mask_payload_frames(
            root["result"]["mask_capacity"].as_u64().unwrap() as usize
        )
        .unwrap()
    );
    assert_eq!(
        root["destination_storage"]["capacity"].as_u64().unwrap() as usize,
        raw_mark_count
    );
    assert_eq!(
        root["result"]["mask_capacity"].as_u64().unwrap() as usize,
        raw_mark_count
    );
    assert!(root["result"]["mask_len"].as_u64().unwrap() as usize <= raw_mark_count);
    if expected_driver == "dynamic" {
        assert!(root["callback_count"].as_u64().unwrap_or(0) > 0);
    }
    let first = root["marks"]["first_attempt"].as_object().unwrap();
    let accepted = root["marks"]["first_accepted_work_after"]
        .as_object()
        .unwrap();
    assert_eq!(
        accepted["plan_work"].as_u64().unwrap(),
        first["plan_work_before"].as_u64().unwrap() + 1
    );
    assert_eq!(
        accepted["cache_plan_work"].as_u64().unwrap(),
        first["cache_plan_work_before"].as_u64().unwrap()
    );
    assert!(
        rows.iter()
            .filter(|row| row["result"]["mask_capacity"].as_u64().unwrap_or(0) > 0)
            .all(|row| {
                assert_inline_source(row, true, true);
                row["source_reservation_frames"].as_u64().unwrap_or(0) > 0
                    && row["destination_reservation"]["accepted"] == true
                    && row["destination_reservation"]["active_frames"]
                        .as_u64()
                        .unwrap_or(0)
                        >= row["source_reservation_frames"]
                            .as_u64()
                            .unwrap_or(u64::MAX)
                    && row["result"]["owned"] == true
                    && row["result"]["destination_identity_preserved"] == true
                    && row["destination_storage"]["capacity"] == row["raw_mark_count"]
                    && row["result"]["mask_capacity"] == row["raw_mark_count"]
            })
    );
}

fn snapshot_live_while_result_held(run: &Run) -> u64 {
    run.observer
        .as_ref()
        .map_or(0, |observer| observer.live_frames())
}

#[test]
fn checked_public_wav_constant_and_dynamic_components_masks_match_old_and_outlive_renderer() {
    for (fixture, start, frames, driver) in [
        (checked_wet_aggregate as fn() -> Checked, 0, 514, "constant"),
        (checked_dynamic_sounds, 8128, 128, "dynamic"),
    ] {
        let old = run(fixture(), start, frames, true, false, 0);
        let active = run(fixture(), start, frames, false, true, 0);
        assert_eq!(
            active.body, old.body,
            "literal old PCM, mask, dependencies, depth, error"
        );
        assert!(active.provider_calls > 0);
        assert_eq!(active.active_after_request, 0);
        assert_eq!(active.sound_context_after_request, 0);
        assert_successful_producer(&active, driver, start);

        let held = snapshot_live_while_result_held(&active);
        assert_eq!(held, active.live_after_controller_drop);
        let block = active
            .result
            .as_ref()
            .expect("checked public request succeeded");
        assert!(block.suppressed.is_owned());
        let payload =
            crate::stages::test_audio_mask_payload_frames(block.suppressed.capacity()).unwrap();
        assert!(payload > 0);
        drop(active.result);
        assert_eq!(
            held,
            active.live_before + point_suppression::shell_frames().unwrap() + payload
        );
        assert_eq!(
            active.observer.as_ref().unwrap().live_frames(),
            active.live_before + point_suppression::shell_frames().unwrap()
        );
        drop(old.result);
    }
}

#[test]
fn checked_public_zero_emission_uses_real_components_path_without_final_payload() {
    let old = run(checked_wet_source_only(), 0, 64, true, false, 0);
    let active = run(checked_wet_source_only(), 0, 64, false, true, 0);
    assert_eq!(active.body, old.body);
    assert!(active.provider_calls > 0);
    assert_eq!(active.active_after_request, 0);
    assert_eq!(active.sound_context_after_request, 0);
    let rows = rows(&active.trace);
    let root = root_row(rows, 0);
    assert!(has_driver(root, "constant"));
    assert!(root["result"]["success"] == true);
    assert!(rows.iter().all(|row| row["source_init"]["success"] == true));
    assert!(rows.iter().all(|row| row["result"]["success"] == true));
    assert_eq!(assert_inline_source(root, false, true), 0);
    assert!(
        rows.iter()
            .all(|row| row["inline_source_scope_closed"] == true)
    );
    assert!(
        rows.iter()
            .all(|row| row["source_heap_allocation"] == false)
    );
    assert!(root["source_reservation_frames"].as_u64().unwrap() > 0);
    assert_eq!(root["destination_reservation"], Value::Null);
    assert_eq!(root["destination_storage"], Value::Null);
    assert_eq!(root["result"]["owned"], false);
    assert_eq!(root["result"]["mask_capacity"], 0);
    assert!(
        active
            .result
            .as_ref()
            .unwrap()
            .suppressed
            .as_slice()
            .is_empty()
    );
    assert!(rows.iter().any(|row| {
        row["destination_reservation"].is_null()
            && row["result"]["mask_capacity"] == 0
            && row["result"]["owned"] == false
    }));
    assert_eq!(
        active.live_after_controller_drop,
        active.live_before + point_suppression::shell_frames().unwrap()
    );
    let observer = active.observer.as_ref().unwrap();
    drop(active.result);
    assert_eq!(
        observer.live_frames(),
        active.live_before + point_suppression::shell_frames().unwrap()
    );
    assert_eq!(active.active_after_request, 0);
    assert_eq!(active.sound_context_after_request, 0);
    drop(old.result);
}

#[test]
fn measured_native_work_prefix_reaches_components_mark_refusal_and_cleans_source() {
    let ample = run(checked_gain_aggregate(), 0, 514, false, true, 0);
    assert!(ample.result.is_ok());
    let root = root_row(rows(&ample.trace), 0);
    assert!(
        has_driver(root, "dynamic"),
        "actual gain-envelope Components driver must be dynamic: {root}"
    );
    assert_eq!(
        root["callback_count"], 0,
        "no recursive EQ callbacks in Root-only fixture"
    );
    let first = root["marks"]["first_attempt"]
        .as_object()
        .expect("root Components Mark attempt");
    let first_work = first["plan_work_before"].as_u64().unwrap() as usize;
    let seed = crate::stages::MAX_PLAN_WORK_PER_READ
        .checked_sub(first_work)
        .unwrap();
    drop(ample.result);

    let failed = run(checked_gain_aggregate(), 0, 514, false, true, seed);
    assert!(matches!(
        &failed.result,
        Err(StageAudioError::Limit("plan work per read"))
    ));
    let rows = rows(&failed.trace);
    assert!(rows.iter().any(|row| row["source_init"]["success"] == true));
    let root = root_row(rows, 0);
    assert!(
        has_driver(root, "dynamic"),
        "actual failure request must use the dynamic Components driver: {root}"
    );
    assert_eq!(
        root["callback_count"], 0,
        "Root-only failure fixture has no recursive EQ callbacks"
    );
    assert_eq!(
        root["marks"]["first_attempt"]["plan_work_before"].as_u64(),
        Some(crate::stages::MAX_PLAN_WORK_PER_READ as u64),
        "measured prefix must place the actual root Mark debit exactly at the ceiling"
    );
    assert_eq!(
        root["marks"]["written_count"].as_u64().unwrap_or(0),
        0,
        "seed={seed}, measured_first={first:?}, failed_root={root}, final_work={:?}, error={:?}",
        failed.work,
        failed.result.as_ref().err()
    );
    assert_inline_source(root, false, false);
    assert!(
        root["marks"]["attempt_count"].as_u64().unwrap_or(0) > 0,
        "seed={seed}, measured_first={first:?}, failed_root={root}, final_work={:?}, error={:?}",
        failed.work,
        failed.result.as_ref().err()
    );
    assert!(
        root["events"]
            .as_array()
            .is_some_and(|events| events.iter().any(|event| event["kind"] == "mark_refused"))
    );
    assert!(
        rows.iter()
            .all(|row| row["inline_source_scope_closed"] == true)
    );
    assert!(
        rows.iter()
            .all(|row| row["source_heap_allocation"] == false)
    );
    assert_eq!(root["destination_reservation"], Value::Null);
    assert_eq!(root["destination_storage"], Value::Null);
    assert!(
        root["result"]
            .as_object()
            .unwrap()
            .get("mask_capacity")
            .is_none(),
        "a failed projection has no mask-capacity result field"
    );
    assert_eq!(failed.active_after_request, 0);
    assert_eq!(failed.sound_context_after_request, 0);
    assert_eq!(
        failed.live_after_controller_drop,
        failed.live_before + point_suppression::shell_frames().unwrap()
    );
    drop(failed.result);
}

pub(super) fn capture_cases() -> Value {
    let mut cases = Vec::new();
    for (name, fixture, start, frames, expected_driver) in [
        (
            "constant_block_positive",
            checked_wet_aggregate as fn() -> Checked,
            0,
            514,
            "constant",
        ),
        (
            "dynamic_positive",
            checked_dynamic_sounds,
            8128,
            128,
            "dynamic",
        ),
        (
            "constant_zero_emission",
            checked_wet_source_only,
            0,
            64,
            "constant",
        ),
    ] {
        let old = run(fixture(), start, frames, true, false, 0);
        let active = run(fixture(), start, frames, false, true, 0);
        let exact = old.body == active.body;
        let active_body = active.body.clone();
        let held_before_drop = snapshot_live_while_result_held(&active);
        let after_controller_drop = active.live_after_controller_drop;
        let output_payload = active
            .result
            .as_ref()
            .ok()
            .filter(|block| block.suppressed.is_owned())
            .and_then(|block| {
                crate::stages::test_audio_mask_payload_frames(block.suppressed.capacity()).ok()
            })
            .unwrap_or(0);
        let after_output_drop = {
            let observer = active.observer.as_ref();
            drop(active.result);
            observer.map_or(0, |observer| observer.live_frames())
        };
        let root = root_row(rows(&active.trace), start);
        let branch_reached = root["parent_row_id"].is_null() && has_driver(root, expected_driver);
        let root_positive_marks = root["marks"]["written_count"].as_u64().unwrap_or(0) > 0;
        let root_dynamic_callbacks = root["callback_count"].as_u64().unwrap_or(0) > 0;
        let inline_mark_count = assert_inline_source(root, name != "constant_zero_emission", true);
        let destination_reservation = &root["destination_reservation"];
        let target_while_source_live = root["source_reservation_frames"].as_u64().unwrap_or(0) > 0
            && destination_reservation["active_frames"]
                .as_u64()
                .unwrap_or(0)
                >= root["source_reservation_frames"]
                    .as_u64()
                    .unwrap_or(u64::MAX);
        let no_target_for_zero = name != "constant_zero_emission"
            || (destination_reservation.is_null()
                && root["destination_storage"].is_null()
                && root["result"]["owned"] == false
                && root["result"]["mask_capacity"] == 0);
        let target_capacity = root["destination_storage"]["capacity"].as_u64();
        let inline_words = root["inline_words_latest"].clone();
        cases.push(json!({
            "name": name,
            "legacy": old.body,
            "active": active_body,
            "exact_output_match": exact,
            "provider_calls": active.provider_calls,
            "provider_was_used": active.provider_calls > 0,
            "expected_driver": expected_driver,
            "actual_driver_reached": branch_reached,
            "root_positive_marks": root_positive_marks,
            "root_dynamic_callbacks": root_dynamic_callbacks,
            "inline_source": {
                "storage": root["source_storage"],
                "heap_allocation": root["source_heap_allocation"],
                "heap_charge_frames": root["source_heap_charge_frames"],
                "scope_closed": root["inline_source_scope_closed"],
                "word_count": root["inline_word_count"],
                "bytes": root["inline_bytes"],
                "words_at_projection": inline_words,
                "raw_mark_count": root["raw_mark_count"],
                "popcount_matches_marks": inline_mark_count == root["marks"]["written_count"].as_u64().unwrap_or(0) as usize
            },
            "target_reserved_while_existing_source_live": target_while_source_live,
            "zero_emission_has_no_target": no_target_for_zero,
            "destination_capacity": target_capacity,
            "destination_reservation": destination_reservation,
            "active_frames_after_request": active.active_after_request,
        "sound_context_frames_after_request": active.sound_context_after_request,
        "held_after_controller_drop": after_controller_drop,
            "held_while_output_live": held_before_drop,
            "sample_capacity": active.body["sample_capacity"],
        "exhausted_capacity": active.body["exhausted_capacity"],
        "output_payload_frames": output_payload,
            "live_after_output_drop": after_output_drop,
            "work": active.work,
            "components_trace": active.trace
        }));
        drop(old.result);
    }

    let ample = run(checked_gain_aggregate(), 0, 514, false, true, 0);
    assert!(ample.result.is_ok());
    let root = root_row(rows(&ample.trace), 0);
    assert!(has_driver(root, "dynamic"));
    assert_eq!(root["callback_count"], 0);
    let first_work = root["marks"]["first_attempt"]["plan_work_before"]
        .as_u64()
        .expect("root request reached an actual mark") as usize;
    drop(ample.result);
    let seed = crate::stages::MAX_PLAN_WORK_PER_READ
        .checked_sub(first_work)
        .unwrap_or(crate::stages::MAX_PLAN_WORK_PER_READ);
    let failed_legacy = run(checked_gain_aggregate(), 0, 514, true, false, seed);
    let failed = run(checked_gain_aggregate(), 0, 514, false, true, seed);
    let failed_root = root_row(rows(&failed.trace), 0);
    let actual_mark_refusal_reached = failed_root["events"]
        .as_array()
        .is_some_and(|events| events.iter().any(|event| event["kind"] == "mark_refused"));
    cases.push(json!({
        "name": "measured_mark_work_refusal",
        "failure_fixture": "checked_gain_aggregate",
        "measured_first_attempt": root["marks"]["first_attempt"],
        "expected_driver": "dynamic",
        "actual_driver_reached": has_driver(failed_root, "dynamic"),
        "root_callback_count": failed_root["callback_count"],
        "root_first_attempt_plan_work_before": failed_root["marks"]["first_attempt"]["plan_work_before"],
        "seed_from_actual_first_mark": seed,
        "legacy_at_same_work_seed": failed_legacy.body,
        "active": failed.body,
        "sample_capacity": failed.body["sample_capacity"],
        "exhausted_capacity": failed.body["exhausted_capacity"],
        "provider_calls": failed.provider_calls,
        "actual_mark_refusal_reached": actual_mark_refusal_reached,
        "inline_source_scope_closed": failed_root["inline_source_scope_closed"],
        "source_heap_allocation": failed_root["source_heap_allocation"],
        "source_heap_charge_frames": failed_root["source_heap_charge_frames"],
        "inline_word_count": failed_root["inline_word_count"],
        "raw_mark_count": failed_root["raw_mark_count"],
        "destination_reservation": failed_root["destination_reservation"],
        "live_before": failed.live_before,
        "live_after_controller_drop": failed.live_after_controller_drop,
        "active_frames_after_request": failed.active_after_request,
        "sound_context_frames_after_request": failed.sound_context_after_request,
        "work": failed.work,
        "components_trace": failed.trace
    }));
    drop(failed_legacy.result);
    drop(failed.result);
    json!({"cases": cases, "scope": "only rows whose actual observer receipt reached the Components producer are attributed"})
}
