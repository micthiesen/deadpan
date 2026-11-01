//! Literal protocol callback refusal prefixes from bounded actual Signal constructors.
use super::*;

fn run_failures(signal: &AudioSignal<'_>, samples: Range<SignalSample>, mode: Mode) -> Value {
    let limits = AudioQueryLimits::default();
    let mut backend = RecordingBackend::new(Fault::None);
    let observer = backend.observer();
    let result = admitted_signal(signal, samples.clone(), limits, mode, &mut backend);
    let baseline = signal_snapshot(&result);
    let live = observer.snapshot();
    let baseline_events = observer.events();
    let peak = live["peak"].as_u64().unwrap() as usize;
    let spends = observer.spends();
    let reservations = observer.reservations();
    assert!(peak > 0 && spends > 0 && reservations > 0);
    drop(result);
    observer.assert_released();
    let baseline_after = observer.snapshot();

    let mut exact = RecordingBackend::new(Fault::Cap(peak));
    let exact_observer = exact.observer();
    let exact_result = admitted_signal(signal, samples.clone(), limits, mode, &mut exact);
    let exact_snapshot = signal_snapshot(&exact_result);
    assert_eq!(exact_snapshot, baseline);
    assert_eq!(exact_observer.snapshot()["peak"], peak);
    let exact_live = exact_observer.snapshot();
    drop(exact_result);
    exact_observer.assert_released();

    let mut below = RecordingBackend::new(Fault::Cap(peak - 1));
    let below_observer = below.observer();
    let below_result = admitted_signal(signal, samples.clone(), limits, mode, &mut below);
    assert!(matches!(
        &below_result,
        Err(PlanError::AudioQueryLimit("test wide allocation refusal"))
    ));
    let below_snapshot = signal_snapshot(&below_result);
    drop(below_result);
    below_observer.assert_released();
    assert!(below_observer.snapshot()["peak"].as_u64().unwrap() < peak as u64);
    let below_events = below_observer.events();
    assert_eq!(below_events.last().unwrap()["event"], "reserve");
    assert_eq!(below_events.last().unwrap()["accepted"], false);

    let mut refusals = Vec::new();
    for (operation, count) in [("spend", spends), ("reserve", reservations)] {
        for at in 1..=count {
            let fault = if operation == "spend" {
                Fault::RefuseSpend(at)
            } else {
                Fault::RefuseReserve(at)
            };
            let mut backend = RecordingBackend::new(fault);
            let observer = backend.observer();
            let result = admitted_signal(signal, samples.clone(), limits, mode, &mut backend);
            let message = if operation == "spend" {
                "test wide work refusal"
            } else {
                "test wide reservation refusal"
            };
            assert!(matches!(&result,Err(PlanError::AudioQueryLimit(value)) if *value==message));
            let returned = signal_snapshot(&result);
            drop(result);
            observer.assert_released();
            let events = observer.events();
            let nth = baseline_events
                .iter()
                .enumerate()
                .filter(|(_, event)| event["event"] == operation)
                .nth(at - 1)
                .unwrap()
                .0;
            assert_eq!(events.len(), nth + 1, "no callback after actual refusal");
            assert_eq!(
                events[..nth],
                baseline_events[..nth],
                "literal accepted callback prefix"
            );
            let failed = events.last().unwrap();
            assert_eq!(failed["event"], operation);
            assert_eq!(failed["accepted"], false);
            assert_eq!(
                if operation == "spend" {
                    observer.spends()
                } else {
                    observer.reservations()
                },
                at
            );
            refusals.push(json!({"operation":operation,"at":at,"returned":returned,
                "after_cleanup":observer.snapshot(),"events":events}));
        }
    }

    let cancellation = if let Some(event) = baseline_events.iter().find(|event| {
        event["event"] == "spend"
            && event["before"]["live"].as_u64().unwrap() > 0
            && event["before"]["forwarded"].as_u64().unwrap() > 0
    }) {
        let threshold = event["before"]["forwarded"].as_u64().unwrap() as usize;
        let mut backend = RecordingBackend::new(Fault::CancelAfter(threshold));
        let observer = backend.observer();
        let result = admitted_signal(signal, samples.clone(), limits, mode, &mut backend);
        assert!(matches!(
            &result,
            Err(PlanError::AudioQueryLimit("test wide cancellation"))
        ));
        let returned = signal_snapshot(&result);
        drop(result);
        observer.assert_released();
        let events = observer.events();
        let stopped = events.last().unwrap();
        assert_eq!(stopped["event"], "spend");
        assert_eq!(stopped["accepted"], false);
        assert!(stopped["before"]["live"].as_u64().unwrap() > 0);
        assert_eq!(
            stopped["before"]["forwarded"].as_u64().unwrap() as usize,
            threshold
        );
        json!({"threshold":threshold,"returned":returned,"after_cleanup":observer.snapshot(),"events":events})
    } else {
        json!({"not_reached":"baseline actual callback sequence has no spend with live storage"})
    };
    let mut rotations = Vec::new();
    for (operation, fault) in [
        ("spend", Fault::RotateSpend(1)),
        ("reserve", Fault::RotateReserve(1)),
    ] {
        let mut backend = RecordingBackend::new(fault);
        let observer = backend.observer();
        let result = admitted_signal(signal, samples.clone(), limits, mode, &mut backend);
        assert!(matches!(
            &result,
            Err(PlanError::AudioMetadataIssuerMismatch)
        ));
        let returned = signal_snapshot(&result);
        drop(result);
        observer.assert_released();
        assert_eq!(observer.events().last().unwrap()["issuer_changed"], true);
        rotations.push(json!({"operation":operation,"returned":returned,
            "after_cleanup":observer.snapshot(),"events":observer.events()}));
    }
    json!({"samples":samples,"mode":mode.name(),"limits":limits_value(limits),
        "baseline":baseline,"baseline_live":live,"baseline_after_drop":baseline_after,"baseline_events":baseline_events,
        "spend_callbacks":spends,"reservation_callbacks":reservations,"measured_peak_bytes":peak,
        "exact_peak_cap":{"returned":exact_snapshot,"live":exact_live,
            "after_drop":exact_observer.snapshot(),"events":exact_observer.events()},
        "one_byte_below":{"returned":below_snapshot,"after_cleanup":below_observer.snapshot(),"events":below_events},
        "refusals":refusals,"cancellation":cancellation,"issuer_rotations":rotations})
}

