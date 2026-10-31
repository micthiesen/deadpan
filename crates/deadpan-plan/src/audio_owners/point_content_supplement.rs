//! Additive OLD witness from checked public physical Point constructions.
use super::*;
use deadpan_core::*;
use std::collections::BTreeMap;

const INITIAL_SHA256: &str = "b7d93aa5673d393b5b74e9e8eb152d510f66f60ab10e70d8e7b0c3cba8b8214e";

fn error_json(value: &PlanError) -> Value {
    match value {
        PlanError::InvalidAudioRootPlacement(message) => {
            json!({"variant":"InvalidAudioRootPlacement","message":message})
        }
        other => snapshot::error_json(other),
    }
}

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn ratio(numerator: i128, denominator: i128) -> ExactRatio {
    ExactRatio::new(numerator, denominator).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn source_audio(start: i64, end: i64) -> SourceAudio {
    let clock = SourceTimeBase::new(1, 48_000).unwrap();
    SourceAudio {
        asset: AssetId::new("point-supplement-media").unwrap(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: start,
                time_base: clock,
            },
            SourceTimestamp {
                ticks: end,
                time_base: clock,
            },
        )
        .unwrap(),
    }
}
fn source(duration: i64, audio: SourceAudio, mapping: SourceAudioMapping) -> BeatNode {
    BeatNode {
        framing: None,
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        label: "Supplement Source".into(),
        kind: NodeKind::Source {
            source: SourceNode {
                edit_window: None,
                duration: frames(duration),
                video: SourceVideo::Blank,
                video_mapping: SourceVideoMapping::FitBeat,
                audio: Some(audio),
                audio_mapping: mapping,
                audio_offset: AudioSample(0),
                link: LinkRelation::Independent,
            },
        },
        cutaways: Vec::new(),
        captions: Vec::new(),
    }
}
fn document(
    name: &str,
    children: Vec<NodeId>,
    mut nodes: BTreeMap<NodeId, BeatNode>,
) -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new(format!("point-supplement-{name}")).unwrap(),
        RevisionId::new("point-supplement-setup").unwrap(),
        PresentationBasis {
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    nodes.insert(id("root"), BeatNode::sequence("Root", children));
    let assets = BTreeMap::from([(
        AssetId::new("point-supplement-media").unwrap(),
        AssetRecord {
            label: "Pure Plan fixture audio".into(),
            content_hash: "a".repeat(64),
            video: None,
            audio: Some(source_audio(-1_000_000, 1_000_000).span),
            still_image: true,
            frame_count: None,
            source_qualification: None,
        },
    )]);
    let mut wire = serde_json::to_value(&empty).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["assets"] = serde_json::to_value(assets).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}
