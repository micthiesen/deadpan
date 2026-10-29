//! One-shot OLD public wide gain queries, using the existing complete serializer.
//! Setup captures are labelled separately from each single measured API call.
use super::*;
use std::{
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
};

fn output_path(path: &Path) -> PathBuf {
    assert!(path.is_absolute(), "wide oracle output must be absolute");
    let parent = path
        .parent()
        .expect("output parent")
        .canonicalize()
        .unwrap();
    let scratch = Path::new("/tmp").canonicalize().unwrap();
    assert!(
        parent.starts_with(scratch),
        "wide oracle output must remain under canonical /tmp"
    );
    parent.join(path.file_name().expect("output filename"))
}

fn sample_window(start: i64, end: i64) -> Range<AudioSample> {
    AudioSample(start)..AudioSample(end)
}

fn frozen_document(fixtures: &Value, index: usize) -> ProjectDocument {
    ProjectDocument::from_json(&fixtures[index]["document"].to_string()).unwrap()
}

fn setup_input<'plan>(
    plan: &'plan RenderPlan,
    alias: &str,
    kind: AudioOwnerKind,
    at: AudioSample,
) -> AudioOwnerInput<'plan> {
    // This is a separate checked point observation for constructing an actual
    // physical Domain. It is not the measured OLD wide gain query below.
    let observed = plan
        .audio_owners(at..AudioSample(at.0 + 1), Default::default())
        .unwrap();
    let owner = observed.spans()[0]
        .owners()
        .iter()
        .find(|owner| owner.instance.node.as_str() == alias && owner.kind == kind)
        .expect("frozen fixture's physical owner");
    plan.audio_owner_input(owner, at, Default::default())
        .unwrap()
}

fn root_case(name: &str, fixture: &str, plan: &RenderPlan, window: Range<AudioSample>) -> Value {
    let limits = AudioQueryLimits::default();
    // Exactly one invocation of the measured public OLD API for this case.
    let value = plan.audio_gain_owners(window.clone(), limits).unwrap();
    json!({
        "name":name,"fixture":fixture,"api":"RenderPlan::audio_gain_owners",
        "request":{"window":sample_range(window,|v|v.0),
            "limits":{"maximum_spans":limits.maximum_spans,"maximum_work":limits.maximum_work}},
        "seed_identity":{"tag":"ImmutablePlanRoot",
            "project_id":plan.metadata.project_id.as_str(),"revision_id":plan.metadata.revision_id.as_str(),
            "root":plan.nodes[plan.root].inspection.id.as_str(),"definition":Value::Null},
        "query":query(&value,plan,|v|v.0),
    })
}

fn domain_case(
    name: &str,
    fixture: &str,
    selected: &AudioOwnerInput<'_>,
    physical: &AudioDomain<'_>,
    window: Range<AudioSample>,
    source: &str,
) -> Value {
    let limits = AudioQueryLimits::default();
    // Exactly one invocation of the measured public OLD API for this case.
    let value = physical.gain_owners(window.clone(), limits).unwrap();
    json!({
        "name":name,"fixture":fixture,"api":"AudioDomain::gain_owners",
        "request":{"window":sample_range(window,|v|v.0),
            "limits":{"maximum_spans":limits.maximum_spans,"maximum_work":limits.maximum_work}},
        "seed_identity":{"tag":"ActualCheckedPhysicalDomain","source":source,
            "setup_input":input(selected),"measured_domain":domain(physical)},
        "query":query(&value,physical.plan,|v|v.0),
    })
}

