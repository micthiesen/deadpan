//! Ordinary actual Root/Domain processing queries before constructor admission.
//! Observe returned values only; ordinary failures expose no partial-work receipt.
use super::*;
use crate::AudioProcessingQuery;
use std::{fs::OpenOptions, io::Write, path::Path};
use wide_baseline::Parent;

fn lookup(value: LookupStats) -> Value {
    let LookupStats {
        visited_nodes,
        sequence_comparisons,
        iteration_run_comparisons,
    } = value;
    json!({
        "visited_nodes":visited_nodes,"sequence_comparisons":sequence_comparisons,
        "iteration_run_comparisons":iteration_run_comparisons,
    })
}

/// Keep full private Stage/Bound history alongside headers and canonical work.
pub(in crate::plan) fn query_snapshot(
    plan: &RenderPlan,
    value: &AudioProcessingQuery<'_>,
) -> Value {
    let AudioProcessingQuery {
        project_id,
        revision_id,
        samples,
        spans,
        lookup: stats,
        work,
    } = value;
    json!({
        "project_id":project_id.as_str(),"revision_id":revision_id.as_str(),
        "samples":samples,"lookup":lookup(*stats),"work":work,
        "spans":spans.iter().map(|span|capture_baseline::processing(plan,span)).collect::<Vec<_>>(),
    })
}

fn result_snapshot(
    plan: &RenderPlan,
    value: &Result<AudioProcessingQuery<'_>, PlanError>,
) -> Value {
    match value {
        Ok(query) => json!({"result":"Ok","query":query_snapshot(plan,query)}),
        Err(error) => json!({
            "result":"Error","error":error_json(error),
            "partial_work":"not exposed by the ordinary constructor error",
        }),
    }
}

fn query<'plan>(
    parent: Parent<'_, 'plan>,
    samples: Range<AudioSample>,
    limits: AudioQueryLimits,
) -> Result<AudioProcessingQuery<'plan>, PlanError> {
    match parent {
        Parent::Root(plan) => plan.audio_processing(samples, limits),
        Parent::Domain(physical) => physical.processing(samples, limits),
    }
}

