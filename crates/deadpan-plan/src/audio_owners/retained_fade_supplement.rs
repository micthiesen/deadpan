//! Additive OLD owning-lane witness. Frozen retained v1 remains unchanged.
use super::*;
use deadpan_core::*;
use snapshot::wide_baseline::Parent as FadeParent;

fn mark_lanes(value: &mut Value) {
    let Some(handles) = value.get_mut("handles").and_then(Value::as_array_mut) else {
        return;
    };
    for handle in handles {
        let Some(input) = handle.get("input").filter(|v| !v.is_null()) else {
            continue;
        };
        let retained = &input["retained"];
        let processing = &retained["processing"];
        let bound = &processing["content"]["value"];
        let lattice = &bound["resolved"]["lattice"];
        let editorial = &retained["editorial"];
        let count = |v: &Value| {
            v.as_array()
                .expect("actual exhaustive snapshot inventory")
                .len()
        };
        let present = |v: &Value| !v.is_null();
        let constraint_gaps = bound["constraints"]
            .as_array()
            .unwrap()
            .iter()
            .chain(editorial["constraints"].as_array().unwrap().iter())
            .filter(|v| present(&v["gap_after"]))
            .count();
        handle["owning_lanes"] = json!({
            "processing_retimes":count(&processing["retimes"]),
            "processing_repeats":count(&processing["instance"]["repeats"]),
            "bound_repeats":count(&bound["instance"]["repeats"]),
            "lattice_repeats":count(&lattice["instance"]["repeats"]),
            "editorial":count(&editorial["editorial"]),
            "editorial_repeats":count(&editorial["repeats"]),
            "processing_definition":present(&processing["definition"]),
            "bound_definition":present(&bound["definition"]),
            "processing_gap_after":present(&processing["gap_after"]),
            "bound_gap":present(&bound["gap"]),
            "lattice_gap_after":present(&lattice["gap_after"]),
            "constraint_gap_identities":constraint_gaps,
            "clock":lattice["clock"]["type"],
            "resolved_work":bound["resolved"]["work"],
            "lattice_work":lattice["work"],
            "raw_domain":retained["raw_domain"]["tag"]
        });
    }
}

fn selected(
    parent: FadeParent<'_, '_>,
    descriptor: Value,
    node: &str,
    kind: crate::AudioOwnerKind,
    at: i64,
    windows: &[(&str, i64, i64)],
) -> Value {
    let limits = AudioQueryLimits::default();
    let samples = AudioSample(at)..AudioSample(at.checked_add(1).unwrap());
    let query = match parent {
        FadeParent::Root(plan) => plan.audio_gain_owners(samples, limits),
        FadeParent::Domain(domain) => domain.gain_owners(samples, limits),
    };
    let plan = match parent {
        FadeParent::Root(plan) => plan,
        FadeParent::Domain(domain) => domain.plan,
    };
    let selector = json!({"node":node,"kind":match kind {crate::AudioOwnerKind::Node=>"Node",crate::AudioOwnerKind::DefaultGap=>"DefaultGap"},"observed_at":at});
    let query = match query {
        Err(error) => {
            return json!({"fixture":descriptor,"selector":selector,"setup_result":"OwnerQueryError","error":snapshot::error_json(&error)});
        }
        Ok(query) => query,
    };
    let Some(owner) = query
        .spans()
        .iter()
        .flat_map(|span| span.owners())
        .find(|owner| owner.instance().node.as_str() == node && owner.kind() == kind)
    else {
        return json!({"fixture":descriptor,"selector":selector,"setup_result":"OwnerAbsent",
            "owner_query":snapshot::wide_baseline::query_snapshot(&query,plan)});
    };
    let input = match parent {
        FadeParent::Root(plan) => plan.audio_owner_input(owner, AudioSample(at), limits),
        FadeParent::Domain(domain) => domain.owner_input(owner, AudioSample(at), limits),
    };
    match input {
        Err(error) => {
            json!({"fixture":descriptor,"selector":selector,"setup_result":"InputError","error":snapshot::error_json(&error),
            "owner_query":snapshot::wide_baseline::query_snapshot(&query,plan)})
        }
        Ok(input) => {
            let mut value = one(&descriptor, &input, windows);
            value["selector"] = selector;
            value["setup_result"] = json!("Ok");
            mark_lanes(&mut value);
            value
        }
    }
}

