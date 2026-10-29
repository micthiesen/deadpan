//! OLD direct owner-input API oracle, separate from owner/candidate setup work.
//! Reuses the complete existing Input and actual capture/discard serializers.
use super::wide_baseline::Parent as WideParent;
use super::*;
use std::{
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
};

fn output_path(path: &Path) -> PathBuf {
    assert!(path.is_absolute(), "direct oracle output must be absolute");
    let parent = path
        .parent()
        .expect("output parent")
        .canonicalize()
        .unwrap();
    let scratch = Path::new("/tmp").canonicalize().unwrap();
    assert!(
        parent.starts_with(scratch),
        "direct oracle output must remain under canonical /tmp"
    );
    parent.join(path.file_name().expect("output filename"))
}

fn setup_query<'plan>(
    parent: WideParent<'_, 'plan>,
    samples: Range<AudioSample>,
) -> AudioOwnerQuery<'plan> {
    match parent {
        WideParent::Root(plan) => plan.audio_gain_owners(samples, Default::default()),
        WideParent::Domain(domain) => domain.gain_owners(samples, Default::default()),
    }
    .unwrap()
}

fn measured(
    parent: WideParent<'_, '_>,
    name: &str,
    selected: &AudioOwnerClock<'_>,
    at: AudioSample,
    limits: AudioQueryLimits,
    kind: &str,
    setup: &Value,
) -> (Value, Option<usize>) {
    let plan = parent.plan();
    // Exactly one actual OLD direct public input call per named case.
    // Internal capture_ref hooks record the very walk this call discards.
    let (result, events) =
        capture_baseline::observe_case(name, "old_direct_public_input", || match parent {
            WideParent::Root(plan) => plan.audio_owner_input(selected, at, limits),
            WideParent::Domain(domain) => domain.owner_input(selected, at, limits),
        });
    let (output, returned_work) = match result {
        Ok(value) => {
            let work = value.work();
            (
                json!({"result":"Ok","input":input(&value),"canonical_total_work":work}),
                Some(work),
            )
        }
        Err(error) => (
            json!({
                "result":"Error","error":error_json(&error),"canonical_total_work":Value::Null,
                "work_observability":"OLD public error has no total-work receipt; actual capture event budget/work retained if capture was reached"
            }),
            None,
        ),
    };
    (
        json!({
            "name":name,
            "api":match parent {WideParent::Root(_)=>"RenderPlan::audio_owner_input",WideParent::Domain(_)=>"AudioDomain::owner_input"},
            "case_kind":kind,"setup":setup,
            "request":{"at":at.0,"selected_owner":clock(selected,selected.plan,|sample|sample.0),
                "selected_owner_belongs_to_request_plan":selected.belongs_to(plan),
                "limits":{"maximum_spans":limits.maximum_spans,"maximum_work":limits.maximum_work}},
            "output":output,"actual_capture_events":events,
        }),
        returned_work,
    )
}

