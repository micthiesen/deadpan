//! Current ordinary Root/Domain policy and actual sealed-finish observations.
//! Prepare only: execute this producer after the processing checkpoint publishes.
use super::*;
use crate::AudioPolicyQuery;
use std::{fs::OpenOptions, io::Write, path::Path};
use wide_baseline::Parent;

#[path = "policy_dense_fallback_cases.rs"]
pub(in crate::plan) mod policy_dense_fallback_cases;
#[path = "policy_retained_cases.rs"]
pub(in crate::plan) mod policy_retained_cases;

#[derive(Clone, Copy)]
enum Route {
    Ordinary,
    SealedFinish,
}

impl Route {
    fn name(self) -> &'static str {
        match self {
            Self::Ordinary => "ordinary_policy",
            Self::SealedFinish => "sealed_finish_policy",
        }
    }
}

fn limits_snapshot(limits: AudioQueryLimits) -> Value {
    json!({"maximum_spans":limits.maximum_spans,"maximum_work":limits.maximum_work})
}

fn lookup(value: LookupStats) -> Value {
    let LookupStats {
        visited_nodes,
        sequence_comparisons,
        iteration_run_comparisons,
    } = value;
    json!({"visited_nodes":visited_nodes,"sequence_comparisons":sequence_comparisons,
        "iteration_run_comparisons":iteration_run_comparisons})
}

pub(in crate::plan) fn query_snapshot(value: &AudioPolicyQuery<AudioSample>) -> Value {
    let AudioPolicyQuery {
        suppressed,
        contents,
        lookup: stats,
        work,
    } = value;
    json!({"suppressed":suppressed,"contents":contents,"lookup":lookup(*stats),"work":work})
}

pub(in crate::plan) fn failed(error: &PlanError) -> Value {
    json!({"result":"Error","error":error_json(error),
        "partial_work":"ordinary policy errors expose no accepted partial-work receipt"})
}

struct Call {
    output: Value,
    policy: Option<Value>,
    second_queries: Vec<Value>,
}

impl Call {
    fn succeeded(&self) -> bool {
        self.policy.is_some()
    }

    fn work(&self) -> usize {
        usize::try_from(self.policy.as_ref().unwrap()["work"].as_u64().unwrap()).unwrap()
    }

    fn observation(
        self,
        name: &str,
        samples: Range<AudioSample>,
        limits: AudioQueryLimits,
    ) -> Value {
        json!({"name":name,"request":{"samples":samples,"limits":limits_snapshot(limits)},
            "output":self.output,"actual_domain_second_query_events":self.second_queries})
    }
}

fn ordinary_call(
    parent: Parent<'_, '_>,
    samples: Range<AudioSample>,
    limits: AudioQueryLimits,
) -> Call {
    let observer = policy_binding_baseline::Observation::start();
    let result = match parent {
        Parent::Root(plan) => plan.audio_policy(samples, limits),
        Parent::Domain(physical) => physical.policy(samples, limits),
    };
    let second_queries = observer.finish();
    let (output, policy) = match result {
        Ok(query) => {
            let policy = query_snapshot(&query);
            (json!({"result":"Ok","policy":policy}), Some(policy))
        }
        Err(error) => (failed(&error), None),
    };
    Call {
        output,
        policy,
        second_queries,
    }
}

fn sealed_call<'plan>(
    physical: &AudioDomain<'plan>,
    first_samples: Range<AudioSample>,
    caller: &AudioDomain<'plan>,
    samples: Range<AudioSample>,
    limits: AudioQueryLimits,
) -> Call {
    let first_limits = AudioQueryLimits::default();
    let first = physical
        .processing_for_policy(first_samples.clone(), first_limits)
        .expect("checked visitor must prepare this actual first processing query");
    let first_snapshot = processing_query_baseline::query_snapshot(physical.plan, first.query());
    let observer = policy_binding_baseline::Observation::start();
    let result = first.finish_policy(caller, samples, limits);
    let second_queries = observer.finish();
    let (finish, policy) = match result {
        Ok(completed) => {
            let (processing, policy) = completed.into_queries();
            let returned = processing_query_baseline::query_snapshot(physical.plan, &processing);
            assert_eq!(
                returned, first_snapshot,
                "finish must preserve the whole first query"
            );
            let policy = query_snapshot(&policy);
            (
                json!({"result":"Ok","processing":returned,"policy":policy}),
                Some(policy),
            )
        }
        Err(error) => (failed(&error), None),
    };
    Call {
        output: json!({
            "first_processing_request":{"samples":first_samples,"limits":limits_snapshot(first_limits)},
            "first_processing":{"result":"Ok","query":first_snapshot},"finish":finish,
        }),
        policy,
        second_queries,
    }
}

