//! Independent OLD processing/policy observations before any tape reuse.
//! The second-query observer records the actual policy call, never a replay.
use super::*;
use std::{cell::RefCell, fs::OpenOptions, io::Write, path::Path};

thread_local! {
    static EVENTS: RefCell<Option<Vec<Value>>> = const { RefCell::new(None) };
}
pub(in crate::plan) struct Observation;
impl Observation {
    pub(in crate::plan) fn start() -> Self {
        EVENTS.with(|slot| {
            assert!(slot.borrow().is_none(), "nested policy observation");
            *slot.borrow_mut() = Some(Vec::new());
        });
        Self
    }
    pub(in crate::plan) fn finish(self) -> Vec<Value> {
        EVENTS.with(|slot| slot.borrow_mut().take().unwrap())
    }
}
impl Drop for Observation {
    fn drop(&mut self) {
        EVENTS.with(|slot| {
            slot.borrow_mut().take();
        });
    }
}
fn active() -> bool {
    EVENTS.with(|slot| slot.borrow().is_some())
}
fn record(event: Value) {
    EVENTS.with(|slot| slot.borrow_mut().as_mut().unwrap().push(event));
}
fn lookup(value: LookupStats) -> Value {
    json!({"visited_nodes":value.visited_nodes,"sequence_comparisons":value.sequence_comparisons,
        "iteration_run_comparisons":value.iteration_run_comparisons})
}
fn processing_query(plan: &RenderPlan, value: &crate::AudioProcessingQuery<'_>) -> Value {
    json!({"project_id":value.project_id.as_str(),"revision_id":value.revision_id.as_str(),
        "samples":value.samples,"work":value.work,"lookup":lookup(value.lookup),
        "spans":value.spans.iter().map(|span|capture_baseline::processing(plan,span)).collect::<Vec<_>>()})
}
fn processing_result(
    plan: &RenderPlan,
    value: &Result<crate::AudioProcessingQuery<'_>, PlanError>,
) -> Value {
    match value {
        Ok(query) => json!({"result":"Ok","query":processing_query(plan,query)}),
        Err(error) => json!({"result":"Error","error":error_json(error),
            "partial_work":"not exposed by the OLD query error"}),
    }
}
fn policy_result(value: &Result<crate::AudioPolicyQuery<AudioSample>, PlanError>) -> Value {
    match value {
        Ok(query) => json!({"result":"Ok","policy":{"suppressed":query.suppressed,
            "contents":query.contents,"lookup":lookup(query.lookup),"work":query.work}}),
        Err(error) => json!({"result":"Error","error":error_json(error),
            "partial_work":"not exposed by the OLD policy error; actual observed second-query events remain separate"}),
    }
}

/// Inactive outside the manual producer. No allocation or extra query occurs
/// before this flag check. The live query value is observed before accounting.
pub(in crate::plan) fn second_returned(
    physical: &AudioDomain<'_>,
    samples: Range<i64>,
    state: (usize, bool, usize),
    result: &Result<crate::AudioProcessingQuery<'_>, PlanError>,
) {
    if !active() {
        return;
    }
    let (depth, endpoints, before) = state;
    record(
        json!({"phase":"actual_second_query_returned","depth":depth,"endpoints":endpoints,
        "samples":{"start":samples.start,"end":samples.end},"physical_domain":domain(physical),
        "policy_remaining_before_query":before,"policy_remaining_at_query_return":before,
        "query":processing_result(physical.plan,result)}),
    );
}

/// This second scalar event proves the actual old policy debit and its order.
/// The full query was recorded once, without duplicating its private payload.
pub(in crate::plan) fn second_accounted(
    samples: Range<i64>,
    state: (usize, usize, usize),
    query_work: usize,
    result: &Result<(), PlanError>,
) {
    if !active() {
        return;
    }
    let (depth, before, after) = state;
    record(
        json!({"phase":"actual_second_query_accounted","depth":depth,
        "samples":{"start":samples.start,"end":samples.end},"query_work":query_work,
        "policy_remaining_before":before,"policy_remaining_after":after,
        "accounting":match result {Ok(())=>json!({"result":"Ok"}),
            Err(error)=>json!({"result":"Error","error":error_json(error)})}}),
    );
}

