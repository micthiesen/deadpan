//! Warm predictions compared with immutable query-derived native witnesses.

use super::*;
use serde_json::{Value, json};

fn integer(value: &Value) -> i128 {
    i128::from(value.as_u64().expect("nonnegative observed work"))
}

fn delta(before: &Value, after: &Value, field: &str) -> i128 {
    integer(&after[field]) - integer(&before[field])
}

fn phase_delta(before: &Value, after: &Value, phase: usize, field: &str) -> i128 {
    integer(&after["phase_work"]["work"][phase][field])
        - integer(&before["phase_work"]["work"][phase][field])
}

fn without_work(mut value: Value) -> Value {
    if let Value::Object(fields) = &mut value {
        fields.remove("plan_work");
        fields.remove("cache_plan_work");
        fields.remove("phase_work");
    }
    value
}

fn compare(legacy: &Value, warm: &Value) -> Value {
    assert_eq!(legacy["legacy_query_mode"], true);
    assert_eq!(warm["legacy_query_mode"], false);
    let old = legacy["cases"].as_array().unwrap();
    let new = warm["cases"].as_array().unwrap();
    assert_eq!(old.len(), new.len());
    let mut records = Vec::new();
    for (old, new) in old.iter().zip(new) {
        let a = &old["result"];
        let b = &new["result"];
        for field in [
            "name",
            "context",
            "range",
            "prediction_enabled",
            "same_cached_arc",
            "fixed_graph_reservation",
            "first",
            "second",
            "independent_query_oracle",
            "wav_sha256",
            "declared_source_audio",
            "all_zero_pcm_permitted",
        ] {
            assert_eq!(a[field], b[field], "native semantic witness: {field}");
        }
        for field in [
            "before",
            "after_first",
            "before_second",
            "after_second",
            "after_arc_verification",
        ] {
            assert_eq!(
                without_work(a[field].clone()),
                without_work(b[field].clone()),
                "non-work accounting: {field}"
            );
        }
        assert_eq!(
            without_work(old["cleanup"].clone()),
            without_work(new["cleanup"].clone())
        );
        let mut intervals = Vec::new();
        for (name, before, after) in [
            ("cold", "before", "after_first"),
            ("repeat", "before_second", "after_second"),
        ] {
            let mut lanes = serde_json::Map::new();
            for field in ["plan_work", "cache_plan_work"] {
                let old_total = delta(&a[before], &a[after], field);
                let new_total = delta(&b[before], &b[after], field);
                let phases: Vec<_> = (0..4)
                    .map(|phase| {
                        phase_delta(&b[before], &b[after], phase, field)
                            - phase_delta(&a[before], &a[after], phase, field)
                    })
                    .collect();
                assert_eq!(
                    new_total - old_total,
                    phases.iter().sum::<i128>(),
                    "every changed actual debit belongs to its observed phase"
                );
                if name == "repeat" && field == "plan_work" {
                    assert_eq!(
                        phase_delta(&b[before], &b[after], 0, field),
                        0,
                        "warm hit must skip query construction"
                    );
                    assert_eq!(
                        phase_delta(&b[before], &b[after], 2, field),
                        0,
                        "the admitted Arc must feed the later numerical read"
                    );
                    assert!(phase_delta(&b[before], &b[after], 3, field) > 0);
                    assert!(new_total < old_total, "measured net warm work must improve");
                }
                lanes.insert(
                    field.into(),
                    json!({
                        "legacy": old_total, "warm": new_total,
                        "actual_delta": new_total - old_total,
                        "phase_deltas": phases,
                    }),
                );
            }
            intervals.push(json!({"interval": name, "lanes": lanes}));
        }
        records.push(json!({"name": a["name"], "context": a["context"], "intervals": intervals}));
    }
    json!(records)
}

fn observations() -> Value {
    let old: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/cached-original-prediction-baseline-v1/snapshot.json"
    ))
    .unwrap();
    let legacy = cached_prediction_trace::with_legacy(cached_prediction_baseline::matrix);
    assert_eq!(
        legacy, old,
        "immutable pre-fast-path full observation witness"
    );
    let warm = cached_prediction_baseline::matrix();
    let reconciliation = compare(&legacy, &warm);
    let old_zero: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/cached-original-zero-prediction-baseline-v1/snapshot.json"
    ))
    .unwrap();
    let legacy_zero = cached_prediction_trace::with_legacy(cached_prediction_baseline::zero_matrix);
    assert_eq!(
        legacy_zero, old_zero,
        "immutable pre-fast-path zero Source witness"
    );
    let warm_zero = cached_prediction_baseline::zero_matrix();
    let zero_reconciliation = compare(&legacy_zero, &warm_zero);
    json!({"legacy": legacy, "warm": warm, "reconciliation": reconciliation,
        "legacy_zero": legacy_zero, "warm_zero": warm_zero,
        "zero_reconciliation": zero_reconciliation})
}

#[test]
fn warm_cached_prediction_preserves_old_pcm_policy_admission_and_reconciles_actual_work() {
    observations();
}

#[test]
#[ignore = "writes the verified current warm/legacy native observations"]
fn produce_current_cached_prediction_observations() {
    use std::io::Write;
    let path = std::env::var_os("DEADPAN_CACHED_PREDICTION_CURRENT").expect("set new output path");
    let observed = observations();
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap();
    file.write_all(&serde_json::to_vec_pretty(&observed).unwrap())
        .unwrap();
    file.write_all(b"\n").unwrap();
}
