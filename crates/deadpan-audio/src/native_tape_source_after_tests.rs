//! Actual routed J requests; observation allocations are outside ReadWork.
use super::*;
use crate::stages::routed::{
    projected_tape_source_request as replay, projected_tape_source_trace as trace,
};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Weak;

#[derive(Default)]
struct Observations {
    rows: Vec<Value>,
    pointer: Option<usize>,
    len: usize,
    capacity: Option<usize>,
    lease_frames: Option<u64>,
    query_work: usize,
    query_ledger: u64,
    query_plan_work: usize,
    handoff_plan_work: usize,
    completed_contents: usize,
    contents: usize,
    prepared_checks: usize,
}

#[derive(Clone, Copy, PartialEq)]
enum Stop {
    None,
    Query,
    Handoff,
}

struct TraceGuard;
impl Drop for TraceGuard {
    fn drop(&mut self) {
        trace::set_observer(None);
        point_suppression::ops::trace::disable();
    }
}

fn install(
    prepared: Option<Weak<PreparedBlock>>,
    stop: Stop,
) -> (Rc<RefCell<Observations>>, TraceGuard) {
    let receipt = Rc::new(RefCell::new(Observations::default()));
    let output = Rc::clone(&receipt);
    trace::set_observer(Some(Box::new(move |event, renderer, control| {
        let mut receipt = output.borrow_mut();
        if let Some(weak) = &prepared {
            let actual = weak
                .upgrade()
                .expect("actual PreparedBlock survives every J seam");
            assert!(
                Arc::strong_count(&actual) >= 4,
                "caller, request memo, actual read and observation"
            );
            assert_eq!(actual.samples.len(), 768);
            assert_eq!(control.work.borrow().projected_resident_frames, 768);
            receipt.prepared_checks += 1;
        }
        let (phase, detail) = match event {
            trace::Event::Query(query) => {
                let view = query.view();
                assert!(query.was_forwarded());
                assert_eq!(
                    query.work(),
                    query.canonical_units() + query.additional_units()
                );
                receipt.pointer = Some(view.suppressed.as_ptr() as usize);
                receipt.len = view.suppressed.len();
                receipt.contents = view.contents.len();
                receipt.query_work = query.work();
                receipt.query_ledger = control.work.borrow().sound_context_frames;
                receipt.query_plan_work = control.work.borrow().plan_work;
                if stop == Stop::Query {
                    control.cancelled.store(true, Ordering::Relaxed);
                }
                (
                    "query",
                    json!({"contents":view.contents,"ranges":view.suppressed,
                    "lookup":view.lookup,"work":query.work(),"canonical":query.canonical_units(),
                    "additional":query.additional_units(),"forwarded":query.was_forwarded()}),
                )
            }
            trace::Event::Use { phase, view } => {
                assert_eq!(Some(view.suppressed.as_ptr() as usize), receipt.pointer);
                assert_eq!(view.suppressed.len(), receipt.len);
                assert_eq!(view.contents.len(), receipt.contents);
                assert_eq!(
                    control.work.borrow().sound_context_frames,
                    receipt.query_ledger
                );
                assert_eq!(
                    control.work.borrow().plan_work,
                    receipt.query_plan_work,
                    "content preflight, control and numeric use never debit forwarded policy again"
                );
                match phase {
                    trace::Phase::ContentPreflight { completed } => {
                        receipt.completed_contents = completed;
                        (
                            "content_preflight",
                            json!({"completed":completed,"contents":view.contents}),
                        )
                    }
                    trace::Phase::ControlChecked => {
                        ("control_checked", json!({"ranges":view.suppressed}))
                    }
                    trace::Phase::BeforeHandoff => {
                        receipt.handoff_plan_work = control.work.borrow().plan_work;
                        if stop == Stop::Handoff {
                            control.cancelled.store(true, Ordering::Relaxed);
                        }
                        ("before_handoff", json!({"ranges":view.suppressed}))
                    }
                }
            }
            trace::Event::PrepareMask {
                capacity,
                lease_frames,
            } => {
                assert_eq!(
                    control.work.borrow().sound_context_frames,
                    receipt.query_ledger
                );
                receipt.capacity = Some(capacity);
                receipt.lease_frames = lease_frames;
                if capacity == 0 {
                    assert_eq!(lease_frames, None);
                } else {
                    let actual =
                        point_suppression::policy_handoff::vector_frames(capacity).unwrap();
                    assert_eq!(lease_frames, Some(actual));
                    assert!(
                        receipt.query_ledger > actual,
                        "inventory is separate from final Vec lease"
                    );
                }
                (
                    "native_prepare_entry",
                    json!({"capacity":capacity,"lease_frames":lease_frames}),
                )
            }
        };
        receipt.rows.push(
            json!({"phase":phase,"detail":detail,"work":work(control.work),
            "native_live_frames":renderer.owned_suppression.live_frames()}),
        );
    })));
    point_suppression::ops::trace::reset();
    (receipt, TraceGuard)
}