fn observe_policy(
    physical: &AudioDomain<'_>,
    samples: Range<AudioSample>,
    limits: AudioQueryLimits,
) -> (
    Result<crate::AudioPolicyQuery<AudioSample>, PlanError>,
    Vec<Value>,
) {
    let observation = Observation::start();
    let result = physical.policy(samples, limits);
    (result, observation.finish())
}

fn sample_window(start: i64, end: i64) -> Range<AudioSample> {
    AudioSample(start)..AudioSample(end)
}
fn limit_value(limits: AudioQueryLimits) -> Value {
    json!({"maximum_work":limits.maximum_work,"maximum_spans":limits.maximum_spans})
}
fn eligibility(value: &Result<crate::AudioProcessingQuery<'_>, PlanError>) -> Value {
    match value {
        Err(_) => json!({"successful_complete_query":false,"content_only_eligible":false}),
        Ok(query) => {
            let mut leaf = 0;
            let mut bound = 0;
            let mut stage = 0;
            let mut projected = 0;
            for span in &query.spans {
                match &span.content {
                    crate::AudioSignalContent::Leaf(_) => leaf += 1,
                    crate::AudioSignalContent::Bound(_) => bound += 1,
                    crate::AudioSignalContent::Stage(_) => stage += 1,
                    crate::AudioSignalContent::ProjectedStage(_) => projected += 1,
                }
            }
            json!({"successful_complete_query":true,"content_only_eligible":stage==0&&projected==0,
                "nonempty_requested_range":query.samples.start<query.samples.end,
                "leaf_spans":leaf,"bound_spans":bound,"stage_spans":stage,"projected_stage_spans":projected})
        }
    }
}
fn parent_case(
    name: &str,
    physical: &AudioDomain<'_>,
    samples: Range<AudioSample>,
    construction: Value,
    extra_refusals: bool,
) -> Value {
    let first = physical.processing(samples.clone(), Default::default());
    let separate_second = physical.processing_inner(samples.clone(), Default::default(), false);
    let (policy, events) = observe_policy(physical, samples.clone(), Default::default());
    let mut policies = vec![
        json!({"name":"default","limits":limit_value(Default::default()),
        "output":policy_result(&policy),"actual_second_query_events":events}),
    ];
    if samples.start < samples.end {
        if let Ok(query) = &policy {
            for (label, maximum) in [
                ("exact_old_policy_work", query.work),
                ("one_short_old_policy_work", query.work.saturating_sub(1)),
            ] {
                let limits = AudioQueryLimits {
                    maximum_work: maximum,
                    ..Default::default()
                };
                let (result, events) = observe_policy(physical, samples.clone(), limits);
                policies.push(json!({"name":label,"limits":limit_value(limits),
                    "output":policy_result(&result),"actual_second_query_events":events}));
            }
        }
        if extra_refusals {
            for (label, limits) in [
                (
                    "minimum_work",
                    AudioQueryLimits {
                        maximum_work: 1,
                        ..Default::default()
                    },
                ),
                (
                    "invalid_span_limit",
                    AudioQueryLimits {
                        maximum_spans: 0,
                        ..Default::default()
                    },
                ),
            ] {
                let (result, events) = observe_policy(physical, samples.clone(), limits);
                policies.push(json!({"name":label,"limits":limit_value(limits),
                    "output":policy_result(&result),"actual_second_query_events":events}));
            }
        }
    }
    json!({"name":name,"construction":construction,"physical_domain":domain(physical),
        "has_audio_bindings":physical.has_audio_bindings(),"request":sample_range(samples,|s|s.0),
        "first_true_limits":limit_value(Default::default()),"first_processing_true":processing_result(physical.plan,&first),
        "separate_false_limits":limit_value(Default::default()),"separate_processing_false":processing_result(physical.plan,&separate_second),
        "first_tape_eligibility":eligibility(&first),"policy_cases":policies})
}

fn tone(frames_count: i64) -> BeatNode {
    let mut node = hold(frames_count);
    let NodeKind::Hold { recipe } = &mut node.kind else {
        unreachable!()
    };
    recipe.audio = HoldAudio::Tone {
        frequency_hz: 440,
        level: GainDb::new(-6000).unwrap(),
    };
    node
}
fn observed_owner<'plan>(plan: &'plan RenderPlan, alias: &str, at: i64) -> AudioOwnerClock<'plan> {
    plan.audio_gain_owners(sample_window(at, at + 1), Default::default())
        .unwrap()
        .spans()[0]
        .owners()
        .iter()
        .find(|owner| owner.instance().node == id(alias))
        .unwrap()
        .clone()
}