fn matrix() -> Value {
    let frozen: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/owner-query-baseline-v1/snapshot.json"
    ))
    .unwrap();
    let fixtures = &frozen["fixtures"];
    let mut cases = Vec::new();

    let doc = frozen_document(fixtures, 0);
    let plan = RenderPlan::compile(&doc).unwrap();
    let fixture = fixtures[0]["fixture"].as_str().unwrap();
    cases.push(root_case(
        "unbound_root_full",
        fixture,
        &plan,
        sample_window(0, 512),
    ));
    let captured = setup_input(&plan, "tone", AudioOwnerKind::Node, AudioSample(128));
    cases.push(domain_case(
        "unbound_actual_tone_domain_full",
        fixture,
        &captured,
        captured.context(),
        sample_window(0, 512),
        "audio_owner_input.context",
    ));

    let doc = frozen_document(fixtures, 1);
    let plan = RenderPlan::compile(&doc).unwrap();
    let fixture = fixtures[1]["fixture"].as_str().unwrap();
    let repeated = root_case(
        "repeat_root_all_five_spans",
        fixture,
        &plan,
        sample_window(0, 14),
    );
    assert_eq!(repeated["query"]["spans"].as_array().unwrap().len(), 5);
    assert!(
        repeated["query"]["spans"]
            .as_array()
            .unwrap()
            .iter()
            .any(|span| span["owners"]
                .as_array()
                .unwrap()
                .iter()
                .any(|owner| !owner["instance"]["repeats"].as_array().unwrap().is_empty()))
    );
    assert!(
        repeated["query"]["spans"]
            .as_array()
            .unwrap()
            .iter()
            .any(|span| span["owners"]
                .as_array()
                .unwrap()
                .iter()
                .any(|owner| owner["kind"] == "DefaultGap"))
    );
    cases.push(repeated);
    let captured = setup_input(&plan, "repeat", AudioOwnerKind::Node, AudioSample(0));
    cases.push(domain_case(
        "repeat_actual_whole_domain_full",
        fixture,
        &captured,
        captured.context(),
        sample_window(0, 14),
        "audio_owner_input.context",
    ));
    let gap = setup_input(&plan, "repeat", AudioOwnerKind::DefaultGap, AudioSample(8));
    cases.push(domain_case(
        "repeat_actual_default_gap_domain_full",
        fixture,
        &gap,
        gap.context(),
        sample_window(8, 10),
        "audio_owner_input.context",
    ));

    let doc = frozen_document(fixtures, 2);
    let plan = RenderPlan::compile(&doc).unwrap();
    let fixture = fixtures[2]["fixture"].as_str().unwrap();
    let exhausted = root_case(
        "hidden_resumed_root_with_exhausted_tail",
        fixture,
        &plan,
        sample_window(1, 9),
    );
    assert!(
        exhausted["query"]["spans"]
            .as_array()
            .unwrap()
            .iter()
            .any(|span| span["support"] == "Inactive")
    );
    cases.push(exhausted);
    cases.push(root_case(
        "hidden_resumed_root_clipped",
        fixture,
        &plan,
        sample_window(2, 8),
    ));
    let captured = setup_input(&plan, "group", AudioOwnerKind::Node, AudioSample(1));
    cases.push(domain_case(
        "hidden_resumed_actual_capture_domain",
        fixture,
        &captured,
        captured.context(),
        sample_window(1, 9),
        "audio_owner_input.context",
    ));
    let retained = captured
        .retained_at(AudioSample(1), Default::default())
        .unwrap()
        .unwrap();
    let AudioBoundDomain::Root(reference) = retained.domain() else {
        panic!(
            "frozen hidden fixture must expose its actual retained RootRoundEven physical Domain"
        )
    };
    cases.push(domain_case(
        "hidden_actual_retained_reference_domain",
        fixture,
        &captured,
        reference,
        sample_window(0, 8),
        "audio_owner_input.retained_at.domain.Root",
    ));

    let doc = frozen_document(fixtures, 3);
    let plan = RenderPlan::compile(&doc).unwrap();
    let fixture = fixtures[3]["fixture"].as_str().unwrap();
    cases.push(root_case(
        "fractional_point_bound_root_full",
        fixture,
        &plan,
        sample_window(0, 3),
    ));
    let captured = setup_input(&plan, "voice", AudioOwnerKind::Node, AudioSample(0));
    cases.push(domain_case(
        "fractional_actual_physical_capture_domain",
        fixture,
        &captured,
        captured.context(),
        sample_window(0, 3),
        "audio_owner_input.context",
    ));

    let doc = frozen_document(fixtures, 4);
    let plan = RenderPlan::compile(&doc).unwrap();
    let fixture = fixtures[4]["fixture"].as_str().unwrap();
    cases.push(root_case(
        "split_root_full",
        fixture,
        &plan,
        sample_window(0, 6),
    ));
    cases.push(root_case(
        "split_root_clipped",
        fixture,
        &plan,
        sample_window(2, 5),
    ));
    let captured = setup_input(&plan, "root", AudioOwnerKind::Node, AudioSample(3));
    cases.push(domain_case(
        "split_actual_physical_capture_domain",
        fixture,
        &captured,
        captured.context(),
        sample_window(0, 6),
        "audio_owner_input.context",
    ));

    assert_eq!(cases.len(), 14);
    json!({
        "schema":"deadpan.plan.old-wide-gain-query.v1",
        "scope":{
            "measured_apis":["RenderPlan::audio_gain_owners","AudioDomain::gain_owners"],
            "measured_calls":"exactly once per named case, before serialization; no canonical receipt normalization",
            "serializer":"unchanged legacy_snapshot_v1::query including private owner identity/origin/play/affine, spans/support/silence, lookup and work",
            "domain_setup":"separate checked one-sample audio_owners observation then actual audio_owner_input; setup_input and measured_domain retain exact definition, instance, placement and seed fields",
            "root_setup":"compile the exact existing frozen ProjectDocument; root identity and complete document recorded",
            "bounds":"14 finite windows; longest 512 samples; repeat has three plays and five output spans",
            "covered":["multi-span Root and actual physical Domain","nonempty Repeat paths and ordinary DefaultGap",
                "fractional source/point-bound clock","clipped windows","exhausted retained support with Inactive tail",
                "actual retained RootRoundEven Domain distinct from live Root"],
            "limitations":["no allocation-policy oracle","no new empty-support fixture",
                "PointCeil retained reference is not an AudioDomain; only its current physical capture Domain is measured",
                "no gigantic repeat expansion","no media decoding or DSP"],
        },
        "fixture_source":"deadpan-plan/tests/fixtures/owner-query-baseline-v1/snapshot.json",
        "documents":fixtures.as_array().unwrap().iter().map(|fixture|json!({
            "fixture":fixture["fixture"],"document":fixture["document"]
        })).collect::<Vec<_>>(),
        "cases":cases,
    })
}

