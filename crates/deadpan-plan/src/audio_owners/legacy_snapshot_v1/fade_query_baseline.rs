//! Prepare-only OLD fade witness. Capture only after the policy checkpoint publishes.
use super::*;
use crate::{AudioFadeEdge, AudioFadeQuery, AudioFadeSpan};
use std::{fs::OpenOptions, io::Write, path::Path};
use wide_baseline::Parent;

#[path = "fade_query_cases.rs"]
pub(in crate::plan) mod cases;

fn lookup(value: LookupStats) -> Value {
    let LookupStats {
        visited_nodes,
        sequence_comparisons,
        iteration_run_comparisons,
    } = value;
    json!({"visited_nodes":visited_nodes,"sequence_comparisons":sequence_comparisons,
        "iteration_run_comparisons":iteration_run_comparisons})
}
fn origin(value: &crate::AudioBoundaryOrigin) -> Value {
    let crate::AudioBoundaryOrigin {
        instance: path,
        gap_after,
        kind,
        placement_support,
        policy,
    } = value;
    json!({"instance":instance(path),"gap_after":gap_after.as_ref().map(iteration),
        "kind":boundary_kind(*kind),"placement_support":placement_support,"policy":policy})
}
fn edge(value: &AudioFadeEdge) -> Value {
    let AudioFadeEdge {
        at,
        length,
        distance_at_start,
        origins,
        editorial,
    } = value;
    json!({"at":exact(*at),"length":length,"distance_at_start":exact(*distance_at_start),
        "origins":origins.iter().map(origin).collect::<Vec<_>>(),"editorial":editorial})
}
fn span(value: &AudioFadeSpan) -> Value {
    let AudioFadeSpan {
        samples,
        start,
        end,
    } = value;
    json!({"samples":samples,"start":start.iter().map(edge).collect::<Vec<_>>(),
        "end":end.iter().map(edge).collect::<Vec<_>>()})
}
pub(in crate::plan) fn query_snapshot(value: &AudioFadeQuery) -> Value {
    let AudioFadeQuery {
        spans,
        lookup: stats,
        work,
    } = value;
    json!({"spans":spans.iter().map(span).collect::<Vec<_>>(),"lookup":lookup(*stats),"work":work})
}
pub(in crate::plan) fn result_snapshot(value: &Result<AudioFadeQuery, PlanError>) -> Value {
    match value {
        Ok(query) => json!({"result":"Ok","query":query_snapshot(query)}),
        Err(error) => json!({"result":"Error","error":error_json(error),
            "partial_work":"ordinary failed fade queries expose no accepted partial-work receipt"}),
    }
}
fn limits_snapshot(limits: AudioQueryLimits) -> Value {
    json!({"maximum_spans":limits.maximum_spans,"maximum_work":limits.maximum_work})
}
fn window(value: &Value) -> Range<AudioSample> {
    AudioSample(value["start"].as_i64().unwrap())..AudioSample(value["end"].as_i64().unwrap())
}
fn query(
    parent: Parent<'_, '_>,
    samples: Range<AudioSample>,
    limits: AudioQueryLimits,
) -> Result<AudioFadeQuery, PlanError> {
    match parent {
        Parent::Root(plan) => plan.audio_fades(samples, limits),
        Parent::Domain(domain) => domain.fades(samples, limits),
    }
}
fn support(parent: Parent<'_, '_>) -> Range<AudioSample> {
    match parent {
        Parent::Root(plan) => AudioSample(0)..plan.audio_duration().unwrap(),
        Parent::Domain(domain) => domain.root_samples(),
    }
}
fn observation(
    name: &str,
    parent: Parent<'_, '_>,
    samples: Range<AudioSample>,
    limits: AudioQueryLimits,
) -> Value {
    let result = query(parent, samples.clone(), limits);
    json!({"name":name,"request":{"samples":samples,"limits":limits_snapshot(limits)},"output":result_snapshot(&result)})
}