pub(super) fn body() -> Value {
    let frozen = signal_frozen();
    let mut structural = false;
    let mut source_output = false;
    let mut projected_error = false;
    let mut rows = Vec::new();
    visit_cases(|plan, name, signal, _| {
        let case = signal_case(&frozen, name);
        let input = signal.provider_before_snapshot(plan);
        let provider = &input["provider"];
        let samples = sample_range(&case["geometry"]["full_samples"]);
        let kind = if !structural && provider["tag"] == "Structural" {
            structural = true;
            "Structural"
        } else if !source_output && provider["tag"] == "Source" && provider["apply_holds"] == true {
            source_output = true;
            "Source output"
        } else if !projected_error
            && provider["tag"] == "Projected"
            && case["full_public_policy_probe"]["result"] == "Error"
        {
            projected_error = true;
            "Projected late policy error"
        } else {
            return;
        };
        for mode in [Mode::Policy, Mode::AfterPreserve] {
            let ordinary = public_policy(
                signal,
                samples.clone(),
                Default::default(),
                mode.after_preserve(),
            );
            let expected = policy_result(&ordinary);
            drop(ordinary);
            let receipt = run_failures(signal, samples.clone(), mode);
            assert_signal_data(&expected, &receipt["baseline"]);
            rows.push(json!({"case":name,"representative":kind,"input_before":input,
                "ordinary":expected,"receipt":receipt,"input_after":signal.provider_before_snapshot(plan)}));
        }
        assert_eq!(signal.provider_before_snapshot(plan), input);
    });
    assert!(
        structural && source_output && projected_error,
        "actual frozen39 representatives must be exercised"
    );
    assert_eq!(rows.len(), 6);
    assert!(
        rows.iter()
            .any(|row| row["receipt"]["cancellation"]["threshold"].is_u64())
    );
    json!({"rows":rows,"scope":"real existing H Policy/Lease backend; each actual spend/reserve refused in turn;
        exact measured Plan allocation peak and one-byte-short refusal; no native account or seeded accepted work"})
}

#[test]
fn standalone_signal_factory_stops_at_each_actual_refusal_and_live_cancellation() {
    body();
}