struct Lane {
    body: Value,
    success: bool,
    cancelled: bool,
    resident: u64,
}

fn warm(start: i64, frames: u32, legacy: bool, ceiling: Option<u64>, stop: Stop) -> Lane {
    let (plan, mut provider) = checked_plan_with_silent_hold();
    let projection = projected(&plan);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let budget = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let control = WorkControl {
        work: &budget,
        cancelled: &cancelled,
        deadline: Instant::now() + TIMEOUT,
    };
    let prepared = renderer
        .prepare_projected(&projection, &mut provider, control, 1)
        .unwrap();
    assert_eq!(budget.borrow().sound_context_frames, 0);
    assert_eq!(renderer.owned_suppression.live_frames(), 0);
    let resident = budget.borrow().projected_resident_frames;
    assert_eq!(resident, 768);
    if let Some(ceiling) = ceiling {
        assert!(ceiling <= u64::from(renderer.limits.maximum_resident_frames));
        renderer.limits.maximum_resident_frames = u32::try_from(ceiling).unwrap();
    }
    let before = work(&budget);
    let before_provider_calls = provider.calls;
    // This checked family uses opaque identity and the literal warm memo scan.
    assert!(projection.owned_key().is_none() && projection.source_key().is_none());
    let memo_work = {
        let work = budget.borrow();
        work.projected.len() + work.projected_preflight.len() + work.projected_active.len() + 1
    };
    let (observations, _guard) = install(Some(Arc::downgrade(&prepared)), stop);
    let read = |renderer: &mut StageAudio,
                provider: &mut crate::stages::controlled_reads::Provider| {
        renderer.read_projected_signal_controlled(
            &projection,
            provider,
            SignalSample(start),
            frames,
            control,
        )
    };
    let result = if legacy {
        replay::with_legacy_projected_tape_source(|| read(&mut renderer, &mut provider))
    } else {
        read(&mut renderer, &mut provider)
    };
    let success = result.is_ok();
    let was_cancelled = result
        .as_ref()
        .err()
        .is_some_and(StageAudioError::is_cancelled);
    let output = signal_result(&result);
    let observations = observations.borrow();
    if !legacy && let Ok(block) = &result {
        assert_eq!(observations.capacity, Some(block.suppressed.capacity()));
        assert_eq!(observations.len, block.suppressed.len());
        assert_eq!(
            observations.pointer,
            Some(block.suppressed.as_slice().as_ptr() as usize)
        );
        assert_eq!(observations.completed_contents, observations.contents);
        assert_eq!(
            observations.query_plan_work,
            before["plan_work"].as_u64().unwrap() as usize + memo_work + observations.query_work,
            "warm preparation work is separate; new account has no constructor host-residency fee"
        );
        assert_eq!(observations.handoff_plan_work, observations.query_plan_work);
        assert!(observations.prepared_checks >= 4);
        for range in block.suppressed.as_slice() {
            assert!(
                block.samples[usize::try_from(range.start.0 - start).unwrap()
                    ..usize::try_from(range.end.0 - start).unwrap()]
                    .iter()
                    .all(|v| *v == [0.0; 2])
            );
        }
    }
    assert_eq!(
        budget.borrow().sound_context_frames,
        0,
        "all Plan query guards release on success/refusal"
    );
    assert!(
        provider.calls > before_provider_calls,
        "real prepared dependencies are observed again"
    );
    let account = renderer.owned_suppression.observer();
    let owned_after = renderer.owned_suppression.live_frames();
    let expected_active_owned = match (&result, legacy) {
        (Ok(block), false) => Some(if block.suppressed.capacity() == 0 {
            0
        } else {
            point_suppression::shell_frames().unwrap()
                + point_suppression::signal_payload_frames(block.suppressed.capacity()).unwrap()
        }),
        _ => None,
    };
    if let Some(expected) = expected_active_owned {
        assert_eq!(owned_after, expected);
    }
    let after = work(&budget);
    let operations = serde_json::to_value(point_suppression::ops::trace::snapshot()).unwrap();
    let rows = observations.rows.clone();
    drop(observations);
    drop(provider);
    drop(budget);
    drop(renderer);
    drop(prepared);
    drop(projection);
    drop(plan);
    assert_eq!(
        signal_result(&result),
        output,
        "held output survives controller, work, provider, PreparedBlock, projection and plan"
    );
    let held = account.as_ref().map(|value| value.live_frames());
    if let Some(expected) = expected_active_owned {
        assert_eq!(held, if expected == 0 { None } else { Some(expected) });
    }
    drop(result);
    let released = account.as_ref().map(|value| value.live_frames());
    if success && held.is_some() {
        assert_eq!(released, Some(point_suppression::shell_frames().unwrap()));
    }
    Lane {
        success,
        cancelled: was_cancelled,
        resident,
        body: json!({"start":start,"frames":frames,
        "legacy_source":legacy,"resident_ceiling":ceiling,"before":before,"after":after,
        "result":output,"observations":rows,"operations":operations,"owned_after":owned_after,
        "account_held_after_all_request_owners_drop":held,"account_after_result_drop":released}),
    }
}