fn call(
    parent: Parent<'_, '_>,
    route: Route,
    samples: Range<AudioSample>,
    limits: AudioQueryLimits,
) -> Call {
    match route {
        Route::Ordinary => ordinary_call(parent, samples, limits),
        Route::SealedFinish => match parent {
            Parent::Domain(physical) => {
                sealed_call(physical, samples.clone(), physical, samples, limits)
            }
            Parent::Root(_) => panic!("Root has no sealed Domain capability"),
        },
    }
}

fn window(value: &Value) -> Range<AudioSample> {
    AudioSample(value["start"].as_i64().unwrap())..AudioSample(value["end"].as_i64().unwrap())
}

fn clipped(samples: &Range<AudioSample>) -> Range<AudioSample> {
    assert!(samples.end.0 - samples.start.0 >= 2);
    let end = if samples.end.0 - samples.start.0 > 2 {
        samples.end.0 - 1
    } else {
        samples.end.0
    };
    AudioSample(samples.start.0 + 1)..AudioSample(end)
}

fn fresh_limit_observations(
    parent: Parent<'_, '_>,
    route: Route,
    samples: Range<AudioSample>,
) -> Vec<Value> {
    let defaults = AudioQueryLimits::default();
    let mut observations = Vec::new();
    for (name, range) in [
        ("full", samples.clone()),
        ("clipped", clipped(&samples)),
        ("empty", samples.start..samples.start),
    ] {
        let full = call(parent, route, range.clone(), defaults);
        assert!(full.succeeded(), "default {name} policy must complete");
        let work = full.work();
        observations.push(full.observation(&format!("default_{name}"), range.clone(), defaults));
        let exact = AudioQueryLimits {
            maximum_work: work,
            ..defaults
        };
        let exact_result = call(parent, route, range.clone(), exact);
        let exact_completes = exact_result.succeeded();
        observations.push(exact_result.observation(
            &format!("{name}_returned_work_limit"),
            range.clone(),
            exact,
        ));
        if !exact_completes {
            // An allowance can be required without becoming returned work.
            let mut probes = Vec::new();
            for cap in work.checked_add(1).unwrap().max(1)..=defaults.maximum_work {
                let limits = AudioQueryLimits {
                    maximum_work: cap,
                    ..defaults
                };
                let result = call(parent, route, range.clone(), limits);
                let complete = result.succeeded();
                probes.push(result.observation(
                    "completing_work_limit_probe",
                    range.clone(),
                    limits,
                ));
                if complete {
                    probes.last_mut().unwrap()["name"] =
                        json!(format!("{name}_exact_completing_work_limit"));
                    break;
                }
            }
            assert_eq!(
                probes.last().unwrap()["name"],
                format!("{name}_exact_completing_work_limit")
            );
            if probes.len() > 1 {
                let preceding = probes.len() - 2;
                probes[preceding]["name"] =
                    json!(format!("{name}_one_short_completing_work_limit"));
            } else {
                observations.last_mut().unwrap()["also_boundary"] =
                    json!(format!("{name}_one_short_completing_work_limit"));
            }
            observations.extend(probes);
        }
        if work > 0 {
            let short = AudioQueryLimits {
                maximum_work: work - 1,
                ..defaults
            };
            let result = call(parent, route, range.clone(), short);
            assert!(
                !result.succeeded(),
                "one-short returned work cannot complete"
            );
            observations.push(result.observation(
                &format!("{name}_one_short_returned_work_limit"),
                range.clone(),
                short,
            ));
        }
        // Aggregate contents do not define a span cap; probe actual calls.
        let mut span_trials = Vec::new();
        for cap in 1..=defaults.maximum_spans {
            let limits = AudioQueryLimits {
                maximum_spans: cap,
                ..defaults
            };
            let result = call(parent, route, range.clone(), limits);
            let complete = result.succeeded();
            span_trials.push(result.observation("span_limit_probe", range.clone(), limits));
            if complete {
                span_trials.last_mut().unwrap()["name"] =
                    json!(format!("{name}_exact_positive_span_limit"));
                if span_trials.len() > 1 {
                    let preceding = span_trials.len() - 2;
                    span_trials[preceding]["name"] =
                        json!(format!("{name}_one_short_positive_span_limit"));
                }
                break;
            }
        }
        assert_eq!(
            span_trials.last().unwrap()["name"],
            format!("{name}_exact_positive_span_limit")
        );
        let zero_is_one_short = span_trials.len() == 1;
        observations.extend(span_trials);
        if zero_is_one_short {
            let zero = AudioQueryLimits {
                maximum_spans: 0,
                ..defaults
            };
            let result = call(parent, route, range.clone(), zero);
            assert!(!result.succeeded());
            observations.push(result.observation(
                &format!("{name}_one_short_span_limit_invalid_zero"),
                range.clone(),
                zero,
            ));
        }
    }

    for (name, limits) in [
        (
            "invalid_zero_work",
            AudioQueryLimits {
                maximum_work: 0,
                ..defaults
            },
        ),
        (
            "invalid_zero_spans",
            AudioQueryLimits {
                maximum_spans: 0,
                ..defaults
            },
        ),
        (
            "invalid_above_maximum_work",
            AudioQueryLimits {
                maximum_work: defaults.maximum_work.checked_add(1).unwrap(),
                ..defaults
            },
        ),
        (
            "invalid_above_maximum_spans",
            AudioQueryLimits {
                maximum_spans: defaults.maximum_spans.checked_add(1).unwrap(),
                ..defaults
            },
        ),
        (
            "tiny_positive_work_full",
            AudioQueryLimits {
                maximum_work: 1,
                ..defaults
            },
        ),
        (
            "tiny_positive_spans_full",
            AudioQueryLimits {
                maximum_spans: 1,
                ..defaults
            },
        ),
    ] {
        observations.push(call(parent, route, samples.clone(), limits).observation(
            name,
            samples.clone(),
            limits,
        ));
    }
    observations
}

