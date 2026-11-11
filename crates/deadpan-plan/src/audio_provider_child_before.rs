//! Manual ordinary provider-child BEFORE capture. No admission backend or native account.
use super::*;
use deadpan_core::*;
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs::OpenOptions, io::Write, path::Path};

const CHECKPOINT: &str = "f10ae0f57d936bcfd16d2edb1f999a768b01fc40";
const SCHEMA: &str = "ordinary-provider-child-before-v1";

impl AudioSignal<'_> {
    pub(crate) fn provider_before_snapshot(&self, plan: &RenderPlan) -> Value {
        assert!(self.belongs_to(plan));
        let Self {
            plan: _,
            root,
            support,
            sampling_support,
            constrain_support,
            repeats,
            definition,
            placed_transform,
            bypass_binding,
            gap,
            provider,
        } = self;
        let provider = match provider {
            SignalProvider::Structural => json!({"tag":"Structural"}),
            SignalProvider::RequiresOwnedInput => json!({"tag":"RequiresOwnedInput"}),
            SignalProvider::Silence => json!({"tag":"Silence"}),
            SignalProvider::Projected(value) => {
                json!({"tag":"Projected","value":value.provider_before_snapshot(plan)})
            }
            SignalProvider::Source {
                recipe,
                apply_holds,
            } => json!({"tag":"Source","apply_holds":apply_holds,
                "value":recipe.provider_before_snapshot()}),
        };
        let bypass = bypass_binding.map(|target| match target {
            BindingTarget::Node(index) => {
                json!({"tag":"Node","index":index,"node":plan.nodes[index].inspection.id})
            }
            BindingTarget::Gap(index) => {
                json!({"tag":"Gap","index":index,"node":plan.nodes[index].inspection.id})
            }
        });
        json!({"belongs_to_expected_plan":true,"root_index":root,"root_node":plan.nodes[*root].inspection.id,
            "support":support,"sampling_support":sampling_support,"constrain_support":constrain_support,
            "repeats":repeats,"repeats_capacity":repeats.capacity(),"definition":definition,
            "placed_transform":placed_transform,"bypass_binding":bypass,"gap":gap,"provider":provider})
    }
}
impl AudioStage<'_> {
    pub(crate) fn provider_before_stage_snapshot(&self, plan: &RenderPlan) -> Value {
        self.point_content_old_snapshot(plan)
    }
}
fn error(value: &PlanError) -> Value {
    match value {
        PlanError::InvalidAudioSourceOccurrence(message) => {
            json!({"variant":"InvalidAudioSourceOccurrence","message":message})
        }
        PlanError::InvalidAudioRootPlacement(message) => {
            json!({"variant":"InvalidAudioRootPlacement","message":message})
        }
        other => json!({"variant":"Other","message":other.to_string()}),
    }
}
fn lookup(value: LookupStats) -> Value {
    let LookupStats {
        visited_nodes,
        sequence_comparisons,
        iteration_run_comparisons,
    } = value;
    json!({"visited_nodes":visited_nodes,"sequence_comparisons":sequence_comparisons,
        "iteration_run_comparisons":iteration_run_comparisons})
}
fn content(plan: &RenderPlan, value: &AudioSignalContent<'_>) -> Value {
    match value {
        AudioSignalContent::Leaf(value) => json!({"tag":"Leaf","value":value}),
        AudioSignalContent::Stage(value) => {
            json!({"tag":"Stage","value":value.point_content_old_snapshot(plan)})
        }
        AudioSignalContent::ProjectedStage(value) => {
            json!({"tag":"ProjectedStage","value":value.provider_before_snapshot(plan)})
        }
    }
}
fn query(plan: &RenderPlan, value: &AudioSignalQuery<'_>) -> Value {
    let AudioSignalQuery {
        project_id,
        revision_id,
        definition,
        samples,
        spans,
        lookup: stats,
        work,
    } = value;
    json!({"project_id":project_id,"revision_id":revision_id,"definition":definition,"samples":samples,
        "span_capacity":spans.capacity(),"lookup":lookup(*stats),"work":work,
        "spans":spans.iter().map(|span|{
            let AudioSignalSpan { definition, samples, allocated_samples, signal_extent, instance,
                gap_after, transform, grid, sampling, retimes, content: value }=span;
            json!({"definition":definition,"samples":samples,"allocated_samples":allocated_samples,
                "signal_extent":signal_extent,"instance":instance,"gap_after":gap_after,
                "transform":transform,"grid":grid,"sampling":sampling,"retimes":retimes,
                "retimes_capacity":retimes.capacity(),"content":content(plan,value)})
        }).collect::<Vec<_>>()})
}
fn query_result(plan: &RenderPlan, value: &Result<AudioSignalQuery<'_>, PlanError>) -> Value {
    match value {
        Ok(value) => json!({"result":"Ok","query":query(plan,value)}),
        Err(value) => json!({"result":"Error","error":error(value),
            "ordinary_partial_work":"not exposed by ordinary constructor errors"}),
    }
}
fn policy_result(value: &Result<crate::AudioPolicyQuery<SignalSample>, PlanError>) -> Value {
    match value {
        Ok(value) => {
            let crate::AudioPolicyQuery {
                suppressed,
                contents,
                lookup: stats,
                work,
            } = value;
            json!({"result":"Ok","query":{"suppressed":suppressed,"suppressed_capacity":suppressed.capacity(),
                "contents":contents,"contents_capacity":contents.capacity(),"lookup":lookup(*stats),"work":work}})
        }
        Err(value) => json!({"result":"Error","error":error(value),
            "ordinary_partial_work":"not exposed by ordinary constructor errors"}),
    }
}
fn hold_result(value: &Result<crate::AudioHoldPolicyQuery<SignalSample>, PlanError>) -> Value {
    match value {
        Ok(value) => {
            let crate::AudioHoldPolicyQuery {
                project_id,
                revision_id,
                samples,
                rules,
                lookup: stats,
                work,
            } = value;
            json!({"result":"Ok","query":{"project_id":project_id,"revision_id":revision_id,"samples":samples,
                "rules":rules,"rules_capacity":rules.capacity(),"lookup":lookup(*stats),"work":work}})
        }
        Err(value) => json!({"result":"Error","error":error(value),
            "ordinary_partial_work":"not exposed by ordinary constructor errors"}),
    }
}
fn limits(value: AudioQueryLimits) -> Value {
    json!({"maximum_spans":value.maximum_spans,"maximum_work":value.maximum_work})
}
fn rule_name(rule: AudioBoundaryRule) -> &'static str {
    match rule {
        AudioBoundaryRule::PointCeil => "PointCeil",
        AudioBoundaryRule::RoundEven => "RoundEven",
    }
}
fn observe_signal(plan: &RenderPlan, name: &str, signal: &AudioSignal<'_>, origin: Value) -> Value {
    let before = signal.provider_before_snapshot(plan);
    let mut grids = Vec::new();
    for rule in [AudioBoundaryRule::PointCeil, AudioBoundaryRule::RoundEven] {
        let geometry = (|| -> Result<_, PlanError> {
            let transform = signal.transform()?;
            let grid = transform.grid(rule)?;
            Ok((
                transform,
                grid,
                grid.boundary(signal.support.start)?..grid.boundary(signal.support.end)?,
            ))
        })();
        let (transform, grid, full) = match geometry {
            Ok(value) => value,
            Err(value) => {
                grids.push(json!({"rule":rule_name(rule),"geometry_error":error(&value)}));
                continue;
            }
        };
        let default = AudioQueryLimits::default();
        let full_current = signal.query_inner(full.clone(), default, rule, false, false);
        let full_policy = signal.policy_on_grid(full.clone(), default, rule, true);
        let mut requests = vec![
            ("full".to_string(), full.clone(), default),
            ("empty_start".into(), full.start..full.start, default),
            ("empty_end".into(), full.end..full.end, default),
            (
                "empty_start_one_work".into(),
                full.start..full.start,
                AudioQueryLimits {
                    maximum_work: 1,
                    ..default
                },
            ),
            (
                "empty_end_one_work".into(),
                full.end..full.end,
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
            (
                "outside_before".into(),
                SignalSample(full.start.0.checked_sub(1).unwrap())..full.start,
                default,
            ),
            (
                "outside_after".into(),
                full.end..SignalSample(full.end.0.checked_add(1).unwrap()),
                default,
            ),
            ("reversed".into(), full.end..full.start, default),
            (
                "one_work".into(),
                full.clone(),
                AudioQueryLimits {
                    maximum_work: 1,
                    ..default
                },
            ),
            (
                "one_span".into(),
                full.clone(),
                AudioQueryLimits {
                    maximum_spans: 1,
                    ..default
                },
            ),
        ];
        if full.end.0 - full.start.0 > 2 {
            requests.push((
                "crop".into(),
                SignalSample(full.start.0 + 1)..SignalSample(full.end.0 - 1),
                default,
            ));
        }
        for (label, work) in [
            ("current", full_current.as_ref().ok().map(|q| q.work)),
            ("policy", full_policy.as_ref().ok().map(|q| q.work)),
        ] {
            if let Some(work) = work {
                requests.push((
                    format!("exact_{label}_work"),
                    full.clone(),
                    AudioQueryLimits {
                        maximum_work: work,
                        ..default
                    },
                ));
                if work > 0 {
                    requests.push((
                        format!("below_{label}_work"),
                        full.clone(),
                        AudioQueryLimits {
                            maximum_work: work - 1,
                            ..default
                        },
                    ));
                }
            }
        }
        let full_current_body = query_result(plan, &full_current);
        let full_policy_body = policy_result(&full_policy);
        drop(full_current);
        drop(full_policy);
        let mut rows = Vec::new();
        for (label, samples, cap) in requests {
            let current = signal.query_inner(samples.clone(), cap, rule, false, false);
            let current_body = query_result(plan, &current);
            drop(current);
            let mut current_stop_variants = Vec::new();
            for (stop_at_preserve, stop_at_bindings) in [(false, true), (true, false), (true, true)]
            {
                let value = signal.query_inner(
                    samples.clone(),
                    cap,
                    rule,
                    stop_at_preserve,
                    stop_at_bindings,
                );
                let observed = query_result(plan, &value);
                drop(value);
                current_stop_variants.push(json!({
                    "stops":[stop_at_preserve,stop_at_bindings],"result":observed,
                }));
            }
            let endpoints = signal.policy_on_grid(samples.clone(), cap, rule, true);
            let endpoints_body = policy_result(&endpoints);
            drop(endpoints);
            let preserved = signal.policy_on_grid(samples.clone(), cap, rule, false);
            let preserved_body = policy_result(&preserved);
            drop(preserved);
            let holds = if matches!(
                signal.provider,
                SignalProvider::Source {
                    apply_holds: true,
                    ..
                }
            ) {
                let value = signal.hold_policy_on_grid(samples.clone(), cap, rule);
                Some(hold_result(&value))
            } else {
                None
            };
            assert_eq!(signal.provider_before_snapshot(plan), before);
            rows.push(json!({"name":label,"samples":samples,"limits":limits(cap),
                "ordinary_current_stops":[false,false],"ordinary_current":current_body,
                "current_stop_variants":current_stop_variants,
                "ordinary_policy_endpoints_true":endpoints_body,
                "ordinary_policy_endpoints_false":preserved_body,
                "separate_ordinary_hold_policy":holds}));
        }
        grids.push(json!({"rule":rule_name(rule),"transform":transform,"grid":grid,"full_samples":full,
            "full_current_probe":full_current_body,"full_policy_probe":full_policy_body,"requests":rows}));
    }
    let after = signal.provider_before_snapshot(plan);
    assert_eq!(before, after);
    json!({"name":name,"origin":origin,"signal_before":before,"signal_after":after,"grids":grids})
}
fn id(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}
fn duration(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn hold(value: i64) -> BeatNode {
    BeatNode::hold(
        "Silent Hold",
        HoldRecipe {
            duration: duration(value),
            video: HoldVideo::Background,
            picture_context: None,
            audio: HoldAudio::Silence,
        },
    )
}
fn source_audio(name: &str) -> SourceAudio {
    let clock = SourceTimeBase::new(1, 48_000).unwrap();
    SourceAudio {
        asset: AssetId::new(name).unwrap(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: 0,
                time_base: clock,
            },
            SourceTimestamp {
                ticks: 100,
                time_base: clock,
            },
        )
        .unwrap(),
    }
}
fn original(value: i64) -> BeatNode {
    let mut node = hold(value);
    node.kind = NodeKind::Source {
        source: SourceNode {
            edit_window: None,
            duration: duration(value),
            video: SourceVideo::Blank,
            video_mapping: SourceVideoMapping::FitBeat,
            audio: Some(source_audio("original")),
            audio_mapping: SourceAudioMapping::Duration {
                frames: ExactRatio::integer(100),
            },
            audio_offset: AudioSample(0),
            link: LinkRelation::Independent,
        },
    };
    node
}
fn retime(child: &str, output: i64, input: i64) -> BeatNode {
    let mut node = hold(output);
    node.kind = NodeKind::Retime {
        purpose: RetimePurpose::Edit,
        child: id(child),
        duration: duration(output),
        mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(input)).unwrap(),
        pitch: PitchPolicy::Preserve,
    };
    node
}
fn document(
    name: &str,
    children: &[&str],
    nodes: Vec<(&str, BeatNode)>,
) -> (Value, Result<ProjectDocument, PlanError>) {
    let base = ProjectDocument::new(
        ProjectId::new(name).unwrap(),
        RevisionId::new("provider-before").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(48_000, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut nodes: BTreeMap<_, _> = nodes
        .into_iter()
        .map(|(name, node)| (id(name), node))
        .collect();
    nodes.insert(
        id("root"),
        BeatNode::sequence("Root", children.iter().map(|name| id(name)).collect()),
    );
    let assets = BTreeMap::from(["original", "sound"].map(|name| {
        (
            AssetId::new(name).unwrap(),
            AssetRecord {
                label: name.into(),
                content_hash: if name == "sound" {
                    "b".repeat(64)
                } else {
                    "a".repeat(64)
                },
                video: None,
                audio: Some(source_audio(name).span),
                still_image: name == "original",
                frame_count: None,
                source_qualification: (name == "sound")
                    .then(|| SourceQualificationId::new("c".repeat(64)).unwrap()),
            },
        )
    }));
    let mut wire = serde_json::to_value(base).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["assets"] = serde_json::to_value(assets).unwrap();
    let checked = ProjectDocument::from_json(&wire.to_string()).map_err(PlanError::from);
    (wire, checked)
}
fn recipe() -> AudioSourceVoiceRecipe {
    AudioSourceVoiceRecipe {
        source: source_audio("sound"),
        mapping: SourceAudioMapping::Duration {
            frames: ExactRatio::integer(100),
        },
        offset: AudioSample(0),
    }
}
fn recipe_snapshot(value: &AudioSourceVoiceRecipe) -> Value {
    let AudioSourceVoiceRecipe {
        source,
        mapping,
        offset,
    } = value;
    json!({"source":source,"mapping":mapping,"offset":offset})
}
fn occurrence_recipe() -> AudioSourceVoiceRecipe {
    let mut source = source_audio("sound");
    source.span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base: SourceTimeBase::new(1, 48_000).unwrap(),
        },
        SourceTimestamp {
            ticks: 4,
            time_base: SourceTimeBase::new(1, 48_000).unwrap(),
        },
    )
    .unwrap();
    AudioSourceVoiceRecipe {
        source,
        mapping: SourceAudioMapping::Duration {
            frames: ExactRatio::integer(4),
        },
        offset: AudioSample(0),
    }
}
fn checked_case(
    name: &str,
    setup: (Value, Result<ProjectDocument, PlanError>),
    run: impl FnOnce(&RenderPlan) -> Value,
) -> Value {
    let (wire, doc) = setup;
    let doc = match doc {
        Ok(doc) => doc,
        Err(value) => {
            return json!({"name":name,"document_wire":wire,"setup":"DocumentError","error":error(&value)});
        }
    };
    let plan = match RenderPlan::compile(&doc) {
        Ok(plan) => plan,
        Err(value) => {
            return json!({"name":name,"document_wire":wire,"document":doc,"setup":"CompileError","error":error(&value)});
        }
    };
    let body = run(&plan);
    json!({"name":name,"document_wire":wire,"document":doc,"setup":"Compiled","observations":body})
}
fn stage_from_public_query(plan: &RenderPlan) -> Result<(AudioStage<'_>, Value), PlanError> {
    let full = plan.audio_signal().query(
        SignalSample(0)..plan.audio_signal().sample_count()?,
        Default::default(),
    )?;
    let observed = query(plan, &full);
    full.spans
        .into_iter()
        .find_map(|span| match span.content {
            AudioSignalContent::Stage(stage) => Some(stage),
            _ => None,
        })
        .map(|stage| (stage, observed))
        .ok_or(PlanError::InvalidPlan(
            "checked provider fixture public query exposed no stage",
        ))
}
fn matrix() -> Value {
    let voice = checked_case(
        "qualified_catalog_voice",
        document(
            "provider-before-voice",
            &["original", "silent", "tail"],
            vec![
                ("original", original(4)),
                ("silent", hold(2)),
                ("tail", original(4)),
            ],
        ),
        |plan| {
            let attempted_recipe = recipe_snapshot(&recipe());
            let voice = match plan.audio_signal().source_voice(recipe()) {
                Ok(value) => value,
                Err(value) => {
                    return json!({"attempted_recipe":attempted_recipe,"setup_error":error(&value)});
                }
            };
            let input = voice.input_signal();
            let output = voice.output_signal();
            let mut rows = vec![
                observe_signal(
                    plan,
                    "structural_carrier",
                    &plan.audio_signal(),
                    json!({"public":"RenderPlan::audio_signal"}),
                ),
                observe_signal(
                    plan,
                    "voice_input",
                    &input,
                    json!({"public":"AudioSourceVoice::input_signal"}),
                ),
                observe_signal(
                    plan,
                    "voice_output",
                    &output,
                    json!({"public":"AudioSourceVoice::output_signal"}),
                ),
            ];
            let mut late = Vec::new();
            for rule in [AudioBoundaryRule::PointCeil, AudioBoundaryRule::RoundEven] {
                let grid = input.transform().unwrap().grid(rule).unwrap();
                let samples = grid.boundary(input.support.start).unwrap()
                    ..grid.boundary(input.support.end).unwrap();
                let complete =
                    input.policy_on_grid(samples.clone(), Default::default(), rule, true);
                if let Ok(value) = &complete {
                    let cap = AudioQueryLimits {
                        maximum_work: value.work,
                        ..Default::default()
                    };
                    let out = output.policy_on_grid(samples.clone(), cap, rule, true);
                    late.push(
                        json!({"rule":rule_name(rule),"samples":samples,"limits":limits(cap),
                    "completed_public_input_policy":policy_result(&complete),
                    "output_at_same_current_budget":policy_result(&out)}),
                    );
                } else {
                    late.push(json!({"input_setup":policy_result(&complete)}));
                }
            }
            let selected = AudioSourceVoiceRecipe {
                source: source_audio("sound"),
                mapping: SourceAudioMapping::SelectedPlacement {
                    start: ExactRatio::new(-1, 2).unwrap(),
                    frames: ExactRatio::integer(100),
                    selection: ExactFrameRange::new(ExactRatio::integer(1), ExactRatio::integer(7))
                        .unwrap(),
                },
                offset: AudioSample(-1),
            };
            let selected_recipe = recipe_snapshot(&selected);
            match plan.audio_signal().source_voice(selected) {
                Ok(value) => {
                    let input = value.input_signal();
                    let output = value.output_signal();
                    rows.push(observe_signal(
                        plan,
                        "selected_negative_offset_input",
                        &input,
                        json!({"public":"source_voice(selected).input_signal"}),
                    ));
                    rows.push(observe_signal(
                        plan,
                        "selected_negative_offset_output",
                        &output,
                        json!({"public":"source_voice(selected).output_signal"}),
                    ));
                }
                Err(value) => rows
                    .push(json!({"name":"selected_negative_offset","setup_error":error(&value)})),
            }
            json!({"attempted_recipe":attempted_recipe,"selected_attempted_recipe":selected_recipe,"signals":rows,"late_carrier_budget":late})
        },
    );
    let occurrence = checked_case(
        "occurrence_padding_and_projected",
        document(
            "provider-before-occurrence",
            &["stage"],
            vec![
                ("stage", retime("group", 8, 12)),
                (
                    "group",
                    BeatNode::sequence("Group", vec![id("before"), id("owner"), id("after")]),
                ),
                ("before", hold(4)),
                ("owner", original(4)),
                ("after", hold(4)),
            ],
        ),
        |plan| {
            let attempted_recipe = recipe_snapshot(&occurrence_recipe());
            let value = plan.source_voice_occurrence(
                InstancePath {
                    node: id("owner"),
                    repeats: Vec::new(),
                },
                occurrence_recipe(),
                Default::default(),
            );
            let occurrence = match value {
                Ok(value) => value,
                Err(value) => {
                    return json!({"attempted_recipe":attempted_recipe,"setup_error":error(&value)});
                }
            };
            let projection = match occurrence.processing_projection() {
                Some(value) => value,
                None => {
                    return json!({"attempted_recipe":attempted_recipe,"setup":"NoProjection","instance":occurrence.instance(),"extent":occurrence.extent()});
                }
            };
            let mut rows = Vec::new();
            if let Some(tape) = projection.input_tape() {
                tape.provider_before_visit(|index,signal,samples|{
                rows.push(observe_signal(plan,&format!("projection_input_run_{index}"),signal,
                    json!({"public":"source_voice_occurrence.processing_projection.input_tape","run_index":index,"run_samples":samples})));
            });
            }
            projection.output_policy().provider_before_visit(|index,signal,samples|{
            rows.push(observe_signal(plan,&format!("neutral_policy_run_{index}"),signal,
                json!({"public":"source_voice_occurrence.processing_projection.output_policy","run_index":index,"run_samples":samples})));
        });
            let output = ExactRatio::ZERO..ExactRatio::integer(projection.output_frames());
            let tape = crate::AudioSignalTape::new(
                plan,
                output.clone(),
                vec![crate::AudioSignalTapeRun::intrinsic(
                    output.clone(),
                    output,
                    Arc::clone(projection),
                )],
            );
            let intrinsic = match tape {
                Ok(tape) => {
                    let snapshot = tape.provider_before_snapshot(plan);
                    tape.provider_before_visit(|index,signal,samples|{
                    rows.push(observe_signal(plan,&format!("intrinsic_projected_run_{index}"),signal,
                        json!({"public":"AudioSignalTape::new with AudioSignalTapeRun::intrinsic","run_index":index,"run_samples":samples})));
                });
                    json!({"result":"Ok","tape":snapshot})
                }
                Err(value) => json!({"result":"Error","error":error(&value)}),
            };
            json!({"attempted_recipe":attempted_recipe,"instance":occurrence.instance(),"extent":occurrence.extent(),"samples":occurrence.samples(),
            "construction_work":occurrence.construction_work(),"projection":projection.provider_before_snapshot(plan),
            "intrinsic_setup":intrinsic,"signals":rows})
        },
    );
    let owned_doc = {
        let (mut wire, _) = document(
            "provider-before-owned",
            &["stage"],
            vec![
                ("stage", retime("group", 2, 4)),
                (
                    "group",
                    BeatNode::sequence("Group", vec![id("left"), id("pause"), id("right")]),
                ),
                ("left", hold(4)),
                ("pause", hold(2)),
                ("right", hold(4)),
            ],
        );
        let range = |a, b| ExactFrameRange {
            start: ExactRatio::integer(a),
            end: ExactRatio::integer(b),
        };
        let run = |destination, source, name: &str| PreserveInputRun {
            destination,
            source,
            provider: PreserveInputProvider::Current {
                target: InstancePath {
                    node: id(name),
                    repeats: Vec::new(),
                },
            },
        };
        wire["derived_clocks"] = serde_json::to_value(BTreeMap::from([(
            id("stage"),
            DerivedRetimeClock {
                extent: ExactRatio::integer(10),
                child_start: ExactRatio::ZERO,
                child_end: ExactRatio::integer(10),
                preserve_input: Some(
                    PreserveInputProjection::new(vec![
                        run(range(0, 2), range(0, 2), "left"),
                        run(range(2, 4), range(0, 2), "right"),
                    ])
                    .unwrap(),
                ),
            },
        )]))
        .unwrap();
        let checked = ProjectDocument::from_json(&wire.to_string()).map_err(PlanError::from);
        (wire, checked)
    };
    let alternate_owned_wire = owned_doc.0.clone();
    let alternate_owned_doc =
        ProjectDocument::from_json(&alternate_owned_wire.to_string()).map_err(PlanError::from);
    let owned = checked_case(
        "owned_stage_input",
        owned_doc,
        |plan| match stage_from_public_query(plan) {
            Ok((stage, setup_query)) => {
                let input = stage.input_signal();
                json!({"stage_setup_query":setup_query,"stage":stage.point_content_old_snapshot(plan),
                    "signals":[observe_signal(plan,"owned_input",&input,json!({"public":"AudioStage::input_signal"}))]})
            }
            Err(value) => json!({"setup_error":error(&value)}),
        },
    );
    let owned_projection = checked_case(
        "owned_stage_input_via_public_projection",
        (alternate_owned_wire, alternate_owned_doc),
        |plan| {
            let selector = crate::AudioDefinitionSelector::Node { node: id("stage") };
            match plan.owned_preserve_projection(selector.clone()) {
                Ok(projection) => {
                    let input = projection.stage().input_signal();
                    json!({"public":"RenderPlan::owned_preserve_projection(selector).stage().input_signal",
                        "selector":selector,"projection":projection.provider_before_snapshot(plan),
                        "signals":[observe_signal(plan,"owned_input_via_projection",&input,
                            json!({"public":"AudioStageProjection::stage().input_signal","selector":selector}))]})
                }
                Err(value) => {
                    json!({"public":"RenderPlan::owned_preserve_projection","selector":selector,
                    "setup_error":error(&value)})
                }
            }
        },
    );
    json!({"schema":SCHEMA,"cases":[voice,occurrence,owned,owned_projection],
        "scope":{"ordinary":"actual checked public providers; borrowed current query and policy snapshots",
            "entry":"private ordinary query_inner false,false and policy_on_grid on exact actual signal; no new admission",
            "inventory":"complete returned AudioContent values; no native provider, recording ledger or allocator ownership claim",
            "identity":"all logical projection token fields serialized; process addresses/Arc strong counts are not semantic before values",
            "snapshot_limit":"a reached Mix input aborts publication until an exhaustive borrowed serializer is supplied",
            "errors":"ordinary errors expose no partial accepted-work counter; failed public setups retained"}})
}
fn strip_envelope(mut value: Value) -> Value {
    for key in ["variant", "source_sha256", "published_checkpoint"] {
        value.as_object_mut().unwrap().remove(key);
    }
    value
}
#[test]
#[ignore = "manual ordinary BEFORE provider-child producer; source-pinned fresh /tmp output"]
fn capture_ordinary_provider_child_before_v1() {
    let output =
        std::env::var("DEADPAN_PROVIDER_CHILD_BEFORE_OUTPUT").expect("explicit fresh output");
    let path = Path::new(&output);
    assert!(path.is_absolute());
    assert!(
        path.parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .starts_with(Path::new("/tmp").canonicalize().unwrap())
    );
    let variant = std::env::var("DEADPAN_PROVIDER_CHILD_BEFORE_VARIANT").expect("actual variant");
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source = std::env::var("DEADPAN_PROVIDER_CHILD_BEFORE_SOURCE_SHA256")
        .expect("actual source manifest digest");
    assert!(source.len() == 64 && source.bytes().all(|b| b.is_ascii_hexdigit()));
    let checkpoint = std::env::var("DEADPAN_PROVIDER_CHILD_BEFORE_PUBLISHED_CHECKPOINT")
        .expect("published G checkpoint");
    assert_eq!(checkpoint, CHECKPOINT);
    let mut body = matrix();
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
#[test]
#[ignore = "manual whole-body provider-child BEFORE regression; independent captured expected path"]
fn ordinary_provider_child_matches_external_before_v1() {
    let expected =
        std::env::var("DEADPAN_PROVIDER_CHILD_BEFORE_EXPECTED").expect("independent captured body");
    let expected: Value = serde_json::from_slice(&std::fs::read(expected).unwrap()).unwrap();
    assert_eq!(matrix(), strip_envelope(expected));
}

#[path = "audio_tape_policy_before_bridge.rs"]
mod tape_policy_before_bridge;