#[test]
fn native_j_positive_and_empty_keep_exact_source_vec_and_forward_work_once() {
    crate::stages::intrinsic_preflight_policy_request::with_legacy_intrinsic_preflight_policy(
        native_j_positive_and_empty_keep_exact_source_vec_and_forward_work_once_historical,
    );
}

fn native_j_positive_and_empty_keep_exact_source_vec_and_forward_work_once_historical() {
    for (start, frames) in [(512, 256), (0, 256), (640, 64)] {
        let old = warm(start, frames, true, None, Stop::None);
        let active = warm(start, frames, false, None, Stop::None);
        assert!(old.success && active.success);
        let old_result = &old.body["result"];
        let active_result = &active.body["result"];
        for key in [
            "status",
            "f32_bits",
            "sample_len",
            "sample_storage",
            "dependencies",
            "relative_depth",
        ] {
            assert_eq!(
                active_result[key], old_result[key],
                "actual paired result {key}"
            );
        }
        for key in [
            "values",
            "len",
            "owned",
            "first",
            "last",
            "reverse",
            "iter_size_hint",
        ] {
            assert_eq!(
                active_result["suppressed"][key], old_result["suppressed"][key],
                "actual paired mask {key}"
            );
        }
        let operations = &active.body["operations"]["plan"];
        assert_eq!(operations["accepted_vector_units"], 0);
        assert_eq!(operations["accepted_map_units"], 0);
        assert_eq!(operations["accepted_storage_units"], u64::from(start != 0));
        if start == 0 {
            assert_eq!(active.body["result"]["suppressed"]["capacity"], 0);
            assert_eq!(active.body["owned_after"], 0);
            assert_eq!(
                active.body["account_held_after_all_request_owners_drop"],
                Value::Null
            );
        }
    }
}

#[test]
fn native_j_real_query_and_pre_handoff_cancellation_drop_actual_guards() {
    crate::stages::intrinsic_preflight_policy_request::with_legacy_intrinsic_preflight_policy(
        native_j_real_query_and_pre_handoff_cancellation_drop_actual_guards_historical,
    );
}

