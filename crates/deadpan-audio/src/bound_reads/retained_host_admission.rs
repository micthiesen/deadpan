//! Uses the independently frozen host setup and real provider unchanged.
use super::*;
use retained_request::trace;

fn receipts() -> Value {
    json!({"fade":trace::fade_snapshot(),"owners":trace::owners_snapshot()})
}
fn reset() {
    original_request::flattened_work_trace::reset();
    trace::reset();
}
fn assert_receipts(value: &Value) {
    for product in ["fade", "owners"] {
        assert_eq!(value[product]["invalid"], false);
        assert_eq!(value[product]["overflow"], false);
        assert_eq!(value[product]["active_depth"], 0);
        for lane in ["plan", "cache"] {
            let receipt = &value[product][lane];
            assert_eq!(
                receipt["accepted_work"].as_u64().unwrap(),
                receipt["canonical_units"].as_u64().unwrap()
                    + receipt["additional_units"].as_u64().unwrap()
                    + receipt["partial_work"].as_u64().unwrap()
                    + receipt["host_eviction_work"].as_u64().unwrap(),
            );
        }
    }
}
fn assert_cleanup(value: &Value) {
    assert_eq!(value["after_handle_drop"]["sound_context_frames"], 0);
    assert_eq!(value["after_renderer_drop"]["sound_context_frames"], 0);
    if value["read"].is_object() {
        assert_eq!(
            value["read"]["controller_after_return_drop"]["active_frames"],
            0
        );
        assert_eq!(
            value["read"]["controller_after_return_drop"]["active_disk_bytes"],
            0
        );
    }
}

#[test]
fn retained_carriers_hold_real_metadata_through_wav_source_success_failure_and_cancel() {
    let fixtures = cases::fixtures();
    for fixture_index in [0, 1] {
        let fixture = &fixtures[fixture_index];
        let (start, frames) = fixture.windows[1];
        for mode in [Mode::OrdinaryProbe, Mode::ActualSelectedShared] {
            for authored in [false, true] {
                for fault in [Fault::None, Fault::ProviderFailure, Fault::ProviderCancel] {
                    reset();
                    let value = scenario(fixture, mode, authored, start, frames, fault);
                    let receipts = receipts();
                    assert_receipts(&receipts);
                    assert_cleanup(&value);
                    assert_eq!(value["setup"]["status"], "Some");
                    assert!(value["provider_calls"].as_u64().unwrap() > 0);
                    let caller_live = value["read"]["before"]["sound_context_frames"]
                        .as_u64()
                        .unwrap();
                    for event in value["provider_events"].as_array().unwrap() {
                        assert!(
                            event["work_before_provider_result"]["sound_context_frames"]
                                .as_u64()
                                .unwrap()
                                > caller_live,
                            "local retained query metadata must remain live at actual source admission"
                        );
                    }
                    assert_eq!(receipts["fade"]["plan"]["completed_queries"], 1);
                    assert_eq!(
                        receipts["owners"]["plan"]["completed_queries"],
                        u64::from(authored)
                    );
                    assert_eq!(
                        value["read"]["output"]["status"],
                        if matches!(fault, Fault::None) {
                            "Ok"
                        } else {
                            "Error"
                        }
                    );
                }
            }
        }
    }
}

fn assert_retained_refusal(value: &Value, fault: Fault) {
    let output = &value["read"]["output"];
    assert_eq!(output["status"], "Error", "{}", fault.name());
    let debug = output["error"]["debug"].as_str().expect("typed read error");
    let expected = match fault {
        Fault::InitialCancel | Fault::ProviderCancel => "Preparation(Cancelled)",
        Fault::ExpiredDeadline => "Timeout",
        Fault::Depth => "Limit(\"nested stage depth\")",
        Fault::ZeroFrames | Fault::TooManyFrames | Fault::Overflow => "Range",
        Fault::Resident => "Limit(\"resident PCM or stage cache\")",
        Fault::CacheMinimumAllowance => "Limit(\"saved audio key work per read\")",
        Fault::Remaining(_) => {
            assert!(
                matches!(
                    debug,
                    "Limit(\"plan work per read\")" | "Plan(AudioQueryLimit(\"structural work\"))"
                ),
                "one-short work budget must fail at a known work guard, got {debug}"
            );
            return;
        }
        Fault::None | Fault::ProviderFailure | Fault::CacheLane => {
            panic!("{} is not a refusal case", fault.name())
        }
    };
    assert_eq!(debug, expected, "{}", fault.name());
}

