//! Public Signal factories paired with literal frozen ordinary PointCeil entry results.
use super::*;
use crate::AdmittedSignalPolicyQuery;
use crate::plan::audio_owners::wide_gain_query_admission_tests::provider_child_backend::{
    Backend as RecordingBackend, Fault, Lease as RecordingLease, Observer as RecordingObserver,
};
use crate::plan::audio_policy::query::provider_child_probe as probe;

#[path = "audio_signal_policy_failure_tests.rs"]
mod failure_tests;

const SIGNAL_FROZEN: &[u8] =
    include_bytes!("../tests/fixtures/signal-policy-baseline-v1/snapshot.json");
type SignalQuery = AdmittedSignalPolicyQuery<usize, RecordingLease>;

#[derive(Clone, Copy)]
enum Mode {
    Policy,
    AfterPreserve,
}
impl Mode {
    fn after_preserve(self) -> bool {
        matches!(self, Self::AfterPreserve)
    }
    fn name(self) -> &'static str {
        match self {
            Self::Policy => "policy",
            Self::AfterPreserve => "after_preserve",
        }
    }
    fn old_key(self) -> &'static str {
        match self {
            Self::Policy => "public_policy",
            Self::AfterPreserve => "public_policy_after_preserve",
        }
    }
}

fn signal_frozen() -> Value {
    serde_json::from_slice(SIGNAL_FROZEN).unwrap()
}
fn signal_case<'a>(frozen: &'a Value, name: &str) -> &'a Value {
    frozen["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == name)
        .unwrap_or_else(|| panic!("actual Signal not frozen: {name}"))
}
fn admitted_signal(
    signal: &AudioSignal<'_>,
    samples: Range<SignalSample>,
    limits: AudioQueryLimits,
    mode: Mode,
    backend: &mut RecordingBackend,
) -> Result<SignalQuery, PlanError> {
    match mode {
        Mode::Policy => signal.policy_admitted(samples, limits, backend),
        Mode::AfterPreserve => signal.policy_after_preserve_admitted(samples, limits, backend),
    }
}
fn signal_admission_error(value: &PlanError) -> Value {
    match value {
        PlanError::AudioMetadataIssuerMismatch => json!({"variant":"AudioMetadataIssuerMismatch"}),
        other => error(other),
    }
}
fn signal_snapshot(result: &Result<SignalQuery, PlanError>) -> Value {
    match result {
        Ok(value) => probe::snapshot(value),
        Err(value) => json!({"result":"Error","error":signal_admission_error(value),
            "error_local_canonical_additional":"unexposed by public Signal constructor; accepted callback prefix recorded separately"}),
    }
}
fn limits_value(value: AudioQueryLimits) -> Value {
    json!({"maximum_spans":value.maximum_spans,"maximum_work":value.maximum_work})
}
fn assert_signal_data(old: &Value, new: &Value) {
    assert_eq!(
        old["result"], new["result"],
        "ordinary={old}; admitted={new}"
    );
    if old["result"] == "Error" {
        assert_eq!(old["error"], new["error"]);
    } else {
        for key in ["suppressed", "contents", "lookup"] {
            assert_eq!(
                old["query"][key], new["query"][key],
                "actual returned {key}"
            );
        }
        for key in ["suppressed_capacity", "contents_capacity"] {
            assert!(old["query"][key].is_u64() && new["query"][key].is_u64());
        }
    }
}
fn assert_signal_work(old: &Value, query: &SignalQuery, observer: &RecordingObserver) {
    assert_eq!(
        query.canonical_units(),
        old["query"]["work"].as_u64().unwrap() as usize
    );
    assert_eq!(
        query.work(),
        query.canonical_units() + query.additional_units()
    );
    assert_eq!(query.work(), query.view().work);
    assert!(query.was_forwarded());
    assert_eq!(observer.forwarded(), query.work());
}