pub(in crate::plan) fn descriptor_snapshot(descriptor: &Value) -> Value {
    let mut value = descriptor.clone();
    if value["fixture_source"]["kind"] == "pinned_test_constructor" {
        // This visitor names a checked constructor rather than retaining its document.
        let source = include_str!("../../audio_allocated/query_storage_tests.rs");
        let (setup, _) = source
            .split_once("\nfn measurement(")
            .expect("existing fixture source boundary");
        value["fixture_source"]["constructor_and_helpers_source"] = json!(setup);
    }
    value
}

fn ordinary_case(parent: Parent<'_, '_>, descriptor: &Value) -> Value {
    let samples = window(&descriptor["window"]);
    let defaults = AudioQueryLimits::default();
    let mut observations = fresh_limit_observations(parent, Route::Ordinary, samples.clone());
    let before = AudioSample(samples.start.0.checked_sub(1).unwrap())..samples.start;
    let after = samples.end..AudioSample(samples.end.0.checked_add(1).unwrap());
    let reversed = AudioSample(samples.start.0 + 1)..samples.start;
    for (name, range, limits) in [
        ("outside_support_before", before.clone(), defaults),
        ("outside_support_after", after, defaults),
        ("reversed_range", reversed.clone(), defaults),
        (
            "invalid_work_before_outside_range",
            before.clone(),
            AudioQueryLimits {
                maximum_work: 0,
                ..defaults
            },
        ),
        (
            "invalid_spans_before_reversed_range",
            reversed,
            AudioQueryLimits {
                maximum_spans: 0,
                ..defaults
            },
        ),
        (
            "tiny_positive_work_before_outside_range",
            before.clone(),
            AudioQueryLimits {
                maximum_work: 1,
                ..defaults
            },
        ),
        (
            "tiny_positive_span_cap_before_outside_range",
            before,
            AudioQueryLimits {
                maximum_spans: 1,
                ..defaults
            },
        ),
    ] {
        observations
            .push(ordinary_call(parent, range.clone(), limits).observation(name, range, limits));
    }
    let mut value = descriptor_snapshot(descriptor);
    value["route"] = json!(Route::Ordinary.name());
    value["observations"] = json!(observations);
    value
}