pub(super) fn descriptor(
    name: &str,
    parent: Parent<'_, '_>,
    source: Value,
    probes: &[(&str, i64, i64)],
) -> Value {
    let identity = match parent {
        Parent::Root(plan) => {
            json!({"tag":"ImmutablePlanRoot","project_id":plan.metadata.project_id.as_str(),
            "revision_id":plan.metadata.revision_id.as_str(),"root":plan.nodes[plan.root].inspection.id.as_str()})
        }
        Parent::Domain(physical) => {
            json!({"tag":"ActualCheckedPhysicalDomain","physical_domain":domain(physical)})
        }
    };
    json!({"name":name,"parent_kind":match parent {Parent::Root(_)=>"Root",Parent::Domain(_)=>"Domain"},
        "parent_identity":identity,"window":sample_range(support(parent),|sample|sample.0),"fixture_source":source,
        "extra_windows":probes.iter().map(|(name,start,end)|json!({"name":name,"samples":{"start":start,"end":end}})).collect::<Vec<_>>()})
}

pub(in crate::plan) fn visit_cases(mut visit: impl FnMut(Parent<'_, '_>, &Value)) {
    processing_query_baseline::visit_cases(|parent, source| {
        let mut source = policy_query_baseline::descriptor_snapshot(source);
        source["name"] = json!(format!(
            "processing_fixture/{}",
            source["name"].as_str().unwrap()
        ));
        source["extra_windows"] = json!([]);
        visit(parent, &source);
    });
    cases::visit_cases(|parent, source| visit(parent, source));
}

fn parent_case(parent: Parent<'_, '_>, descriptor: &Value) -> Value {
    let samples = window(&descriptor["window"]);
    let defaults = AudioQueryLimits::default();
    let full = query(parent, samples.clone(), defaults);
    let mut observations = vec![
        json!({"name":"default_full","request":{"samples":samples,"limits":limits_snapshot(defaults)},
        "output":result_snapshot(&full)}),
    ];
    let width = samples.end.0.checked_sub(samples.start.0).unwrap();
    let crop = match width {
        0 | 1 => samples.clone(),
        2 => AudioSample(samples.start.0 + 1)..samples.end,
        _ => AudioSample(samples.start.0 + 1)..AudioSample(samples.end.0 - 1),
    };
    observations.push(observation(
        "default_clipped",
        parent,
        crop.clone(),
        defaults,
    ));
    observations.push(observation(
        "default_empty",
        parent,
        samples.start..samples.start,
        defaults,
    ));
    observations.push(observation(
        "empty_minimum_positive_limits",
        parent,
        samples.start..samples.start,
        AudioQueryLimits {
            maximum_work: 1,
            maximum_spans: 1,
        },
    ));
    for (name, limits) in [
        (
            "empty_invalid_zero_work",
            AudioQueryLimits {
                maximum_work: 0,
                ..defaults
            },
        ),
        (
            "empty_invalid_zero_spans",
            AudioQueryLimits {
                maximum_spans: 0,
                ..defaults
            },
        ),
    ] {
        observations.push(observation(
            name,
            parent,
            samples.start..samples.start,
            limits,
        ));
    }
    if let Ok(clipped) = query(parent, crop.clone(), defaults) {
        observations.push(observation(
            "clipped_literal_returned_work_limit",
            parent,
            crop.clone(),
            AudioQueryLimits {
                maximum_work: clipped.work,
                ..defaults
            },
        ));
        if clipped.work > 0 {
            observations.push(observation(
                "clipped_one_short_returned_work_limit",
                parent,
                crop,
                AudioQueryLimits {
                    maximum_work: clipped.work - 1,
                    ..defaults
                },
            ));
        }
    }
    if let Ok(full) = &full {
        let mut work_limits = vec![("literal_returned_work_limit", full.work)];
        if full.work > 0 {
            work_limits.push(("one_short_returned_work_limit", full.work - 1));
        }
        for (name, maximum_work) in work_limits {
            observations.push(observation(
                name,
                parent,
                samples.clone(),
                AudioQueryLimits {
                    maximum_work,
                    ..defaults
                },
            ));
        }
        let count = full.spans.len();
        let cap = count.max(1);
        observations.push(observation(
            if count == 0 {
                "empty_minimum_positive_span_limit"
            } else {
                "exact_positive_output_span_limit"
            },
            parent,
            samples.clone(),
            AudioQueryLimits {
                maximum_spans: cap,
                ..defaults
            },
        ));
        if count > 1 {
            observations.push(observation(
                "one_short_output_span_limit",
                parent,
                samples.clone(),
                AudioQueryLimits {
                    maximum_spans: cap - 1,
                    ..defaults
                },
            ));
        }
    }
    let bounds = support(parent);
    let before = AudioSample(bounds.start.0.checked_sub(1).unwrap())..bounds.start;
    let after = bounds.end..AudioSample(bounds.end.0.checked_add(1).unwrap());
    let reversed = AudioSample(samples.start.0.checked_add(1).unwrap())..samples.start;
    for (name, range, limits) in [
        ("outside_support_before", before.clone(), defaults),
        ("outside_support_after", after, defaults),
        ("reversed_range", reversed.clone(), defaults),
        (
            "invalid_zero_work",
            samples.clone(),
            AudioQueryLimits {
                maximum_work: 0,
                ..defaults
            },
        ),
        (
            "invalid_zero_spans",
            samples.clone(),
            AudioQueryLimits {
                maximum_spans: 0,
                ..defaults
            },
        ),
        (
            "invalid_above_maximum_work",
            samples.clone(),
            AudioQueryLimits {
                maximum_work: defaults.maximum_work + 1,
                ..defaults
            },
        ),
        (
            "invalid_above_maximum_spans",
            samples.clone(),
            AudioQueryLimits {
                maximum_spans: defaults.maximum_spans + 1,
                ..defaults
            },
        ),
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
            "tiny_positive_work_full",
            samples.clone(),
            AudioQueryLimits {
                maximum_work: 1,
                ..defaults
            },
        ),
        (
            "tiny_positive_spans_full",
            samples.clone(),
            AudioQueryLimits {
                maximum_spans: 1,
                ..defaults
            },
        ),
        (
            "tiny_positive_work_outside_range",
            before,
            AudioQueryLimits {
                maximum_work: 1,
                ..defaults
            },
        ),
    ] {
        observations.push(observation(name, parent, range, limits));
    }
    if width > 0 {
        let midpoint = AudioSample(samples.start.0 + width / 2);
        let mut anchors = vec![samples.start, AudioSample(samples.end.0 - 1), midpoint];
        anchors.sort_by_key(|sample| sample.0);
        anchors.dedup();
        for (index, at) in anchors.into_iter().enumerate() {
            observations.push(observation(
                &format!("one_sample_{index}"),
                parent,
                at..AudioSample(at.0 + 1),
                defaults,
            ));
        }
        if midpoint > samples.start && midpoint < samples.end {
            observations.push(observation(
                "adjacent_page_left",
                parent,
                samples.start..midpoint,
                defaults,
            ));
            observations.push(observation(
                "adjacent_page_right",
                parent,
                midpoint..samples.end,
                defaults,
            ));
        }
    }
    if let Some(probes) = descriptor["extra_windows"].as_array() {
        for probe in probes {
            observations.push(observation(
                probe["name"].as_str().unwrap(),
                parent,
                window(&probe["samples"]),
                defaults,
            ));
        }
    }
    let mut value = descriptor.clone();
    value["one_short_output_span_limit_availability"] = match &full {
        Ok(query) if query.spans.len() > 1 => json!("legal_positive_predecessor"),
        Ok(_) => json!("no_legal_positive_predecessor"),
        Err(_) => json!("full_query_failed"),
    };
    value["observations"] = json!(observations);
    value
}