/// Checked additional constructors remain available to later differential
/// tests. Every visit borrows the actual input-produced physical Domain.
pub(in crate::plan) fn visit_additional(
    mut visit: impl FnMut(&str, &ProjectDocument, &AudioOwnerInput<'_>, &AudioDomain<'_>),
) {
    for (name, pitch, duration, selected) in [
        (
            "exposed_preserve_and_silent_hold",
            PitchPolicy::Preserve,
            12,
            "root",
        ),
        (
            "shift_at_unity_and_silent_hold",
            PitchPolicy::Shift { semitones: 3 },
            8,
            "root",
        ),
        (
            "inside_preserve_actual_group_domain",
            PitchPolicy::Preserve,
            12,
            "group",
        ),
    ] {
        let doc = document(
            48_000,
            &["stage", "silent"],
            vec![
                ("voice", tone(8)),
                ("group", sequence(&["voice"])),
                (
                    "stage",
                    retime("group", duration, 0..8, pitch, RetimePurpose::Edit),
                ),
                ("silent", hold(2)),
            ],
        );
        // Bind only the unrelated sibling. The selected root still exposes
        // the authored Stage, while plan-global binding presence forces the
        // actual OLD second query and exact-budget fallback path to execute.
        let doc = if selected == "root" {
            bind(
                &doc,
                &doc,
                "silent",
                AudioClockRoot::ProjectRootRoundEven,
                None,
            )
        } else {
            doc
        };
        let plan = RenderPlan::compile(&doc).unwrap();
        let owner = observed_owner(&plan, selected, 0);
        let input = plan
            .audio_owner_input(&owner, AudioSample(0), Default::default())
            .unwrap();
        visit(name, &doc, &input, input.context());
    }
    let nodes = || {
        vec![
            ("voice", tone(8)),
            (
                "stage",
                retime(
                    "voice",
                    12,
                    0..8,
                    PitchPolicy::Preserve,
                    RetimePurpose::Edit,
                ),
            ),
            ("silent", hold(2)),
            ("group", sequence(&["stage", "silent"])),
        ]
    };
    let old = document(48_000, &["group"], nodes());
    let current = document(48_000, &["group"], nodes());
    let doc = bind(
        &current,
        &old,
        "stage",
        AudioClockRoot::ProjectRootRoundEven,
        None,
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    let owner = observed_owner(&plan, "root", 0);
    let input = plan
        .audio_owner_input(&owner, AudioSample(0), Default::default())
        .unwrap();
    visit(
        "opaque_bound_stage_with_preserve_and_silence",
        &doc,
        &input,
        input.context(),
    );

    // Edit retains its authored crop and inherited retime. The checked
    // Partition fixture instead exposes the complete signed physical group.
    for (name, purpose, signed) in [
        (
            "cropped_group_domain_with_inherited_edit_retime",
            RetimePurpose::Edit,
            false,
        ),
        (
            "hidden_signed_partition_group_with_unrelated_binding",
            RetimePurpose::Partition,
            true,
        ),
    ] {
        let doc = document(
            48_000,
            &["prefix", "crop"],
            vec![
                ("prefix", hold(1)),
                ("a", hold(4)),
                ("b", hold(8)),
                ("c", hold(4)),
                ("group", sequence(&["a", "b", "c"])),
                (
                    "crop",
                    retime("group", 8, 4..12, PitchPolicy::FollowSpeed, purpose),
                ),
            ],
        );
        let doc = if signed {
            bind(
                &doc,
                &doc,
                "prefix",
                AudioClockRoot::ProjectRootRoundEven,
                None,
            )
        } else {
            doc
        };
        let plan = RenderPlan::compile(&doc).unwrap();
        let owner = observed_owner(&plan, "group", 3);
        let input = plan
            .audio_owner_input(&owner, AudioSample(3), Default::default())
            .unwrap();
        if signed {
            assert_eq!(input.context().root_samples(), sample_window(-3, 13));
            assert_eq!(input.context().visible_samples(), sample_window(1, 9));
            assert!(input.context().has_audio_bindings());
        }
        visit(name, &doc, &input, input.context());
    }
}

fn matrix() -> Value {
    let wide: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/wide-gain-owner-baseline-v1/snapshot.json"
    ))
    .unwrap();
    let mut cases = Vec::new();
    let mut root_references = Vec::new();
    let mut additional_documents = Vec::new();
    wide_baseline::visit_cases(&wide, |parent, case| match parent {
        wide_baseline::Parent::Root(_) => root_references.push(json!({
            "name":case["name"],"scope":"Root ordinary path remains out of candidate scope",
            "request":case["request"],"seed_identity":case["seed_identity"]})),
        wide_baseline::Parent::Domain(physical) => {
            let window = &case["request"]["window"];
            let samples = sample_window(
                window["start"].as_i64().unwrap(),
                window["end"].as_i64().unwrap(),
            );
            let name = case["name"].as_str().unwrap();
            cases.push(parent_case(
                name,
                physical,
                samples.clone(),
                case["seed_identity"].clone(),
                name == "hidden_resumed_actual_capture_domain",
            ));
            cases.push(parent_case(
                &format!("{name}/empty"),
                physical,
                samples.start..samples.start,
                json!({"same_actual_parent_as":name}),
                false,
            ));
        }
    });
    visit_additional(|name, doc, input, physical| {
        additional_documents.push(
            json!({"name":name,"document":serde_json::to_value(doc).unwrap(),
            "setup_input":super::input(input)}),
        );
        let samples = physical.root_samples();
        cases.push(parent_case(
            name,
            physical,
            samples.clone(),
            json!({"source":"actual_root_or_group_owner_input.context",
            "setup_input":super::input(input)}),
            name == "shift_at_unity_and_silent_hold",
        ));
        cases.push(parent_case(
            &format!("{name}/empty"),
            physical,
            samples.start..samples.start,
            json!({"same_actual_parent_as":name}),
            false,
        ));
    });
    let mut coverage = serde_json::Map::new();
    let mut eligible = 0;
    let mut noneligible = 0;
    let mut top_second = 0;
    let mut recursive_second = 0;
    let mut policy_calls = 0;
    let mut negative = 0;
    let mut inherited = 0;
    let mut bypass = 0;
    let mut bound = 0;
    let mut stage = 0;
    let mut silent_hold_contents = 0;
    let mut source_contents = 0;
    let mut suppressed_ranges = 0;
    let mut noneligible_binding_with_actual_second = 0;
    let mut negative_binding_with_actual_second = 0;
    for case in &cases {
        let e = &case["first_tape_eligibility"];
        let has_stage = e["stage_spans"].as_u64().unwrap_or(0) > 0
            || e["projected_stage_spans"].as_u64().unwrap_or(0) > 0;
        eligible += usize::from(
            e["content_only_eligible"] == true && e["nonempty_requested_range"] == true,
        );
        noneligible += usize::from(has_stage);
        bound += e["bound_spans"].as_u64().unwrap_or(0);
        stage += e["stage_spans"].as_u64().unwrap_or(0);
        let d = &case["physical_domain"];
        let is_negative = d["root_samples"]["start"].as_i64().unwrap() < 0;
        negative += usize::from(is_negative);
        inherited += usize::from(!d["seed"]["retimes"].as_array().unwrap().is_empty());
        bypass += usize::from(!d["seed"]["bypass_binding"].is_null());
        for policy in case["policy_cases"].as_array().unwrap() {
            policy_calls += 1;
            if is_negative && case["has_audio_bindings"] == true && policy["name"] == "default" {
                negative_binding_with_actual_second += usize::from(
                    policy["actual_second_query_events"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|event| {
                            event["phase"] == "actual_second_query_returned" && event["depth"] == 0
                        }),
                );
            }
            if has_stage && case["has_audio_bindings"] == true && policy["name"] == "default" {
                noneligible_binding_with_actual_second += usize::from(
                    policy["actual_second_query_events"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|event| {
                            event["phase"] == "actual_second_query_returned" && event["depth"] == 0
                        }),
                );
            }
            if let Some(contents) = policy["output"]["policy"]["contents"].as_array() {
                silent_hold_contents += contents
                    .iter()
                    .filter(|v| v["type"] == "silence" && v["reason"] == "silent_hold")
                    .count();
                source_contents += contents.iter().filter(|v| v["type"] == "source").count();
            }
            if let Some(ranges) = policy["output"]["policy"]["suppressed"].as_array() {
                suppressed_ranges += ranges.len();
            }
            for event in policy["actual_second_query_events"].as_array().unwrap() {
                if event["phase"] == "actual_second_query_returned" {
                    if event["depth"] == 0 {
                        top_second += 1;
                    } else {
                        recursive_second += 1;
                    }
                }
            }
        }
    }
    assert!(
        noneligible_binding_with_actual_second > 0,
        "coverage requires an actual Stage-exposing binding Domain's second query"
    );
    assert!(
        negative_binding_with_actual_second > 0,
        "coverage requires an actual signed binding Domain's second query"
    );
    for (name, value) in [
        ("actual_domain_window_records", cases.len()),
        ("root_out_of_scope_references", root_references.len()),
        ("nonempty_content_eligible_records", eligible),
        ("noneligible_stage_records", noneligible),
        ("policy_api_calls", policy_calls),
        ("actual_top_second_query_calls", top_second),
        ("actual_recursive_second_query_calls", recursive_second),
        ("negative_domain_records", negative),
        (
            "negative_binding_records_with_actual_top_second_query",
            negative_binding_with_actual_second,
        ),
        ("inherited_retime_domain_records", inherited),
        ("bypass_binding_domain_records", bypass),
        ("observed_bound_spans", bound as usize),
        ("observed_stage_spans", stage as usize),
        ("policy_silent_hold_content_entries", silent_hold_contents),
        ("policy_source_content_entries", source_contents),
        ("policy_suppressed_range_entries", suppressed_ranges),
        (
            "noneligible_binding_records_with_actual_top_second_query",
            noneligible_binding_with_actual_second,
        ),
    ] {
        coverage.insert(name.to_owned(), json!(value));
    }
    json!({"schema":"old-policy-binding-query-baseline-v1","scope":"Actual OLD Domain first processing(true), separate false, full policy and actual depth-tagged second-query observations; Root and synthetic ProjectedStage are not certified here",
        "documents":wide["documents"],"additional_documents":additional_documents,
        "root_out_of_scope_references":root_references,"cases":cases,"coverage":coverage})
}