fn support(parent: Parent<'_, '_>) -> Range<AudioSample> {
    match parent {
        Parent::Root(plan) => AudioSample(0)..plan.audio_duration().unwrap(),
        Parent::Domain(physical) => physical.root_samples(),
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

fn descriptor(name: &str, parent: Parent<'_, '_>, source: Value) -> Value {
    json!({
        "name":name,
        "parent_kind":match parent {Parent::Root(_)=>"Root",Parent::Domain(_)=>"Domain"},
        "window":sample_range(support(parent),|sample|sample.0),
        "parent_identity":parent_identity(parent),"fixture_source":source,
    })
}

fn checked_leaf_document(node: BeatNode, source: &SourceAudio) -> ProjectDocument {
    let base = document(48_000, &["voice"], vec![("voice", hold(8))]);
    let mut wire = serde_json::to_value(base).unwrap();
    wire["nodes"]["voice"] = serde_json::to_value(node).unwrap();
    wire["assets"] = serde_json::to_value(BTreeMap::from([(
        source.asset.clone(),
        AssetRecord {
            label: "Processing baseline planning metadata".into(),
            content_hash: "a".repeat(64),
            video: None,
            audio: Some(source.span),
            still_image: true,
            frame_count: None,
            source_qualification: None,
        },
    )]))
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn visit_leaf_roots(mut visit: impl FnMut(Parent<'_, '_>, &Value)) {
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    let source = SourceAudio {
        asset: AssetId::new("processing-baseline-owning-media").unwrap(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: 0,
                time_base,
            },
            SourceTimestamp {
                ticks: 16,
                time_base,
            },
        )
        .unwrap(),
    };
    for name in [
        "source_fit_beat",
        "source_absent_audio",
        "source_selected_placement",
        "hold_silence",
        "hold_tone",
        "hold_room_tone",
        "hold_reverse",
        "hold_tail",
    ] {
        let mut node = hold(8);
        match name {
            "source_fit_beat" | "source_absent_audio" | "source_selected_placement" => {
                node.kind = NodeKind::Source {
                    source: SourceNode {
                        edit_window: None,
                        duration: frames(8),
                        video: if name == "source_absent_audio" {
                            SourceVideo::Still {
                                asset: source.asset.clone(),
                            }
                        } else {
                            SourceVideo::Blank
                        },
                        video_mapping: SourceVideoMapping::FitBeat,
                        audio: (name != "source_absent_audio").then(|| source.clone()),
                        audio_mapping: if name == "source_selected_placement" {
                            SourceAudioMapping::SelectedPlacement {
                                start: ExactRatio::ZERO,
                                frames: ratio(8, 1),
                                selection: ExactFrameRange {
                                    start: ratio(2, 1),
                                    end: ratio(6, 1),
                                },
                            }
                        } else {
                            SourceAudioMapping::FitBeat
                        },
                        audio_offset: AudioSample(if name == "source_selected_placement" {
                            -1
                        } else {
                            0
                        }),
                        link: LinkRelation::Independent,
                    },
                };
            }
            _ => {
                let NodeKind::Hold { recipe } = &mut node.kind else {
                    unreachable!()
                };
                recipe.audio = match name {
                    "hold_silence" => HoldAudio::Silence,
                    "hold_tone" => HoldAudio::Tone {
                        frequency_hz: 440,
                        level: GainDb::new(-6000).unwrap(),
                    },
                    "hold_room_tone" => HoldAudio::RoomTone {
                        source: source.clone(),
                    },
                    "hold_reverse" => HoldAudio::Reverse {
                        source: source.clone(),
                    },
                    "hold_tail" => HoldAudio::Tail {
                        maximum: frames(4),
                        effect: Default::default(),
                        input: Default::default(),
                    },
                    _ => unreachable!(),
                };
            }
        }
        let document = checked_leaf_document(node, &source);
        let plan = RenderPlan::compile(&document).unwrap();
        let parent = Parent::Root(&plan);
        let case = descriptor(
            &format!("leaf/{name}/root"),
            parent,
            json!({
                "kind":"checked_test_document","document":document,
                "path":"src/audio_owners/legacy_snapshot_v1/processing_query_baseline.rs",
                "symbol":"visit_leaf_roots","case":name,
            }),
        );
        visit(parent, &case);
    }
}

/// Setup constructs checked parents; every measured invocation is separate.
pub(in crate::plan) fn visit_cases(mut visit: impl FnMut(Parent<'_, '_>, &Value)) {
    let wide: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/wide-gain-owner-baseline-v1/snapshot.json"
    ))
    .unwrap();
    let mut roots = 0;
    let mut domains = 0;
    for fixture in wide["documents"].as_array().unwrap() {
        let document = ProjectDocument::from_json(&fixture["document"].to_string()).unwrap();
        let plan = RenderPlan::compile(&document).unwrap();
        let parent = Parent::Root(&plan);
        let case = descriptor(
            &format!("wide/{}/root", fixture["fixture"].as_str().unwrap()),
            parent,
            json!({
                "kind":"immutable_wide_document",
                "path":"tests/fixtures/wide-gain-owner-baseline-v1/snapshot.json",
                "fixture":fixture["fixture"],"document":fixture["document"],
            }),
        );
        visit(parent, &case);
        roots += 1;
    }
    wide_baseline::visit_cases(&wide, |parent, original| {
        let Parent::Domain(_) = parent else { return };
        let fixture = wide["documents"]
            .as_array()
            .unwrap()
            .iter()
            .find(|fixture| fixture["fixture"] == original["fixture"])
            .unwrap();
        let case = descriptor(
            &format!("wide/{}/domain", original["name"].as_str().unwrap()),
            parent,
            json!({
                "kind":"immutable_wide_checked_visitor",
                "path":"tests/fixtures/wide-gain-owner-baseline-v1/snapshot.json",
                "case":original["name"],"fixture":original["fixture"],
                "document":fixture["document"],"checked_setup_identity":original["seed_identity"],
            }),
        );
        visit(parent, &case);
        domains += 1;
    });
    policy_binding_baseline::visit_additional(|name, document, input, physical| {
        let root = Parent::Root(physical.plan);
        let source = json!({
            "kind":"checked_additional_owner_input",
            "path":"src/audio_owners/legacy_snapshot_v1/policy_binding_baseline.rs",
            "symbol":"visit_additional","case":name,"document":document,
            "setup_input":super::input(input),
        });
        let case = descriptor(&format!("additional/{name}/root"), root, source.clone());
        visit(root, &case);
        roots += 1;
        let parent = Parent::Domain(physical);
        let case = descriptor(&format!("additional/{name}/domain"), parent, source);
        visit(parent, &case);
        domains += 1;
    });
    let plan = crate::audio_allocated::query_storage_tests::fixture();
    let parent = Parent::Root(&plan);
    let case = descriptor(
        "storage/source_repeat_nested_preserve/root",
        parent,
        json!({
            "kind":"pinned_test_constructor",
            "path":"src/audio_allocated/query_storage_tests.rs","symbol":"fixture",
        }),
    );
    visit(parent, &case);
    roots += 1;
    visit_leaf_roots(|parent, descriptor| {
        visit(parent, descriptor);
        roots += 1;
    });
    assert_eq!(roots, 20);
    assert_eq!(domains, 13);
}