fn native_j_real_query_and_pre_handoff_cancellation_drop_actual_guards_historical() {
    for stop in [Stop::Query, Stop::Handoff] {
        let lane = warm(512, 256, false, None, stop);
        assert!(!lane.success && lane.cancelled);
        assert_eq!(lane.body["owned_after"], 0);
        let rows = lane.body["observations"].as_array().unwrap();
        assert!(rows.iter().any(|row| row["phase"] == "query"));
        assert_eq!(
            rows.iter().any(|row| row["phase"] == "before_handoff"),
            stop == Stop::Handoff
        );
    }
}

fn resident_boundary() -> Value {
    // Fresh real requests each lower their default quota once. No live ledger is filled or reset.
    let baseline = warm(512, 256, false, None, Stop::None);
    let mut low = baseline.resident;
    let mut high = low + 4096;
    let upper = warm(512, 256, false, Some(high), Stop::None);
    assert!(upper.success);
    let lower = warm(512, 256, false, Some(low), Stop::None);
    assert!(!lower.success);
    let mut search = Vec::new();
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        let request = warm(512, 256, false, Some(middle), Stop::None);
        if request.success {
            high = middle;
        } else {
            low = middle;
        }
        search.push(request.body);
    }
    let exact = warm(512, 256, false, Some(high), Stop::None);
    let below = warm(512, 256, false, Some(high - 1), Stop::None);
    assert!(exact.success && !below.success);
    assert_eq!(below.body["result"]["error"]["kind"], "Limit");
    assert!(
        below.body["result"]["error"]["display"]
            .as_str()
            .unwrap()
            .contains("resident PCM or stage cache")
    );

    let (plan, mut provider) = checked_plan_with_silent_hold();
    let projection = projected(&plan);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let budget = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let control = WorkControl {
        work: &budget,
        cancelled: &cancelled,
        deadline: Instant::now() + TIMEOUT,
    };
    let prepared = renderer
        .prepare_projected(&projection, &mut provider, control, 1)
        .unwrap();
    let fixed_ceiling = high + point_suppression::shell_frames().unwrap();
    assert!(fixed_ceiling <= u64::from(renderer.limits.maximum_resident_frames));
    renderer.limits.maximum_resident_frames = u32::try_from(fixed_ceiling).unwrap();
    let before_first = json!({"work":work(&budget),"controller":controller(&renderer),
        "provider_calls":provider.calls});
    let first = renderer.read_projected_signal_controlled(
        &projection,
        &mut provider,
        SignalSample(512),
        256,
        control,
    );
    assert!(first.as_ref().unwrap().suppressed.is_owned());
    let after_first = json!({"result":signal_result(&first),"work":work(&budget),
        "controller":controller(&renderer),"provider_calls":provider.calls});
    let held = renderer.owned_suppression.live_frames();
    let blocked = renderer.read_projected_signal_controlled(
        &projection,
        &mut provider,
        SignalSample(512),
        256,
        control,
    );
    assert!(matches!(
        blocked,
        Err(StageAudioError::Limit("resident PCM or stage cache"))
    ));
    assert_eq!(renderer.owned_suppression.live_frames(), held);
    assert_eq!(budget.borrow().sound_context_frames, 0);
    let after_blocked = json!({"result":signal_result(&blocked),"work":work(&budget),
        "controller":controller(&renderer),"provider_calls":provider.calls});
    drop(first);
    assert_eq!(
        renderer.owned_suppression.live_frames(),
        point_suppression::shell_frames().unwrap()
    );
    let after_first_drop = json!({"work":work(&budget),"controller":controller(&renderer),
        "provider_calls":provider.calls});
    let recovered = renderer.read_projected_signal_controlled(
        &projection,
        &mut provider,
        SignalSample(512),
        256,
        control,
    );
    assert!(recovered.as_ref().unwrap().suppressed.is_owned());
    assert_eq!(budget.borrow().sound_context_frames, 0);
    assert!(Arc::strong_count(&prepared) >= 2);
    let after_recovered = json!({"result":signal_result(&recovered),"work":work(&budget),
        "controller":controller(&renderer),"provider_calls":provider.calls});
    json!({"scope":"exact measured full warm request resident minimum; no allocation-callback peak claim",
        "baseline":baseline.body,"initial_lower":lower.body,"initial_upper":upper.body,
        "search_requests":search,"minimum_success_resident_frames":high,
        "exact":exact.body,"one_below":below.body,
        "fixed_quota":{"maximum_resident_frames":fixed_ceiling,"before_first":before_first,
            "after_first":after_first,"after_blocked":after_blocked,
            "after_first_drop":after_first_drop,"after_recovered":after_recovered}})
}

