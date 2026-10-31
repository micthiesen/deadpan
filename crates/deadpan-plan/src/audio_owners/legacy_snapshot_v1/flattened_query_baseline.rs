//! Complete ordinary Root/Domain query observations before constructor admission.
//! This producer only borrows actual returned queries; failed calls expose no work.
use super::*;
use crate::{AudioBoundaries, AudioBoundaryOrigin, AudioQuery, AudioSpan};
use std::{fs::OpenOptions, io::Write, path::Path};
use wide_baseline::Parent;

fn lookup(value: LookupStats) -> Value {
    let LookupStats {
        visited_nodes,
        sequence_comparisons,
        iteration_run_comparisons,
    } = value;
    json!({
        "visited_nodes":visited_nodes,
        "sequence_comparisons":sequence_comparisons,
        "iteration_run_comparisons":iteration_run_comparisons,
    })
}

fn boundary(value: &AudioBoundaryOrigin) -> Value {
    let AudioBoundaryOrigin {
        instance: path,
        gap_after,
        kind,
        placement_support,
        policy,
    } = value;
    json!({
        "instance":instance(path),"gap_after":gap_after.as_ref().map(iteration),
        "kind":boundary_kind(*kind),"placement_support":placement_support,"policy":policy,
    })
}

fn boundaries(value: &AudioBoundaries) -> Value {
    let AudioBoundaries { start, end } = value;
    json!({
        "start":start.iter().map(boundary).collect::<Vec<_>>(),
        "end":end.iter().map(boundary).collect::<Vec<_>>(),
    })
}

fn span(value: &AudioSpan) -> Value {
    let AudioSpan {
        definition,
        samples,
        allocated_samples,
        project_extent,
        envelope_extent,
        envelope_samples,
        envelope,
        boundaries: edges,
        instance: path,
        gap_after,
        transform,
        grid: sample_grid,
        sampling,
        retimes,
        content,
    } = value;
    json!({
        "definition":selector(definition.as_ref()),"samples":samples,
        "allocated_samples":allocated_samples,"project_extent":exact_range(project_extent),
        "envelope_extent":exact_range(envelope_extent),"envelope_samples":envelope_samples,
        "envelope":envelope,"boundaries":boundaries(edges),"instance":instance(path),
        "gap_after":gap_after.as_ref().map(iteration),"transform":transform,
        "grid":grid(*sample_grid),"sampling":sampling,"retimes":retimes,"content":content,
    })
}

/// A complete logical query, including the canonical work skipped by Serialize.
pub(in crate::plan) fn query_snapshot(value: &AudioQuery) -> Value {
    let AudioQuery {
        project_id,
        revision_id,
        samples,
        spans,
        lookup: stats,
        work,
    } = value;
    json!({
        "project_id":project_id.as_str(),"revision_id":revision_id.as_str(),
        "samples":samples,"spans":spans.iter().map(span).collect::<Vec<_>>(),
        "lookup":lookup(*stats),"work":work,
    })
}

fn result_snapshot(value: &Result<AudioQuery, PlanError>) -> Value {
    match value {
        Ok(query) => json!({"result":"Ok","query":query_snapshot(query)}),
        Err(error) => json!({
            "result":"Error","error":error_json(error),
            "partial_work":"not exposed by the ordinary constructor error",
        }),
    }
}

fn query(
    parent: Parent<'_, '_>,
    samples: Range<AudioSample>,
    limits: AudioQueryLimits,
) -> Result<AudioQuery, PlanError> {
    match parent {
        Parent::Root(plan) => plan.audio(samples, limits),
        Parent::Domain(physical) => physical.audio(samples, limits),
    }
}

fn window(value: &Value) -> Range<AudioSample> {
    AudioSample(value["start"].as_i64().unwrap())..AudioSample(value["end"].as_i64().unwrap())
}

fn parent_identity(parent: Parent<'_, '_>) -> Value {
    match parent {
        Parent::Root(plan) => json!({
            "tag":"ImmutablePlanRoot",
            "project_id":plan.metadata.project_id.as_str(),
            "revision_id":plan.metadata.revision_id.as_str(),
            "root":plan.nodes[plan.root].inspection.id.as_str(),
        }),
        Parent::Domain(physical) => json!({
            "tag":"ActualCheckedPhysicalDomain","physical_domain":domain(physical),
        }),
    }
}

fn descriptor(
    name: &str,
    parent: Parent<'_, '_>,
    samples: Range<AudioSample>,
    source: Value,
) -> Value {
    json!({
        "name":name,
        "parent_kind":match parent {Parent::Root(_)=>"Root",Parent::Domain(_)=>"Domain"},
        "window":sample_range(samples,|sample|sample.0),
        "parent_identity":parent_identity(parent),"fixture_source":source,
    })
}

