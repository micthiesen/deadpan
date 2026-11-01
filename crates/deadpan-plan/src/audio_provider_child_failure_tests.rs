//! Failure-prefix cleanup and guarded-result lifetime on checked public providers.
use super::*;
use serde_json::json;

fn full_request(signal: &Value) -> (&Value, AudioBoundaryRule) {
    let grid = &signal["grids"][0];
    let request = grid["requests"]
        .as_array()
        .unwrap()
        .iter()
        .find(|request| request["name"] == "full")
        .unwrap();
    (request, rule(&grid["rule"]))
}

fn assert_refused_event(observer: &Observer, kind: &str) {
    assert!(
        observer
            .events()
            .iter()
            .any(|event| event["event"] == kind && event["accepted"] == false),
        "the configured backend refusal must be observed at its real callback"
    );
}

#[test]
fn checked_source_silence_and_projected_children_release_every_real_failure_prefix() {
    let mut exercised = 0;
    visit_fixtures(|_plan, name, signal, frozen_signal| {
        if !matches!(
            (
                name,
                frozen_signal["signal_before"]["provider"]["tag"].as_str()
            ),
            ("voice_input", Some("Source"))
                | ("neutral_policy_run_0", Some("Silence"))
                | ("intrinsic_projected_run_0", Some("Projected"))
        ) {
            return;
        }
        let (request, boundary) = full_request(frozen_signal);
        let samples = request_range(request);
        let limits = request_limits(request);

        let mut baseline = Backend::new(Fault::None);
        let baseline_observer = baseline.observer();
        let (baseline_result, baseline_budget) = run_current(
            signal,
            samples.clone(),
            limits,
            boundary,
            (false, false),
            &mut baseline,
        );
        let baseline_value = baseline_result.unwrap();
        assert_eq!(
            baseline_budget["attempted_canonical"].as_u64().unwrap() as usize
                + baseline_budget["attempted_additional"].as_u64().unwrap() as usize,
            baseline_observer.forwarded(),
            "successful provider query forwards its actual canonical and additional usage"
        );
        let reservations = baseline_observer.reservations();
        let spends = baseline_observer.spends();
        assert!(reservations > 0, "{} has guarded returned storage", name);
        assert!(spends > 0, "{} reaches actual work admission", name);
        drop(baseline_value);
        baseline_observer.assert_released();

        for index in 1..=reservations {
            let mut backend = Backend::new(Fault::RefuseReserve(index));
            let observer = backend.observer();
            let (result, _) = run_current(
                signal,
                samples.clone(),
                limits,
                boundary,
                (false, false),
                &mut backend,
            );
            assert!(
                matches!(
                    result,
                    Err(PlanError::AudioQueryLimit("test wide reservation refusal"))
                ),
                "{name} reservation callback {index}"
            );
            assert_refused_event(&observer, "reserve");
            drop(result);
            observer.assert_released();
        }
        for index in 1..=spends {
            let mut backend = Backend::new(Fault::RefuseSpend(index));
            let observer = backend.observer();
            let (result, _) = run_current(
                signal,
                samples.clone(),
                limits,
                boundary,
                (false, false),
                &mut backend,
            );
            assert!(
                matches!(
                    result,
                    Err(PlanError::AudioQueryLimit("test wide work refusal"))
                ),
                "{name} work callback {index}"
            );
            assert_refused_event(&observer, "spend");
            drop(result);
            observer.assert_released();
        }
        let peak = baseline_observer.snapshot()["peak"].as_u64().unwrap() as usize;
        if peak > 0 {
            let mut backend = Backend::new(Fault::Cap(peak - 1));
            let observer = backend.observer();
            let (result, _) = run_current(
                signal,
                samples,
                limits,
                boundary,
                (false, false),
                &mut backend,
            );
            assert!(matches!(
                result,
                Err(PlanError::AudioQueryLimit("test wide allocation refusal"))
            ));
            drop(result);
            observer.assert_released();
        }
        exercised += 1;
    });
    assert_eq!(exercised, 3);
}