#[test]
fn native_j_full_warm_request_has_exact_measured_resident_boundary() {
    crate::stages::intrinsic_preflight_policy_request::with_legacy_intrinsic_preflight_policy(
        native_j_full_warm_request_has_exact_measured_resident_boundary_historical,
    );
}

fn native_j_full_warm_request_has_exact_measured_resident_boundary_historical() {
    resident_boundary();
}

#[test]
fn native_j_qualified_wav_public_source_attempts_match_literal_legacy_results() {
    crate::stages::intrinsic_preflight_policy_request::with_legacy_intrinsic_preflight_policy(
        native_j_qualified_wav_public_source_attempts_match_literal_legacy_results_historical,
    );
}

fn native_j_qualified_wav_public_source_attempts_match_literal_legacy_results_historical() {
    let old = replay::with_legacy_projected_tape_source(qualified_wav::capture_family);
    let (observations, _guard) = install(None, Stop::None);
    let active = qualified_wav::capture_family();
    assert_ne!(
        old["setup"], "Error",
        "qualified WAV setup must actually run"
    );
    assert_ne!(
        active["setup"], "Error",
        "qualified WAV setup must actually run"
    );
    let old_attempts = old["attempts"].as_array().unwrap();
    let active_attempts = active["attempts"].as_array().unwrap();
    for attempts in [old_attempts, active_attempts] {
        assert_eq!(attempts.len(), 3);
        for (index, output) in [false, true].into_iter().enumerate() {
            let attempt = &attempts[index];
            assert_eq!(attempt["source_output"], output);
            assert_eq!(attempt["fit_beat"], false);
            assert_eq!(attempt["setup"], "Ok");
            for lane in ["public_empty", "public_late", "warm"] {
                assert_eq!(attempt[lane]["result"]["status"], "Ok");
            }
        }
        let fit_beat = &attempts[2];
        assert_eq!(fit_beat["source_output"], true);
        assert_eq!(fit_beat["fit_beat"], true);
        assert_eq!(fit_beat["setup"], "Error");
        assert_eq!(fit_beat["error"]["kind"], "Plan");
        assert_eq!(
            fit_beat["error"]["debug"],
            "Plan(InvalidPlan(\"source voice requires an explicit natural-rate mapping\"))"
        );
    }
    for (old, active) in old_attempts.iter().zip(active_attempts) {
        assert_eq!(old["setup"], active["setup"]);
        if old["setup"] == "Error" {
            assert_eq!(old["error"], active["error"]);
            continue;
        }
        for lane in ["public_empty", "public_late", "warm"] {
            let old_result = &old[lane]["result"];
            let current = &active[lane]["result"];
            assert_eq!(old_result["status"], current["status"]);
            if old_result["status"] == "Error" {
                assert_eq!(old_result, current);
                continue;
            }
            for key in [
                "f32_bits",
                "dependencies",
                "relative_depth",
                "sample_len",
                "start",
                "allocation",
            ] {
                assert_eq!(
                    old_result[key], current[key],
                    "qualified Source {lane} {key}"
                );
            }
            assert_eq!(
                old_result["suppressed"]["values"],
                current["suppressed"]["values"]
            );
        }
    }
    assert!(
        observations
            .borrow()
            .rows
            .iter()
            .any(|row| row["phase"] == "native_prepare_entry")
    );
}

