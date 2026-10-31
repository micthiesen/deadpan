//! Actual NEW native reads paired with frozen OLD requests and exact input origins.
use super::*;
use retained_request::trace;
use std::cell::Cell;

const OLD_SHA256: &str = "838a7165fe4fd9142450568158359b00b05d6cd867aceaef77bfbd49e2c52f75";
thread_local! {
    static ACTIVE:Cell<bool>=const {Cell::new(false)};
    static BEFORE:RefCell<Option<Value>>=const {RefCell::new(None)};
}
fn hex(bytes: impl AsRef<[u8]>) -> String {
    use std::fmt::Write as _;
    let mut value = String::new();
    for byte in bytes.as_ref() {
        write!(value, "{byte:02x}").unwrap();
    }
    value
}
fn frozen() -> Value {
    let bytes = include_bytes!("../../tests/fixtures/retained-host-read-baseline-v1/snapshot.json");
    assert_eq!(hex(Sha256::digest(bytes)), OLD_SHA256);
    serde_json::from_slice(bytes).unwrap()
}
fn reset() {
    original_request::flattened_work_trace::reset();
    trace::reset();
}
fn receipts() -> Value {
    json!({"fade":trace::fade_snapshot(),"owners":trace::owners_snapshot(),
    "original_queries":{
        "flattened":original_request::flattened_work_trace::snapshot(),
        "processing":original_request::flattened_work_trace::processing_snapshot(),
        "policy":original_request::flattened_work_trace::policy_snapshot(),
        "fade":original_request::flattened_work_trace::fade_snapshot(),
    }})
}
fn integer(value: &Value) -> u64 {
    value.as_u64().expect("actual integer receipt")
}
fn delta(value: &Value, lane: &str) -> u64 {
    integer(&value["read"]["after_return_drop"][lane]) - integer(&value["read"]["before"][lane])
}
fn lane_total(receipts: &Value, field: &str, lane: &str) -> u64 {
    integer(&receipts["fade"][lane][field]) + integer(&receipts["owners"][lane][field])
}
fn assert_receipts(receipts: &Value) {
    for product in ["fade", "owners"] {
        assert_eq!(receipts[product]["invalid"], false);
        assert_eq!(receipts[product]["overflow"], false);
        assert_eq!(receipts[product]["active_depth"], 0);
        for lane in ["plan", "cache"] {
            let v = &receipts[product][lane];
            assert_eq!(
                integer(&v["accepted_work"]),
                integer(&v["canonical_units"])
                    + integer(&v["additional_units"])
                    + integer(&v["partial_work"])
                    + integer(&v["host_eviction_work"])
            );
        }
    }
}
fn assert_cleanup(value: &Value) {
    assert_eq!(value["after_handle_drop"]["sound_context_frames"], 0);
    assert_eq!(value["after_renderer_drop"]["sound_context_frames"], 0);
    assert_eq!(
        value["read"]["controller_after_return_drop"]["active_frames"],
        0
    );
    assert_eq!(
        value["read"]["controller_after_return_drop"]["active_disk_bytes"],
        0
    );
    assert_eq!(
        value["read"]["after_return_drop"]["sound_context_frames"],
        value["read"]["before"]["sound_context_frames"],
        "local fade and owner leases end while the original input carrier still lives"
    );
}