fn sealed_case(physical: &AudioDomain<'_>, descriptor: &Value) -> Value {
    let samples = window(&descriptor["window"]);
    let defaults = AudioQueryLimits::default();
    let mut observations = fresh_limit_observations(
        Parent::Domain(physical),
        Route::SealedFinish,
        samples.clone(),
    );
    let other: AudioDomain<'_> = physical.clone();
    let before = AudioSample(samples.start.0.checked_sub(1).unwrap())..samples.start;
    let reversed = AudioSample(samples.start.0 + 1)..samples.start;
    for (name, caller, range, limits) in [
        (
            "equal_cloned_domain_identity",
            &other,
            samples.clone(),
            defaults,
        ),
        (
            "equal_cloned_domain_before_invalid_work",
            &other,
            samples.clone(),
            AudioQueryLimits {
                maximum_work: 0,
                ..defaults
            },
        ),
        (
            "equal_cloned_domain_before_invalid_spans",
            &other,
            samples.clone(),
            AudioQueryLimits {
                maximum_spans: 0,
                ..defaults
            },
        ),
        (
            "changed_in_support_range",
            physical,
            clipped(&samples),
            defaults,
        ),
        (
            "changed_empty_range",
            physical,
            samples.start..samples.start,
            defaults,
        ),
        ("outside_range_identity", physical, before.clone(), defaults),
        (
            "reversed_range_identity",
            physical,
            reversed.clone(),
            defaults,
        ),
        (
            "outside_range_before_invalid_work",
            physical,
            before,
            AudioQueryLimits {
                maximum_work: 0,
                ..defaults
            },
        ),
        (
            "reversed_range_before_invalid_spans",
            physical,
            reversed,
            AudioQueryLimits {
                maximum_spans: 0,
                ..defaults
            },
        ),
    ] {
        let result = sealed_call(physical, samples.clone(), caller, range.clone(), limits);
        assert_eq!(
            result.output["finish"]["error"],
            json!({
                "variant":"InvalidPlan","message":"processing policy identity mismatch",
            })
        );
        assert!(
            result.second_queries.is_empty(),
            "identity refusal precedes policy"
        );
        let mut observation = result.observation(name, range, limits);
        observation["finish_caller"] = json!(if std::ptr::eq(physical, caller) {
            "same_domain_object"
        } else {
            "actual_equal_cloned_domain_object"
        });
        observations.push(observation);
    }
    let mut value = descriptor_snapshot(descriptor);
    value["route"] = json!(Route::SealedFinish.name());
    value["observations"] = json!(observations);
    value
}