/// Reconstruct six real parents. Setup queries are separate from measured calls.
/// Visitors borrow actual Domains produced by the checked existing constructors.
pub(in crate::plan) fn visit_cases(mut visit: impl FnMut(Parent<'_, '_>, &Value)) {
    let plan = crate::audio_allocated::query_storage_tests::fixture();
    let parent = Parent::Root(&plan);
    let case = descriptor(
        "source_repeat_nested_preserve_root",
        parent,
        AudioSample(0)..plan.audio_duration().unwrap(),
        json!({
            "kind":"pinned_test_constructor",
            "path":"src/audio_allocated/query_storage_tests.rs",
            "symbol":"fixture",
        }),
    );
    visit(parent, &case);
    let mut count = 1;

    let wide: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/wide-gain-owner-baseline-v1/snapshot.json"
    ))
    .unwrap();
    wide_baseline::visit_cases(&wide, |parent, original| {
        let name = original["name"].as_str().unwrap();
        if !matches!(
            name,
            "repeat_root_all_five_spans"
                | "repeat_actual_default_gap_domain_full"
                | "fractional_actual_physical_capture_domain"
        ) {
            return;
        }
        let fixture = wide["documents"]
            .as_array()
            .unwrap()
            .iter()
            .find(|fixture| fixture["fixture"] == original["fixture"])
            .unwrap();
        let case = descriptor(
            name,
            parent,
            window(&original["request"]["window"]),
            json!({
                "kind":"immutable_wide_checked_visitor",
                "path":"tests/fixtures/wide-gain-owner-baseline-v1/snapshot.json",
                "case":name,"fixture":original["fixture"],"document":fixture["document"],
                "checked_setup_identity":original["seed_identity"],
            }),
        );
        visit(parent, &case);
        count += 1;
    });
    policy_binding_baseline::visit_additional(|name, document, input, physical| {
        if !matches!(
            name,
            "cropped_group_domain_with_inherited_edit_retime"
                | "hidden_signed_partition_group_with_unrelated_binding"
        ) {
            return;
        }
        let parent = Parent::Domain(physical);
        let case = descriptor(
            name,
            parent,
            physical.root_samples(),
            json!({
                "kind":"checked_additional_owner_input",
                "path":"src/audio_owners/legacy_snapshot_v1/policy_binding_baseline.rs",
                "symbol":"visit_additional","case":name,"document":document,
                "setup_input":super::input(input),
            }),
        );
        visit(parent, &case);
        count += 1;
    });
    assert_eq!(count, 6);
}

fn observation(
    name: &str,
    parent: Parent<'_, '_>,
    samples: Range<AudioSample>,
    limits: AudioQueryLimits,
) -> Value {
    // Observe this one actual invocation, not an inferred result or second walk.
    let result = query(parent, samples.clone(), limits);
    match name {
        "default_clipped" => assert!(!result.as_ref().unwrap().spans.is_empty()),
        "default_empty" => {
            let query = result.as_ref().unwrap();
            assert!(query.spans.is_empty());
            assert_eq!(query.work, 0);
        }
        "invalid_zero_work" | "invalid_zero_spans" => {
            assert!(matches!(&result, Err(PlanError::InvalidAudioLimits)));
        }
        "outside_support_before" | "outside_support_after" => {
            assert!(matches!(&result, Err(PlanError::AudioRangeOutOfRange)));
        }
        _ => panic!("unclassified ordinary query observation: {name}"),
    }
    json!({
        "name":name,
        "request":{"samples":samples,
            "limits":{"maximum_spans":limits.maximum_spans,"maximum_work":limits.maximum_work}},
        "output":result_snapshot(&result),
    })
}