/// Test helper seam only. Standalone ordinary metadata calls have no ReadWork bill
/// and run before the measured reader; they inspect this exact borrowed operand.
pub(super) fn before_read(
    retained: &AudioOwnerRetainedInput<'_>,
    start: i64,
    frames: u32,
    authored: bool,
    fault: Fault,
) {
    if !ACTIVE.get() {
        return;
    }
    let skip = matches!(
        fault,
        Fault::InitialCancel
            | Fault::ExpiredDeadline
            | Fault::Depth
            | Fault::ZeroFrames
            | Fault::TooManyFrames
            | Fault::Overflow
    );
    let value = if skip {
        json!({"status":"NotInspected","reason":"actual reader must reject before constructors"})
    } else {
        let samples =
            AudioSample(start)..AudioSample(start.checked_add(i64::from(frames)).unwrap());
        let fade = match retained.fades(samples.clone(), Default::default()) {
            Ok(query) => json!({"status":"Ok","work":query.work,"query":query}),
            Err(e) => json!({"status":"Error","error":error(&StageAudioError::from(e))}),
        };
        let owners = if authored {
            match retained.owners(samples, Default::default()) {
                Ok(query) => {
                    json!({"status":"Ok","work":query.work(),"span_count":query.spans().len()})
                }
                Err(e) => json!({"status":"Error","error":error(&StageAudioError::from(e))}),
            }
        } else {
            json!({"status":"NotRequested"})
        };
        json!({"status":"Inspected","actual_retained_work":retained.work(),"actual_bound_work":retained.bound().work(),
            "fade":fade,"owners":owners,
            "scope":"ordinary standalone constructor witness on the exact borrowed handle; outside measured ReadWork, no reconstruction or normalization"})
    };
    BEFORE.with(|slot| assert!(slot.replace(Some(value)).is_none()));
    reset();
}
fn invoke(
    fixture: &cases::Fixture,
    mode: Mode,
    authored: bool,
    start: i64,
    frames: u32,
    fault: Fault,
    legacy: bool,
) -> (Value, Value, Value) {
    assert!(!ACTIVE.replace(true));
    BEFORE.with(|slot| assert!(slot.borrow().is_none()));
    struct Clear;
    impl Drop for Clear {
        fn drop(&mut self) {
            ACTIVE.set(false);
            BEFORE.with(|slot| {
                slot.replace(None);
            });
        }
    }
    let _clear = Clear;
    reset();
    let value = if legacy {
        retained_request::with_legacy_retained_queries(|| {
            scenario(fixture, mode, authored, start, frames, fault)
        })
    } else {
        scenario(fixture, mode, authored, start, frames, fault)
    };
    let witness = BEFORE
        .with(|slot| slot.replace(None))
        .unwrap_or(Value::Null);
    let receipts = receipts();
    assert_receipts(&receipts);
    assert_cleanup(&value);
    (value, witness, receipts)
}
fn mode(value: &Value) -> Mode {
    match value.as_str().unwrap() {
        "ordinary_probe" => Mode::OrdinaryProbe,
        "actual_admitted_probe_unmetered" => Mode::AdmittedFalseProbe,
        "actual_admitted_probe_metered" => Mode::AdmittedTrueProbe,
        "actual_selected_then_shared" => Mode::ActualSelectedShared,
        v => panic!("unknown frozen actual mode {v}"),
    }
}
fn fault(value: &Value) -> Fault {
    match value["read"]["request"]["fault"].as_str().unwrap() {
        "default" => Fault::None,
        "cancelled_before_read" => Fault::InitialCancel,
        "expired_deadline_before_read" => Fault::ExpiredDeadline,
        "provider_failure_behavior" => Fault::ProviderFailure,
        "provider_cancel_behavior" => Fault::ProviderCancel,
        "depth_one_above_limit" => Fault::Depth,
        "zero_frames" => Fault::ZeroFrames,
        "output_frames_one_above_limit" => Fault::TooManyFrames,
        "signed_end_overflow" => Fault::Overflow,
        "one_resident_frame" => Fault::Resident,
        "actual_cache_key_scope" => Fault::CacheLane,
        "cache_scope_at_query_allowance" => Fault::CacheMinimumAllowance,
        "injected_prior_plan_work" => {
            let d = &value["boundary_derivation"];
            let units = i128::from(integer(&d["observed_default_read_units"]))
                + i128::from(d["offset"].as_i64().unwrap());
            Fault::Remaining(usize::try_from(units).unwrap())
        }
        v => panic!("unknown frozen fault {v}"),
    }
}
fn args(value: &Value) -> (Mode, bool, i64, u32, Fault) {
    let request = &value["read"]["request"];
    (
        mode(&value["mode"]),
        value["authored"].as_bool().unwrap(),
        request["start"].as_i64().unwrap(),
        u32::try_from(integer(&request["frames"])).unwrap(),
        fault(value),
    )
}
fn compare_canonical(witness: &Value, receipts: &Value) {
    for product in ["fade", "owners"] {
        let completed = lane_total_product(receipts, product, "completed_queries");
        if completed > 0 {
            assert_eq!(
                completed, 1,
                "these fresh one-read scenarios have one actual local product"
            );
            assert_eq!(witness[product]["status"], "Ok");
            let canonical = lane_total_product(receipts, product, "canonical_units");
            assert_eq!(
                canonical,
                integer(&witness[product]["work"]),
                "independent ordinary work on the exact actual input provenance"
            );
        }
    }
}
fn lane_total_product(receipts: &Value, product: &str, field: &str) -> u64 {
    integer(&receipts[product]["plan"][field]) + integer(&receipts[product]["cache"][field])
}
fn pair(fixture: &cases::Fixture, old: &Value, verify_legacy: bool) -> Value {
    let (mode, authored, start, frames, fault) = args(old);
    let legacy_receipt = if verify_legacy {
        let (actual, _, receipt) = invoke(fixture, mode, authored, start, frames, fault, true);
        assert_eq!(
            &actual, old,
            "legacy full body including every field/work counter must stay frozen"
        );
        assert_eq!(
            lane_total(&receipt, "constructor_calls", "plan")
                + lane_total(&receipt, "constructor_calls", "cache"),
            0
        );
        receipt
    } else {
        Value::Null
    };
    let (new, witness, receipt) = invoke(fixture, mode, authored, start, frames, fault, false);
    for field in [
        "mode",
        "authored",
        "input",
        "retained",
        "before_setup",
        "setup",
    ] {
        assert_eq!(new[field], old[field], "actual setup provenance {field}");
    }
    for field in [
        "before_injection",
        "before",
        "controller_before",
        "injected_prior_work",
    ] {
        assert_eq!(
            new["read"][field], old["read"][field],
            "fresh read setup {field}"
        );
    }
    compare_canonical(&witness, &receipt);
    let old_output = &old["read"]["output"];
    let new_output = &new["read"]["output"];
    let comparison = match (
        old_output["status"].as_str().unwrap(),
        new_output["status"].as_str().unwrap(),
    ) {
        ("Ok", "Ok") => {
            if old_output == new_output {
                "same_complete_pcm_and_metadata"
            } else {
                "unexpected_success_payload_difference"
            }
        }
        ("Error", "Error") => {
            if old_output == new_output {
                "same_complete_typed_error"
            } else {
                "different_typed_error"
            }
        }
        ("Ok", "Error") => "new_refusal",
        ("Error", "Ok") => "new_success_requires_review",
        _ => unreachable!(),
    };
    let accounting = json!({
        "input_work_literal":new["retained"]["retained_work"],
        "old_read_delta":{"plan":delta(old,"plan_work"),"cache":delta(old,"cache_plan_work")},
        "new_read_delta":{"plan":delta(&new,"plan_work"),"cache":delta(&new,"cache_plan_work")},
        "partial_scope":"failed product reports its actual accepted prefix separately; no canonical/additional split of a nonexistent final receipt is inferred",
        "host_evictions":{"old_paired_receipt":legacy_receipt["fade"]["all_host_eviction_work"],"new":receipt["fade"]["all_host_eviction_work"],
            "old_receipt_scope":if verify_legacy {"actual paired legacy execution"} else {"not rerun; immutable complete OLD operation remains authority"}},
    });
    json!({"comparison":comparison,"independent_ordinary_metadata":witness,"legacy_receipts":legacy_receipt,"receipts":receipt,"accounting":accounting,"new":new})
}
fn assert_work_fields_equal_except_explicit_plan_debit(actual: &Value, old: &Value) {
    assert_eq!(
        actual.as_object().unwrap().len(),
        old.as_object().unwrap().len()
    );
    for (key, expected) in old.as_object().unwrap() {
        if !matches!(key.as_str(), "plan_work" | "cache_plan_work") {
            assert_eq!(&actual[key], expected, "non-debit ReadWork field {key}");
        }
    }
}
fn assert_success_pair(old: &Value, pair: &Value) {
    assert_eq!(pair["comparison"], "same_complete_pcm_and_metadata");
    let new = &pair["new"];
    let receipt = &pair["receipts"];
    assert_eq!(new["provider_calls"], old["provider_calls"]);
    assert_eq!(new["cancelled_at_end"], old["cancelled_at_end"]);
    assert_eq!(
        new["read"]["controller_after_return_drop"],
        old["read"]["controller_after_return_drop"]
    );
    assert_eq!(
        new["controller_after_handle_drop"],
        old["controller_after_handle_drop"]
    );
    for phase in ["after_return_drop", "after_scope_drop"] {
        assert_work_fields_equal_except_explicit_plan_debit(
            &new["read"][phase],
            &old["read"][phase],
        );
    }
    assert_work_fields_equal_except_explicit_plan_debit(
        &new["after_handle_drop"],
        &old["after_handle_drop"],
    );
    assert_work_fields_equal_except_explicit_plan_debit(
        &new["after_renderer_drop"],
        &old["after_renderer_drop"],
    );
    for (lane, counter) in [("plan", "plan_work"), ("cache", "cache_plan_work")] {
        assert_eq!(lane_total(receipt, "partial_work", lane), 0);
        // These fresh paired defaults have no preexisting evictable cache entry.
        assert_eq!(lane_total(receipt, "host_eviction_work", lane), 0);
        assert_eq!(
            receipt["fade"]["all_host_eviction_work"][usize::from(lane == "cache")],
            0
        );
        assert_eq!(
            delta(new, counter) - delta(old, counter),
            lane_total(receipt, "additional_units", lane),
            "complete NEW minus OLD debit equals independent successful additional receipts"
        );
    }
    let actual_events = new["provider_events"].as_array().unwrap();
    let old_events = old["provider_events"].as_array().unwrap();
    assert_eq!(actual_events.len(), old_events.len());
    for (new, old) in actual_events.iter().zip(old_events) {
        assert_eq!(
            new.as_object().unwrap().len(),
            old.as_object().unwrap().len()
        );
        for (key, value) in old.as_object().unwrap() {
            if key != "work_before_provider_result" {
                assert_eq!(&new[key], value, "actual source callback {key}")
            }
        }
        let nw = &new["work_before_provider_result"];
        let ow = &old["work_before_provider_result"];
        for (key, value) in ow.as_object().unwrap() {
            if !matches!(
                key.as_str(),
                "plan_work" | "cache_plan_work" | "sound_context_frames"
            ) {
                assert_eq!(&nw[key], value, "source callback non-admission field {key}");
            }
        }
        assert!(integer(&nw["sound_context_frames"]) > integer(&ow["sound_context_frames"]));
    }
}
fn fixture_matches(fixture: &cases::Fixture, old: &Value) {
    assert_eq!(old["name"], fixture.name);
    assert_eq!(
        old["current_document"],
        serde_json::to_value(&fixture.current).unwrap()
    );
    assert_eq!(
        old["reference_document"],
        serde_json::to_value(&fixture.reference).unwrap()
    );
    assert_eq!(old["wav"], cases::wav_descriptor(fixture.wav));
}
#[test]
fn new_native_success_matches_full_old_pcm_masks_dependencies_depth_and_explicit_fees() {
    let old = frozen();
    let fixtures = cases::fixtures();
    let mut count = 0;
    for fixture in &fixtures {
        let old_case = old["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["name"] == fixture.name)
            .unwrap();
        fixture_matches(fixture, old_case);
        let (start, frames) = fixture.windows[1];
        for value in old_case["operations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|v| {
                v["read"]["request"]["fault"] == "default"
                    && v["read"]["request"]["start"] == start
                    && v["read"]["request"]["frames"] == frames
            })
        {
            let paired = pair(fixture, value, true);
            assert_success_pair(value, &paired);
            count += 1;
        }
    }
    assert_eq!(count, 40);
}
#[test]
fn actual_early_and_source_failures_preserve_old_priority_with_separate_partial_receipts() {
    let old = frozen();
    let fixtures = cases::fixtures();
    let mut count = 0;
    for fixture in &fixtures[..2] {
        let old_case = old["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["name"] == fixture.name)
            .unwrap();
        fixture_matches(fixture, old_case);
        for value in old_case["operations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|v| {
                matches!(
                    v["mode"].as_str(),
                    Some("ordinary_probe" | "actual_selected_then_shared")
                ) && matches!(
                    v["read"]["request"]["fault"].as_str(),
                    Some(
                        "cancelled_before_read"
                            | "expired_deadline_before_read"
                            | "depth_one_above_limit"
                            | "zero_frames"
                            | "output_frames_one_above_limit"
                            | "signed_end_overflow"
                            | "provider_failure_behavior"
                            | "provider_cancel_behavior"
                    )
                )
            })
        {
            let paired = pair(fixture, value, true);
            assert_eq!(paired["comparison"], "same_complete_typed_error");
            assert_eq!(paired["new"]["provider_calls"], value["provider_calls"]);
            let calls = integer(&paired["new"]["provider_calls"]);
            if calls == 0 {
                assert_eq!(
                    lane_total(&paired["receipts"], "constructor_calls", "plan")
                        + lane_total(&paired["receipts"], "constructor_calls", "cache"),
                    0
                );
            } else {
                assert_eq!(paired["receipts"]["fade"]["plan"]["completed_queries"], 1);
            }
            count += 1;
        }
    }
    assert_eq!(count, 64);
}

