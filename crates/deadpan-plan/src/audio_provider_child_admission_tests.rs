//! Pair actual checked ordinary providers with the private admitted child.
use super::*;
use crate::AdmittedSignalPolicyQuery;
use crate::plan::audio::Budget as LocalBudget;
use crate::plan::audio_owners::admission::{Budget as QueryBudget, outer};
use crate::plan::audio_owners::wide_gain_query_admission_tests::provider_child_backend::{
    Backend, Fault, Lease, Observer,
};
use crate::plan::audio_policy::query::provider_child_probe as policy_probe;
use crate::plan::audio_signal::processing_query::SignalValue;

#[path = "audio_provider_child_failure_tests.rs"]
mod failure_tests;

#[path = "audio_provider_child_metadata_tests.rs"]
mod metadata_tests;

const FROZEN: &[u8] = include_bytes!("../tests/fixtures/provider-child-baseline-v1/snapshot.json");
fn frozen() -> Value {
    serde_json::from_slice(FROZEN).unwrap()
}
fn request_range(request: &Value) -> Range<SignalSample> {
    SignalSample(request["samples"]["start"].as_i64().unwrap())
        ..SignalSample(request["samples"]["end"].as_i64().unwrap())
}
fn request_limits(request: &Value) -> AudioQueryLimits {
    AudioQueryLimits {
        maximum_spans: request["limits"]["maximum_spans"].as_u64().unwrap() as usize,
        maximum_work: request["limits"]["maximum_work"].as_u64().unwrap() as usize,
    }
}
fn receipt(budget: &QueryBudget<'_, Backend>) -> Value {
    json!({"attempted_canonical":budget.canonical_used,"attempted_additional":budget.additional_used,
        "remaining":budget.remaining,"lookup":lookup(budget.lookup)})
}
fn run_current<'plan>(
    signal: &AudioSignal<'plan>,
    samples: Range<SignalSample>,
    limits: AudioQueryLimits,
    rule: AudioBoundaryRule,
    stops: (bool, bool),
    backend: &mut Backend,
) -> (Result<SignalValue<'plan, Backend>, PlanError>, Value) {
    let mut local = LocalBudget {
        remaining: limits.maximum_work,
        lookup: LookupStats::default(),
    };
    let mut budget = QueryBudget::new(&mut local, backend);
    let result =
        outer::<_, Backend>(signal.provider_child_query(samples, limits, rule, stops, &mut budget));
    (result, receipt(&budget))
}
fn run_policy(
    signal: &AudioSignal<'_>,
    samples: Range<SignalSample>,
    limits: AudioQueryLimits,
    rule: AudioBoundaryRule,
    endpoints: bool,
    backend: &mut Backend,
) -> (
    Result<AdmittedSignalPolicyQuery<usize, Lease>, PlanError>,
    Value,
) {
    let mut local = LocalBudget {
        remaining: limits.maximum_work,
        lookup: LookupStats::default(),
    };
    let mut budget = QueryBudget::new(&mut local, backend);
    let result = outer::<_, Backend>(policy_probe::run(
        signal,
        samples,
        limits,
        rule,
        endpoints,
        &mut budget,
    ));
    (result, receipt(&budget))
}
fn policy_snapshot(value: &AdmittedSignalPolicyQuery<usize, Lease>) -> Value {
    policy_probe::snapshot(value)
}
fn fixture_row<'a>(case: &'a Value, name: &str) -> &'a Value {
    case["observations"]["signals"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["name"] == name)
        .unwrap_or_else(|| panic!("missing frozen actual signal {name}"))
}
fn recipe_from(value: &Value) -> AudioSourceVoiceRecipe {
    AudioSourceVoiceRecipe {
        source: serde_json::from_value(value["source"].clone()).unwrap(),
        mapping: serde_json::from_value(value["mapping"].clone()).unwrap(),
        offset: serde_json::from_value(value["offset"].clone()).unwrap(),
    }
}
fn visit_fixtures(mut visit: impl FnMut(&RenderPlan, &str, &AudioSignal<'_>, &Value)) {
    let expected = frozen();
    for case in expected["cases"].as_array().unwrap() {
        let document = ProjectDocument::from_json(&case["document_wire"].to_string()).unwrap();
        let plan = RenderPlan::compile(&document).unwrap();
        match case["name"].as_str().unwrap() {
            "qualified_catalog_voice" => {
                let root = plan.audio_signal();
                visit(
                    &plan,
                    "structural_carrier",
                    &root,
                    fixture_row(case, "structural_carrier"),
                );
                let voice = root
                    .source_voice(recipe_from(&case["observations"]["attempted_recipe"]))
                    .unwrap();
                for (name, signal) in [
                    ("voice_input", voice.input_signal()),
                    ("voice_output", voice.output_signal()),
                ] {
                    visit(&plan, name, &signal, fixture_row(case, name));
                }
                let voice = root
                    .source_voice(recipe_from(
                        &case["observations"]["selected_attempted_recipe"],
                    ))
                    .unwrap();
                for (name, signal) in [
                    ("selected_negative_offset_input", voice.input_signal()),
                    ("selected_negative_offset_output", voice.output_signal()),
                ] {
                    visit(&plan, name, &signal, fixture_row(case, name));
                }
            }
            "occurrence_padding_and_projected" => {
                let instance =
                    serde_json::from_value(case["observations"]["instance"].clone()).unwrap();
                let voice = plan
                    .source_voice_occurrence(
                        instance,
                        recipe_from(&case["observations"]["attempted_recipe"]),
                        Default::default(),
                    )
                    .unwrap();
                let projection = voice.processing_projection().unwrap();
                projection
                    .input_tape()
                    .unwrap()
                    .provider_before_visit(|index, signal, _| {
                        let name = format!("projection_input_run_{index}");
                        visit(&plan, &name, signal, fixture_row(case, &name));
                    });
                projection
                    .output_policy()
                    .provider_before_visit(|index, signal, _| {
                        let name = format!("neutral_policy_run_{index}");
                        visit(&plan, &name, signal, fixture_row(case, &name));
                    });
                let full = ExactRatio::ZERO..ExactRatio::integer(projection.output_frames());
                let tape = crate::AudioSignalTape::new(
                    &plan,
                    full.clone(),
                    vec![crate::AudioSignalTapeRun::intrinsic(
                        full.clone(),
                        full,
                        Arc::clone(projection),
                    )],
                )
                .unwrap();
                tape.provider_before_visit(|index, signal, _| {
                    let name = format!("intrinsic_projected_run_{index}");
                    visit(&plan, &name, signal, fixture_row(case, &name));
                });
            }
            "owned_stage_input" => {
                assert!(
                    case["observations"]["signals"].is_null(),
                    "the failed public root setup stays a gap"
                );
            }
            "owned_stage_input_via_public_projection" => {
                let projection = plan
                    .owned_preserve_projection(crate::AudioDefinitionSelector::Node {
                        node: id("stage"),
                    })
                    .unwrap();
                let signal = projection.stage().input_signal();
                visit(
                    &plan,
                    "owned_input_via_projection",
                    &signal,
                    fixture_row(case, "owned_input_via_projection"),
                );
            }
            other => panic!("new checked fixture requires explicit public reconstruction: {other}"),
        }
    }
}

fn current_snapshot(
    plan: &RenderPlan,
    value: &Result<SignalValue<'_, Backend>, PlanError>,
) -> Value {
    match value {
        Ok(value) => json!({"result":"Ok","query":query(plan,&value.query)}),
        Err(value) => json!({"result":"Error","error":error(value)}),
    }
}
fn admitted_policy_result(
    value: &Result<AdmittedSignalPolicyQuery<usize, Lease>, PlanError>,
) -> Value {
    match value {
        Ok(value) => policy_snapshot(value),
        Err(value) => json!({"result":"Error","error":error(value)}),
    }
}
fn assert_values(ordinary: &Value, admitted: &Value, capacity_keys: &[&str]) {
    assert_eq!(
        ordinary["result"], admitted["result"],
        "complete outcomes:\nordinary={ordinary}\nadmitted={admitted}"
    );
    if ordinary["result"] == "Error" {
        assert_eq!(ordinary["error"], admitted["error"]);
        return;
    }
    let old = ordinary["query"].as_object().unwrap();
    let new = admitted["query"].as_object().unwrap();
    assert_eq!(
        old.len(),
        new.len(),
        "all ordinary returned fields retained"
    );
    for (key, value) in old {
        if key == "work" || capacity_keys.contains(&key.as_str()) {
            continue;
        }
        assert_eq!(
            new.get(key),
            Some(value),
            "returned field {key}; complete literal snapshots:\n{ordinary}\n{admitted}"
        );
    }
    for key in capacity_keys {
        assert!(ordinary["query"][key].is_u64());
        assert!(admitted["query"][key].is_u64());
    }
}
fn assert_success_work(ordinary: &Value, admitted: &Value, budget: &Value, observer: &Observer) {
    let canonical = ordinary["query"]["work"].as_u64().unwrap() as usize;
    let additional = budget["attempted_additional"].as_u64().unwrap() as usize;
    assert_eq!(
        budget["attempted_canonical"].as_u64().unwrap() as usize,
        canonical
    );
    assert_eq!(
        admitted["query"]["work"].as_u64().unwrap() as usize,
        canonical + additional
    );
    assert_eq!(observer.forwarded(), canonical + additional);
}
fn rule(value: &Value) -> AudioBoundaryRule {
    match value.as_str().unwrap() {
        "PointCeil" => AudioBoundaryRule::PointCeil,
        "RoundEven" => AudioBoundaryRule::RoundEven,
        _ => panic!("unknown frozen rule"),
    }
}
fn exact_current_old(request: &Value, stops: (bool, bool)) -> &Value {
    if stops == (false, false) {
        return &request["ordinary_current"];
    }
    &request["current_stop_variants"]
        .as_array()
        .unwrap()
        .iter()
        .find(|value| value["stops"] == json!([stops.0, stops.1]))
        .unwrap()["result"]
}
fn paired_matrix() -> Value {
    let mut signals = 0usize;
    let mut requests = 0usize;
    let mut current_lanes = 0usize;
    let mut policy_lanes = 0usize;
    let mut rows = Vec::new();
    visit_fixtures(|plan, name, signal, old_signal| {
        signals += 1;
        let before = signal.provider_before_snapshot(plan);
        assert_eq!(before, old_signal["signal_before"]);
        for grid in old_signal["grids"].as_array().unwrap() {
            let boundary = rule(&grid["rule"]);
            for request in grid["requests"].as_array().unwrap() {
                requests += 1;
                let samples = request_range(request);
                let exact = request_limits(request);
                // A separate adequate-work query preserves this row's actual span limit.
                // The original request/cap and actual refusal remain in exact_* below.
                let adequate = AudioQueryLimits {
                    maximum_work: AudioQueryLimits::default().maximum_work,
                    ..exact
                };
                let mut pairs = Vec::new();
                for stops in [(false, false), (false, true), (true, false), (true, true)] {
                    current_lanes += 1;
                    let mut backend = Backend::new(Fault::None);
                    let observer = backend.observer();
                    let (exact_result, exact_budget) = run_current(
                        signal,
                        samples.clone(),
                        exact,
                        boundary,
                        stops,
                        &mut backend,
                    );
                    let exact_body = current_snapshot(plan, &exact_result);
                    drop(exact_result);
                    observer.assert_released();
                    let exact_events = observer.events();
                    let ordinary =
                        signal.query_inner(samples.clone(), adequate, boundary, stops.0, stops.1);
                    let old = query_result(plan, &ordinary);
                    drop(ordinary);
                    let mut backend = Backend::new(Fault::None);
                    let observer = backend.observer();
                    let (result, budget) = run_current(
                        signal,
                        samples.clone(),
                        adequate,
                        boundary,
                        stops,
                        &mut backend,
                    );
                    let observed = current_snapshot(plan, &result);
                    assert_values(&old, &observed, &["span_capacity"]);
                    if result.is_ok() {
                        assert_success_work(&old, &observed, &budget, &observer);
                    }
                    drop(result);
                    observer.assert_released();
                    pairs.push(json!({"kind":"current","stops":[stops.0,stops.1],
                        "ordinary_at_original_limit":exact_current_old(request,stops),
                        "admitted_at_original_limit":exact_body,"exact_budget":exact_budget,"exact_events":exact_events,
                        "adequate_limits":limits(adequate),"ordinary_at_adequate_work":old,
                        "admitted_at_adequate_work":observed,"adequate_budget":budget,"adequate_events":observer.events()}));
                }
                for endpoints in [true, false] {
                    policy_lanes += 1;
                    let mut backend = Backend::new(Fault::None);
                    let observer = backend.observer();
                    let (exact_result, exact_budget) = run_policy(
                        signal,
                        samples.clone(),
                        exact,
                        boundary,
                        endpoints,
                        &mut backend,
                    );
                    let exact_body = admitted_policy_result(&exact_result);
                    drop(exact_result);
                    observer.assert_released();
                    let exact_events = observer.events();
                    let ordinary =
                        signal.policy_on_grid(samples.clone(), adequate, boundary, endpoints);
                    let old = policy_result(&ordinary);
                    drop(ordinary);
                    let mut backend = Backend::new(Fault::None);
                    let observer = backend.observer();
                    let (result, budget) = run_policy(
                        signal,
                        samples.clone(),
                        adequate,
                        boundary,
                        endpoints,
                        &mut backend,
                    );
                    let observed = admitted_policy_result(&result);
                    assert_values(
                        &old,
                        &observed,
                        &["suppressed_capacity", "contents_capacity"],
                    );
                    if let Ok(value) = &result {
                        assert_success_work(&old, &observed, &budget, &observer);
                        assert_eq!(
                            value.canonical_units(),
                            budget["attempted_canonical"].as_u64().unwrap() as usize
                        );
                        assert_eq!(
                            value.additional_units(),
                            budget["attempted_additional"].as_u64().unwrap() as usize
                        );
                        assert!(value.was_forwarded());
                    }
                    drop(result);
                    observer.assert_released();
                    let old_key = if endpoints {
                        "ordinary_policy_endpoints_true"
                    } else {
                        "ordinary_policy_endpoints_false"
                    };
                    pairs.push(json!({"kind":"policy","endpoints":endpoints,
                        "ordinary_at_original_limit":request[old_key],"admitted_at_original_limit":exact_body,
                        "exact_budget":exact_budget,"exact_events":exact_events,"adequate_limits":limits(adequate),
                        "ordinary_at_adequate_work":old,"admitted_at_adequate_work":observed,
                        "adequate_budget":budget,"adequate_events":observer.events()}));
                }
                assert_eq!(signal.provider_before_snapshot(plan), before);
                rows.push(
                    json!({"signal":name,"rule":grid["rule"],"request":request["name"],
                    "samples":samples,"original_limits":limits(exact),"pairs":pairs}),
                );
            }
        }
        assert_eq!(
            signal.provider_before_snapshot(plan),
            old_signal["signal_after"]
        );
    });
    json!({"schema":"provider-child-paired-tests-v1","signals":signals,"requests":requests,
        "current_lanes":current_lanes,"policy_lanes":policy_lanes,"rows":rows,
        "scope":{"exact":"original ordinary request and fresh admitted result retain their literal work/error/capacities",
            "adequate":"independent default-work lane, original span cap; ordinary queried at same adequate limits",
            "capacity":"actual capacities recorded separately; semantic equality does not assert allocator capacity parity",
            "receipts":"attempted query-budget counters and actual accepted callback totals are separate on failures"}})
}
#[test]
fn ordinary_provider_children_match_frozen_baseline_v1() {
    // External freeze proof pins SHA-256 efcbab2068e2fc3a4e3be459a92954b2283f94d79feacf8211ff697c62bee0ad.
    assert_eq!(FROZEN.len(), 1_726_397);
    let mut actual = serde_json::to_vec(&matrix()).unwrap();
    actual.push(b'\n');
    assert_eq!(
        actual, FROZEN,
        "ordinary body remains byte-exact; no provenance or work normalization"
    );
}
#[test]
fn paired_provider_children_preserve_complete_values_and_canonical_work() {
    let observed = paired_matrix();
    assert_eq!(observed["signals"], 11);
    assert_eq!(observed["requests"], 340);
    assert_eq!(observed["current_lanes"], 1360);
    assert_eq!(observed["policy_lanes"], 680);
}
#[test]
fn source_output_inventory_and_projected_leaf_barrier_are_real() {
    let mut source_outputs = 0;
    let mut projected = 0;
    let mut owned = 0;
    visit_fixtures(|plan, name, signal, old| {
        let grid = &old["grids"][0];
        let full = grid["requests"]
            .as_array()
            .unwrap()
            .iter()
            .find(|request| request["name"] == "full")
            .unwrap();
        let samples = request_range(full);
        let cap = AudioQueryLimits::default();
        if name == "voice_output" || name == "selected_negative_offset_output" {
            let mut backend = Backend::new(Fault::None);
            let observer = backend.observer();
            let (value, _) = run_policy(
                signal,
                samples.clone(),
                cap,
                AudioBoundaryRule::PointCeil,
                true,
                &mut backend,
            );
            let value = value.unwrap();
            let current = signal
                .query_inner(
                    samples.clone(),
                    cap,
                    AudioBoundaryRule::PointCeil,
                    false,
                    false,
                )
                .unwrap();
            let current_contents = current
                .spans
                .iter()
                .map(|span| {
                    let AudioSignalContent::Leaf(content) = &span.content else {
                        panic!("Source current query is not Leaf")
                    };
                    serde_json::to_value(content).unwrap()
                })
                .collect::<Vec<_>>();
            drop(current);
            assert_eq!(
                serde_json::to_value(value.view().contents).unwrap(),
                json!(current_contents),
                "only exact independent voice current inventory survives the carrier"
            );
            let mut catalog_sources = 0;
            for content in value.view().contents {
                if let AudioContent::Source { source, .. } = content {
                    assert_eq!(source.asset, AssetId::new("sound").unwrap());
                    catalog_sources += 1;
                }
            }
            assert!(catalog_sources > 0);
            drop(value);
            observer.assert_released();
            source_outputs += 1;
        }
        if name == "intrinsic_projected_run_0" {
            for empty in [false, true] {
                let selected = if empty {
                    samples.start..samples.start
                } else {
                    samples.clone()
                };
                let mut backend = Backend::new(Fault::None);
                let observer = backend.observer();
                let (current, current_budget) = run_current(
                    signal,
                    selected.clone(),
                    cap,
                    AudioBoundaryRule::PointCeil,
                    (false, false),
                    &mut backend,
                );
                let current = current.unwrap();
                assert_eq!(current_budget["attempted_canonical"], 1);
                assert_eq!(current.query.spans.is_empty(), empty);
                if !empty {
                    assert!(matches!(
                        current.query.spans[0].content,
                        AudioSignalContent::ProjectedStage(_)
                    ));
                }
                drop(current);
                observer.assert_released();
                let mut backend = Backend::new(Fault::None);
                let observer = backend.observer();
                let (policy, budget) = run_policy(
                    signal,
                    selected,
                    cap,
                    AudioBoundaryRule::PointCeil,
                    false,
                    &mut backend,
                );
                assert_eq!(budget["attempted_canonical"], 2);
                if empty {
                    assert!(policy.is_ok());
                } else {
                    assert!(matches!(
                        policy,
                        Err(PlanError::InvalidPlan("current policy retained processing"))
                    ));
                }
                drop(policy);
                observer.assert_released();
            }
            projected += 1;
        }
        if name == "owned_input_via_projection" {
            for invalid in [false, true] {
                let cap = AudioQueryLimits {
                    maximum_spans: if invalid { 0 } else { cap.maximum_spans },
                    ..cap
                };
                let mut backend = Backend::new(Fault::None);
                let observer = backend.observer();
                let (value, _) = run_current(
                    signal,
                    samples.clone(),
                    cap,
                    AudioBoundaryRule::PointCeil,
                    (false, false),
                    &mut backend,
                );
                if invalid {
                    assert!(matches!(value, Err(PlanError::InvalidAudioLimits)));
                } else {
                    assert!(matches!(
                        value,
                        Err(PlanError::InvalidPlan(
                            "owned Preserve input requires its compiled projection"
                        ))
                    ));
                }
                assert!(observer.events().is_empty());
                drop(value);
                observer.assert_released();
            }
            owned += 1;
        }
        assert_eq!(signal.provider_before_snapshot(plan), old["signal_before"]);
    });
    assert_eq!((source_outputs, projected, owned), (2, 1, 1));
}

#[test]
#[ignore = "manual paired Plan receipt; fresh /tmp output with source provenance"]
fn capture_admitted_provider_child_pairs_v1() {
    let output =
        std::env::var("DEADPAN_PROVIDER_CHILD_PAIRED_OUTPUT").expect("explicit fresh /tmp output");
    let path = Path::new(&output);
    assert!(path.is_absolute());
    assert!(
        path.parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .starts_with(Path::new("/tmp").canonicalize().unwrap())
    );
    let variant = std::env::var("DEADPAN_PROVIDER_CHILD_PAIRED_VARIANT").expect("actual variant");
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source = std::env::var("DEADPAN_PROVIDER_CHILD_PAIRED_SOURCE_SHA256")
        .expect("actual complete source digest");
    assert!(source.len() == 64 && source.bytes().all(|b| b.is_ascii_hexdigit()));
    let checkpoint = std::env::var("DEADPAN_PROVIDER_CHILD_PAIRED_PUBLISHED_CHECKPOINT")
        .expect("published base G");
    assert_eq!(checkpoint, CHECKPOINT);
    let mut body = paired_matrix();
    body["variant"] = json!(variant);
    body["source_sha256"] = json!(source);
    body["published_checkpoint"] = json!(checkpoint);
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