fn edit(document: &ProjectDocument, revision: &str, command: Command) -> ProjectDocument {
    apply(
        document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new(revision).unwrap(),
            command,
        },
    )
    .unwrap()
    .forward
    .apply(document)
    .unwrap()
}

fn captured_marked_fragment(descriptor: &Value) -> Value {
    let before =
        ProjectDocument::from_json(&descriptor["fixture_source"]["document"].to_string()).unwrap();
    let timing = AudioTimingId {
        allocation: RevisionId::new("retained-supplement-capture").unwrap(),
        ordinal: 0,
    };
    let mut source = json!({
        "name":"retained_supplement/captured_marked_repeat_fragment",
        "source":"exact checked fade/repeat_fragment/marked plus public capture_unbound_audio_bindings and checked installation",
        "parent":descriptor,
        "before_document":before,
        "capture_timing":timing,
        "selection":{"node":"voice","kind":"Node","observed_at":6400}
    });
    let bindings = match capture_unbound_audio_bindings(&before, timing) {
        Ok(value) => value,
        Err(error) => {
            source["setup_result"] = json!("CaptureError");
            source["error"] = snapshot::error_json(&PlanError::from(error));
            return source;
        }
    };
    let mut wire = serde_json::to_value(&before).unwrap();
    wire["audio_bindings"] = serde_json::to_value(&bindings).unwrap();
    source["captured_bindings"] = serde_json::to_value(&bindings).unwrap();
    let captured = match ProjectDocument::from_json(&wire.to_string()) {
        Ok(value) => value,
        Err(error) => {
            source["setup_result"] = json!("CheckedInstallationError");
            source["error"] = snapshot::error_json(&PlanError::from(error));
            return source;
        }
    };
    source["document"] = serde_json::to_value(&captured).unwrap();
    let plan = match RenderPlan::compile(&captured) {
        Ok(value) => value,
        Err(error) => {
            source["setup_result"] = json!("CompileError");
            source["error"] = snapshot::error_json(&error);
            return source;
        }
    };
    selected(
        FadeParent::Root(&plan),
        source,
        "voice",
        crate::AudioOwnerKind::Node,
        6400,
        &[
            ("second_marked_play", 6400, 12800),
            ("repeat_seam", 6399, 6401),
        ],
    )
}

fn cases() -> Vec<Value> {
    let mut output = Vec::new();
    fade_query_baseline::cases::visit_cases(|parent, descriptor| {
        let Some(name) = descriptor["name"].as_str() else {
            return;
        };
        if name == "fade/repeat_fragment/marked" {
            output.push(captured_marked_fragment(descriptor));
            return;
        }
        let Some((node, kind, at, windows)) = (match name {
            "fade/signed/retained_negative_origin" => Some((
                "voice",
                crate::AudioOwnerKind::Node,
                2,
                &[("retime_complete", -2, 6)][..],
            )),
            "fade/editorial/entry_after" => Some((
                "voice",
                crate::AudioOwnerKind::Node,
                2,
                &[("incident", 2, 3), ("next", 3, 4)][..],
            )),
            "fade/point/definition_birth" => Some((
                "voice",
                crate::AudioOwnerKind::Node,
                0,
                &[("definition_support", -2, 6)][..],
            )),
            "fade/gap_bound/root" => Some((
                "child",
                crate::AudioOwnerKind::Node,
                8008,
                &[("second_play", 8008, 9610), ("repeat_seam", 8007, 8009)][..],
            )),
            _ => None,
        }) else {
            return;
        };
        let source = json!({"name":format!("retained_supplement/{name}"),"source":"exact existing checked fade visitor","parent":descriptor});
        output.push(selected(parent, source, node, kind, at, windows));
        if name == "fade/gap_bound/root" {
            output.push(selected(parent,json!({"name":"retained_supplement/gap_occurrence","source":"exact existing checked fade visitor","parent":descriptor}),
                "repeat",crate::AudioOwnerKind::DefaultGap,4805,&[("gap",4805,8008),("gap_entry",4804,4806)]));
        }
    });
    definition_gap::visit(&mut output);
    output
}