#[test]
#[ignore = "manual NEW native paired capture; same 744 frozen OLD requests, fresh create_new /tmp output"]
fn capture_new_retained_host_pairs_v1() {
    let path =
        std::env::var("DEADPAN_RETAINED_HOST_PAIRS_OUTPUT").expect("explicit fresh NEW path");
    let path = Path::new(&path);
    assert!(path.is_absolute());
    assert!(
        path.parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .starts_with(Path::new("/tmp").canonicalize().unwrap())
    );
    let variant = std::env::var("DEADPAN_RETAINED_HOST_VARIANT").expect("explicit variant");
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source =
        std::env::var("DEADPAN_RETAINED_HOST_SOURCE_SHA256").expect("actual NEW source manifest");
    assert!(source.len() == 64 && source.bytes().all(|v| v.is_ascii_hexdigit()));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .unwrap();
    let old = frozen();
    let mut output = Vec::new();
    let mut count = 0;
    for fixture in cases::fixtures() {
        let old_case = old["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["name"] == fixture.name)
            .unwrap();
        fixture_matches(&fixture, old_case);
        let operations=old_case["operations"].as_array().unwrap().iter().enumerate().map(|(index,row)|{
            count+=1;
            json!({"old_operation_index":index,"old_request":row["read"]["request"],"mode":row["mode"],"authored":row["authored"],
                "paired":pair(&fixture,row,false)})
        }).collect::<Vec<_>>();
        output.push(json!({"fixture":fixture.name,"operations":operations}));
    }
    assert_eq!(count, 744);
    let value = json!({"schema":"new-retained-native-pairs-v1","variant":variant,"source_sha256":source,
        "old_frozen_sha256":OLD_SHA256,"operations":count,
        "scope":"Complete actual NEW results against the same literal OLD request set and actual input provenance. Tight old work/residency requests may refuse earlier; classifications preserve the full typed errors and accepted-prefix receipts.",
        "cases":output});
    serde_json::to_writer_pretty(&mut file, &value).unwrap();
    file.write_all(b"\n").unwrap();
    file.sync_all().unwrap();
}