fn limits_snapshot(limits: AudioQueryLimits) -> Value {
    json!({"maximum_spans":limits.maximum_spans,"maximum_work":limits.maximum_work})
}

fn observation(
    name: &str,
    parent: Parent<'_, '_>,
    samples: Range<AudioSample>,
    limits: AudioQueryLimits,
) -> Value {
    let result = query(parent, samples.clone(), limits);
    match name {
        "default_clipped" | "default_empty" => {
            let query = result.as_ref().unwrap();
            assert_eq!(query.samples, samples);
            if samples.start == samples.end {
                assert!(query.spans.is_empty());
                assert_eq!(query.work, 0);
            }
        }
        "invalid_zero_work"
        | "invalid_zero_spans"
        | "invalid_work_before_outside_range"
        | "invalid_spans_before_reversed_range" => {
            assert!(matches!(&result, Err(PlanError::InvalidAudioLimits)));
        }
        "outside_support_before" | "outside_support_after" | "reversed_range" => {
            assert!(matches!(&result, Err(PlanError::AudioRangeOutOfRange)));
        }
        "one_short_canonical_work" | "insufficient_positive_span_limit" => {
            assert!(
                result.is_err(),
                "{name} must not produce a complete ordinary query"
            );
        }
        _ => panic!("unclassified processing query observation: {name}"),
    }
    json!({
        "name":name,"request":{"samples":samples,"limits":limits_snapshot(limits)},
        "output":result_snapshot(parent.plan(),&result),
    })
}