#[test]
fn actual_public_provider_paths_reject_rotated_issuers_and_release_the_original_ledger() {
    let mut current_cases = 0;
    visit_fixtures(|_plan, name, signal, frozen_signal| {
        if !matches!(
            (
                name,
                frozen_signal["signal_before"]["provider"]["tag"].as_str()
            ),
            ("voice_input", Some("Source"))
                | ("neutral_policy_run_0", Some("Silence"))
                | ("intrinsic_projected_run_0", Some("Projected"))
        ) {
            return;
        }
        let (request, boundary) = full_request(frozen_signal);
        let samples = request_range(request);
        let limits = request_limits(request);
        for fault in [Fault::RotateReserve(1), Fault::RotateSpend(1)] {
            let mut backend = Backend::new(fault);
            let observer = backend.observer();
            let (result, _) = run_current(
                signal,
                samples.clone(),
                limits,
                boundary,
                (false, false),
                &mut backend,
            );
            assert!(matches!(
                result,
                Err(PlanError::AudioMetadataIssuerMismatch)
            ));
            let events = observer.events();
            assert!(events.iter().any(|event| event["issuer_changed"] == true));
            drop(result);
            observer.assert_released();
        }
        current_cases += 1;
    });
    assert_eq!(current_cases, 3);

    let mut policy_cases = 0;
    visit_fixtures(|_plan, name, signal, frozen_signal| {
        if name != "voice_output" {
            return;
        }
        let (request, boundary) = full_request(frozen_signal);
        let samples = request_range(request);
        let limits = request_limits(request);
        for fault in [Fault::RotateReserve(1), Fault::RotateSpend(1)] {
            let mut backend = Backend::new(fault);
            let observer = backend.observer();
            let (result, _) = run_policy(
                signal,
                samples.clone(),
                limits,
                boundary,
                true,
                &mut backend,
            );
            assert!(matches!(
                result,
                Err(PlanError::AudioMetadataIssuerMismatch)
            ));
            assert!(
                observer
                    .events()
                    .iter()
                    .any(|event| event["issuer_changed"] == true)
            );
            drop(result);
            observer.assert_released();
        }
        policy_cases += 1;
    });
    assert_eq!(policy_cases, 1);
}

#[test]
fn actual_source_results_keep_their_leases_after_backend_drop() {
    let mut exercised = 0;
    visit_fixtures(|_plan, name, signal, frozen_signal| {
        if !matches!(
            (
                name,
                frozen_signal["signal_before"]["provider"]["tag"].as_str()
            ),
            ("voice_input", Some("Source"))
                | ("neutral_policy_run_0", Some("Silence"))
                | ("intrinsic_projected_run_0", Some("Projected"))
        ) {
            return;
        }
        let (request, boundary) = full_request(frozen_signal);
        let mut backend = Backend::new(Fault::None);
        let observer = backend.observer();
        let (result, budget) = run_current(
            signal,
            request_range(request),
            request_limits(request),
            boundary,
            (false, false),
            &mut backend,
        );
        let result = result.unwrap();
        assert!(observer.live() > 0, "returned guard owns admitted storage");
        assert_eq!(
            result.query.work,
            budget["attempted_canonical"].as_u64().unwrap() as usize
                + budget["attempted_additional"].as_u64().unwrap() as usize
        );
        drop(backend);
        assert!(observer.live() > 0, "result outlives the temporary backend");
        drop(result);
        observer.assert_released();
        if name == "intrinsic_projected_run_0" {
            let mut backend = Backend::new(Fault::None);
            let observer = backend.observer();
            let start = request_range(request).start;
            let (empty, budget) = run_current(
                signal,
                start..start,
                request_limits(request),
                boundary,
                (false, false),
                &mut backend,
            );
            let empty = empty.unwrap();
            assert!(empty.query.spans.is_empty());
            assert_eq!(budget["attempted_canonical"], 1);
            assert_eq!(
                budget["attempted_additional"].as_u64().unwrap() as usize,
                observer.forwarded() - 1,
                "actual empty-query header allocations remain charged separately"
            );
            drop(empty);
            observer.assert_released();
        }
        exercised += 1;
    });
    assert_eq!(exercised, 3);
}

