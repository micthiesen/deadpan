//! Compare the actual sealed product with immutable pre-reuse private tapes.
use super::*;
use crate::plan::audio_owners::tests::legacy_snapshot_v1::{
    capture_baseline, policy_binding_baseline, wide_baseline,
};
use serde_json::{Value, json};

#[path = "audio_domain_admitted_policy_tests.rs"]
mod admitted;

fn lookup(value: LookupStats) -> Value {
    json!({"visited_nodes":value.visited_nodes,"sequence_comparisons":value.sequence_comparisons,
        "iteration_run_comparisons":value.iteration_run_comparisons})
}
fn processing(domain: &AudioDomain<'_>, value: &AudioProcessingQuery<'_>) -> Value {
    json!({"project_id":value.project_id.as_str(),"revision_id":value.revision_id.as_str(),
        "samples":value.samples,"work":value.work,"lookup":lookup(value.lookup),
        "spans":value.spans.iter().map(|span|capture_baseline::processing(domain.plan,span)).collect::<Vec<_>>()})
}
fn error(value: &PlanError) -> Value {
    match value {
        PlanError::InvalidAudioLimits => json!({"variant":"InvalidAudioLimits"}),
        PlanError::AudioQueryLimit(message) => {
            json!({"variant":"AudioQueryLimit","budget":message})
        }
        PlanError::InvalidPlan(message) => json!({"variant":"InvalidPlan","message":message}),
        other => panic!("unexpected differential error: {other:?}"),
    }
}
fn policy(value: &crate::AudioPolicyQuery<AudioSample>) -> Value {
    json!({"suppressed":value.suppressed,"contents":value.contents,
        "lookup":lookup(value.lookup),"work":value.work})
}
fn window(value: &Value) -> Range<AudioSample> {
    AudioSample(value["start"].as_i64().unwrap())..AudioSample(value["end"].as_i64().unwrap())
}
fn limits(value: &Value) -> AudioQueryLimits {
    AudioQueryLimits {
        maximum_spans: value["maximum_spans"].as_u64().unwrap() as usize,
        maximum_work: value["maximum_work"].as_u64().unwrap() as usize,
    }
}
fn frozen() -> Value {
    serde_json::from_str(include_str!(
        "../tests/fixtures/policy-binding-baseline-v1/snapshot.json"
    ))
    .unwrap()
}
fn visit(mut callback: impl FnMut(&AudioDomain<'_>, &Value)) {
    let frozen = frozen();
    let wide: Value = serde_json::from_str(include_str!(
        "../tests/fixtures/wide-gain-owner-baseline-v1/snapshot.json"
    ))
    .unwrap();
    let mut count = 0;
    let mut visit_parent = |name: &str, domain: &AudioDomain<'_>| {
        for case in frozen["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|case| case["name"] == name || case["name"] == format!("{name}/empty"))
        {
            callback(domain, case);
            count += 1;
        }
    };
    wide_baseline::visit_cases(&wide, |parent, case| {
        if let wide_baseline::Parent::Domain(domain) = parent {
            visit_parent(case["name"].as_str().unwrap(), domain);
        }
    });
    policy_binding_baseline::visit_additional(|name, _, _, domain| visit_parent(name, domain));
    assert_eq!(count, 26);
}
fn removed_second(case: &Value) -> Option<&Value> {
    if case["first_tape_eligibility"]["content_only_eligible"] != true {
        return None;
    }
    case["policy_cases"][0]["actual_second_query_events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|event| event["phase"] == "actual_second_query_returned" && event["depth"] == 0)
        .map(|event| &event["query"]["query"])
}
fn expected_policy(case: &Value) -> Value {
    let mut expected = case["policy_cases"][0]["output"]["policy"].clone();
    if let Some(removed) = removed_second(case) {
        let new_work = expected["work"]
            .as_u64()
            .unwrap()
            .checked_sub(removed["work"].as_u64().unwrap())
            .unwrap();
        expected["work"] = json!(new_work);
        for key in [
            "visited_nodes",
            "sequence_comparisons",
            "iteration_run_comparisons",
        ] {
            expected["lookup"][key] = json!(
                expected["lookup"][key]
                    .as_u64()
                    .unwrap()
                    .checked_sub(removed["lookup"][key].as_u64().unwrap())
                    .unwrap()
            );
        }
    }
    expected
}

#[test]
fn sealed_processing_and_policy_match_full_private_old_corpus() {
    let mut eligible = 0;
    let mut removed = 0;
    let mut policies = 0;
    visit(|domain, case| {
        let name = case["name"].as_str().unwrap();
        let samples = window(&case["request"]);
        let first = domain
            .processing_for_policy(samples.clone(), Default::default())
            .unwrap();
        assert_eq!(
            processing(domain, first.query()),
            case["first_processing_true"]["query"],
            "first complete private query: {name}"
        );
        let content_eligible = case["first_tape_eligibility"]["content_only_eligible"] == true;
        assert_eq!(
            first.stops.permits_binding_reuse(),
            content_eligible,
            "co-produced terminal witness: {name}"
        );
        if content_eligible {
            eligible += 1;
            assert_eq!(
                case["first_processing_true"], case["separate_processing_false"],
                "OLD true/false complete equality: {name}"
            );
        }
        let expected = expected_policy(case);
        removed += usize::from(removed_second(case).is_some());
        let completed = first
            .finish_policy(domain, samples.clone(), Default::default())
            .unwrap();
        let (returned, result) = completed.into_queries();
        assert_eq!(
            processing(domain, &returned),
            case["first_processing_true"]["query"],
            "owning pair preserves first storage: {name}"
        );
        assert_eq!(
            policy(&result),
            expected,
            "full policy and actual newly performed counters: {name}"
        );
        for trial in case["policy_cases"].as_array().unwrap() {
            let first = domain
                .processing_for_policy(samples.clone(), Default::default())
                .unwrap();
            let trial_limits = limits(&trial["limits"]);
            let result = first.finish_policy(domain, samples.clone(), trial_limits);
            let old_output = &trial["output"];
            match result {
                Ok(completed) => {
                    let (_, actual) = completed.into_queries();
                    assert_eq!(
                        policy(&actual),
                        expected,
                        "policy trial: {name}/{}",
                        trial["name"]
                    );
                    assert!(content_eligible || old_output["result"] == "Ok");
                    assert!(trial_limits.maximum_work >= actual.work);
                }
                Err(actual) => {
                    assert_eq!(
                        old_output["result"], "Error",
                        "no regression on old successful budget: {name}/{}",
                        trial["name"]
                    );
                    if !content_eligible
                        || removed_second(case).is_none()
                        || trial["name"] != "one_short_old_policy_work"
                    {
                        assert_eq!(
                            error(&actual),
                            old_output["error"],
                            "exact failure: {name}/{}",
                            trial["name"]
                        );
                    }
                }
            }
            policies += 1;
        }
    });
    assert_eq!(eligible, 22);
    assert_eq!(removed, 5);
    assert_eq!(policies, 56);
}

#[test]
fn eligible_binding_policy_uses_its_exact_new_work_allowance() {
    let mut checked = 0;
    visit(|domain, case| {
        if removed_second(case).is_none() {
            return;
        }
        let samples = window(&case["request"]);
        let expected = expected_policy(case);
        let needed = expected["work"].as_u64().unwrap() as usize;
        assert!(needed > 0);
        let first = domain
            .processing_for_policy(samples.clone(), Default::default())
            .unwrap();
        let first_work = first.query().work;
        let exact = AudioQueryLimits {
            maximum_work: needed,
            ..Default::default()
        };
        let (returned, actual) = first
            .finish_policy(domain, samples.clone(), exact)
            .unwrap()
            .into_queries();
        assert_eq!(returned.work, first_work);
        assert_eq!(
            policy(&actual),
            expected,
            "exact fresh allowance: {}",
            case["name"]
        );
        let short = domain
            .processing_for_policy(samples.clone(), Default::default())
            .unwrap()
            .finish_policy(
                domain,
                samples,
                AudioQueryLimits {
                    maximum_work: needed - 1,
                    ..Default::default()
                },
            );
        assert!(
            matches!(
                short,
                Err(PlanError::AudioQueryLimit(_)) | Err(PlanError::InvalidAudioLimits)
            ),
            "one-short fresh allowance: {}",
            case["name"]
        );
        checked += 1;
    });
    assert_eq!(checked, 5);
}

#[test]
fn sealed_product_rejects_equal_but_foreign_domain_and_changed_range() {
    let mut checked = 0;
    visit(|domain, case| {
        let samples = window(&case["request"]);
        let clone = domain.clone();
        let product = domain
            .processing_for_policy(samples.clone(), Default::default())
            .unwrap();
        let result = product.finish_policy(&clone, samples.clone(), Default::default());
        assert!(matches!(
            result,
            Err(PlanError::InvalidPlan(
                "processing policy identity mismatch"
            ))
        ));
        let product = domain
            .processing_for_policy(samples.clone(), Default::default())
            .unwrap();
        let changed = if samples.is_empty() {
            AudioSample(samples.start.0.saturating_sub(1))..samples.end
        } else {
            samples.start..samples.start
        };
        assert!(matches!(
            product.finish_policy(domain, changed, Default::default()),
            Err(PlanError::InvalidPlan(
                "processing policy identity mismatch"
            ))
        ));
        checked += 1;
    });
    assert_eq!(checked, 26);
}

#[test]
fn first_factory_preserves_exact_processing_budget_and_failures() {
    let mut nonempty = 0;
    visit(|domain, case| {
        let samples = window(&case["request"]);
        let first_work = case["first_processing_true"]["query"]["work"]
            .as_u64()
            .unwrap() as usize;
        if samples.is_empty() {
            return;
        }
        for maximum_work in [first_work, first_work.saturating_sub(1), 1] {
            let limits = AudioQueryLimits {
                maximum_work,
                ..Default::default()
            };
            let old = domain.processing(samples.clone(), limits);
            let new = domain.processing_for_policy(samples.clone(), limits);
            match (old, new) {
                (Ok(old), Ok(new)) => {
                    assert_eq!(processing(domain, &old), processing(domain, new.query()))
                }
                (Err(old), Err(new)) => assert_eq!(format!("{old:?}"), format!("{new:?}")),
                _ => panic!("factory changed exact processing budget: {}", case["name"]),
            }
        }
        let invalid = AudioQueryLimits {
            maximum_spans: 0,
            ..Default::default()
        };
        assert!(matches!(
            domain.processing_for_policy(samples, invalid),
            Err(PlanError::InvalidAudioLimits)
        ));
        nonempty += 1;
    });
    assert_eq!(nonempty, 13);
}

#[test]
fn exposed_stage_keeps_exact_global_and_one_short_fallback_budget() {
    let mut checked = 0;
    visit(|domain, case| {
        if case["first_tape_eligibility"]["content_only_eligible"] == true {
            return;
        }
        let samples = window(&case["request"]);
        let flattened = domain
            .audio(samples.clone(), Default::default())
            .unwrap()
            .work;
        let first_work = case["first_processing_true"]["query"]["work"]
            .as_u64()
            .unwrap() as usize;
        let policy_work = case["policy_cases"][0]["output"]["policy"]["work"]
            .as_u64()
            .unwrap() as usize;
        for short in [0, 1] {
            let mut remaining = flattened + first_work + policy_work - short;
            let flat = domain
                .audio(
                    samples.clone(),
                    AudioQueryLimits {
                        maximum_work: remaining,
                        ..Default::default()
                    },
                )
                .unwrap();
            remaining -= flat.work;
            let first = domain
                .processing_for_policy(
                    samples.clone(),
                    AudioQueryLimits {
                        maximum_work: remaining,
                        ..Default::default()
                    },
                )
                .unwrap();
            assert!(!first.stops.permits_binding_reuse());
            remaining -= first.query().work;
            assert_eq!(remaining, policy_work - short);
            let trial = AudioQueryLimits {
                maximum_work: remaining,
                ..Default::default()
            };
            let old = domain.policy(samples.clone(), trial);
            let new = first.finish_policy(domain, samples.clone(), trial);
            match (old, new) {
                (Ok(old), Ok(new)) => {
                    let (_, new) = new.into_queries();
                    assert_eq!(policy(&old), policy(&new));
                    assert_eq!(new.work, remaining);
                }
                (Err(old), Err(new)) => assert_eq!(format!("{old:?}"), format!("{new:?}")),
                _ => panic!(
                    "exposed Stage changed global fallback budget: {}",
                    case["name"]
                ),
            }
        }
        checked += 1;
    });
    assert_eq!(checked, 4);
}

#[test]
fn fresh_span_limits_still_apply_after_a_wider_first_factory() {
    let mut checked = 0;
    visit(|domain, case| {
        let samples = window(&case["request"]);
        let first = domain
            .processing_for_policy(samples.clone(), Default::default())
            .unwrap();
        if first.query().spans.len() <= 1 {
            return;
        }
        let short = AudioQueryLimits {
            maximum_spans: 1,
            ..Default::default()
        };
        let old = domain.policy(samples.clone(), short);
        let new = first.finish_policy(domain, samples, short);
        match (old, new) {
            (Err(old), Err(new)) => assert_eq!(format!("{old:?}"), format!("{new:?}")),
            (Ok(old), Ok(new)) => {
                let (_, new) = new.into_queries();
                assert_eq!(policy(&old), policy(&new));
            }
            _ => panic!("fresh span cap bypass: {}", case["name"]),
        }
        checked += 1;
    });
    assert_eq!(checked, 6);
}