pub(in crate::plan) fn matrix() -> Value {
    let mut ordinary = Vec::new();
    let mut sealed = Vec::new();
    processing_query_baseline::visit_cases(|parent, descriptor| {
        ordinary.push(ordinary_case(parent, descriptor));
        if let Parent::Domain(physical) = parent {
            sealed.push(sealed_case(physical, descriptor));
        }
    });
    assert_eq!(ordinary.len(), 33);
    assert_eq!(sealed.len(), 13);
    let count = |cases: &[Value]| {
        cases
            .iter()
            .map(|case| case["observations"].as_array().unwrap().len())
            .sum::<usize>()
    };
    json!({
        "schema":"old-policy-query-baseline-v1",
        "scope":"Complete actual ordinary Root/Domain policy and actual single-use Domain finish; checked fixture setup is separate",
        "producer":"ordinary-policy-before-policy-constructor-admission",
        "error_work":"No ordinary failed-policy partial-work receipt is exposed; observed actual second-query prefixes are separate",
        "sealed_first_work":"Every trial constructs a fresh actual first query under separately recorded defaults; finish limits cover policy only",
        "span_boundary":"Actual increasing positive-cap calls stop at first successful full/cropped/empty policy, including all failed probes",
        "returned_work_boundary":"Literal returned canonical work and one-short calls; exact returned work need not complete when a later empty child requires positive remaining allowance",
        "coverage":{"ordinary_parents":ordinary.len(),"root_parents":20,"domain_parents":sealed.len(),
            "ordinary_observations":count(&ordinary),"sealed_observations":count(&sealed)},
        "excluded_factories":["arbitrary AudioSignal/source-voice policy","ProjectedStage policy factory","creative fades","additional real next-contract fixtures"],
        "ordinary_cases":ordinary,"sealed_cases":sealed,
        "retained_policy_cases":policy_retained_cases::matrix(),
        "dense_and_fallback_cases":policy_dense_fallback_cases::matrix(),
    })
}

#[test]
#[ignore = "manual immutable OLD policy producer; publish current checkpoint first; requires new /tmp output"]
fn capture_old_policy_query_baseline_v1() {
    let output =
        std::env::var("DEADPAN_POLICY_QUERY_OUTPUT").expect("new explicit policy output required");
    let output = Path::new(&output);
    assert!(output.is_absolute());
    assert!(
        output
            .parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .starts_with(Path::new("/tmp").canonicalize().unwrap()),
        "policy output must remain under canonical /tmp"
    );
    let variant = std::env::var("DEADPAN_POLICY_QUERY_VARIANT")
        .expect("explicit main-inherited/scoped-published variant required");
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source = std::env::var("DEADPAN_POLICY_QUERY_SOURCE_SHA256")
        .expect("pinned OLD source-manifest SHA256 required");
    assert!(source.len() == 64 && source.bytes().all(|value| value.is_ascii_hexdigit()));
    let checkpoint = std::env::var("DEADPAN_POLICY_QUERY_PUBLISHED_CHECKPOINT")
        .expect("published processing checkpoint required before OLD capture");
    assert!(!checkpoint.trim().is_empty());
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .expect("policy output must be new");
    let mut value = matrix();
    value["variant"] = json!(variant);
    value["source_sha256"] = json!(source);
    value["published_checkpoint"] = json!(checkpoint);
    serde_json::to_writer_pretty(&mut file, &value).unwrap();
    file.write_all(b"\n").unwrap();
    file.sync_all().unwrap();
}

fn difference_path(expected: &Value, actual: &Value, path: &str) -> String {
    match (expected, actual) {
        (Value::Object(expected), Value::Object(actual)) => {
            for (key, value) in expected {
                let next = format!("{path}.{key}");
                let Some(other) = actual.get(key) else {
                    return next;
                };
                if value != other {
                    return difference_path(value, other, &next);
                }
            }
            format!("{path} (object keys)")
        }
        (Value::Array(expected), Value::Array(actual)) => {
            if expected.len() != actual.len() {
                return format!(
                    "{path} (array lengths {} / {})",
                    expected.len(),
                    actual.len()
                );
            }
            let index = expected
                .iter()
                .zip(actual)
                .position(|(a, b)| a != b)
                .unwrap();
            difference_path(
                &expected[index],
                &actual[index],
                &format!("{path}[{index}]"),
            )
        }
        _ => format!("{path}: expected {expected}, actual {actual}"),
    }
}

#[test]
fn ordinary_policy_queries_match_frozen_pre_admission_body() {
    let expected: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/policy-query-baseline-v1/snapshot.json"
    )))
    .unwrap();
    let actual = matrix();
    if actual != expected {
        panic!(
            "OLD policy mismatch at {}",
            difference_path(&expected, &actual, "$")
        );
    }
}