fn parent_case(parent: Parent<'_, '_>, descriptor: &Value) -> Value {
    let samples = window(&descriptor["window"]);
    assert!(samples.end.0 - samples.start.0 >= 2);
    let defaults = AudioQueryLimits::default();
    let full = query(parent, samples.clone(), defaults).unwrap();
    assert!(!full.spans.is_empty());
    let mut observations = vec![json!({
        "name":"default_full","request":{"samples":samples,"limits":limits_snapshot(defaults)},
        "output":{"result":"Ok","query":query_snapshot(parent.plan(),&full)},
    })];
    let clipped_end = if samples.end.0 - samples.start.0 > 2 {
        samples.end.0 - 1
    } else {
        samples.end.0
    };
    observations.push(observation(
        "default_clipped",
        parent,
        AudioSample(samples.start.0 + 1)..AudioSample(clipped_end),
        defaults,
    ));
    observations.push(observation(
        "default_empty",
        parent,
        samples.start..samples.start,
        defaults,
    ));
    if full.work > 0 {
        let exact = AudioQueryLimits {
            maximum_work: full.work,
            ..defaults
        };
        let result = query(parent, samples.clone(), exact);
        assert_eq!(
            query_snapshot(parent.plan(), result.as_ref().unwrap()),
            query_snapshot(parent.plan(), &full)
        );
        observations.push(json!({
            "name":"exact_canonical_work","request":{"samples":samples,"limits":limits_snapshot(exact)},
            "output":result_snapshot(parent.plan(),&result),
        }));
        observations.push(observation(
            "one_short_canonical_work",
            parent,
            samples.clone(),
            AudioQueryLimits {
                maximum_work: full.work - 1,
                ..defaults
            },
        ));
    }
    let exact_spans = AudioQueryLimits {
        maximum_spans: full.spans.len(),
        ..defaults
    };
    let result = query(parent, samples.clone(), exact_spans);
    assert_eq!(
        query_snapshot(parent.plan(), result.as_ref().unwrap()),
        query_snapshot(parent.plan(), &full)
    );
    observations.push(json!({
        "name":"exact_positive_span_limit","request":{"samples":samples,"limits":limits_snapshot(exact_spans)},
        "output":result_snapshot(parent.plan(),&result),
    }));
    if full.spans.len() > 1 {
        observations.push(observation(
            "insufficient_positive_span_limit",
            parent,
            samples.clone(),
            AudioQueryLimits {
                maximum_spans: full.spans.len() - 1,
                ..defaults
            },
        ));
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
    let before = AudioSample(samples.start.0.checked_sub(1).unwrap())..samples.start;
    let after = samples.end..AudioSample(samples.end.0.checked_add(1).unwrap());
    let reversed = AudioSample(samples.start.0 + 1)..samples.start;
    observations.push(observation(
        "outside_support_before",
        parent,
        before.clone(),
        defaults,
    ));
    observations.push(observation(
        "outside_support_after",
        parent,
        after,
        defaults,
    ));
    observations.push(observation(
        "reversed_range",
        parent,
        reversed.clone(),
        defaults,
    ));
    observations.push(observation(
        "invalid_work_before_outside_range",
        parent,
        before,
        AudioQueryLimits {
            maximum_work: 0,
            ..defaults
        },
    ));
    observations.push(observation(
        "invalid_spans_before_reversed_range",
        parent,
        reversed,
        AudioQueryLimits {
            maximum_spans: 0,
            ..defaults
        },
    ));
    let mut value = descriptor.clone();
    value["observations"] = json!(observations);
    value
}

pub(in crate::plan) fn matrix() -> Value {
    let mut cases = Vec::new();
    visit_cases(|parent, descriptor| cases.push(parent_case(parent, descriptor)));
    assert_eq!(cases.len(), 33);
    let count = |kind: &str| {
        cases
            .iter()
            .filter(|case| case["parent_kind"] == kind)
            .count()
    };
    assert_eq!(count("Root"), 20);
    assert_eq!(count("Domain"), 13);
    let observations = cases
        .iter()
        .map(|case| case["observations"].as_array().unwrap().len())
        .sum::<usize>();
    assert!(cases.iter().any(|case| {
        case["observations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["name"] == "insufficient_positive_span_limit")
    }));
    json!({
        "schema":"old-processing-query-baseline-v1",
        "scope":"Complete ordinary RenderPlan::audio_processing and AudioDomain::processing results; checked setup is separate; no constructor memory-peak claim",
        "producer":"ordinary-root-domain-before-processing-constructor-admission",
        "error_work":"Ordinary failed constructors expose no partial-work receipt",
        "excluded_factories":["arbitrary AudioSignal/source-voice processing","ProjectedStage factory"],
        "coverage":{"parents":cases.len(),"root_parents":count("Root"),"domain_parents":count("Domain"),"observations":observations},
        "cases":cases,
    })
}

#[test]
fn ordinary_processing_queries_match_frozen_pre_admission_body() {
    let expected: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/processing-query-baseline-v1/snapshot.json"
    ))
    .unwrap();
    assert_eq!(matrix(), expected);
}

#[test]
#[ignore = "manual immutable OLD processing-query producer; requires a new /tmp output"]
fn capture_old_processing_query_baseline_v1() {
    let output = std::env::var("DEADPAN_PROCESSING_QUERY_OUTPUT")
        .expect("new explicit processing-query output required");
    let output = Path::new(&output);
    assert!(output.is_absolute());
    assert!(
        output
            .parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .starts_with(Path::new("/tmp").canonicalize().unwrap()),
        "processing-query output must remain under canonical /tmp"
    );
    let variant = std::env::var("DEADPAN_PROCESSING_QUERY_VARIANT")
        .expect("explicit main-inherited/scoped-published variant required");
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source = std::env::var("DEADPAN_PROCESSING_QUERY_SOURCE_SHA256")
        .expect("pinned OLD source-manifest SHA256 required");
    assert!(source.len() == 64 && source.bytes().all(|value| value.is_ascii_hexdigit()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .expect("processing-query output must be a new file");
    let mut value = matrix();
    value["variant"] = json!(variant);
    value["source_sha256"] = json!(source);
    serde_json::to_writer_pretty(&mut file, &value).unwrap();
    file.write_all(b"\n").unwrap();
    file.sync_all().unwrap();
}