fn coverage(cases: &[Value]) -> Value {
    let required = [
        "processing_retimes",
        "processing_repeats",
        "bound_repeats",
        "lattice_repeats",
        "editorial",
        "editorial_repeats",
        "processing_definition",
        "bound_definition",
        "processing_gap_after",
        "bound_gap",
        "lattice_gap_after",
        "constraint_gap_identities",
        "clock_definition_point_ceil",
        "clock_gap_definition_point_ceil",
    ];
    let mut counts = std::collections::BTreeMap::from(required.map(|name| (name, 0usize)));
    let mut modes = std::collections::BTreeMap::<String, usize>::new();
    for case in cases {
        let Some(handles) = case["handles"].as_array() else {
            continue;
        };
        for handle in handles {
            *modes
                .entry(format!(
                    "{}/{}",
                    handle["mode"].as_str().unwrap(),
                    handle["result"].as_str().unwrap()
                ))
                .or_default() += 1;
            let Some(lanes) = handle.get("owning_lanes") else {
                continue;
            };
            for (name, count) in &mut counts {
                let reached = match *name {
                    "clock_definition_point_ceil" => lanes["clock"] == "definition_point_ceil",
                    "clock_gap_definition_point_ceil" => {
                        lanes["clock"] == "gap_definition_point_ceil"
                    }
                    _ => lanes[*name]
                        .as_bool()
                        .unwrap_or_else(|| lanes[*name].as_u64().unwrap() > 0),
                };
                if reached {
                    *count += 1
                }
            }
        }
    }
    json!({"owning_lanes_actual_some_mode_counts":counts,
        "every_required_lane_reached":counts.values().all(|count|*count>0),
        "actual_handle_results":modes,
        "interpretation":"Only actual Some metadata counts; None, rejected selection and setup errors remain literal. A false predicate leaves a coverage gap; no case is fabricated."})
}

pub(in crate::plan) fn supplement_matrix() -> Value {
    let cases = cases();
    json!({"schema":"old-retained-fade-owning-lanes-supplement-v1",
        "scope":"Additive actual checked constructors; retained v1 is unchanged",
        "coverage":coverage(&cases),"cases":cases})
}

#[test]
#[ignore = "manual additive OLD producer; explicit create_new /tmp output, before retained implementation"]
fn capture_old_retained_fade_supplement_v1() {
    let target = std::env::var("DEADPAN_RETAINED_FADE_SUPPLEMENT_OUTPUT")
        .expect("explicit new supplement output required");
    let path = Path::new(&target);
    assert!(path.is_absolute());
    assert!(
        path.parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .starts_with(Path::new("/tmp").canonicalize().unwrap())
    );
    let variant =
        std::env::var("DEADPAN_RETAINED_FADE_VARIANT").expect("explicit variant required");
    assert!(matches!(
        variant.as_str(),
        "main-inherited" | "scoped-published"
    ));
    let source = std::env::var("DEADPAN_RETAINED_FADE_SOURCE_SHA256")
        .expect("complete actual OLD manifest identity required");
    assert!(source.len() == 64 && source.bytes().all(|v| v.is_ascii_hexdigit()));
    let checkpoint = std::env::var("DEADPAN_RETAINED_FADE_PUBLISHED_CHECKPOINT")
        .expect("published fade checkpoint identity");
    assert!(!checkpoint.trim().is_empty());
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap();
    let mut value = supplement_matrix();
    value["variant"] = json!(variant);
    value["source_sha256"] = json!(source);
    value["published_fade_checkpoint"] = json!(checkpoint);
    serde_json::to_writer_pretty(&mut file, &value).unwrap();
    file.write_all(b"\n").unwrap();
    file.sync_all().unwrap();
}

#[test]
#[ignore = "manual additive OLD external regression; no expected body supplied"]
fn ordinary_retained_fade_supplement_matches_external_old_v1() {
    let path = std::env::var("DEADPAN_RETAINED_FADE_SUPPLEMENT_EXPECTED")
        .expect("independent complete OLD supplement required");
    let expected: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(supplement_matrix(), body(expected));
}

#[test]
fn ordinary_retained_fade_supplement_matches_frozen_old_v1() {
    let expected: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/retained-fade-query-supplement-v1/snapshot.json"
    ))
    .unwrap();
    assert_eq!(supplement_matrix(), expected);
}

#[test]
#[ignore = "manual coverage diagnostic; actual missing lanes remain an explicit failed proof"]
fn actual_retained_supplement_reaches_owning_lanes() {
    let value = supplement_matrix();
    assert_eq!(
        value["coverage"]["every_required_lane_reached"],
        json!(true),
        "{}",
        value["coverage"]
    );
}