fn parent_case(parent: Parent<'_, '_>, descriptor: &Value) -> Value {
    let samples = window(&descriptor["window"]);
    assert!(samples.end.0 - samples.start.0 >= 2);
    let defaults = AudioQueryLimits::default();
    let full = query(parent, samples.clone(), defaults).unwrap();
    assert!(
        full.work > 1,
        "one-short must retain a valid positive limit"
    );
    assert!(!full.spans.is_empty());
    let mut observations = vec![json!({
        "name":"default_full",
        "request":{"samples":samples,
            "limits":{"maximum_spans":defaults.maximum_spans,"maximum_work":defaults.maximum_work}},
        "output":{"result":"Ok","query":query_snapshot(&full)},
    })];
    let clipped_end = if samples.end.0 - samples.start.0 > 2 {
        samples.end.0 - 1
    } else {
        samples.end.0
    };
    let clipped = AudioSample(samples.start.0 + 1)..AudioSample(clipped_end);
    observations.push(observation("default_clipped", parent, clipped, defaults));
    observations.push(observation(
        "default_empty",
        parent,
        samples.start..samples.start,
        defaults,
    ));
    let exact = AudioQueryLimits {
        maximum_work: full.work,
        ..defaults
    };
    let exact_result = query(parent, samples.clone(), exact);
    assert_eq!(
        query_snapshot(exact_result.as_ref().unwrap()),
        query_snapshot(&full),
        "exact canonical allowance must retain the ordinary complete query",
    );
    observations.push(json!({
        "name":"exact_canonical_work",
        "request":{"samples":samples,
            "limits":{"maximum_spans":exact.maximum_spans,"maximum_work":exact.maximum_work}},
        "output":result_snapshot(&exact_result),
    }));
    let short = AudioQueryLimits {
        maximum_work: full.work - 1,
        ..defaults
    };
    let short_result = query(parent, samples.clone(), short);
    assert!(matches!(&short_result, Err(PlanError::AudioQueryLimit(_))));
    observations.push(json!({
        "name":"one_short_canonical_work",
        "request":{"samples":samples,
            "limits":{"maximum_spans":short.maximum_spans,"maximum_work":short.maximum_work}},
        "output":result_snapshot(&short_result),
    }));
    if full.spans.len() > 1 {
        let limited = AudioQueryLimits {
            maximum_spans: full.spans.len() - 1,
            ..defaults
        };
        let result = query(parent, samples.clone(), limited);
        assert!(matches!(
            &result,
            Err(PlanError::AudioQueryLimit("span count"))
        ));
        observations.push(json!({
            "name":"insufficient_positive_span_limit",
            "request":{"samples":samples,
                "limits":{"maximum_spans":limited.maximum_spans,"maximum_work":limited.maximum_work}},
            "output":result_snapshot(&result),
        }));
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
    ] {
        observations.push(observation(name, parent, samples.clone(), limits));
    }
    observations.push(observation(
        "outside_support_before",
        parent,
        AudioSample(samples.start.0.checked_sub(1).unwrap())..samples.start,
        defaults,
    ));
    observations.push(observation(
        "outside_support_after",
        parent,
        samples.end..AudioSample(samples.end.0.checked_add(1).unwrap()),
        defaults,
    ));
    let mut value = descriptor.clone();
    value["observations"] = json!(observations);
    value
}

/// Ordinary logical matrix retained for future OLD differential consumers.
pub(in crate::plan) fn matrix() -> Value {
    let mut cases = Vec::new();
    visit_cases(|parent, descriptor| cases.push(parent_case(parent, descriptor)));
    assert_eq!(
        cases
            .iter()
            .filter(|case| case["parent_kind"] == "Root")
            .count(),
        2,
    );
    assert_eq!(
        cases
            .iter()
            .filter(|case| case["parent_kind"] == "Domain")
            .count(),
        4,
    );
    assert!(cases.iter().any(|case| {
        case["observations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|observation| observation["name"] == "insufficient_positive_span_limit")
    }));
    let signed = cases
        .iter()
        .find(|case| case["name"] == "hidden_signed_partition_group_with_unrelated_binding")
        .unwrap();
    assert_eq!(signed["window"], json!({"start":-3,"end":13}));
    json!({
        "schema":"old-flattened-query-baseline-v1",
        "scope":"Complete ordinary RenderPlan::audio and AudioDomain::audio results; setup is separate; no constructor memory-peak claim",
        "producer":"ordinary-root-domain-before-constructor-admission",
        "error_work":"Ordinary failed constructors expose no partial-work receipt",
        "cases":cases,
    })
}

#[test]
#[ignore = "manual immutable OLD flattened-query producer; requires a new /tmp output"]
fn capture_old_flattened_query_baseline_v1() {
    let output = std::env::var("DEADPAN_FLATTENED_QUERY_OUTPUT")
        .expect("new explicit flattened-query output required");
    let output = Path::new(&output);
    assert!(output.is_absolute());
    assert!(
        output
            .parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .starts_with(Path::new("/tmp").canonicalize().unwrap()),
        "flattened-query output must remain under canonical /tmp",
    );
    let variant = std::env::var("DEADPAN_FLATTENED_QUERY_VARIANT")
        .expect("explicit main-inherited/scoped-published variant required");
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source = std::env::var("DEADPAN_FLATTENED_QUERY_SOURCE_SHA256")
        .expect("pinned OLD source-manifest SHA256 required");
    assert!(source.len() == 64 && source.bytes().all(|value| value.is_ascii_hexdigit()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .expect("flattened-query output must be a new file");
    let mut value = matrix();
    value["variant"] = json!(variant);
    value["source_sha256"] = json!(source);
    serde_json::to_writer_pretty(&mut file, &value).unwrap();
    file.write_all(b"\n").unwrap();
    file.sync_all().unwrap();
}