#[test]
fn retained_receipts_keep_input_debit_separate_and_release_on_local_refusals() {
    let fixtures = cases::fixtures();
    let fixture = &fixtures[0];
    let (start, frames) = fixture.windows[1];
    for authored in [false, true] {
        reset();
        let successful = scenario(
            fixture,
            Mode::ActualSelectedShared,
            authored,
            start,
            frames,
            Fault::None,
        );
        assert_eq!(successful["read"]["output"]["status"], "Ok");
        assert_receipts(&receipts());
        let completed = receipts();
        let construction = successful["retained"]["retained_work"].as_u64().unwrap()
            + ["fade", "owners"]
                .into_iter()
                .map(|product| {
                    completed[product]["plan"]["canonical_units"]
                        .as_u64()
                        .unwrap()
                        + completed[product]["plan"]["additional_units"]
                            .as_u64()
                            .unwrap()
                })
                .sum::<u64>();
        assert!(construction > 1);
        for fault in [
            Fault::InitialCancel,
            Fault::ExpiredDeadline,
            Fault::Depth,
            Fault::ZeroFrames,
            Fault::TooManyFrames,
            Fault::Overflow,
            Fault::Resident,
            Fault::Remaining(usize::try_from(construction - 1).unwrap()),
            Fault::CacheMinimumAllowance,
        ] {
            reset();
            let value = scenario(
                fixture,
                Mode::ActualSelectedShared,
                authored,
                start,
                frames,
                fault,
            );
            assert_retained_refusal(&value, fault);
            assert_eq!(value["provider_calls"], 0, "{}", fault.name());
            if matches!(
                fault,
                Fault::InitialCancel
                    | Fault::ExpiredDeadline
                    | Fault::Depth
                    | Fault::ZeroFrames
                    | Fault::TooManyFrames
                    | Fault::Overflow
                    | Fault::CacheMinimumAllowance
            ) {
                let trace = receipts();
                for product in ["fade", "owners"] {
                    for lane in ["plan", "cache"] {
                        assert_eq!(trace[product][lane]["constructor_calls"], 0);
                    }
                }
            }
            assert_receipts(&receipts());
            assert_cleanup(&value);
        }

        reset();
        let cached = scenario(
            fixture,
            Mode::ActualSelectedShared,
            authored,
            start,
            frames,
            Fault::CacheLane,
        );
        assert_eq!(cached["read"]["output"]["status"], "Ok");
        assert!(cached["provider_calls"].as_u64().unwrap() > 0);
        assert_eq!(
            cached["read"]["after_return_drop"]["plan_work"], cached["read"]["before"]["plan_work"],
            "cache-scoped work does not debit the plan lane"
        );
        assert!(
            cached["read"]["after_return_drop"]["cache_plan_work"]
                .as_u64()
                .unwrap()
                > cached["read"]["before"]["cache_plan_work"]
                    .as_u64()
                    .unwrap()
        );
        assert_receipts(&receipts());
        assert_cleanup(&cached);
    }
}

#[test]
fn retained_later_owner_refusal_drops_completed_fade_without_refunding_work() {
    let fixtures = cases::fixtures();
    let fixture = &fixtures[0];
    let (start, frames) = fixture.windows[1];
    reset();
    let success = scenario(
        fixture,
        Mode::OrdinaryProbe,
        true,
        start,
        frames,
        Fault::None,
    );
    assert_eq!(success["read"]["output"]["status"], "Ok");
    let measured = receipts();
    assert_receipts(&measured);
    let total = |product| {
        measured[product]["plan"]["canonical_units"]
            .as_u64()
            .unwrap()
            + measured[product]["plan"]["additional_units"]
                .as_u64()
                .unwrap()
    };
    let input_work = success["retained"]["retained_work"].as_u64().unwrap();
    let fade = total("fade");
    let owners = total("owners");
    assert!(fade > 0 && owners > 0);
    for (allowance, failed_product) in [
        (input_work + fade - 1, "fade"),
        (input_work + fade + owners - 1, "owners"),
    ] {
        reset();
        let value = scenario(
            fixture,
            Mode::OrdinaryProbe,
            true,
            start,
            frames,
            Fault::Remaining(usize::try_from(allowance).unwrap()),
        );
        let receipt = receipts();
        assert_receipts(&receipt);
        assert_cleanup(&value);
        assert_eq!(value["read"]["output"]["status"], "Error");
        assert_eq!(value["provider_calls"], 0);
        assert_eq!(receipt[failed_product]["plan"]["failed_queries"], 1);
        if failed_product == "owners" {
            assert_eq!(receipt["fade"]["plan"]["completed_queries"], 1);
            assert_eq!(
                receipt["fade"]["plan"]["canonical_units"],
                measured["fade"]["plan"]["canonical_units"]
            );
            assert_eq!(
                receipt["fade"]["plan"]["additional_units"],
                measured["fade"]["plan"]["additional_units"]
            );
        }
    }
}

#[test]
fn retained_legacy_scope_is_independent_and_whole_original_legacy_chains_it() {
    let fixtures = cases::fixtures();
    let fixture = &fixtures[0];
    let (start, frames) = fixture.windows[1];
    let run = || {
        scenario(
            fixture,
            Mode::OrdinaryProbe,
            true,
            start,
            frames,
            Fault::None,
        )
    };
    reset();
    let value = original_request::with_legacy_fade_queries(run);
    assert_eq!(value["read"]["output"]["status"], "Ok");
    assert_eq!(receipts()["fade"]["plan"]["completed_queries"], 1);
    for whole_original in [false, true] {
        reset();
        let value = if whole_original {
            original_request::with_legacy_original_queries(run)
        } else {
            retained_request::with_legacy_retained_queries(run)
        };
        assert_eq!(value["read"]["output"]["status"], "Ok");
        assert_eq!(receipts()["fade"]["plan"]["constructor_calls"], 0);
        assert_eq!(receipts()["owners"]["plan"]["constructor_calls"], 0);
        assert_cleanup(&value);
    }
    reset();
    run();
    assert_eq!(receipts()["fade"]["plan"]["completed_queries"], 1);
}
