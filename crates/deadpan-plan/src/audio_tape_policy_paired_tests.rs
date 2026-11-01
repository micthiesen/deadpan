//! Actual Tape factories paired with the independent ordinary Tape corpus.
use super::*;
use crate::audio_tape::policy_before::visit_cases;
use crate::plan::audio_owners::wide_gain_query_admission_tests::provider_child_backend::{
    Backend as RecordingBackend, Fault, Lease as RecordingLease, Observer as RecordingObserver,
};
use crate::{AudioPolicyQuery, AudioQueryLimits, AudioSignalTape};
use std::{fs::OpenOptions, io::Write, path::Path};

const FROZEN: &[u8] = include_bytes!("../tests/fixtures/tape-policy-baseline-v1/snapshot.json");
const CHECKPOINT: &str = "a6bc3de7e4e27f9fe11252c28cca07af385e4755";
type Query = AdmittedSignalPolicyQuery<usize, RecordingLease>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Policy,
    AfterPreserve,
}
impl Mode {
    fn name(self) -> &'static str {
        match self {
            Self::Policy => "policy",
            Self::AfterPreserve => "after_preserve",
        }
    }
    fn old_key(self) -> &'static str {
        match self {
            Self::Policy => "ordinary_policy",
            Self::AfterPreserve => "ordinary_policy_after_preserve",
        }
    }
}
fn frozen() -> Value {
    serde_json::from_slice(FROZEN).unwrap()
}
fn case<'a>(body: &'a Value, name: &str) -> &'a Value {
    body["families"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|f| f["tapes"].as_array().unwrap())
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("unfrozen actual Tape {name}"))
}
fn request_range(r: &Value) -> Range<SignalSample> {
    SignalSample(r["samples"]["start"].as_i64().unwrap())
        ..SignalSample(r["samples"]["end"].as_i64().unwrap())
}
fn request_limits(r: &Value) -> AudioQueryLimits {
    AudioQueryLimits {
        maximum_work: r["limits"]["maximum_work"].as_u64().unwrap() as usize,
        maximum_spans: r["limits"]["maximum_spans"].as_u64().unwrap() as usize,
    }
}
fn limits(l: AudioQueryLimits) -> Value {
    json!({"maximum_work":l.maximum_work,"maximum_spans":l.maximum_spans})
}
fn error(e: &PlanError) -> Value {
    e.tape_before_error_snapshot()
}
fn ordinary(
    t: &AudioSignalTape<'_>,
    r: Range<SignalSample>,
    l: AudioQueryLimits,
    m: Mode,
) -> Result<AudioPolicyQuery<SignalSample>, PlanError> {
    match m {
        Mode::Policy => t.policy(r, l),
        Mode::AfterPreserve => t.policy_after_preserve(r, l),
    }
}
fn admitted(
    t: &AudioSignalTape<'_>,
    r: Range<SignalSample>,
    l: AudioQueryLimits,
    m: Mode,
    b: &mut RecordingBackend,
) -> Result<Query, PlanError> {
    match m {
        Mode::Policy => t.policy_admitted(r, l, b),
        Mode::AfterPreserve => t.policy_after_preserve_admitted(r, l, b),
    }
}
fn ordinary_snapshot(v: &Result<AudioPolicyQuery<SignalSample>, PlanError>) -> Value {
    match v {
        Ok(v) => {
            let AudioPolicyQuery {
                suppressed,
                contents,
                lookup,
                work,
            } = v;
            json!({"result":"Ok","query":{"suppressed":suppressed,"suppressed_capacity":suppressed.capacity(),
                "contents":contents,"contents_capacity":contents.capacity(),"lookup":lookup,"work":work}})
        }
        Err(e) => json!({"result":"Error","error":error(e),
            "ordinary_partial_work":"not exposed by ordinary constructor errors"}),
    }
}
fn snapshot(v: &Result<Query, PlanError>) -> Value {
    match v {
        Ok(v) => super::super::provider_child_probe::snapshot(v),
        Err(e) => json!({"result":"Error","error":error(e),
            "error_local_canonical_additional":"not exposed by public Tape factory errors"}),
    }
}
fn assert_data(old: &Value, new: &Value) {
    assert_eq!(
        old["result"], new["result"],
        "complete outcomes\nordinary={old}\nadmitted={new}"
    );
    if old["result"] == "Error" {
        assert_eq!(old["error"], new["error"]);
        return;
    }
    let a = old["query"].as_object().unwrap();
    let b = new["query"].as_object().unwrap();
    assert_eq!(a.keys().collect::<Vec<_>>(), b.keys().collect::<Vec<_>>());
    for (key, value) in a {
        if ["work", "suppressed_capacity", "contents_capacity"].contains(&key.as_str()) {
            continue;
        }
        assert_eq!(
            Some(value),
            b.get(key),
            "returned field {key}\nordinary={old}\nadmitted={new}"
        );
    }
    for key in ["suppressed_capacity", "contents_capacity"] {
        assert!(old["query"][key].is_u64() && new["query"][key].is_u64());
    }
}
fn assert_work(old: &Value, q: &Query, o: &RecordingObserver) {
    assert_eq!(
        q.canonical_units(),
        old["query"]["work"].as_u64().unwrap() as usize
    );
    assert_eq!(q.work(), q.canonical_units() + q.additional_units());
    assert_eq!(q.work(), q.view().work);
    assert!(q.was_forwarded());
    assert_eq!(o.forwarded(), q.work());
}
fn matched_adequate<'a>(c: &'a Value, request: &Value, cap: AudioQueryLimits) -> &'a Value {
    c["requests"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["samples"] == request["samples"] && r["limits"] == limits(cap))
        .unwrap_or_else(|| {
            panic!(
                "no frozen same-range/span/adequate-work witness {}",
                request["name"]
            )
        })
}
fn paired_body() -> Value {
    let old = frozen();
    let mut rows = Vec::new();
    let mut input_states = Vec::new();
    let mut visited = 0;
    let mut requests = 0;
    let setups = visit_cases(|plan, name, tape| {
        visited += 1;
        let c = case(&old, name);
        let before = tape.provider_before_snapshot(plan);
        assert_eq!(before, c["tape_before"]);
        for r in c["requests"].as_array().unwrap() {
            requests += 1;
            let samples = request_range(r);
            let exact = request_limits(r);
            let adequate = AudioQueryLimits {
                maximum_work: AudioQueryLimits::default().maximum_work,
                ..exact
            };
            let witness = matched_adequate(c, r, adequate);
            for mode in [Mode::Policy, Mode::AfterPreserve] {
                let mut backend = RecordingBackend::new(Fault::None);
                let observer = backend.observer();
                let result = admitted(tape, samples.clone(), exact, mode, &mut backend);
                let exact_snapshot = snapshot(&result);
                let exact_live = observer.snapshot();
                if let Ok(value) = &result {
                    assert_data(&r[mode.old_key()], &exact_snapshot);
                    assert_work(&r[mode.old_key()], value, &observer);
                }
                drop(result);
                observer.assert_released();
                let exact_after = observer.snapshot();
                let exact_events = observer.events();
                let fresh = ordinary(tape, samples.clone(), adequate, mode);
                let fresh_snapshot = ordinary_snapshot(&fresh);
                drop(fresh);
                assert_eq!(
                    fresh_snapshot,
                    witness[mode.old_key()],
                    "ordinary reference remains frozen"
                );
                let mut backend = RecordingBackend::new(Fault::None);
                let observer = backend.observer();
                let result = admitted(tape, samples.clone(), adequate, mode, &mut backend);
                let adequate_snapshot = snapshot(&result);
                let adequate_live = observer.snapshot();
                assert_data(&fresh_snapshot, &adequate_snapshot);
                if let Ok(value) = &result {
                    assert_work(&fresh_snapshot, value, &observer);
                }
                drop(result);
                observer.assert_released();
                assert_eq!(before, tape.provider_before_snapshot(plan));
                rows.push(json!({"case":name,"request":r["name"],"samples":samples,"mode":mode.name(),
                    "original_limits":limits(exact),"ordinary_at_original_limit":r[mode.old_key()],
                    "admitted_at_original_limit":exact_snapshot,"exact_while_result_live":exact_live,
                    "exact_after_result_drop":exact_after,"exact_events":exact_events,
                    "adequate_limits":limits(adequate),"ordinary_at_adequate_work":fresh_snapshot,
                    "admitted_at_adequate_work":adequate_snapshot,"adequate_while_result_live":adequate_live,
                    "adequate_after_result_drop":observer.snapshot(),"adequate_events":observer.events()}));
            }
        }
        let after = tape.provider_before_snapshot(plan);
        assert_eq!(after, c["tape_after"]);
        input_states.push(json!({"case":name,"before":before,"after":after}));
    });
    assert_eq!(visited, 9);
    assert_eq!(requests, 165);
    assert_eq!(rows.len(), 330);
    json!({"schema":"admitted-tape-policy-pairs-v1","cases":visited,"requests":requests,
        "mode_lanes":rows.len(),"setup_dispositions":setups,"input_states":input_states,"rows":rows,
        "scope":{"exact":"original limits/errors preserved; actual new admission refusals remain literal",
            "adequate":"separate default-work lane, same original span cap; complete ordinary reference anchored to frozen37",
            "capacity":"literal capacities recorded, never normalized to ordinary allocation growth",
            "receipts":"success canonical/additional/forwarded checked; public errors expose accepted callback prefix only",
            "ownership":"real existing Plan test Policy/Lease; no native Account/Box or Tape input-graph admission"}})
}
#[test]
fn tape_factories_pair_all_frozen_requests_without_work_or_capacity_normalization() {
    assert_eq!(FROZEN.len(), 430_342);
    // Parent freeze proof pins SHA-25698ec737a653d5ec192d991717c70bfce5cc22f5c4a3b37197f8c815398bd81a0.
    let body = paired_body();
    assert_eq!(body["mode_lanes"], 330);
}
fn full(c: &Value) -> &Value {
    c["requests"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "full")
        .unwrap()
}
fn callback_refusals(tape: &AudioSignalTape<'_>, samples: Range<SignalSample>, mode: Mode) {
    let cap = AudioQueryLimits::default();
    let mut backend = RecordingBackend::new(Fault::None);
    let observer = backend.observer();
    let result = admitted(tape, samples.clone(), cap, mode, &mut backend).unwrap();
    let baseline_events = observer.events();
    let peak = observer.snapshot()["peak"].as_u64().unwrap() as usize;
    let spends = observer.spends();
    let reserves = observer.reservations();
    assert!(spends > 0 && reserves > 0 && observer.live() > 0);
    drop(result);
    observer.assert_released();
    assert!(peak > 0);
    let mut exact = RecordingBackend::new(Fault::Cap(peak));
    let exact_observer = exact.observer();
    let exact_result = admitted(tape, samples.clone(), cap, mode, &mut exact).unwrap();
    assert_eq!(exact_observer.snapshot()["peak"], peak);
    drop(exact_result);
    exact_observer.assert_released();
    let mut below = RecordingBackend::new(Fault::Cap(peak - 1));
    let below_observer = below.observer();
    let below_result = admitted(tape, samples.clone(), cap, mode, &mut below);
    assert!(matches!(
        &below_result,
        Err(PlanError::AudioQueryLimit("test wide allocation refusal"))
    ));
    drop(below_result);
    below_observer.assert_released();
    let below_events = below_observer.events();
    let refused = below_events.last().expect("actual one-byte-short refusal");
    assert_eq!(refused["event"], "reserve");
    assert_eq!(refused["accepted"], false);
    assert!(below_observer.snapshot()["peak"].as_u64().unwrap() <= (peak - 1) as u64);
    for (operation, count) in [("spend", spends), ("reserve", reserves)] {
        for at in 1..=count {
            let mut backend = RecordingBackend::new(if operation == "spend" {
                Fault::RefuseSpend(at)
            } else {
                Fault::RefuseReserve(at)
            });
            let observer = backend.observer();
            let result = admitted(tape, samples.clone(), cap, mode, &mut backend);
            let message = if operation == "spend" {
                "test wide work refusal"
            } else {
                "test wide reservation refusal"
            };
            assert!(matches!(&result,Err(PlanError::AudioQueryLimit(value))if *value==message));
            drop(result);
            observer.assert_released();
            let events = observer.events();
            let failed = events.last().expect("actual refused callback");
            assert_eq!(failed["event"], operation);
            assert_eq!(failed["accepted"], false);
            let nth = baseline_events
                .iter()
                .enumerate()
                .filter(|(_, e)| e["event"] == operation)
                .nth(at - 1)
                .unwrap()
                .0;
            assert_eq!(
                events.len(),
                nth + 1,
                "nothing executes after callback refusal"
            );
            assert_eq!(
                events[..nth],
                baseline_events[..nth],
                "literal accepted callback prefix"
            );
            assert_eq!(
                if operation == "spend" {
                    observer.spends()
                } else {
                    observer.reservations()
                },
                at
            );
        }
    }
    let cancel = baseline_events
        .iter()
        .find(|e| {
            e["event"] == "spend"
                && e["before"]["live"].as_u64().unwrap() > 0
                && e["before"]["forwarded"].as_u64().unwrap() > 0
        })
        .expect("actual allocated cancellation prefix");
    let threshold = cancel["before"]["forwarded"].as_u64().unwrap() as usize;
    let mut backend = RecordingBackend::new(Fault::CancelAfter(threshold));
    let observer = backend.observer();
    let result = admitted(tape, samples.clone(), cap, mode, &mut backend);
    assert!(matches!(
        &result,
        Err(PlanError::AudioQueryLimit("test wide cancellation"))
    ));
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
    for fault in [Fault::RotateSpend(1), Fault::RotateReserve(1)] {
        let mut backend = RecordingBackend::new(fault);
        let observer = backend.observer();
        let result = admitted(tape, samples.clone(), cap, mode, &mut backend);
        assert!(matches!(
            &result,
            Err(PlanError::AudioMetadataIssuerMismatch)
        ));
        drop(result);
        observer.assert_released();
        assert_eq!(observer.events().last().unwrap()["issuer_changed"], true);
    }
}
#[test]
fn tape_factory_stops_at_each_real_callback_refusal_and_live_cancellation() {
    let old = frozen();
    let mut tested = 0;
    visit_cases(|plan, name, tape| {
        if !matches!(
            name,
            "structural_three_runs" | "intrinsic_override_then_source"
        ) {
            return;
        }
        let before = tape.provider_before_snapshot(plan);
        let request = full(case(&old, name));
        for mode in [Mode::Policy, Mode::AfterPreserve] {
            callback_refusals(tape, request_range(request), mode);
            tested += 1;
        }
        assert_eq!(before, tape.provider_before_snapshot(plan));
    });
    assert_eq!(tested, 4);
}
#[test]
fn actual_tape_result_outlives_backend_and_input_fixture() {
    let old = frozen();
    let mut held = None;
    let mut saved = None;
    let mut state = None;
    visit_cases(|_plan, name, tape| {
        if name != "structural_three_runs" {
            return;
        }
        let mut backend = RecordingBackend::new(Fault::None);
        let observer = backend.observer();
        let result = admitted(
            tape,
            request_range(full(case(&old, name))),
            Default::default(),
            Mode::Policy,
            &mut backend,
        )
        .unwrap();
        let snapshot = super::super::provider_child_probe::snapshot(&result);
        assert!(observer.live() > 0);
        drop(backend);
        assert_eq!(
            super::super::provider_child_probe::snapshot(&result),
            snapshot
        );
        held = Some(result);
        saved = Some(snapshot);
        state = Some(observer);
    });
    let value = held
        .take()
        .expect("actual public Tape result held past all fixtures");
    let observer = state.unwrap();
    assert!(observer.live() > 0);
    assert_eq!(
        super::super::provider_child_probe::snapshot(&value),
        saved.unwrap()
    );
    let accepted = observer.forwarded();
    drop(value);
    observer.assert_released();
    assert_eq!(observer.forwarded(), accepted, "no refund");
}
#[test]
fn late_second_child_error_keeps_other_real_result_live_and_releases_only_its_own_guards() {
    let old = frozen();
    let mut tested = 0;
    visit_cases(|plan, name, tape| {
        if name != "source_then_requires_owned_input" {
            return;
        }
        let c = case(&old, name);
        let before = tape.provider_before_snapshot(plan);
        let prefix = c["requests"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["name"] == "first_run_only")
            .unwrap();
        for mode in [Mode::Policy, Mode::AfterPreserve] {
            let mut backend = RecordingBackend::new(Fault::None);
            let observer = backend.observer();
            let first = admitted(
                tape,
                request_range(prefix),
                Default::default(),
                mode,
                &mut backend,
            )
            .unwrap();
            let first_snapshot = super::super::provider_child_probe::snapshot(&first);
            let live = observer.live();
            let accepted = observer.forwarded();
            assert!(live > 0 && !first.view().contents.is_empty());
            let failure = admitted(
                tape,
                request_range(full(c)),
                Default::default(),
                mode,
                &mut backend,
            );
            assert!(failure.is_err());
            assert_eq!(
                snapshot(&failure)["error"],
                full(c)[mode.old_key()]["error"]
            );
            drop(failure);
            assert_eq!(
                observer.live(),
                live,
                "failed construction releases its own guards, not the held result"
            );
            assert!(observer.forwarded() > accepted);
            assert_eq!(
                super::super::provider_child_probe::snapshot(&first),
                first_snapshot
            );
            drop(first);
            observer.assert_released();
            tested += 1;
        }
        assert_eq!(tape.provider_before_snapshot(plan), before);
    });
    assert_eq!(tested, 2);
}
#[test]
#[ignore = "manual actual Tape paired receipt; source-pinned fresh /tmp output"]
fn capture_admitted_tape_policy_pairs_v1() {
    let output =
        std::env::var("DEADPAN_TAPE_POLICY_PAIRED_OUTPUT").expect("fresh explicit /tmp output");
    let path = Path::new(&output);
    assert!(path.is_absolute());
    assert!(
        path.parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .starts_with(Path::new("/tmp").canonicalize().unwrap())
    );
    let variant = std::env::var("DEADPAN_TAPE_POLICY_PAIRED_VARIANT").expect("actual variant");
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source = std::env::var("DEADPAN_TAPE_POLICY_PAIRED_SOURCE_SHA256")
        .expect("actual source manifest digest");
    assert!(source.len() == 64 && source.bytes().all(|b| b.is_ascii_hexdigit()));
    let checkpoint =
        std::env::var("DEADPAN_TAPE_POLICY_PAIRED_PUBLISHED_CHECKPOINT").expect("published H");
    assert_eq!(checkpoint, CHECKPOINT);
    let mut body = paired_body();
    body["variant"] = json!(variant);
    body["source_sha256"] = json!(source);
    body["published_checkpoint"] = json!(checkpoint);
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap();
    let mut writer = std::io::BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, &body).unwrap();
    writer.write_all(b"\n").unwrap();
    writer.flush().unwrap();
    writer.get_ref().sync_all().unwrap();
}