fn matrix() -> Value {
    let wide: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/wide-gain-owner-baseline-v1/snapshot.json"
    ))
    .unwrap();
    let mut cases = Vec::new();
    let mut inactive_points = 0;
    let mut ownerless_points = 0;
    let mut repeated_points = 0;
    let mut default_gap_points = 0;
    let mut retained_points = 0;
    wide_baseline::visit_cases(&wide, |parent, parent_case| {
        let parent_name = parent_case["name"].as_str().unwrap();
        let window = &parent_case["request"]["window"];
        let samples = AudioSample(window["start"].as_i64().unwrap())
            ..AudioSample(window["end"].as_i64().unwrap());
        // Separate setup: this wide membership query is never represented as
        // work of the following measured direct public call.
        let observed = setup_query(parent, samples);
        assert_eq!(
            wide_baseline::query_snapshot(&observed, parent.plan()),
            parent_case["query"]
        );
        let setup_query = wide_baseline::query_snapshot(&observed, parent.plan());
        for (span_index, span) in observed.spans().iter().enumerate() {
            let at = span.samples().start;
            let owners = span.owners();
            if owners.is_empty() {
                // The actual Domain can have no owner in an exhausted span.
                // Borrow a real earlier observation from this same parent,
                // then ask the OLD API about it at the ownerless sample.
                let (observed_span_index, observed_span) = observed
                    .spans()
                    .iter()
                    .enumerate()
                    .take(span_index)
                    .find(|(_, span)| !span.owners().is_empty())
                    .expect("frozen ownerless tail has an earlier checked owner witness");
                let observed_member_index = observed_span.owners().len() - 1;
                let witness = &observed_span.owners()[observed_member_index];
                let setup = json!({
                    "parent_case":parent_name,"parent_identity":parent_case["seed_identity"],
                    "fixture":parent_case["fixture"],"observed_query":setup_query,
                    "selected_span_index":observed_span_index,
                    "selected_member_index":observed_member_index,
                    "owner_observed_at":observed_span.samples().start.0,
                    "ownerless_target_span_index":span_index,
                    "target_support":match span.support(){
                        AudioOwnerSupport::Active=>"Active",AudioOwnerSupport::Inactive=>"Inactive"
                    }
                });
                let (value, work) = measured(
                    parent,
                    &format!("{parent_name}/span-{span_index}/ownerless-sample"),
                    witness,
                    at,
                    Default::default(),
                    "valid_owner_at_ownerless_sample",
                    &setup,
                );
                assert_eq!(
                    value["output"]["error"]["message"],
                    "audio input owner is not current at sample"
                );
                assert!(
                    work.is_none(),
                    "OLD public error carries no total-work receipt"
                );
                cases.push(value);
                ownerless_points += 1;
                continue;
            }
            let mut selected_indices = vec![0];
            if owners.len() > 1 {
                selected_indices.push(owners.len() - 1);
            }
            for member in selected_indices {
                let selected = &owners[member];
                inactive_points += usize::from(span.support() == AudioOwnerSupport::Inactive);
                repeated_points += usize::from(!selected.instance.repeats.is_empty());
                default_gap_points += usize::from(selected.kind == AudioOwnerKind::DefaultGap);
                retained_points += usize::from(matches!(
                    selected.origin,
                    AudioOwnerClockOrigin::Retained { .. }
                ));
                let setup = json!({
                    "parent_case":parent_name,"parent_identity":parent_case["seed_identity"],
                    "fixture":parent_case["fixture"],"observed_query":setup_query,
                    "selected_span_index":span_index,"selected_member_index":member,
                    "selected_support":match span.support(){AudioOwnerSupport::Active=>"Active",AudioOwnerSupport::Inactive=>"Inactive"}
                });
                let prefix = format!("{parent_name}/span-{span_index}/member-{member}");
                let (ample, work) = measured(
                    parent,
                    &format!("{prefix}/ample"),
                    selected,
                    at,
                    Default::default(),
                    "actual_checked_member",
                    &setup,
                );
                cases.push(ample);
                if let Some(work) = work {
                    for (suffix, maximum_work) in [("exact", work), ("one_short", work - 1)] {
                        let (value, _) = measured(
                            parent,
                            &format!("{prefix}/{suffix}"),
                            selected,
                            at,
                            AudioQueryLimits {
                                maximum_work,
                                ..Default::default()
                            },
                            suffix,
                            &setup,
                        );
                        cases.push(value);
                    }
                }
            }
        }
        let selected = observed.spans()[0].owners().last().unwrap();
        let at = observed.spans()[0].samples().start;
        let setup = json!({
            "parent_case":parent_name,"parent_identity":parent_case["seed_identity"],
            "fixture":parent_case["fixture"],"observed_query":setup_query,
            "selected_span_index":0,"selected_member_index":observed.spans()[0].owners().len()-1,
        });
        let mut absent = selected.clone();
        absent.instance.node = NodeId::new("not-a-current-direct-input-owner").unwrap();
        let (value, _) = measured(
            parent,
            &format!("{parent_name}/absent-instance"),
            &absent,
            at,
            Default::default(),
            "absent_instance",
            &setup,
        );
        assert_eq!(value["output"]["result"], "Error");
        cases.push(value);

        let mut phase = selected.clone();
        phase.sampling = crate::AudioSampleMap::new(
            selected.sampling.anchor(),
            selected
                .sampling
                .local_at_anchor()
                .checked_add(ExactRatio::integer(1))
                .unwrap(),
            selected.sampling.local_frames_per_sample(),
        )
        .unwrap();
        let (value, _) = measured(
            parent,
            &format!("{parent_name}/phase-mismatch"),
            &phase,
            at,
            Default::default(),
            "sampling_phase_mismatch",
            &setup,
        );
        assert_eq!(value["output"]["result"], "Error");
        cases.push(value);

        let mut slope = selected.clone();
        slope.sampling = crate::AudioSampleMap::new(
            at,
            selected.sampling.local_at(at).unwrap(),
            selected
                .sampling
                .local_frames_per_sample()
                .checked_mul(ExactRatio::integer(2))
                .unwrap(),
        )
        .unwrap();
        let (value, _) = measured(
            parent,
            &format!("{parent_name}/slope-mismatch"),
            &slope,
            at,
            Default::default(),
            "sampling_slope_mismatch_same_local_at_observation",
            &setup,
        );
        assert_eq!(value["output"]["result"], "Error");
        cases.push(value);

        let mut play = selected.clone();
        play.play = Some(selected.play.map_or(0, |play| play.checked_add(1).unwrap()));
        let (value, _) = measured(
            parent,
            &format!("{parent_name}/play-mismatch"),
            &play,
            at,
            Default::default(),
            "hidden_play_mismatch",
            &setup,
        );
        assert_eq!(value["output"]["result"], "Error");
        cases.push(value);

        let fixture = wide["documents"]
            .as_array()
            .unwrap()
            .iter()
            .find(|fixture| fixture["fixture"] == parent_case["fixture"])
            .unwrap();
        let foreign_document =
            ProjectDocument::from_json(&fixture["document"].to_string()).unwrap();
        let foreign_plan = RenderPlan::compile(&foreign_document).unwrap();
        let mut foreign = selected.clone();
        foreign.plan = &foreign_plan;
        let (value, _) = measured(
            parent,
            &format!("{parent_name}/foreign-plan"),
            &foreign,
            at,
            Default::default(),
            "foreign_plan_same_document",
            &setup,
        );
        assert_eq!(
            value["output"]["error"]["message"],
            "foreign audio input owner"
        );
        cases.push(value);
        let (value, _) = measured(
            parent,
            &format!("{parent_name}/invalid-limits-before-foreign"),
            &foreign,
            at,
            AudioQueryLimits {
                maximum_spans: 0,
                ..Default::default()
            },
            "invalid_limits_precede_foreign_owner",
            &setup,
        );
        assert_eq!(value["output"]["error"]["variant"], "InvalidAudioLimits");
        cases.push(value);
        let (value, _) = measured(
            parent,
            &format!("{parent_name}/sample-overflow"),
            selected,
            AudioSample(i64::MAX),
            Default::default(),
            "checked_sample_plus_one_overflow",
            &setup,
        );
        assert_eq!(value["output"]["error"]["variant"], "Time");
        cases.push(value);
    });
    assert!(
        inactive_points > 0 && repeated_points > 0 && default_gap_points > 0 && retained_points > 0
    );
    json!({
        "schema":"deadpan.plan.old-direct-owner-input.v1",
        "scope":{
            "measured_apis":["RenderPlan::audio_owner_input","AudioDomain::owner_input"],
            "measured_calls":"exactly one OLD public direct API invocation per distinct named case; exact and one-short are separate measured calls",
            "setup":"immutable wide-matrix visitor rebuilds actual parents; each parent has a separate OLD gain_owners setup query; no strict audio_owners substitution for measured membership",
            "result_serializer":"unchanged legacy_snapshot_v1::input: complete owner, observed_at, scope, work and exhaustive physical Domain/seed",
            "walk_serializer":"unchanged capture_baseline::capture_ref observes the actual discarded walk, capture slot, selected owner, budget and work from the same direct call",
            "covered":["actual Root and actual captured physical Domain","hidden retained RootRoundEven Domain",
                "fractional PointCeil-bound fixture through its current physical parent","nonempty Repeat path and ordinary DefaultGap",
                "inactive retained-support points","actual ownerless exhausted Domain tail with earlier same-parent owner witness",
                "absent instance","sampling phase mismatch","sampling slope mismatch at same local observation",
                "hidden play mismatch","foreign Plan identity","invalid-limit ordering","checked sample arithmetic",
                "successful canonical work and exact/one-short limits"],
            "limitations":["public errors return no total-work receipt, so no inferred error canonical total is invented",
                "PointCeil retained reference has no AudioDomain.owner_input API; no direct hidden Point query is claimed",
                "fixture setup queries/captures are separate from measured API work","no allocation-policy oracle",
                "no decoder, media provider, DSP, giant Repeat expansion or new empty-support fixture"],
        },
        "fixture_source":"deadpan-plan/tests/fixtures/wide-gain-owner-baseline-v1/snapshot.json",
        "documents":wide["documents"],
        "coverage":{"inactive_points":inactive_points,"ownerless_missing_owner_errors":ownerless_points,
            "nonempty_repeat_points":repeated_points,
            "default_gap_points":default_gap_points,"retained_origin_points":retained_points},
        "cases":cases,
    })
}