struct Case {
    name: &'static str,
    document: ProjectDocument,
    node: &'static str,
    origin: ExactRatio,
    scale: ExactRatio,
    grid_origin: ExactRatio,
    source: Value,
}
fn cases() -> Vec<Case> {
    let placed = document(
        "placed-source",
        vec![id("source")],
        BTreeMap::from([(
            id("source"),
            source(
                12,
                source_audio(0, 6406),
                SourceAudioMapping::Placement {
                    start: ExactRatio::integer(2),
                    frames: ExactRatio::integer(4),
                },
            ),
        )]),
    );
    let stage = BeatNode {
        framing: None,
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        label: "Genuine Preserve".into(),
        kind: NodeKind::Retime {
            purpose: RetimePurpose::Edit,
            child: id("source"),
            duration: frames(6),
            mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(12)).unwrap(),
            pitch: PitchPolicy::Preserve,
        },
        cutaways: Vec::new(),
        captions: Vec::new(),
    };
    let preserved = document(
        "preserve",
        vec![id("stage")],
        BTreeMap::from([
            (
                id("source"),
                source(12, source_audio(0, 19219), SourceAudioMapping::FitBeat),
            ),
            (id("stage"), stage),
        ]),
    );
    let old: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/wide-gain-owner-baseline-v1/snapshot.json"
    ))
    .unwrap();
    let captured = old["documents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["fixture"] == "nonunity_shift_fractional_point_ceil_source44100")
        .unwrap();
    let bound_document = ProjectDocument::from_json(&captured["document"].to_string()).unwrap();
    let sequence_nodes: BTreeMap<_, _> = (0..9)
        .map(|index| {
            let name = id(&format!("source-{index}"));
            (
                name,
                source(
                    4,
                    source_audio(index * 6406, (index + 1) * 6406),
                    SourceAudioMapping::FitBeat,
                ),
            )
        })
        .collect();
    let sequence_children = sequence_nodes.keys().cloned().collect();
    let sequence = document("sequence-rejection", sequence_children, sequence_nodes);
    vec![
        Case {
            name: "placed-source-multiple-leaves",
            document: placed,
            node: "source",
            origin: ratio(-1, 3),
            scale: ratio(7, 5),
            grid_origin: ratio(1, 7),
            source: json!({"kind":"checked_public_core_document",
                "construction":"SourceNode with explicit Placement between real silent prefix/suffix",
                "intent":"genuine multiple Leaf spans and admitted accumulator replacements",
                "reference":"tests/audio_signal.rs::point_grid_keeps_every_integer_position_in_fractional_source_support"}),
        },
        Case {
            name: "preserve-stage",
            document: preserved,
            node: "stage",
            origin: ratio(-2, 5),
            scale: ratio(3, 2),
            grid_origin: ratio(1, 11),
            source: json!({"kind":"checked_public_core_document",
                "construction":"nonunity Preserve Retime selected by public audio_definition/in_point_clock",
                "intent":"genuine Stage content with complete borrowed Stage snapshot",
                "reference":"tests/audio_signal.rs::point_grid_keeps_every_integer_position_in_fractional_source_support"}),
        },
        Case {
            name: "captured-source-bound",
            document: bound_document,
            node: "voice",
            origin: ExactRatio::ZERO,
            scale: ExactRatio::ONE,
            grid_origin: ExactRatio::ZERO,
            source: json!({"kind":"immutable_checked_document",
                "path":"tests/fixtures/wide-gain-owner-baseline-v1/snapshot.json",
                "fixture":captured["fixture"],
                "construction":"existing captured Source binding left exact; public Point has no raw-constructor bypass",
                "intent":"genuine Bound content if current public definition binding resolves"}),
        },
        Case {
            name: "public-sequence-point-rejection",
            document: sequence,
            node: "root",
            origin: ExactRatio::ZERO,
            scale: ExactRatio::ONE,
            grid_origin: ExactRatio::ZERO,
            source: json!({"kind":"checked_public_core_document",
                "construction":"nine-child Sequence passed to existing public in_point_clock",
                "intent":"record actual public composite-placement rejection without manufacturing a Point",
                "restriction_source":"audio_definition.rs::validate_physical_placement"}),
        },
    ]
}
fn helper_observation(
    plan: &RenderPlan,
    point: &crate::AudioPointDomain<'_>,
    request: &Request,
) -> Value {
    let input_before = point.probe_baseline_snapshot(plan);
    super::admission_tests::observe_request(plan, point, request, None);
    let mut policy = recording();
    let policy_before = state(&policy.policy.0);
    let ordinary = point
        .signal_ref()
        .query(request.samples.clone(), request.limits);
    let ordinary_body = query_result(plan, &ordinary);
    let input_while_ordinary_live = point.probe_baseline_snapshot(plan);
    let ordinary_live = state(&policy.policy.0);
    assert_eq!(ordinary_live, policy_before);
    assert_eq!(input_before, input_while_ordinary_live);
    drop(ordinary);
    let ordinary_after_drop = state(&policy.policy.0);
    assert_eq!(ordinary_after_drop, policy_before);

    let mut local = Budget {
        remaining: request.limits.maximum_work,
        lookup: Default::default(),
    };
    let local_before = json!({"remaining":local.remaining,"lookup":local.lookup});
    let (result, canonical, additional, remaining, lookup) = {
        let mut budget = QueryBudget::new(&mut local, &mut policy);
        let result = admission::outer::<_, Recording>(point.signal_ref().structural_child_query(
            request.samples.clone(),
            request.limits,
            AudioBoundaryRule::PointCeil,
            (true, true),
            &mut budget,
        ));
        (
            result,
            budget.canonical_used,
            budget.additional_used,
            budget.remaining,
            budget.lookup,
        )
    };
    let helper_body = match &result {
        Ok(value) => json!({"result":"Ok","query":query_snapshot(plan,&value.query)}),
        Err(error) => json!({"result":"Error","error":error_json(error)}),
    };
    let input_while_helper_live = point.probe_baseline_snapshot(plan);
    let helper_live = state(&policy.policy.0);
    assert_eq!(input_before, input_while_helper_live);
    drop(result);
    let after_drop = state(&policy.policy.0);
    assert_eq!(after_drop["live"], 0);
    let input_after = point.probe_baseline_snapshot(plan);
    assert_eq!(input_before, input_after);
    let mut value = json!({"request":request.snapshot(),"input_before":input_before,
        "input_after":input_after,
        "ordinary":{"result":ordinary_body,"input_while_result_live":input_while_ordinary_live,
            "query_policy_before":policy_before,"query_policy_while_result_live":ordinary_live,
            "query_policy_after_result_drop":ordinary_after_drop}});
    value["existing_internal_metered_helper_isolated_query_policy"] = json!({
        "result":helper_body,"canonical":canonical,"additional":additional,
        "local_before":local_before,"local_after":{"remaining":remaining,"lookup":lookup},
        "events":policy.events,"ledger_while_result_live":helper_live,
        "ledger_after_result_drop":after_drop,
        "accepted_work":policy.policy.0.borrow().forwarded,
        "input_while_result_live":input_while_helper_live});
    value
}
fn coverage(value: &Result<AudioSignalQuery<'_>, PlanError>) -> Value {
    match value {
        Ok(query) => {
            let mut counts = BTreeMap::<&str, usize>::new();
            for span in &query.spans {
                let kind = match &span.content {
                    AudioSignalContent::Leaf(_) => "Leaf",
                    AudioSignalContent::Stage(_) => "Stage",
                    AudioSignalContent::Bound(_) => "Bound",
                    AudioSignalContent::ProjectedStage(_) => "ProjectedStage",
                };
                *counts.entry(kind).or_default() += 1;
            }
            json!({"status":"Ok","span_count":query.spans.len(),
                "span_capacity":query.spans.capacity(),"content_counts":counts,
                "literal_work":query.work})
        }
        Err(error) => json!({"status":"Error","error":error_json(error)}),
    }
}
fn capture_case(case: Case) -> Value {
    let before_document = serde_json::to_value(&case.document).unwrap();
    let mut value = json!({"name":case.name,"source":case.source,
        "document_before":before_document,"selected_node":case.node,
        "input_admission":{"kind":"ordinary_public_Point_construction",
            "input_storage":"unadmitted and outside every query-only recording policy",
            "media_admission":"not measured; pure checked Plan documents and stored recipes only"}});
    let plan = match RenderPlan::compile(&case.document) {
        Ok(plan) => plan,
        Err(error) => {
            value["setup"] = json!({"status":"CompileError","error":error_json(&error)});
            value["document_after"] = serde_json::to_value(&case.document).unwrap();
            assert_eq!(value["document_before"], value["document_after"]);
            return value;
        }
    };
    let definition = match plan.audio_definition(crate::AudioDefinitionSelector::Node {
        node: id(case.node),
    }) {
        Ok(definition) => definition,
        Err(error) => {
            value["setup"] = json!({"status":"DefinitionError","error":error_json(&error)});
            value["document_after"] = serde_json::to_value(&case.document).unwrap();
            assert_eq!(value["document_before"], value["document_after"]);
            return value;
        }
    };
    let placement = crate::AudioRootPlacement::new(
        case.origin,
        case.scale,
        ExactRatio::ZERO..definition.exact_duration(),
    )
    .unwrap();
    value["point_request"] = json!({"selector":definition.selector(),
        "root":definition.root(),"duration":definition.duration(),
        "exact_duration":definition.exact_duration(),
        "placement":placement,"grid_origin":case.grid_origin});
    let point = match definition.in_point_clock(placement, case.grid_origin) {
        Ok(point) => point,
        Err(error) => {
            value["setup"] = json!({"status":"PointConstructionError","error":error_json(&error)});
            value["document_after"] = serde_json::to_value(&case.document).unwrap();
            assert_eq!(value["document_before"], value["document_after"]);
            return value;
        }
    };
    let input_before = point.probe_baseline_snapshot(&plan);
    let support = point.reference_samples();
    let samples = point.signal_at_reference(support.start).unwrap()
        ..point.signal_at_reference(support.end).unwrap();
    let full = point
        .signal_ref()
        .query(samples.clone(), Default::default());
    let pending = requests(&point, &full);
    let full_snapshot = query_result(&plan, &full);
    let actual_coverage = coverage(&full);
    let input_while_geometry_live = point.probe_baseline_snapshot(&plan);
    assert_eq!(input_before, input_while_geometry_live);
    drop(full);
    assert_eq!(point.probe_baseline_snapshot(&plan), input_before);
    value["setup"] = json!({"status":"PointConstructed","input_before":input_before,
        "reference_support":support,"signal_support":samples,
        "full_ordinary_result":full_snapshot,"actual_full_query_coverage":actual_coverage,
        "input_while_geometry_query_live":input_while_geometry_live});
    value["requests"] = Value::Array(
        pending
            .iter()
            .map(|request| helper_observation(&plan, &point, request))
            .collect(),
    );
    value["input_after_all_queries"] = point.probe_baseline_snapshot(&plan);
    assert_eq!(
        value["setup"]["input_before"],
        value["input_after_all_queries"]
    );
    value["document_after"] = serde_json::to_value(&case.document).unwrap();
    assert_eq!(value["document_before"], value["document_after"]);
    value
}
pub(super) fn matrix_supplement() -> Value {
    let values = cases().into_iter().map(capture_case).collect::<Vec<_>>();
    let request_count: usize = values
        .iter()
        .map(|v| v["requests"].as_array().map_or(0, Vec::len))
        .sum();
    json!({"schema":"old-point-content-query-public-supplement-v1",
        "prior_point_content_witness":{"path":"tests/fixtures/point-content-query-baseline-v1/snapshot.json",
            "sha256":INITIAL_SHA256},
        "cases":values,"actual_request_count":request_count,
        "scope":{"ordinary":"fresh public physical Point construction and query; input metadata remains unadmitted",
            "helper":"fresh isolated query-only policy per request; ordinary result drops before helper; input stays alive",
            "peak":"actual cumulative query-only requested Layout peak, excludes input/harness/ordinary allocations, never reset",
            "composites":"existing public placement restrictions preserved; explicit rejected Sequence attempt is not a fabricated Point",
            "growth":"three returned spans may exercise admitted1-to2-to4 accumulator replacement; actual events decide coverage, no >4 ordinary growth claim"}})
}
#[test]
#[ignore = "manual additive OLD Point content supplement; fresh /tmp output and current published source required"]
fn capture_old_point_content_public_supplement_v1() {
    let output = std::env::var("DEADPAN_POINT_SUPPLEMENT_OUTPUT").expect("explicit fresh output");
    let path = Path::new(&output);
    assert!(path.is_absolute());
    assert!(
        path.parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .starts_with(Path::new("/tmp").canonicalize().unwrap())
    );
    let variant = std::env::var("DEADPAN_POINT_SUPPLEMENT_VARIANT").expect("actual source variant");
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source = std::env::var("DEADPAN_POINT_SUPPLEMENT_SOURCE_SHA256")
        .expect("full source manifest digest");
    assert!(source.len() == 64 && source.bytes().all(|b| b.is_ascii_hexdigit()));
    let checkpoint = std::env::var("DEADPAN_POINT_SUPPLEMENT_PUBLISHED_CHECKPOINT")
        .expect("published Point borrow checkpoint");
    assert_eq!(checkpoint, CHECKPOINT);
    let mut body = matrix_supplement();
    body["variant"] = json!(variant);
    body["source_sha256"] = json!(source);
    body["published_point_borrow_checkpoint"] = json!(checkpoint);
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
#[test]
#[ignore = "external full additive OLD Point supplement regression; independent immutable body required"]
fn point_content_public_supplement_matches_explicit_external_old_v1() {
    let path = std::env::var("DEADPAN_POINT_SUPPLEMENT_EXPECTED")
        .expect("independent immutable supplemental body");
    let expected: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(matrix_supplement(), strip_envelope(expected));
}

#[test]
fn ordinary_point_content_matches_frozen_public_supplement_v1() {
    let expected: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/point-content-query-public-supplement-v1/snapshot.json"
    ))
    .unwrap();
    assert_eq!(
        include_str!(
            "../../tests/fixtures/point-content-query-public-supplement-v1/snapshot.sha256"
        )
        .trim(),
        "9fdedc89939dc218bf8e5f01e020f312e81d7025328019c2c70cc00760b6a8ae"
    );
    assert_eq!(matrix_supplement(), expected);
}