#[test]
fn source_output_carrier_failures_release_all_prefixes() {
    let mut exercised = 0;
    visit_fixtures(|_plan, name, signal, frozen_signal| {
        if name != "voice_output"
            || frozen_signal["signal_before"]["provider"]["tag"] != "Source"
            || frozen_signal["signal_before"]["provider"]["apply_holds"] != true
        {
            return;
        }
        let (request, boundary) = full_request(frozen_signal);
        let samples = request_range(request);
        let limits = request_limits(request);
        let mut baseline = Backend::new(Fault::None);
        let baseline_observer = baseline.observer();
        let (baseline_result, _) = run_policy(
            signal,
            samples.clone(),
            limits,
            boundary,
            true,
            &mut baseline,
        );
        let baseline_result = baseline_result.unwrap();
        let baseline_value = policy_snapshot(&baseline_result);
        assert_eq!(baseline_value["result"], "Ok");
        let reservations = baseline_observer.reservations();
        let spends = baseline_observer.spends();
        assert!(baseline_observer.live() > 0);
        drop(baseline);
        assert!(
            baseline_observer.live() > 0,
            "the Source output policy result outlives its temporary backend"
        );
        drop(baseline_result);
        baseline_observer.assert_released();

        for index in 1..=reservations {
            let mut backend = Backend::new(Fault::RefuseReserve(index));
            let observer = backend.observer();
            let (result, _) = run_policy(
                signal,
                samples.clone(),
                limits,
                boundary,
                true,
                &mut backend,
            );
            assert!(
                matches!(
                    result,
                    Err(PlanError::AudioQueryLimit("test wide reservation refusal"))
                ),
                "Source output carrier reservation callback {index}"
            );
            assert_refused_event(&observer, "reserve");
            drop(result);
            observer.assert_released();
        }
        for index in 1..=spends {
            let mut backend = Backend::new(Fault::RefuseSpend(index));
            let observer = backend.observer();
            let (result, _) = run_policy(
                signal,
                samples.clone(),
                limits,
                boundary,
                true,
                &mut backend,
            );
            assert!(
                matches!(
                    result,
                    Err(PlanError::AudioQueryLimit("test wide work refusal"))
                ),
                "Source output carrier work callback {index}"
            );
            assert_refused_event(&observer, "spend");
            drop(result);
            observer.assert_released();
        }
        exercised += 1;
    });
    assert_eq!(exercised, 1);
}