fn paired_body() -> Value {
    let frozen = signal_frozen();
    let mut rows = Vec::new();
    let mut inputs = Vec::new();
    let mut signals = 0;
    let mut requests = 0;
    let setups = visit_cases(|plan, name, signal, _| {
        signals += 1;
        let case = signal_case(&frozen, name);
        let before = signal.provider_before_snapshot(plan);
        assert_eq!(before, case["signal_before"]);
        for request in case["requests"].as_array().unwrap() {
            requests += 1;
            let samples = request_range(request);
            let original = request_limits(request);
            for mode in [Mode::Policy, Mode::AfterPreserve] {
                for (lane, limits) in [
                    ("original", original),
                    (
                        "default_work",
                        AudioQueryLimits {
                            maximum_work: AudioQueryLimits::default().maximum_work,
                            ..original
                        },
                    ),
                ] {
                    assert_eq!(limits.maximum_spans, original.maximum_spans);
                    let ordinary =
                        public_policy(signal, samples.clone(), limits, mode.after_preserve());
                    let old = policy_result(&ordinary);
                    drop(ordinary);
                    let frozen_witness = case["requests"].as_array().unwrap().iter().find(|row| {
                        row["samples"] == request["samples"]
                            && row["limits"] == limits_value(limits)
                    });
                    if let Some(witness) = frozen_witness {
                        assert_eq!(old, witness[mode.old_key()]);
                    }
                    if lane == "original" {
                        assert_eq!(old, request[mode.old_key()]);
                    }
                    assert_eq!(signal.provider_before_snapshot(plan), before);

                    let unmetered = signal.signal_policy_unmetered_test_receipt(
                        samples.clone(),
                        limits,
                        mode.after_preserve(),
                    );
                    assert_signal_data(&old, &unmetered["returned"]);
                    if old["result"] == "Ok" {
                        assert_eq!(unmetered["returned"]["query"], old["query"]);
                        assert_eq!(unmetered["returned"]["canonical"], old["query"]["work"]);
                        assert_eq!(unmetered["returned"]["additional"], 0);
                        assert_eq!(unmetered["returned"]["forwarded"], false);
                    }
                    let mut backend = RecordingBackend::new(Fault::None);
                    let observer = backend.observer();
                    let result =
                        admitted_signal(signal, samples.clone(), limits, mode, &mut backend);
                    let actual = signal_snapshot(&result);
                    let live = observer.snapshot();
                    if let Err(error) = &result
                        && (old["result"] == "Ok" || old["error"] != actual["error"])
                    {
                        assert!(
                            matches!(error, PlanError::AudioQueryLimit(_)),
                            "new earlier refusal must remain a resource limit: {actual}"
                        );
                    }
                    if lane == "default_work" {
                        assert_signal_data(&old, &actual);
                    }
                    if let Ok(query) = &result {
                        assert_signal_data(&old, &actual);
                        assert_signal_work(&old, query, &observer);
                    }
                    drop(result);
                    observer.assert_released();
                    assert_eq!(signal.provider_before_snapshot(plan), before);
                    rows.push(json!({"case":name,"request":request["name"],"samples":samples,"mode":mode.name(),
                        "lane":lane,"limits":limits_value(limits),"frozen_same_range_and_limits":frozen_witness.is_some(),
                        "ordinary":old,"unmetered":unmetered,"admitted":actual,"while_result_live":live,
                        "after_result_drop":observer.snapshot(),"events":observer.events()}));
                }
            }
        }
        let after = signal.provider_before_snapshot(plan);
        assert_eq!(after, before);
        assert_eq!(after, case["signal_after"]);
        inputs.push(json!({"case":name,"before":before,"after":after}));
    });
    assert_eq!(signals, 16);
    assert_eq!(signals, frozen["cases"].as_array().unwrap().len());
    assert_eq!(requests, 207);
    assert_eq!(requests, frozen["request_count"].as_u64().unwrap() as usize);
    assert_eq!(rows.len(), requests * 4);
    json!({"schema":"admitted-public-signal-policy-pairs-v1","signals":signals,"requests":requests,
        "mode_and_limit_lanes":rows.len(),"setups":setups,"inputs":inputs,"rows":rows,
        "scope":{"entry":"actual public AudioSignal facade, PointCeil modes true/false",
            "original":"all frozen39 requests at original limits; new earlier resource refusals retained",
            "default_work":"separate actual ordinary/admitted lane with unchanged original span cap",
            "work":"success canonical equals literal ordinary work; additional and accepted total remain distinct",
            "errors":"constructor-local canonical/additional are unexposed; actual accepted prefix only",
            "storage":"literal capacities and existing H Plan Policy leases; no native account claim"}})
}

#[test]
fn standalone_signal_factories_pair_all_frozen_original_and_default_work_requests() {
    assert_eq!(SIGNAL_FROZEN.len(), 816_706);
    paired_body();
}

