//! Public ordinary Signal entry evidence; Point/Tape factories are not substituted.
use super::*;

const SIGNAL_CHECKPOINT: &str = "ee9dac9efe35803c9350fd95b72b2553f33b69c5";
const SIGNAL_SCHEMA: &str = "ordinary-public-signal-policy-before-v1";

fn public_policy(
    signal: &AudioSignal<'_>,
    samples: Range<SignalSample>,
    limits: AudioQueryLimits,
    after_preserve: bool,
) -> Result<crate::AudioPolicyQuery<SignalSample>, PlanError> {
    if after_preserve {
        signal.policy_after_preserve(samples, limits)
    } else {
        signal.policy(samples, limits)
    }
}
fn point_geometry(signal: &AudioSignal<'_>) -> Result<Value, PlanError> {
    let transform = signal.transform()?;
    let grid = transform.grid(AudioBoundaryRule::PointCeil)?;
    let support = signal.support();
    Ok(json!({"transform":transform,"grid":grid,"support":support,
        "full_samples":grid.boundary(support.start)?..grid.boundary(support.end)?}))
}
fn sample_range(value: &Value) -> Range<SignalSample> {
    SignalSample(value["start"].as_i64().unwrap())..SignalSample(value["end"].as_i64().unwrap())
}
fn push_if_new(
    rows: &mut Vec<(String, Range<SignalSample>, AudioQueryLimits)>,
    name: &str,
    samples: Range<SignalSample>,
    limits: AudioQueryLimits,
) {
    if !rows.iter().any(|(_, r, l)| {
        *r == samples
            && l.maximum_spans == limits.maximum_spans
            && l.maximum_work == limits.maximum_work
    }) {
        rows.push((name.into(), samples, limits));
    }
}
fn observe(plan: &RenderPlan, name: &str, signal: &AudioSignal<'_>, origin: &Value) -> Value {
    let before = signal.provider_before_snapshot(plan);
    let geometry = match point_geometry(signal) {
        Ok(value) => value,
        Err(value) => {
            return json!({"name":name,"origin":origin,"signal_before":before,
            "geometry_error":error(&value),"signal_after":signal.provider_before_snapshot(plan)});
        }
    };
    let full = sample_range(&geometry["full_samples"]);
    let default = AudioQueryLimits::default();
    let mut requests = if let Some(requests) = origin["request_source"]["requests"].as_array() {
        requests
            .iter()
            .map(|r| {
                (
                    r["name"].as_str().unwrap().to_owned(),
                    request_range(r),
                    request_limits(r),
                )
            })
            .collect::<Vec<_>>()
    } else {
        let mut rows = vec![
            ("full".into(), full.clone(), default),
            ("empty_start".into(), full.start..full.start, default),
            ("empty_end".into(), full.end..full.end, default),
            (
                "one_work".into(),
                full.clone(),
                AudioQueryLimits {
                    maximum_work: 1,
                    ..default
                },
            ),
            (
                "invalid_zero_spans".into(),
                full.clone(),
                AudioQueryLimits {
                    maximum_spans: 0,
                    ..default
                },
            ),
        ];
        if full.end.0 - full.start.0 > 2 {
            rows.push((
                "crop".into(),
                SignalSample(full.start.0 + 1)..SignalSample(full.end.0 - 1),
                default,
            ));
        }
        rows
    };
    push_if_new(
        &mut requests,
        "zero_work",
        full.clone(),
        AudioQueryLimits {
            maximum_work: 0,
            ..default
        },
    );
    let direct = signal.policy(full.clone(), default);
    let direct_probe = policy_result(&direct);
    let direct_work = direct.as_ref().ok().map(|v| v.work);
    drop(direct);
    assert_eq!(signal.provider_before_snapshot(plan), before);
    let preserved = signal.policy_after_preserve(full.clone(), default);
    let preserved_probe = policy_result(&preserved);
    let preserved_work = preserved.as_ref().ok().map(|v| v.work);
    drop(preserved);
    assert_eq!(signal.provider_before_snapshot(plan), before);
    if origin["family"] == "H_PointCeil_subset" {
        for (mode, work) in [
            ("public_policy", direct_work),
            ("public_after_preserve", preserved_work),
        ] {
            if let Some(work) = work {
                push_if_new(
                    &mut requests,
                    &format!("exact_{mode}_work"),
                    full.clone(),
                    AudioQueryLimits {
                        maximum_work: work,
                        ..default
                    },
                );
                if work > 0 {
                    push_if_new(
                        &mut requests,
                        &format!("below_{mode}_work"),
                        full.clone(),
                        AudioQueryLimits {
                            maximum_work: work - 1,
                            ..default
                        },
                    );
                }
            }
        }
    }
    let mut rows = Vec::new();
    for (label, samples, limits) in requests {
        let current = signal.query(samples.clone(), limits);
        let current_body = query_result(plan, &current);
        drop(current);
        assert_eq!(signal.provider_before_snapshot(plan), before);
        let direct = public_policy(signal, samples.clone(), limits, false);
        let direct_body = policy_result(&direct);
        drop(direct);
        assert_eq!(signal.provider_before_snapshot(plan), before);
        let preserved = public_policy(signal, samples.clone(), limits, true);
        let preserved_body = policy_result(&preserved);
        drop(preserved);
        let after = signal.provider_before_snapshot(plan);
        assert_eq!(after, before);
        rows.push(
            json!({"name":label,"samples":samples,"limits":super::super::super::limits(limits),
            "public_current_reference":current_body,"public_policy":direct_body,
            "public_policy_after_preserve":preserved_body,"signal_after":after}),
        );
    }
    json!({"name":name,"origin":origin,"geometry":geometry,"signal_before":before,
        "full_public_policy_probe":direct_probe,"full_public_after_preserve_probe":preserved_probe,
        "requests":rows,"signal_after":signal.provider_before_snapshot(plan)})
}
fn actual_failed_root_attempt() -> Value {
    let frozen = super::super::frozen();
    let case = &frozen["cases"][2];
    let document = match ProjectDocument::from_json(&case["document_wire"].to_string()) {
        Ok(value) => value,
        Err(value) => {
            return json!({"document_wire":case["document_wire"],"setup":"DocumentError","error":error(&value.into())});
        }
    };
    let plan = match RenderPlan::compile(&document) {
        Ok(value) => value,
        Err(value) => {
            return json!({"document_wire":case["document_wire"],"setup":"CompileError","error":error(&value)});
        }
    };
    match super::super::super::stage_from_public_query(&plan) {
        Ok((stage, query)) => json!({"setup":"Ok","actual_public_query":query,
            "input":stage.input_signal().provider_before_snapshot(&plan)}),
        Err(value) => json!({"setup":"Error","error":error(&value),
            "entry":"actual existing public root stage extraction; no replacement provider",
            "document_wire":case["document_wire"],"checked_document":document}),
    }
}
pub(in crate::plan) fn visit_cases(
    mut visit: impl FnMut(&RenderPlan, &str, &AudioSignal<'_>, &Value),
) -> Value {
    super::super::visit_fixtures(|plan, name, signal, old| {
        let grid = old["grids"]
            .as_array()
            .unwrap()
            .iter()
            .find(|g| g["rule"] == "PointCeil")
            .unwrap();
        let source = json!({"family":"H_PointCeil_subset","origin":old["origin"],
            "request_source":{"requests":grid["requests"].as_array().unwrap().iter().map(|r|
                json!({"name":r["name"],"samples":r["samples"],"limits":r["limits"]})).collect::<Vec<_>>()}});
        visit(plan, name, signal, &source);
        if name == "owned_input_via_projection" {
            let projection = plan
                .owned_preserve_projection(crate::AudioDefinitionSelector::Node {
                    node: id("stage"),
                })
                .unwrap();
            let full = ExactRatio::ZERO..ExactRatio::integer(projection.output_frames());
            let tape = crate::AudioSignalTape::new(
                plan,
                full.clone(),
                vec![crate::AudioSignalTapeRun::intrinsic(
                    full.clone(),
                    full,
                    Arc::clone(&projection),
                )],
            )
            .unwrap();
            tape.provider_before_visit(|index,child,samples|{
                visit(plan,&format!("owned_definition_projected_{index}"),child,
                    &json!({"family":"H_metadata_supplement","public":"owned_preserve_projection + intrinsic TapeRun borrowed child",
                        "allocated_samples":samples}));
            });
        }
    });
    let (plan, instance, recipe) = super::repeated_preserve_plan();
    let rows = super::checked_signal_rows(&plan, instance, recipe.clone());
    let mut projected = false;
    for (name, signal, origin) in rows {
        if name.starts_with("repeated_stage_source_")
            || (!projected && name.starts_with("intrinsic_projected_"))
        {
            if name.starts_with("intrinsic_projected_") {
                projected = true;
            }
            visit(
                &plan,
                &name,
                &signal,
                &json!({"family":"H_metadata_supplement",
                "existing_checked_helper":"metadata_tests::repeated_preserve_plan + checked_signal_rows",
                "public_origin":origin}),
            );
        }
    }
    let definition = plan
        .audio_definition(crate::AudioDefinitionSelector::RepeatGap {
            repeat: id("repeat"),
        })
        .unwrap();
    let point = definition
        .in_point_clock(
            crate::AudioRootPlacement::new(
                ExactRatio::ZERO,
                ExactRatio::ONE,
                ExactRatio::ZERO..ExactRatio::ONE,
            )
            .unwrap(),
            ExactRatio::ZERO,
        )
        .unwrap();
    let signal = point.signal().source_voice(recipe).unwrap().output_signal();
    visit(
        &plan,
        "repeat_gap_source_output",
        &signal,
        &json!({"family":"H_metadata_supplement",
        "public":"audio_definition(RepeatGap).in_point_clock(...).signal().source_voice(...).output_signal()"}),
    );
    json!({"owned_root_attempt":actual_failed_root_attempt(),
        "fixture_origin":"existing H public constructors and metadata fixtures; no private provider mutation",
        "source_qualification":"H checked Plan-only declared asset metadata; no actual media/store/native qualification claim"})
}
fn body() -> Value {
    let mut cases = Vec::new();
    let setups =
        visit_cases(|plan, name, signal, origin| cases.push(observe(plan, name, signal, origin)));
    let requests = cases
        .iter()
        .map(|row| row["requests"].as_array().map_or(0, Vec::len))
        .sum::<usize>();
    let source_documents = super::super::frozen()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| json!({"name":case["name"],"document_wire":case["document_wire"]}))
        .collect::<Vec<_>>();
    json!({"schema":SIGNAL_SCHEMA,"source_documents":source_documents,"setups":setups,"cases":cases,"request_count":requests,
        "scope":{"entry":"public AudioSignal::policy and policy_after_preserve, literal PointCeil/endpoints true and false",
            "reference":"public AudioSignal::query per identical request; complete existing H borrowed snapshots",
            "work":"literal ordinary returned work, capacities, lookup and errors; constructor errors expose no partial work",
            "ownership":"no admission backend, native account, native provider, Tape policy facade or Point-domain policy facade",
            "inputs":"complete borrowed logical Signal snapshots before and after each public query/policy call",
            "metadata":"bounded actual repeated Source/Projected and RepeatGap/owned-definition witnesses"}})
}
fn strip(mut value: Value) -> Value {
    for key in ["variant", "source_sha256", "published_checkpoint"] {
        value.as_object_mut().unwrap().remove(key);
    }
    value
}
#[test]
#[ignore = "manual ordinary public Signal policy before39 capture; fresh /tmp output"]
fn capture_public_signal_policy_before_v1() {
    let output = std::env::var("DEADPAN_SIGNAL_POLICY_BEFORE_OUTPUT").expect("fresh output");
    let path = Path::new(&output);
    assert!(
        path.is_absolute()
            && path
                .parent()
                .unwrap()
                .canonicalize()
                .unwrap()
                .starts_with(Path::new("/tmp").canonicalize().unwrap())
    );
    let variant = std::env::var("DEADPAN_SIGNAL_POLICY_BEFORE_VARIANT").unwrap();
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source = std::env::var("DEADPAN_SIGNAL_POLICY_BEFORE_SOURCE_SHA256").unwrap();
    assert!(source.len() == 64 && source.bytes().all(|v| v.is_ascii_hexdigit()));
    assert_eq!(
        std::env::var("DEADPAN_SIGNAL_POLICY_BEFORE_PUBLISHED_CHECKPOINT").unwrap(),
        SIGNAL_CHECKPOINT
    );
    let mut value = body();
    value["variant"] = json!(variant);
    value["source_sha256"] = json!(source);
    value["published_checkpoint"] = json!(SIGNAL_CHECKPOINT);
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .unwrap();
    let mut writer = std::io::BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, &value).unwrap();
    writer.write_all(b"\n").unwrap();
    writer.flush().unwrap();
    writer.get_ref().sync_all().unwrap();
}
#[test]
#[ignore = "manual whole-body ordinary Signal before cross-regression; independently captured expected path"]
fn public_signal_policy_matches_external_before_v1() {
    let path = std::env::var("DEADPAN_SIGNAL_POLICY_BEFORE_EXPECTED")
        .expect("independently captured expected body");
    let expected = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(body(), strip(expected));
}

#[test]
fn public_signal_policy_frozen_before_body_is_exact() {
    let expected: Value = serde_json::from_slice(include_bytes!(
        "../tests/fixtures/signal-policy-baseline-v1/snapshot.json"
    ))
    .unwrap();
    assert_eq!(body(), expected);
}

#[path = "audio_signal_policy_paired_tests.rs"]
mod paired;