#[test]
fn actual_source_paths_cancel_at_zero_and_measured_interior_prefixes() {
    let mut current_cases = 0;
    visit_fixtures(|_plan, name, signal, frozen_signal| {
        if name != "voice_input" {
            return;
        }
        let (request, boundary) = full_request(frozen_signal);
        let samples = request_range(request);
        let limits = request_limits(request);
        let mut backend = Backend::new(Fault::CancelAfter(0));
        let observer = backend.observer();
        let (result, _) = run_current(
            signal,
            samples,
            limits,
            boundary,
            (false, false),
            &mut backend,
        );
        assert!(matches!(
            result,
            Err(PlanError::AudioQueryLimit("test wide cancellation"))
        ));
        assert_eq!(
            observer.forwarded(),
            0,
            "cancellation before the first accepted unit"
        );
        drop(result);
        observer.assert_released();
        current_cases += 1;
    });
    assert_eq!(current_cases, 1);

    let mut output_cases = 0;
    visit_fixtures(|_plan, name, signal, frozen_signal| {
        if name != "voice_output" {
            return;
        }
        let (request, boundary) = full_request(frozen_signal);
        let samples = request_range(request);
        let limits = request_limits(request);
        let mut baseline = Backend::new(Fault::None);
        let baseline_observer = baseline.observer();
        let (baseline_result, _) = run_policy(
            signal,
            samples.clone(),
            limits,
            boundary,
            true,
            &mut baseline,
        );
        let baseline_result = baseline_result.unwrap();
        let total = baseline_observer.forwarded();
        let interior = baseline_observer
            .events()
            .iter()
            .find(|event| {
                event["event"] == "spend"
                    && event["accepted"] == true
                    && event["before"]["live"].as_u64().unwrap() > 0
            })
            .map(|event| event["before"]["forwarded"].as_u64().unwrap() as usize)
            .unwrap();
        assert!(interior > 0 && interior < total);
        drop(baseline_result);
        baseline_observer.assert_released();

        let mut backend = Backend::new(Fault::CancelAfter(interior));
        let observer = backend.observer();
        let (result, _) = run_policy(signal, samples, limits, boundary, true, &mut backend);
        assert!(matches!(
            result,
            Err(PlanError::AudioQueryLimit("test wide cancellation"))
        ));
        assert_eq!(
            observer.forwarded(),
            interior,
            "cancellation preserves the successful measured callback prefix"
        );
        assert_refused_event(&observer, "spend");
        assert!(observer.events().iter().any(|event| {
            event["event"] == "spend"
                && event["accepted"] == false
                && event["before"]["live"].as_u64().unwrap() > 0
                && event["before"]["forwarded"] == json!(interior)
        }));
        drop(result);
        observer.assert_released();
        output_cases += 1;
    });
    assert_eq!(output_cases, 1);
}

#[test]
fn actual_input_policy_succeeds_at_its_measured_work_while_output_source_needs_the_carrier_budget()
{
    let source_case = frozen()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "qualified_catalog_voice")
        .unwrap()
        .clone();
    let recipe = recipe_from(&source_case["observations"]["attempted_recipe"]);
    let mut exercised = 0;
    visit_fixtures(|plan, name, _signal, frozen_signal| {
        if name != "voice_input" {
            return;
        }
        let (request, boundary) = full_request(frozen_signal);
        let samples = request_range(request);
        let ordinary_limits = request_limits(request);
        let voice = plan.audio_signal().source_voice(recipe.clone()).unwrap();
        let input = voice.input_signal();
        let output = voice.output_signal();
        assert_eq!(
            input.provider_before_snapshot(plan),
            frozen_signal["signal_before"]
        );

        let mut measure = Backend::new(Fault::None);
        let measure_observer = measure.observer();
        let (measured, _) = run_policy(
            &input,
            samples.clone(),
            ordinary_limits,
            boundary,
            true,
            &mut measure,
        );
        let measured = measured.unwrap();
        let measured_snapshot = policy_snapshot(&measured);
        let exact_work = measured_snapshot["canonical"].as_u64().unwrap() as usize
            + measured_snapshot["additional"].as_u64().unwrap() as usize;
        assert_eq!(measure_observer.forwarded(), exact_work);
        drop(measured);
        measure_observer.assert_released();

        let exact = AudioQueryLimits {
            maximum_work: exact_work,
            ..ordinary_limits
        };
        let mut input_backend = Backend::new(Fault::None);
        let input_observer = input_backend.observer();
        let (input_result, input_budget) = run_policy(
            &input,
            samples.clone(),
            exact,
            boundary,
            true,
            &mut input_backend,
        );
        let input_result = input_result.unwrap();
        assert_eq!(input_observer.forwarded(), exact_work);
        assert_eq!(
            input_budget["remaining"].as_u64().unwrap(),
            0,
            "the input path consumes exactly its measured policy work"
        );
        drop(input_result);
        input_observer.assert_released();

        let mut output_backend = Backend::new(Fault::None);
        let output_observer = output_backend.observer();
        let (output_result, output_budget) =
            run_policy(&output, samples, exact, boundary, true, &mut output_backend);
        assert!(matches!(
            output_result,
            Err(PlanError::AudioQueryLimit("recursive policy work"))
        ));
        assert!(output_budget["attempted_canonical"].as_u64().unwrap() > 0);
        drop(output_result);
        output_observer.assert_released();
        exercised += 1;
    });
    assert_eq!(exercised, 1);
}