#[test]
#[ignore = "parent-owned OLD wide gain-query capture with source and executable pins"]
fn capture_old_wide_gain_query_snapshot_v1() {
    let output = PathBuf::from(
        std::env::var_os("DEADPAN_WIDE_OWNER_OUTPUT").expect("explicit new /tmp output required"),
    );
    let output = output_path(&output);
    let variant = std::env::var("DEADPAN_WIDE_OWNER_VARIANT").expect("explicit variant required");
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source = std::env::var("DEADPAN_WIDE_OWNER_SOURCE_SHA256")
        .expect("explicit source-manifest SHA256 required");
    assert!(source.len() == 64 && source.bytes().all(|v| v.is_ascii_hexdigit()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)
        .expect("wide oracle output must be a new file");
    let mut document = matrix();
    document["variant"] = json!(variant);
    document["runtime_source_manifest_sha256"] = json!(source);
    serde_json::to_writer_pretty(&mut file, &document).unwrap();
    file.write_all(b"\n").unwrap();
    file.sync_all().unwrap();
}

/// Test-only parent reconstructed from the immutable wide-query oracle.
#[derive(Clone, Copy)]
pub(in crate::plan) enum Parent<'context, 'plan> {
    Root(&'plan RenderPlan),
    Domain(&'context AudioDomain<'plan>),
}

impl<'plan> Parent<'_, 'plan> {
    pub(in crate::plan) fn plan(self) -> &'plan RenderPlan {
        match self {
            Self::Root(plan) => plan,
            Self::Domain(domain) => domain.plan,
        }
    }
}

/// Keep the original full logical serializer unchanged and narrowly accessible.
pub(in crate::plan) fn query_snapshot(value: &AudioOwnerQuery<'_>, plan: &RenderPlan) -> Value {
    super::query(value, plan, |sample| sample.0)
}