#[test]
fn native_j_actual_tape_query_refuses_foreign_native_request_issuer() {
    crate::stages::intrinsic_preflight_policy_request::with_legacy_intrinsic_preflight_policy(
        native_j_actual_tape_query_refuses_foreign_native_request_issuer_historical,
    );
}

fn native_j_actual_tape_query_refuses_foreign_native_request_issuer_historical() {
    // Supplemental factory boundary: this does not substitute for the routed tests above.
    let (plan, mut provider) = checked_plan_with_silent_hold();
    let projection = projected(&plan);
    let mut source = StageAudio::new(Arc::clone(&plan));
    let mut destination = StageAudio::new(Arc::clone(&plan));
    let source_work = RefCell::new(ReadWork::default());
    let destination_work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let source_control = WorkControl {
        work: &source_work,
        cancelled: &cancelled,
        deadline: Instant::now() + TIMEOUT,
    };
    let destination_control = WorkControl {
        work: &destination_work,
        ..source_control
    };
    let prepared = source
        .prepare_projected(&projection, &mut provider, source_control, 1)
        .unwrap();
    let query = projection
        .output_policy()
        .policy_after_preserve_admitted(
            SignalSample(512)..SignalSample(768),
            AudioQueryLimits::default(),
            &mut sound_context_admission::Admission::new(&mut source, source_control),
        )
        .unwrap();
    assert!(source_work.borrow().sound_context_frames > 0);
    assert!(query.was_forwarded());
    let result = query.into_suppression(&mut sound_context_admission::Admission::new(
        &mut destination,
        destination_control,
    ));
    assert!(matches!(
        result,
        Err(StageAudioError::Plan(
            PlanError::AudioMetadataIssuerMismatch
        ))
    ));
    assert_eq!(source_work.borrow().sound_context_frames, 0);
    assert_eq!(destination_work.borrow().sound_context_frames, 0);
    assert_eq!(source.owned_suppression.live_frames(), 0);
    assert_eq!(destination.owned_suppression.live_frames(), 0);
    assert!(Arc::strong_count(&prepared) >= 2);
}

#[test]
#[ignore = "manual fresh actual J routed runtime capture"]
fn capture_native_tape_source_after_j() {
    crate::stages::intrinsic_preflight_policy_request::with_legacy_intrinsic_preflight_policy(
        capture_native_tape_source_after_j_historical,
    );
}

fn capture_native_tape_source_after_j_historical() {
    let output = std::env::var_os("DEADPAN_NATIVE_TAPE_SOURCE_AFTER_OUTPUT").unwrap();
    let output = Path::new(&output);
    assert!(output.is_absolute() && output.starts_with("/tmp"));
    let mut rows = Vec::new();
    for legacy in [true, false] {
        for (start, frames) in [(512, 256), (0, 256), (640, 64)] {
            rows.push(warm(start, frames, legacy, None, Stop::None).body);
        }
    }
    for stop in [Stop::Query, Stop::Handoff] {
        rows.push(warm(512, 256, false, None, stop).body);
    }
    let resident_boundary = resident_boundary();
    let qualified_legacy = replay::with_legacy_projected_tape_source(qualified_wav::capture_family);
    let (qualified_observations, _guard) = install(None, Stop::None);
    let qualified_active = qualified_wav::capture_family();
    let body = json!({"schema":"native-tape-source-j-runtime-v1","published_checkpoint":CHECKPOINT,
        "rows":rows,"resident_boundary":resident_boundary,
        "qualified_legacy":qualified_legacy,"qualified_active":qualified_active,
        "qualified_active_J_observations":qualified_observations.borrow().rows,
        "gaps":["resident boundary is a measured full warm request minimum, not an allocation-callback peak receipt",
            "native plan-work exhaustion has no natural bounded request witness; no accepted counters seeded",
            "same-issuer and post-Box cancellation remain existing native custody tests, not routed J claims",
            "qualified Source outcomes are real attempts and may reject; successful Source is asserted only if present"]});
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .unwrap();
    file.write_all(&serde_json::to_vec_pretty(&body).unwrap())
        .unwrap();
    file.write_all(b"\n").unwrap();
    file.sync_all().unwrap();
}