#[test]
fn legacy_domain_policy_binding_queries_match_immutable_old_matrix() {
    let bytes = include_str!("../../../tests/fixtures/policy-binding-baseline-v1/snapshot.json");
    assert_eq!(bytes.len(), 971_166, "immutable OLD snapshot byte count");
    let mut frozen: Value = serde_json::from_str(bytes).unwrap();
    let metadata = frozen.as_object_mut().unwrap();
    assert_eq!(metadata.remove("variant"), Some(json!("main-inherited")));
    assert_eq!(
        metadata.remove("source_sha256"),
        Some(json!(
            "51025c4287c705f3596ba06a047e00752a26efebc638755389ccb9588687c5cb"
        ))
    );
    // Only producer provenance is excluded. Complete query payloads, actual
    // second-query events, policy results and OLD receipts remain literal.
    assert_eq!(matrix(), frozen);
}

#[test]
#[ignore = "manual immutable OLD producer; requires a new absolute /tmp output"]
fn capture_old_policy_binding_baseline_v1() {
    let output = std::env::var("DEADPAN_POLICY_BINDING_OUTPUT").expect("explicit output required");
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
    let variant =
        std::env::var("DEADPAN_POLICY_BINDING_VARIANT").expect("explicit variant required");
    let source = std::env::var("DEADPAN_POLICY_BINDING_SOURCE_SHA256")
        .expect("source manifest SHA256 required");
    assert_eq!(source.len(), 64);
    assert!(source.bytes().all(|v| v.is_ascii_hexdigit()));
    let mut value = matrix();
    value["variant"] = json!(variant);
    value["source_sha256"] = json!(source);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .unwrap();
    serde_json::to_writer_pretty(&mut file, &value).unwrap();
    file.write_all(b"\n").unwrap();
    file.sync_all().unwrap();
}