/// Rebuild each actual checked parent from the immutable document and setup
/// identity. The visitor borrows the physical Domain only during its call.
pub(in crate::plan) fn visit_cases(
    snapshot: &Value,
    mut visit: impl FnMut(Parent<'_, '_>, &Value),
) {
    let mut visited = 0;
    for fixture in snapshot["documents"].as_array().expect("frozen documents") {
        let document = ProjectDocument::from_json(&fixture["document"].to_string()).unwrap();
        let plan = RenderPlan::compile(&document).unwrap();
        for case in snapshot["cases"]
            .as_array()
            .expect("frozen cases")
            .iter()
            .filter(|case| case["fixture"] == fixture["fixture"])
        {
            match case["api"].as_str().expect("frozen public API") {
                "RenderPlan::audio_gain_owners" => visit(Parent::Root(&plan), case),
                "AudioDomain::gain_owners" => {
                    let seed = &case["seed_identity"];
                    let setup = &seed["setup_input"];
                    let owner = &setup["owner"];
                    let alias = owner["instance"]["node"].as_str().unwrap();
                    let kind = match owner["kind"].as_str().unwrap() {
                        "Node" => AudioOwnerKind::Node,
                        "DefaultGap" => AudioOwnerKind::DefaultGap,
                        other => panic!("unknown frozen owner kind: {other}"),
                    };
                    let at = AudioSample(setup["observed_at"].as_i64().unwrap());
                    let selected = setup_input(&plan, alias, kind, at);
                    assert_eq!(
                        input(&selected),
                        *setup,
                        "actual setup identity: {}",
                        case["name"]
                    );
                    match seed["source"].as_str().expect("actual Domain source") {
                        "audio_owner_input.context" => {
                            let physical = selected.context();
                            assert_eq!(domain(physical), seed["measured_domain"]);
                            visit(Parent::Domain(physical), case);
                        }
                        "audio_owner_input.retained_at.domain.Root" => {
                            let retained = selected
                                .retained_at(at, Default::default())
                                .unwrap()
                                .unwrap();
                            let AudioBoundDomain::Root(physical) = retained.domain() else {
                                panic!("frozen actual retained RootRoundEven Domain")
                            };
                            assert_eq!(domain(physical), seed["measured_domain"]);
                            visit(Parent::Domain(physical), case);
                        }
                        other => panic!("unknown frozen physical Domain source: {other}"),
                    }
                }
                other => panic!("unknown frozen query API: {other}"),
            }
            visited += 1;
        }
    }
    assert_eq!(visited, snapshot["cases"].as_array().unwrap().len());
    assert_eq!(visited, 14);
}

#[test]
fn legacy_wide_gain_queries_match_immutable_full_matrix() {
    let mut frozen: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/wide-gain-owner-baseline-v1/snapshot.json"
    ))
    .unwrap();
    let metadata = frozen.as_object_mut().unwrap();
    metadata.remove("variant");
    metadata.remove("runtime_source_manifest_sha256");
    // The original producer matrix and every canonical receipt stay literal.
    assert_eq!(matrix(), frozen);
}

#[test]
fn wide_case_visitor_reconstructs_actual_parents_and_full_old_queries() {
    let frozen: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/wide-gain-owner-baseline-v1/snapshot.json"
    ))
    .unwrap();
    visit_cases(&frozen, |parent, case| {
        let request = &case["request"];
        let window = sample_window(
            request["window"]["start"].as_i64().unwrap(),
            request["window"]["end"].as_i64().unwrap(),
        );
        let limits = AudioQueryLimits {
            maximum_spans: usize::try_from(request["limits"]["maximum_spans"].as_u64().unwrap())
                .unwrap(),
            maximum_work: usize::try_from(request["limits"]["maximum_work"].as_u64().unwrap())
                .unwrap(),
        };
        let actual = match parent {
            Parent::Root(plan) => plan.audio_gain_owners(window, limits),
            Parent::Domain(domain) => domain.gain_owners(window, limits),
        }
        .unwrap();
        assert_eq!(
            query_snapshot(&actual, parent.plan()),
            case["query"],
            "{}",
            case["name"]
        );
    });
}
