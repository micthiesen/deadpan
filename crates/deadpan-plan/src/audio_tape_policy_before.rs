//! Ordinary Tape policy BEFORE values. No new admission or native execution.
use super::*;
use crate::{AudioProjectedRoot, AudioRootPlacement, AudioSourceVoiceRecipe};
use deadpan_core::{ExactRatio, FrameDuration, InstancePath, NodeId, ProjectDocument};
use serde_json::{Value, json};
use std::{fs::OpenOptions, io::Write, path::Path};
const CHECKPOINT: &str = "a6bc3de7e4e27f9fe11252c28cca07af385e4755";
const SCHEMA: &str = "ordinary-tape-policy-before-v1";
fn error(e: &PlanError) -> Value {
    e.tape_before_error_snapshot()
}
fn query(value: &Result<AudioSignalQuery<'_>, PlanError>, plan: &RenderPlan) -> Value {
    match value {
        Ok(q) => json!({"result":"Ok","query":q.tape_before_query_snapshot(plan)}),
        Err(e) => {
            json!({"result":"Error","error":error(e),"ordinary_partial_work":"not exposed by ordinary constructor errors"})
        }
    }
}
fn policy(value: &Result<AudioPolicyQuery<SignalSample>, PlanError>) -> Value {
    match value {
        Ok(q) => {
            let AudioPolicyQuery {
                suppressed,
                contents,
                lookup,
                work,
            } = q;
            json!({"result":"Ok","query":{"suppressed":suppressed,"suppressed_capacity":suppressed.capacity(),
                "contents":contents,"contents_capacity":contents.capacity(),"lookup":lookup,"work":work}})
        }
        Err(e) => {
            json!({"result":"Error","error":error(e),"ordinary_partial_work":"not exposed by ordinary constructor errors"})
        }
    }
}
fn limits(q: AudioQueryLimits) -> Value {
    json!({"maximum_spans":q.maximum_spans,"maximum_work":q.maximum_work})
}
fn observe(plan: &RenderPlan, name: &str, tape: &AudioSignalTape<'_>, origin: Value) -> Value {
    let before = tape.provider_before_snapshot(plan);
    let geometry = (|| -> Result<_, PlanError> {
        Ok((
            tape.sample_start()?..tape.grid.boundary(tape.route.end)?,
            tape.grid.boundary(tape.support.start)?..tape.sample_count()?,
        ))
    })();
    let (full, support_full) = match geometry {
        Ok(q) => q,
        Err(e) => {
            return json!({"name":name,"origin":origin,
        "tape_before":before,"geometry_error":error(&e),"tape_after":tape.provider_before_snapshot(plan)});
        }
    };
    let defaults = AudioQueryLimits::default();
    let a = tape.policy(full.clone(), defaults);
    let a_body = policy(&a);
    let aw = a.as_ref().ok().map(|q| q.work);
    drop(a);
    let b = tape.policy_after_preserve(full.clone(), defaults);
    let b_body = policy(&b);
    let bw = b.as_ref().ok().map(|q| q.work);
    drop(b);
    let mut pending = vec![
        ("full".to_string(), full.clone(), defaults),
        ("empty_start".into(), full.start..full.start, defaults),
        ("empty_end".into(), full.end..full.end, defaults),
        (
            "empty_start_one_work".into(),
            full.start..full.start,
            AudioQueryLimits {
                maximum_work: 1,
                ..defaults
            },
        ),
        ("reversed".into(), full.end..full.start, defaults),
        (
            "outside_before".into(),
            SignalSample(full.start.0.checked_sub(1).unwrap())..full.start,
            defaults,
        ),
        (
            "outside_after".into(),
            full.end..SignalSample(full.end.0.checked_add(1).unwrap()),
            defaults,
        ),
        (
            "span_cap_one".into(),
            full.clone(),
            AudioQueryLimits {
                maximum_spans: 1,
                ..defaults
            },
        ),
        (
            "invalid_zero_spans".into(),
            full.clone(),
            AudioQueryLimits {
                maximum_spans: 0,
                ..defaults
            },
        ),
        (
            "work_one".into(),
            full.clone(),
            AudioQueryLimits {
                maximum_work: 1,
                ..defaults
            },
        ),
    ];
    for work in [0, 2, 3] {
        pending.push((
            format!("work_{work}"),
            full.clone(),
            AudioQueryLimits {
                maximum_work: work,
                ..defaults
            },
        ));
    }
    pending.push((
        "invalid_limits_and_bad_range".into(),
        SignalSample(full.start.0.checked_sub(1).unwrap())..full.start,
        AudioQueryLimits {
            maximum_spans: 0,
            ..defaults
        },
    ));
    if support_full != full {
        pending.push(("support_full".into(), support_full, defaults));
    }
    if full.end.0 - full.start.0 > 2 {
        pending.push((
            "crop".into(),
            SignalSample(full.start.0 + 1)..SignalSample(full.end.0 - 1),
            defaults,
        ));
    }
    if let Some(first) = tape.runs.iter().find(|r| r.samples.end > full.start) {
        let end = first.samples.end.min(full.end);
        if full.start < end && end < full.end {
            pending.push(("first_run_only".into(), full.start..end, defaults));
            pending.push(("empty_first_run_boundary".into(), end..end, defaults));
        }
    }
    let mut measured = Vec::new();
    for (mode, work) in [("policy", aw), ("after_preserve", bw)] {
        if let Some(work) = work {
            if measured.contains(&work) {
                continue;
            }
            measured.push(work);
            pending.push((
                format!("exact_{mode}_work"),
                full.clone(),
                AudioQueryLimits {
                    maximum_work: work,
                    ..defaults
                },
            ));
            if work > 0 {
                pending.push((
                    format!("below_{mode}_work"),
                    full.clone(),
                    AudioQueryLimits {
                        maximum_work: work - 1,
                        ..defaults
                    },
                ));
            }
        }
    }
    let mut rows = Vec::new();
    for (request, samples, cap) in pending {
        let q = tape.query(samples.clone(), cap);
        let q_body = query(&q, plan);
        drop(q);
        let a = tape.policy(samples.clone(), cap);
        let a_body = policy(&a);
        drop(a);
        let b = tape.policy_after_preserve(samples.clone(), cap);
        let b_body = policy(&b);
        drop(b);
        assert_eq!(before, tape.provider_before_snapshot(plan));
        rows.push(json!({"name":request,"samples":samples,"limits":limits(cap),
            "ordinary_query":q_body,"ordinary_policy":a_body,"ordinary_policy_after_preserve":b_body}));
    }
    json!({"name":name,"origin":origin,"full_samples":full,"tape_before":before,
        "full_policy_probe":a_body,"full_after_preserve_probe":b_body,
        "requests":rows,"tape_after":tape.provider_before_snapshot(plan)})
}
fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}
fn range(a: i64, b: i64) -> Range<ExactRatio> {
    ExactRatio::integer(a)..ExactRatio::integer(b)
}
fn recipe(value: &Value) -> AudioSourceVoiceRecipe {
    AudioSourceVoiceRecipe {
        source: serde_json::from_value(value["source"].clone()).unwrap(),
        mapping: serde_json::from_value(value["mapping"].clone()).unwrap(),
        offset: serde_json::from_value(value["offset"].clone()).unwrap(),
    }
}
fn id(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}
type Visit<'a> = dyn FnMut(&RenderPlan, &str, &AudioSignalTape<'_>, Value) -> Value + 'a;
fn built(
    plan: &RenderPlan,
    name: &str,
    origin: Value,
    value: Result<AudioSignalTape<'_>, PlanError>,
    visit: &mut Visit<'_>,
) -> Value {
    match value {
        Ok(tape) => visit(plan, name, &tape, origin),
        Err(e) => json!({"name":name,"origin":origin,"setup":"Error","error":error(&e)}),
    }
}
fn partition<'plan>(
    plan: &'plan RenderPlan,
    signal: &AudioSignal<'plan>,
    negative: bool,
) -> Result<AudioSignalTape<'plan>, PlanError> {
    let offset = if negative {
        ratio(-1, 2)
    } else {
        ExactRatio::ZERO
    };
    let at = |n: i64| offset.checked_add(ExactRatio::integer(n)).unwrap();
    AudioSignalTape::new(
        plan,
        at(0)..at(10),
        vec![
            AudioSignalTapeRun::new(at(0)..at(4), range(0, 4), signal.clone()),
            AudioSignalTapeRun::new(at(4)..at(6), range(4, 6), signal.clone()),
            AudioSignalTapeRun::new(at(6)..at(10), range(6, 10), signal.clone()),
        ],
    )
}
fn family(index: usize, visit: &mut Visit<'_>) -> Value {
    let frozen: Value = serde_json::from_str(include_str!(
        "../tests/fixtures/provider-child-baseline-v1/snapshot.json"
    ))
    .unwrap();
    let input = &frozen["cases"][index];
    let wire = &input["document_wire"];
    let doc = match ProjectDocument::from_json(&wire.to_string()) {
        Ok(v) => v,
        Err(e) => {
            return json!({"document_wire":wire,"setup":"DocumentError","error":error(&e.into())});
        }
    };
    let plan = match RenderPlan::compile(&doc) {
        Ok(v) => v,
        Err(e) => return json!({"document_wire":wire,"setup":"CompileError","error":error(&e)}),
    };
    let mut tapes = Vec::new();
    match index {
        0 => {
            let root = plan.audio_signal();
            tapes.push(built(
                &plan,
                "structural_three_runs",
                json!({"public":"Tape::new; three exact current windows"}),
                partition(&plan, &root, false),
                visit,
            ));
            let public_recipe = recipe(&input["observations"]["attempted_recipe"]);
            match root.source_voice(public_recipe){
                Ok(voice)=>{
                    tapes.push(built(&plan,"source_input_three_runs_negative_half",json!({"public":"SourceVoice::input_signal then Tape::new","support_offset":ratio(-1,2)}),
                        partition(&plan,&voice.input_signal(),true),visit));
                    tapes.push(built(&plan,"source_output_three_runs_negative_half",json!({"public":"SourceVoice::output_signal then Tape::new","support_offset":ratio(-1,2)}),
                        partition(&plan,&voice.output_signal(),true),visit));
                },
                Err(e)=>tapes.push(json!({"name":"catalog_voice_partition","recipe":input["observations"]["attempted_recipe"],"setup_error":error(&e)})),
            }
            match root.source_voice(recipe(&input["observations"]["selected_attempted_recipe"])){
                Ok(voice)=>tapes.push(built(&plan,"selected_output_two_runs",json!({"public":"selected SourceVoice::output_signal then Tape::new"}),
                    AudioSignalTape::new(&plan,range(0,10),vec![
                        AudioSignalTapeRun::new(range(0,4),range(0,4),voice.output_signal()),
                        AudioSignalTapeRun::new(range(4,10),range(4,10),voice.output_signal())]),visit)),
                Err(e)=>tapes.push(json!({"name":"selected_output_two_runs","recipe":input["observations"]["selected_attempted_recipe"],"setup_error":error(&e)})),
            }
            tapes.push(built(&plan,"zero_sample_middle_run",json!({"public":"Tape::new; positive quarter-frame windows, actual grid decides zero allocation"}),
                AudioSignalTape::new(&plan,range(0,2),vec![
                    AudioSignalTapeRun::new(ExactRatio::ZERO..ratio(1,4),range(0,4),root.clone()),
                    AudioSignalTapeRun::new(ratio(1,4)..ratio(1,2),range(4,6),root.clone()),
                    AudioSignalTapeRun::new(ratio(1,2)..ExactRatio::integer(2),range(6,10),root)]),visit));
        }
        1 => {
            let instance: InstancePath =
                serde_json::from_value(input["observations"]["instance"].clone()).unwrap();
            let occurrence = plan.source_voice_occurrence(
                instance,
                recipe(&input["observations"]["attempted_recipe"]),
                Default::default(),
            );
            match occurrence {
                Err(e) => tapes
                    .push(json!({"name":"public_occurrence_projection","setup_error":error(&e)})),
                Ok(occurrence) => match occurrence.processing_projection() {
                    None => tapes.push(
                        json!({"name":"public_occurrence_projection","setup":"NoProjection"}),
                    ),
                    Some(occurrence_projection) => {
                        if let Some(tape) = occurrence_projection.input_tape() {
                            tapes.push(visit(&plan,"public_occurrence_padding_tape",tape,
                                json!({"public":"SourceVoiceOccurrence::processing_projection().input_tape()"})));
                        }
                        let stage = occurrence_projection.stage().clone();
                        let child = stage.input_signal();
                        let support = child.support();
                        let full = range(0, stage.descriptor().duration.frames());
                        let projection = (|| -> Result<_, PlanError> {
                            let input = AudioSignalTape::new(
                                &plan,
                                support.clone(),
                                vec![AudioSignalTapeRun::new(
                                    support.clone(),
                                    support.clone(),
                                    child.clone(),
                                )],
                            )?;
                            let policy = AudioSignalTape::new(
                                &plan,
                                full.clone(),
                                vec![AudioSignalTapeRun::new(full.clone(), support, child)],
                            )?;
                            crate::AudioStageProjection::new(
                                stage,
                                input,
                                policy,
                                FrameDuration::new(8)?,
                            )
                        })();
                        match projection{
                            Err(e)=>tapes.push(json!({"name":"structural_public_projection","setup_error":error(&e)})),
                            Ok(projection)=>{
                                let voice=plan.audio_signal().source_voice(recipe(&frozen["cases"][0]["observations"]["attempted_recipe"]));
                                match voice{
                                    Ok(voice)=>tapes.push(built(&plan,"intrinsic_override_then_source",json!({"public":"TapeRun::intrinsic followed by Source input; recursive override always after-Preserve"}),
                                        AudioSignalTape::new(&plan,range(0,16),vec![
                                            AudioSignalTapeRun::intrinsic(range(0,8),range(0,8),Arc::clone(&projection)),
                                            AudioSignalTapeRun::new(range(8,16),range(0,8),voice.input_signal())]),visit)),
                                    Err(e)=>tapes.push(json!({"name":"intrinsic_override_then_source","setup_error":error(&e)})),
                                }
                                let placement=AudioRootPlacement::new(ratio(-3,2),ratio(3,2),range(0,8));
                                let root=placement.and_then(|placement|AudioProjectedRoot::new(Arc::clone(&projection),placement));
                                match root{
                                    Err(e)=>tapes.push(json!({"name":"public_root_negative_round_even","setup_error":error(&e)})),
                                    Ok(root)=>{
                                        let before=root.tape_before_snapshot(&plan);
                                        root.tape_before_visit(|tape|tapes.push(visit(&plan,"public_root_negative_round_even",tape,
                                            json!({"public":"AudioProjectedRoot::new; borrowed actual stored policy","root_before":before}))));
                                        assert_eq!(before,root.tape_before_snapshot(&plan));
                                    },
                                }
                            },
                        }
                    }
                },
            }
        }
        3 => {
            let projection = plan.owned_preserve_projection(crate::AudioDefinitionSelector::Node {
                node: id("stage"),
            });
            match projection{
                Err(e)=>tapes.push(json!({"name":"late_owned_input","setup_error":error(&e)})),
                Ok(projection)=>match plan.audio_signal().source_voice(recipe(&frozen["cases"][0]["observations"]["attempted_recipe"])){
                    Err(e)=>tapes.push(json!({"name":"late_owned_input","setup_error":error(&e)})),
                    Ok(voice)=>tapes.push(built(&plan,"source_then_requires_owned_input",json!({"public":"first Source input, then actual owned Stage::input_signal; first_run_only is separate complete prefix witness"}),
                        AudioSignalTape::new(&plan,range(0,2),vec![
                            AudioSignalTapeRun::new(range(0,1),range(0,1),voice.input_signal()),
                            AudioSignalTapeRun::new(range(1,2),range(0,1),projection.stage().input_signal())]),visit)),
                },
            }
        }
        _ => unreachable!(),
    }
    json!({"source_provider_case":index,"document_wire":wire,"checked_document":doc,"tapes":tapes})
}
fn corpus(visit: &mut Visit<'_>) -> Value {
    json!({"schema":SCHEMA,"families":[family(0,visit),family(1,visit),family(3,visit)],
        "scope":{"ordinary":"actual public Tape::policy and policy_after_preserve; no admission backend",
            "query":"complete borrowed public Tape query headers/spans/content from H serializers",
            "inputs":"complete borrowed Tape state before/after; public Root stored policy observed without mutation",
            "errors":"ordinary errors expose no partial inventory/work receipt; first_run_only is separate fresh prefix witness",
            "reachability":"Projected appears through intrinsic Tape query; policy follows its real stored override. Silence comes from actual occurrence padding",
            "identity":"H logical snapshots exclude process addresses/Arc refcounts; no allocator/native ownership claim"}})
}
pub(crate) fn visit_cases(
    mut visitor: impl FnMut(&RenderPlan, &str, &AudioSignalTape<'_>),
) -> Value {
    corpus(&mut |plan, name, tape, origin| {
        visitor(plan, name, tape);
        json!({"name":name,"origin":origin,"setup":"Visited"})
    })
}
fn matrix() -> Value {
    corpus(&mut observe)
}
fn strip(mut value: Value) -> Value {
    for key in ["variant", "source_sha256", "published_checkpoint"] {
        value.as_object_mut().unwrap().remove(key);
    }
    value
}
#[test]
#[ignore = "manual ordinary Tape policy BEFORE capture; actual pinned source and fresh /tmp output"]
fn capture_ordinary_tape_policy_before_v1() {
    let output = std::env::var("DEADPAN_TAPE_POLICY_BEFORE_OUTPUT").expect("explicit fresh output");
    let path = Path::new(&output);
    assert!(path.is_absolute());
    assert!(
        path.parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .starts_with(Path::new("/tmp").canonicalize().unwrap())
    );
    let variant = std::env::var("DEADPAN_TAPE_POLICY_BEFORE_VARIANT").expect("actual variant");
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source = std::env::var("DEADPAN_TAPE_POLICY_BEFORE_SOURCE_SHA256")
        .expect("actual complete source digest");
    assert!(source.len() == 64 && source.bytes().all(|b| b.is_ascii_hexdigit()));
    let checkpoint =
        std::env::var("DEADPAN_TAPE_POLICY_BEFORE_PUBLISHED_CHECKPOINT").expect("published H");
    assert_eq!(checkpoint, CHECKPOINT);
    let mut value = matrix();
    value["variant"] = json!(variant);
    value["source_sha256"] = json!(source);
    value["published_checkpoint"] = json!(checkpoint);
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
#[ignore = "manual whole-body ordinary Tape regression; independent actual expected body"]
fn ordinary_tape_policy_matches_external_before_v1() {
    let file = std::env::var("DEADPAN_TAPE_POLICY_BEFORE_EXPECTED")
        .expect("independent actual before body");
    let expected: Value = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
    assert_eq!(matrix(), strip(expected));
}

#[test]
fn ordinary_tape_policy_matches_frozen_baseline_v1() {
    let expected = include_bytes!("../tests/fixtures/tape-policy-baseline-v1/snapshot.json");
    let mut observed = serde_json::to_vec(&matrix()).unwrap();
    observed.push(b'\n');
    assert_eq!(
        observed, expected,
        "complete ordinary Tape body, without work or capacity normalization"
    );
}