mod definition_gap {
    use super::*;
    fn id(value: &str) -> NodeId {
        NodeId::new(value).unwrap()
    }
    fn frames(value: i64) -> FrameDuration {
        FrameDuration::new(value).unwrap()
    }
    fn ratio(n: i128, d: i128) -> ExactRatio {
        ExactRatio::new(n, d).unwrap()
    }
    fn recipe(duration: i64) -> HoldRecipe {
        HoldRecipe {
            duration: frames(duration),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
            picture_context: None,
        }
    }
    fn play(ordinal: u32) -> IterationId {
        IterationId {
            allocation: RevisionId::new("plays").unwrap(),
            ordinal,
        }
    }
    fn captured(plays: u32) -> ProjectDocument {
        let document = ProjectDocument::new(
            ProjectId::new("bound-gaps").unwrap(),
            RevisionId::new("initial").unwrap(),
            PresentationBasis {
                width: 16,
                height: 16,
                frame_rate: FrameRate::new(30_000, 1001).unwrap(),
                color_policy: ColorPolicy::SdrRec709,
            },
            id("root"),
        )
        .unwrap();
        let mut wire = serde_json::to_value(document).unwrap();
        for (name, node) in [
            (
                "root",
                BeatNode::sequence("Root", vec![id("prefix"), id("repeat")]),
            ),
            ("prefix", BeatNode::hold("Prefix", recipe(1))),
            ("child", BeatNode::hold("Child", recipe(1))),
            (
                "repeat",
                BeatNode {
                    label: "Repeat".into(),
                    framing: None,
                    audio_treatments: Default::default(),
                    audio_editorial_edges: Default::default(),
                    audio_edges: AudioEdgePolicies {
                        node_start: AudioEdgePolicy::Hard,
                        node_end: AudioEdgePolicy::Hard,
                        repeat_gap_start: AudioEdgePolicy::Hard,
                        ..Default::default()
                    },
                    kind: NodeKind::Repeat {
                        child: id("child"),
                        iterations: IterationOrder::new(play(0).allocation, plays).unwrap(),
                        gap: Some(recipe(2)),
                        escalation: None,
                    },
                    cutaways: Vec::new(),
                    captions: Vec::new(),
                },
            ),
        ] {
            wire["nodes"][name] = serde_json::to_value(node).unwrap();
        }
        let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
        wire["audio_bindings"] = serde_json::to_value(
            capture_unbound_audio_bindings(
                &document,
                AudioTimingId {
                    allocation: RevisionId::new("capture").unwrap(),
                    ordinal: 0,
                },
            )
            .unwrap(),
        )
        .unwrap();
        ProjectDocument::from_json(&wire.to_string()).unwrap()
    }

    pub(super) fn visit(output: &mut Vec<Value>) {
        let original = captured(1);
        let command = Command::SetRepeat {
            node: id("repeat"),
            plays: 1,
            gap: Some(recipe(3)),
        };
        let command_value = serde_json::to_value(&command).unwrap();
        let changed = super::edit(&original, "extend", command);
        let plan = RenderPlan::compile(&changed).unwrap();
        let selector = crate::AudioDefinitionSelector::RepeatGap {
            repeat: id("repeat"),
        };
        let definition = plan.audio_definition(selector.clone()).unwrap();
        let placement =
            crate::AudioRootPlacement::new(ratio(-1, 3), ratio(3, 2), ratio(1, 4)..ratio(7, 4))
                .unwrap();
        let placed = definition.in_root_clock(placement.clone()).unwrap();
        let at = placed.root_samples().start.0;
        output.push(selected(FadeParent::Domain(&placed),json!({
            "name":"retained_supplement/one_play_gap_definition",
            "source":{"path":"crates/deadpan-plan/tests/gap_bindings.rs","symbol":"one_play_gap_definition_uses_its_own_point_clock_and_current_duration"},
            "original_document":original,"command":command_value,"document":changed,
            "selector":selector,"placement":placement,"actual_domain_samples":samples_value(&placed.root_samples())
        }),"repeat",crate::AudioOwnerKind::DefaultGap,at,&[("explicit_definition_history",at-1,at+2)]));
    }
}