#[test]
#[ignore = "parent-owned OLD direct owner-input capture with source and executable pins"]
fn capture_old_direct_owner_input_snapshot_v1() {
    let output = PathBuf::from(
        std::env::var_os("DEADPAN_DIRECT_OWNER_OUTPUT").expect("explicit new /tmp output required"),
    );
    let output = output_path(&output);
    let variant = std::env::var("DEADPAN_DIRECT_OWNER_VARIANT").expect("explicit variant required");
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source = std::env::var("DEADPAN_DIRECT_OWNER_SOURCE_SHA256")
        .expect("explicit source-manifest SHA256 required");
    assert!(source.len() == 64 && source.bytes().all(|value| value.is_ascii_hexdigit()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)
        .expect("direct oracle output must be a new file");
    let mut document = matrix();
    document["variant"] = json!(variant);
    document["runtime_source_manifest_sha256"] = json!(source);
    serde_json::to_writer_pretty(&mut file, &document).unwrap();
    file.write_all(b"\n").unwrap();
    file.sync_all().unwrap();
}

/// Reuse the complete original Input serializer without creating another shape.
pub(in crate::plan) fn input_snapshot(value: &AudioOwnerInput<'_>) -> Value {
    super::input(value)
}

/// Reuse the original typed Plan error serializer.
pub(in crate::plan) fn error_snapshot(value: &PlanError) -> Value {
    super::error_json(value)
}

/// Rebuild the same actual checked parent and base observation, then apply only
/// the isolated mutation recorded by the OLD direct-input oracle. The visitor
/// cannot retain the locally borrowed owner or foreign Plan.
pub(in crate::plan) fn visit_cases(
    snapshot: &Value,
    mut visit: impl FnMut(WideParent<'_, '_>, &AudioOwnerClock<'_>, &Value),
) {
    let wide: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/wide-gain-owner-baseline-v1/snapshot.json"
    ))
    .unwrap();
    assert_eq!(
        snapshot["documents"], wide["documents"],
        "same immutable fixture documents"
    );
    let mut visited = 0;
    wide_baseline::visit_cases(&wide, |parent, parent_case| {
        let parent_name = parent_case["name"].as_str().unwrap();
        let window = &parent_case["request"]["window"];
        let samples = AudioSample(window["start"].as_i64().unwrap())
            ..AudioSample(window["end"].as_i64().unwrap());
        let observed = setup_query(parent, samples);
        let full_setup = wide_baseline::query_snapshot(&observed, parent.plan());
        assert_eq!(full_setup, parent_case["query"]);
        for case in snapshot["cases"]
            .as_array()
            .expect("frozen direct cases")
            .iter()
            .filter(|case| case["setup"]["parent_case"] == parent_name)
        {
            let setup = &case["setup"];
            assert_eq!(
                setup["observed_query"], full_setup,
                "complete setup query: {}",
                case["name"]
            );
            assert_eq!(setup["parent_identity"], parent_case["seed_identity"]);
            assert_eq!(setup["fixture"], parent_case["fixture"]);
            let span_index =
                usize::try_from(setup["selected_span_index"].as_u64().unwrap()).unwrap();
            let member = usize::try_from(setup["selected_member_index"].as_u64().unwrap()).unwrap();
            let span = &observed.spans()[span_index];
            let selected = &span.owners()[member];
            let kind = case["case_kind"].as_str().unwrap();
            let mut candidate = selected.clone();
            match kind {
                "actual_checked_member" | "exact" | "one_short" => {}
                "valid_owner_at_ownerless_sample" => {
                    assert_eq!(setup["owner_observed_at"], json!(span.samples().start.0));
                    let target_index =
                        usize::try_from(setup["ownerless_target_span_index"].as_u64().unwrap())
                            .unwrap();
                    let target = &observed.spans()[target_index];
                    assert!(target.owners().is_empty());
                    assert_eq!(case["request"]["at"], json!(target.samples().start.0));
                }
                "absent_instance" => {
                    candidate.instance.node =
                        NodeId::new("not-a-current-direct-input-owner").unwrap();
                }
                "sampling_phase_mismatch" => {
                    candidate.sampling = crate::AudioSampleMap::new(
                        selected.sampling.anchor(),
                        selected
                            .sampling
                            .local_at_anchor()
                            .checked_add(ExactRatio::integer(1))
                            .unwrap(),
                        selected.sampling.local_frames_per_sample(),
                    )
                    .unwrap();
                }
                "sampling_slope_mismatch_same_local_at_observation" => {
                    let at = AudioSample(case["request"]["at"].as_i64().unwrap());
                    candidate.sampling = crate::AudioSampleMap::new(
                        at,
                        selected.sampling.local_at(at).unwrap(),
                        selected
                            .sampling
                            .local_frames_per_sample()
                            .checked_mul(ExactRatio::integer(2))
                            .unwrap(),
                    )
                    .unwrap();
                }
                "hidden_play_mismatch" => {
                    candidate.play =
                        Some(selected.play.map_or(0, |play| play.checked_add(1).unwrap()));
                }
                "foreign_plan_same_document" | "invalid_limits_precede_foreign_owner" => {
                    let fixture = snapshot["documents"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|fixture| fixture["fixture"] == parent_case["fixture"])
                        .unwrap();
                    let document =
                        ProjectDocument::from_json(&fixture["document"].to_string()).unwrap();
                    let foreign = RenderPlan::compile(&document).unwrap();
                    candidate.plan = &foreign;
                    assert_eq!(
                        clock(&candidate, candidate.plan, |sample| sample.0),
                        case["request"]["selected_owner"]
                    );
                    assert_eq!(
                        json!(candidate.belongs_to(parent.plan())),
                        case["request"]["selected_owner_belongs_to_request_plan"]
                    );
                    visit(parent, &candidate, case);
                    visited += 1;
                    continue;
                }
                "checked_sample_plus_one_overflow" => {}
                other => panic!("unknown frozen direct-input case kind: {other}"),
            }
            assert_eq!(
                clock(&candidate, candidate.plan, |sample| sample.0),
                case["request"]["selected_owner"],
                "exact selected owner: {}",
                case["name"]
            );
            assert_eq!(
                json!(candidate.belongs_to(parent.plan())),
                case["request"]["selected_owner_belongs_to_request_plan"]
            );
            visit(parent, &candidate, case);
            visited += 1;
        }
    });
    assert_eq!(visited, snapshot["cases"].as_array().unwrap().len());
    assert_eq!(visited, 258);
}

#[test]
fn legacy_direct_owner_input_matches_immutable_full_matrix() {
    let mut frozen: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/direct-owner-input-baseline-v1/snapshot.json"
    ))
    .unwrap();
    let metadata = frozen.as_object_mut().unwrap();
    metadata.remove("variant");
    metadata.remove("runtime_source_manifest_sha256");
    assert_eq!(matrix(), frozen);
}