#[test]
fn source_output_at_frozen_input_work_limit_retains_its_actual_admission_prefix() {
    let source_case = frozen()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "qualified_catalog_voice")
        .unwrap()
        .clone();
    let pairs = source_case["observations"]["late_carrier_budget"]
        .as_array()
        .unwrap();
    let mut exercised = 0;
    visit_fixtures(|plan, name, signal, frozen_signal| {
        if name != "voice_output" {
            return;
        }
        for pair in pairs {
            assert_eq!(
                pair["output_at_same_current_budget"]["error"],
                json!({"variant":"AudioQueryLimit","budget":"recursive policy work"})
            );
            let samples = request_range(pair);
            let limits = request_limits(pair);
            let boundary = rule(&pair["rule"]);
            let mut backend = Backend::new(Fault::None);
            let observer = backend.observer();
            let (result, attempted) =
                run_policy(signal, samples, limits, boundary, true, &mut backend);
            assert!(
                matches!(result, Err(PlanError::AudioQueryLimit("structural work"))),
                "actual admission result: {:?}; budget: {attempted}; events: {:?}",
                result.as_ref().err(),
                observer.events()
            );
            assert_eq!(attempted["remaining"], 0);
            assert_eq!(attempted["attempted_canonical"], 2);
            assert_eq!(attempted["attempted_additional"], 1);
            assert_eq!(observer.forwarded(), 3);
            assert_eq!(observer.reservations(), 0);
            drop(result);
            observer.assert_released();
            exercised += 1;
        }
        assert_eq!(
            signal.provider_before_snapshot(plan),
            frozen_signal["signal_after"]
        );
    });
    assert_eq!(exercised, pairs.len());
}

#[test]
fn empty_output_source_still_runs_the_actual_carrier_path_without_inventing_spans() {
    let source_case = frozen()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "qualified_catalog_voice")
        .unwrap()
        .clone();
    let recipe = recipe_from(&source_case["observations"]["attempted_recipe"]);
    let mut exercised = 0;
    visit_fixtures(|plan, name, signal, frozen_signal| {
        if name != "voice_output" {
            return;
        }
        let (request, boundary) = full_request(frozen_signal);
        let start = request_range(request).start;
        let voice = plan.audio_signal().source_voice(recipe.clone()).unwrap();
        let input = voice.input_signal();
        let mut input_backend = Backend::new(Fault::None);
        let input_observer = input_backend.observer();
        let (input_result, _) = run_policy(
            &input,
            start..start,
            request_limits(request),
            boundary,
            true,
            &mut input_backend,
        );
        let input_result = input_result.unwrap();
        let input_reservations = input_observer.reservations();
        drop(input_result);
        input_observer.assert_released();

        let mut backend = Backend::new(Fault::None);
        let observer = backend.observer();
        let (result, _) = run_policy(
            signal,
            start..start,
            request_limits(request),
            boundary,
            true,
            &mut backend,
        );
        let result = result.unwrap();
        let snapshot = policy_snapshot(&result);
        assert!(
            snapshot["query"]["suppressed"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(snapshot["query"]["contents"].as_array().unwrap().is_empty());
        assert!(
            observer.reservations() > input_reservations,
            "empty output performs the extra structural-carrier header admissions"
        );
        assert_eq!(
            signal.provider_before_snapshot(plan),
            frozen_signal["signal_after"]
        );
        drop(result);
        observer.assert_released();
        exercised += 1;
    });
    assert_eq!(exercised, 1);
}