fn retained_supplement() -> Value {
    let mut cases = Vec::new();
    policy_binding_baseline::visit_additional(|name, document, input, physical| {
        let at = input.observed_at();
        let result = input.retained_at(at, Default::default());
        let setup = json!({"name":name,"document":document,"setup_input":super::input(input),
            "request":{"observed_at":at,"limits":limits_snapshot(Default::default())}});
        let value = match result {
            Err(error) => {
                json!({"setup":setup,"retained":{"result":"Error","error":error_json(&error)}})
            }
            Ok(None) => json!({"setup":setup,"retained":{"result":"Ok","value":null}}),
            Ok(Some(retained)) => {
                let delivery = physical.root_samples();
                let wide = AudioSample(delivery.start.0.checked_sub(1).unwrap())
                    ..AudioSample(delivery.end.0.checked_add(1).unwrap());
                let mut observations = Vec::new();
                for (label, range) in [
                    ("delivery", delivery.clone()),
                    ("one_sample_before_and_after_delivery", wide),
                    ("empty", delivery.start..delivery.start),
                    ("reversed", AudioSample(at.0 + 1)..at),
                ] {
                    let result = retained.fades(range.clone(), Default::default());
                    observations.push(json!({"name":label,"request":{"samples":range,"limits":limits_snapshot(Default::default())},
                        "output":result_snapshot(&result)}));
                }
                if let Ok(query) = retained.fades(delivery.clone(), Default::default()) {
                    let mut work_limits = vec![("exact_actual_retained_work", query.work)];
                    if query.work > 0 {
                        work_limits.push(("one_short_actual_retained_work", query.work - 1));
                    }
                    for (label, work) in work_limits {
                        let limits = AudioQueryLimits {
                            maximum_work: work,
                            ..Default::default()
                        };
                        let result = retained.fades(delivery.clone(), limits);
                        observations.push(json!({"name":label,"request":{"samples":delivery,"limits":limits_snapshot(limits)},"output":result_snapshot(&result)}));
                    }
                }
                json!({"setup":setup,"retained":{"result":"Ok","value":retained.capture_baseline_snapshot(physical.plan)},"observations":observations})
            }
        };
        cases.push(value);
    });
    json!({"scope":"Actual standalone retained-input fades preserve their distinct prepaid canonical work and unrestricted history range; no Root/Domain admission closure claimed", "cases":cases})
}