fn custody_body() -> Value {
    let frozen = signal_frozen();
    let mut rows = Vec::new();
    let mut positive = 0;
    let mut zero_capacity = 0;
    visit_cases(|plan, name, signal, _| {
        let before = signal.provider_before_snapshot(plan);
        let full = sample_range(&signal_case(&frozen, name)["geometry"]["full_samples"]);
        for samples in [full.clone(), full.start..full.start] {
            for mode in [Mode::Policy, Mode::AfterPreserve] {
                let receipt = signal
                    .signal_policy_custody_test_receipt(samples.clone(), mode.after_preserve());
                if receipt["custody"]["handoff"] == "Committed" {
                    if receipt["custody"]["capacity"].as_u64().unwrap() > 0 {
                        positive += 1;
                    } else {
                        zero_capacity += 1;
                    }
                }
                rows.push(
                    json!({"case":name,"samples":samples,"mode":mode.name(),"receipt":receipt}),
                );
            }
        }
        assert_eq!(signal.provider_before_snapshot(plan), before);
    });
    assert!(positive > 0 && zero_capacity > 0);
    json!({"positive":positive,"zero_capacity":zero_capacity,"rows":rows,
        "scope":"existing Plan protocol prepare/commit and lease transfer, no native Account/Box"})
}

#[test]
fn standalone_signal_origin_custody_preserves_positive_and_empty_final_allocations() {
    custody_body();
}

fn held_body() -> Value {
    let frozen = signal_frozen();
    let mut held = None;
    let mut saved = None;
    let mut state = None;
    let mut selected = None;
    visit_cases(|plan, name, signal, _| {
        if held.is_some() {
            return;
        }
        let full = sample_range(&signal_case(&frozen, name)["geometry"]["full_samples"]);
        let mut backend = RecordingBackend::new(Fault::None);
        let observer = backend.observer();
        let result = admitted_signal(signal, full, Default::default(), Mode::Policy, &mut backend);
        let query = match result {
            Ok(query) if !query.view().suppressed.is_empty() => query,
            other => {
                drop(other);
                observer.assert_released();
                return;
            }
        };
        let snapshot = probe::snapshot(&query);
        assert!(observer.live() > 0);
        assert_eq!(
            signal.provider_before_snapshot(plan),
            signal_case(&frozen, name)["signal_before"]
        );
        drop(backend);
        assert_eq!(probe::snapshot(&query), snapshot);
        held = Some(query);
        saved = Some(snapshot);
        state = Some(observer);
        selected = Some(name.to_owned());
    });
    let query = held.unwrap();
    let observer = state.unwrap();
    assert!(observer.live() > 0);
    let after = probe::snapshot(&query);
    assert_eq!(&after, saved.as_ref().unwrap());
    let live = observer.snapshot();
    let accepted = observer.forwarded();
    drop(query);
    observer.assert_released();
    assert_eq!(observer.forwarded(), accepted);
    json!({"case":selected,"before_backend_and_fixture_drop":saved,"after_backend_and_fixture_drop":after,
        "held":live,"released":observer.snapshot(),"events":observer.events(),
        "owners_dropped":"recording backend and all actual visit_cases Signal/plan/projection builders"})
}

#[test]
fn standalone_signal_result_outlives_backend_and_all_actual_fixture_builders() {
    held_body();
}

#[test]
#[ignore = "manual actual public Signal paired, failure and custody capture; fresh /tmp output"]
fn capture_admitted_public_signal_policy_pairs_v1() {
    let output = std::env::var("DEADPAN_SIGNAL_POLICY_PAIRED_OUTPUT").expect("fresh output");
    let path = Path::new(&output);
    assert!(
        path.is_absolute()
            && path
                .parent()
                .unwrap()
                .canonicalize()
                .unwrap()
                .starts_with(Path::new("/tmp").canonicalize().unwrap())
    );
    let variant = std::env::var("DEADPAN_SIGNAL_POLICY_PAIRED_VARIANT").unwrap();
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source = std::env::var("DEADPAN_SIGNAL_POLICY_PAIRED_SOURCE_SHA256").unwrap();
    assert!(source.len() == 64 && source.bytes().all(|v| v.is_ascii_hexdigit()));
    assert_eq!(
        std::env::var("DEADPAN_SIGNAL_POLICY_PAIRED_PUBLISHED_CHECKPOINT").unwrap(),
        SIGNAL_CHECKPOINT
    );
    let body = json!({"schema":"admitted-public-signal-policy-qualification-v1","variant":variant,
        "source_sha256":source,"published_checkpoint":SIGNAL_CHECKPOINT,"paired":paired_body(),
        "failures":failure_tests::body(),"custody":custody_body(),"held_result":held_body()});
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .unwrap();
    let mut writer = std::io::BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, &body).unwrap();
    writer.write_all(b"\n").unwrap();
    writer.flush().unwrap();
    writer.get_ref().sync_all().unwrap();
}