pub(in crate::plan) fn matrix() -> Value {
    let mut cases = Vec::new();
    visit_cases(|parent, descriptor| cases.push(parent_case(parent, descriptor)));
    let roots = cases
        .iter()
        .filter(|case| case["parent_kind"] == "Root")
        .count();
    let domains = cases.len() - roots;
    let observations = cases
        .iter()
        .map(|case| case["observations"].as_array().unwrap().len())
        .sum::<usize>();
    json!({"schema":"old-fade-query-baseline-v1",
        "scope":"Actual ordinary Root/physical-Domain creative fade products; standalone retained-input supplement is separate",
        "producer":"ordinary-fades-before-fade-constructor-admission-after-published-policy",
        "error_work":"No failed ordinary fade partial-work receipt is exposed",
        "empty_work":"Empty fade returns canonical zero under positive valid limits; literal returned-work cap zero remains an InvalidAudioLimits trial",
        "short_windows":"A one-sample or empty parent has no strict nonempty clipped subwindow, so its clipped call repeats that actual window",
        "coverage":{"parents":cases.len(),"root_parents":roots,"domain_parents":domains,"observations":observations},
        "cases":cases,"standalone_retained_input":retained_supplement()})
}

fn capture_body(mut value: Value) -> Value {
    for key in ["variant", "source_sha256", "published_policy_checkpoint"] {
        value.as_object_mut().unwrap().remove(key);
    }
    value
}

#[test]
#[ignore = "manual OLD fade producer; wait for published policy checkpoint and a new explicit /tmp output"]
fn capture_old_fade_query_baseline_v1() {
    let output =
        std::env::var("DEADPAN_FADE_QUERY_OUTPUT").expect("new explicit fade output required");
    let output = Path::new(&output);
    assert!(output.is_absolute());
    assert!(
        output
            .parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .starts_with(Path::new("/tmp").canonicalize().unwrap())
    );
    let variant = std::env::var("DEADPAN_FADE_QUERY_VARIANT").expect("explicit variant required");
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source = std::env::var("DEADPAN_FADE_QUERY_SOURCE_SHA256")
        .expect("complete pinned OLD source-manifest SHA256 required");
    assert!(source.len() == 64 && source.bytes().all(|value| value.is_ascii_hexdigit()));
    let published = std::env::var("DEADPAN_FADE_QUERY_PUBLISHED_POLICY_CHECKPOINT")
        .expect("published policy checkpoint identity required");
    assert!(!published.trim().is_empty());
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(output)
        .expect("fade output must be a new file");
    let mut value = matrix();
    value["variant"] = json!(variant);
    value["source_sha256"] = json!(source);
    value["published_policy_checkpoint"] = json!(published);
    serde_json::to_writer_pretty(&mut output, &value).unwrap();
    output.write_all(b"\n").unwrap();
    output.sync_all().unwrap();
}

#[test]
#[ignore = "regression scaffold only; use an independently captured OLD body after policy publication"]
fn ordinary_fades_match_explicit_external_old_capture_v1() {
    let path = std::env::var("DEADPAN_FADE_QUERY_EXPECTED")
        .expect("explicit independent OLD capture required");
    let expected: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(matrix(), capture_body(expected));
}

#[test]
fn ordinary_fades_match_frozen_old_capture_v1() {
    let expected: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/fade-query-baseline-v1/snapshot.json"
    )))
    .unwrap();
    assert_eq!(matrix(), expected);
}
