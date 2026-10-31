//! Actual dense Bound inventory and reached sealed policy fallback witnesses.
use super::*;

const DENSE_SPANS: usize = 256;

fn dense_documents() -> (ProjectDocument, ProjectDocument) {
    // Port the checked integration fixture without changing its timing or binding capture.
    let node = |name: &str| NodeId::new(name).unwrap();
    let initial = ProjectDocument::new(
        ProjectId::new("dense-policy").unwrap(),
        RevisionId::new("initial").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(48_000, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    let mut wire = serde_json::to_value(initial).unwrap();
    wire["nodes"]["root"] =
        serde_json::to_value(BeatNode::sequence("Root", vec![node("repeat")])).unwrap();
    wire["nodes"]["voice"] = serde_json::to_value(BeatNode::hold(
        "Silence",
        HoldRecipe {
            duration: FrameDuration::new(1).unwrap(),
            video: HoldVideo::Background,
            picture_context: None,
            audio: HoldAudio::Silence,
        },
    ))
    .unwrap();
    wire["nodes"]["repeat"] = serde_json::to_value(BeatNode {
        label: "Dense repeat".into(),
        framing: None,
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Repeat {
            child: node("voice"),
            iterations: IterationOrder::new(RevisionId::new("plays").unwrap(), 256).unwrap(),
            gap: None,
            escalation: None,
        },
        cutaways: Vec::new(),
        captions: Vec::new(),
    })
    .unwrap();
    let unbound = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let bindings = capture_unbound_audio_bindings(
        &unbound,
        AudioTimingId {
            allocation: RevisionId::new("capture").unwrap(),
            ordinal: 0,
        },
    )
    .unwrap();
    wire["audio_bindings"] = serde_json::to_value(bindings).unwrap();
    let bound = ProjectDocument::from_json(&wire.to_string()).unwrap();
    (unbound, bound)
}

fn dense_limits() -> AudioQueryLimits {
    AudioQueryLimits {
        maximum_spans: DENSE_SPANS,
        maximum_work: 65_536,
    }
}

fn descriptor(name: &str, parent: Parent<'_, '_>, source: Value) -> Value {
    let (samples, identity) = match parent {
        Parent::Root(plan) => (
            AudioSample(0)..plan.audio_duration().unwrap(),
            json!({"tag":"ImmutablePlanRoot","project_id":plan.metadata.project_id.as_str(),
                "revision_id":plan.metadata.revision_id.as_str(),"root":plan.nodes[plan.root].inspection.id.as_str()}),
        ),
        Parent::Domain(physical) => (
            physical.root_samples(),
            json!({"tag":"ActualCheckedPhysicalDomain","physical_domain":super::super::domain(physical)}),
        ),
    };
    json!({"name":name,"parent_kind":match parent {Parent::Root(_)=>"Root",Parent::Domain(_)=>"Domain"},
        "window":sample_range(samples,|sample|sample.0),"parent_identity":identity,"fixture_source":source})
}

fn visit_dense(mut visit: impl FnMut(Parent<'_, '_>, &Value)) {
    let (unbound_document, bound_document) = dense_documents();
    let mut source = json!({
        "kind":"exact_ported_checked_test_fixture",
        "path":"tests/audio_policy.rs",
        "symbol":"dense_bound_repeat_inventory_uses_shared_work_not_output_span_capacity",
        "original_fixture_source":include_str!("../../../tests/audio_policy.rs"),
        "unbound_document":unbound_document,"document":bound_document,
        "binding_capture":{"api":"capture_unbound_audio_bindings","timing":{"allocation":"capture","ordinal":0}},
    });
    let unbound = RenderPlan::compile(&unbound_document).unwrap();
    let bound = RenderPlan::compile(&bound_document).unwrap();
    for (name, plan, selected_document) in [
        (
            "dense/actual_256_repeat_unbound/root",
            &unbound,
            &unbound_document,
        ),
        (
            "dense/actual_256_repeat_bound/root",
            &bound,
            &bound_document,
        ),
    ] {
        let parent = Parent::Root(plan);
        let mut fixture = source.clone();
        fixture["selected_document"] = json!(selected_document);
        fixture["setup"] = json!({"api":"RenderPlan::compile","selector":"immutable_plan_root"});
        visit(parent, &descriptor(name, parent, fixture));
    }
    let at = AudioSample(0);
    let owners = bound
        .audio_gain_owners(at..AudioSample(1), Default::default())
        .unwrap();
    let root = owners.spans()[0]
        .owners()
        .iter()
        .find(|owner| {
            owner.kind() == AudioOwnerKind::Node && owner.instance().node.as_str() == "root"
        })
        .unwrap();
    let selected = bound
        .audio_owner_input(root, at, Default::default())
        .unwrap();
    let parent = Parent::Domain(selected.context());
    source["selected_document"] = json!(bound_document);
    source["setup"] = json!({"owner_query":{"api":"RenderPlan::audio_gain_owners","samples":{"start":0,"end":1},
        "limits":limits_snapshot(Default::default())},"selector":{"node":"root","kind":"Node"},
        "capture":{"api":"RenderPlan::audio_owner_input","observed_at":0,"limits":limits_snapshot(Default::default())},
        "setup_input":super::super::input(&selected)});
    visit(
        parent,
        &descriptor("dense/actual_256_repeat_bound/domain", parent, source),
    );
}

fn visit_fallback(mut visit: impl FnMut(Parent<'_, '_>, &Value)) {
    let mut count = 0;
    processing_query_baseline::visit_cases(|parent, source| {
        if !matches!(parent, Parent::Domain(_)) {
            return;
        }
        let name = source["name"].as_str().unwrap();
        if !matches!(
            name,
            "additional/exposed_preserve_and_silent_hold/domain"
                | "additional/shift_at_unity_and_silent_hold/domain"
        ) {
            return;
        }
        let mut descriptor = source.clone();
        descriptor["name"] = json!(format!("fresh_span_fallback/{name}"));
        descriptor["reused_fixture"] = json!({"source":"processing_query_baseline::visit_cases",
            "old_sealed_tests":"audio_domain_policy_reuse_tests::fresh_span_limits_still_apply_after_a_wider_first_factory",
            "old_case_name":name});
        visit(parent, &descriptor);
        count += 1;
    });
    assert_eq!(count, 2);
}

pub(in crate::plan) fn visit_cases(mut visit: impl FnMut(Parent<'_, '_>, &Value)) {
    let mut count = 0;
    visit_dense(|parent, descriptor| {
        visit(parent, descriptor);
        count += 1;
    });
    visit_fallback(|parent, descriptor| {
        visit(parent, descriptor);
        count += 1;
    });
    assert_eq!(count, 5);
}

fn dense_case(parent: Parent<'_, '_>, descriptor: &Value) -> Value {
    let samples = window(&descriptor["window"]);
    let limits = dense_limits();
    let full = ordinary_call(parent, samples.clone(), limits);
    let work = full.policy.as_ref().map(|_| full.work());
    let mut observations =
        vec![full.observation("explicit_256_span_full", samples.clone(), limits)];
    for (name, range, cap) in [
        ("explicit_256_span_clipped", clipped(&samples), DENSE_SPANS),
        (
            "explicit_256_span_empty",
            samples.start..samples.start,
            DENSE_SPANS,
        ),
        ("one_short_255_span_full", samples.clone(), DENSE_SPANS - 1),
    ] {
        let trial = AudioQueryLimits {
            maximum_spans: cap,
            ..limits
        };
        observations
            .push(ordinary_call(parent, range.clone(), trial).observation(name, range, trial));
    }
    if let Some(work) = work {
        for (name, maximum_work) in [
            ("actual_returned_work_limit", work),
            (
                "one_short_actual_returned_work_limit",
                work.checked_sub(1).unwrap(),
            ),
            ("original_fixture_small_work_refusal", 16),
        ] {
            let trial = AudioQueryLimits {
                maximum_work,
                ..limits
            };
            observations.push(ordinary_call(parent, samples.clone(), trial).observation(
                name,
                samples.clone(),
                trial,
            ));
        }
    }
    let mut value = descriptor.clone();
    value["route"] = json!("ordinary_policy_explicit_dense_limits");
    value["observations"] = json!(observations);
    value
}

fn fallback_case(physical: &AudioDomain<'_>, descriptor: &Value) -> Value {
    let samples = window(&descriptor["window"]);
    let defaults = AudioQueryLimits::default();
    let current = physical.audio(samples.clone(), defaults).unwrap();
    let cap = current.spans.len();
    assert_eq!(
        cap, 2,
        "these exact checked Stage fixtures have two current spans"
    );
    let fresh = AudioQueryLimits {
        maximum_spans: cap,
        ..defaults
    };
    let ordinary = ordinary_call(Parent::Domain(physical), samples.clone(), fresh);
    let sealed = sealed_call(physical, samples.clone(), physical, samples.clone(), fresh);
    assert!(ordinary.succeeded() && sealed.succeeded());
    assert_eq!(
        ordinary.policy, sealed.policy,
        "complete ordinary fallback policy equality"
    );
    let first_spans = sealed.output["first_processing"]["query"]["spans"]
        .as_array()
        .unwrap();
    assert_eq!(first_spans.len(), 2);
    assert!(
        first_spans
            .iter()
            .any(|span| span["content"]["tag"] == "Stage")
    );
    let depth_zero = |phase: &str| {
        sealed
            .second_queries
            .iter()
            .filter(|event| event["depth"] == 0 && event["phase"] == phase)
            .collect::<Vec<_>>()
    };
    let returned = depth_zero("actual_second_query_returned");
    let accounted = depth_zero("actual_second_query_accounted");
    assert_eq!(
        returned.len(),
        1,
        "must observe the actual second walk return"
    );
    assert_eq!(accounted.len(), 1, "must observe its actual accounting");
    assert_eq!(returned[0]["query"]["result"], "Ok");
    assert_eq!(accounted[0]["accounting"]["result"], "Ok");
    assert_eq!(
        returned[0]["query"]["query"]["spans"]
            .as_array()
            .unwrap()
            .len(),
        cap
    );

    let short = AudioQueryLimits {
        maximum_spans: cap - 1,
        ..defaults
    };
    let current_short = physical.audio(samples.clone(), short);
    assert!(matches!(
        &current_short,
        Err(PlanError::AudioQueryLimit("span count"))
    ));
    let current_short_snapshot = match current_short {
        Ok(_) => unreachable!(),
        Err(error) => json!({"result":"Error","error":error_json(&error)}),
    };
    let short_finish = sealed_call(physical, samples.clone(), physical, samples.clone(), short);
    assert_eq!(
        short_finish.output["finish"]["error"],
        json!({"variant":"AudioQueryLimit","budget":"span count"})
    );
    let policy_work = sealed.work();
    let mut observations = vec![
        ordinary.observation("ordinary_fresh_span_two", samples.clone(), fresh),
        sealed.observation(
            "sealed_fresh_span_two_reached_second_walk",
            samples.clone(),
            fresh,
        ),
        short_finish.observation("sealed_fresh_span_one_refusal", samples.clone(), short),
    ];
    for (name, maximum_work) in [
        ("sealed_exact_actual_policy_work", policy_work),
        (
            "sealed_one_short_actual_policy_work",
            policy_work.checked_sub(1).unwrap(),
        ),
    ] {
        let trial = AudioQueryLimits {
            maximum_work,
            ..fresh
        };
        observations.push(
            sealed_call(physical, samples.clone(), physical, samples.clone(), trial).observation(
                name,
                samples.clone(),
                trial,
            ),
        );
    }
    let mut value = descriptor.clone();
    value["route"] = json!("actual_stage_witness_fallback_with_fresh_span_limits");
    value["independent_current_query"] = json!({"request":{"samples":samples,"limits":limits_snapshot(defaults)},
        "output":{"result":"Ok","query":flattened_query_baseline::query_snapshot(&current)}});
    value["independent_one_short_current_query"] = json!({"request":{"samples":samples,"limits":limits_snapshot(short)},"output":current_short_snapshot});
    value["observations"] = json!(observations);
    value
}

fn span_length_gate_survey() -> Value {
    let mut cases = Vec::new();
    processing_query_baseline::visit_cases(|parent, descriptor| {
        let Parent::Domain(physical) = parent else {
            return;
        };
        let samples = window(&descriptor["window"]);
        let defaults = AudioQueryLimits::default();
        let first = physical
            .processing_for_policy(samples.clone(), defaults)
            .unwrap();
        let first_count = first.query().spans.len();
        if first_count <= 1 {
            return;
        }
        let first_snapshot =
            processing_query_baseline::query_snapshot(physical.plan, first.query());
        drop(first);
        let fresh = AudioQueryLimits {
            maximum_spans: first_count - 1,
            ..defaults
        };
        let current = physical.audio(samples.clone(), fresh);
        assert!(
            matches!(&current, Err(PlanError::AudioQueryLimit("span count"))),
            "actual wider-first fixture must refuse in its independently called current query: {}",
            descriptor["name"]
        );
        let current_error = match current {
            Ok(_) => unreachable!(),
            Err(error) => error_json(&error),
        };
        let finish = sealed_call(physical, samples.clone(), physical, samples.clone(), fresh);
        assert_eq!(finish.output["first_processing"]["query"], first_snapshot);
        assert_eq!(finish.output["finish"]["error"], current_error);
        let mut case = descriptor.clone();
        case["first_tape_spans"] = json!(first_count);
        case["fresh_span_limit"] = json!(fresh.maximum_spans);
        case["independent_current_query"] = json!({"request":{"samples":samples,"limits":limits_snapshot(fresh)},
            "output":{"result":"Error","error":current_error}});
        case["sealed_finish"] =
            finish.observation("actual_first_tape_over_fresh_cap", samples, fresh);
        cases.push(case);
    });
    assert_eq!(
        cases.len(),
        6,
        "the six nonempty wider-first cases from the existing sealed corpus"
    );
    json!({"cases":cases,
        "bounded_finding":"Every existing wider-first Domain fixture independently refuses its current flattened query at first_tape_spans-1; no cap-length-forced second walk is claimed reached",
        "event_limit":"Second-query events are retained literally; event absence alone does not prove reuse or call non-entry"})
}

pub(super) fn matrix() -> Value {
    let mut cases = Vec::new();
    visit_cases(|parent, descriptor| {
        if descriptor["name"].as_str().unwrap().starts_with("dense/") {
            cases.push(dense_case(parent, descriptor));
        } else {
            let Parent::Domain(physical) = parent else {
                unreachable!()
            };
            cases.push(fallback_case(physical, descriptor));
        }
    });
    json!({"schema":"old-policy-dense-reached-fallback-cases-v1","cases":cases,"density_proof":density_proof(),
        "existing_wider_first_span_gate_survey":span_length_gate_survey(),
        "dense_limits":"Literal existing checked fixture limits, recorded per call; no default success assumption",
        "fallback_cause":"Co-produced exposed Stage witness selects ordinary policy; adequate fresh cap reaches and accounts its second walk",
        "span_length_gate":"One-short cap is below the actual first tape length and independently refuses in the current flattened query; this is not claimed to reach a cap-length-forced second walk"})
}

fn density_proof() -> Value {
    let (unbound, bound) = dense_documents();
    let unbound = RenderPlan::compile(&unbound).unwrap();
    let bound = RenderPlan::compile(&bound).unwrap();
    let samples = AudioSample(0)..AudioSample(256);
    let limits = dense_limits();
    let plain = unbound.audio_policy(samples.clone(), limits).unwrap();
    let retained = bound.audio_policy(samples.clone(), limits).unwrap();
    let processing = bound.audio_processing(samples.clone(), limits).unwrap();
    assert!(
        processing
            .spans
            .iter()
            .any(|span| matches!(&span.content, crate::AudioSignalContent::Bound(_)))
    );
    assert!(retained.contents.len() > limits.maximum_spans);
    assert_eq!(retained.suppressed, plain.suppressed);
    assert_eq!(retained.suppressed, vec![AudioSample(0)..AudioSample(256)]);
    assert!(retained.work > plain.work && retained.work <= limits.maximum_work);
    json!({
        "request":{"samples":samples,"limits":limits_snapshot(limits)},
        "unbound_policy":{"result":"Ok","policy":query_snapshot(&plain)},
        "bound_policy":{"result":"Ok","policy":query_snapshot(&retained)},
        "actual_bound_processing":{"result":"Ok","query":processing_query_baseline::query_snapshot(&bound,&processing)},
        "inventory_exceeds_output_span_cap":true,
    })
}

#[test]
fn dense_bound_inventory_exceeds_actual_output_span_cap() {
    let value = matrix();
    assert_eq!(value["cases"].as_array().unwrap().len(), 5);
    assert_eq!(
        value["density_proof"]["inventory_exceeds_output_span_cap"],
        true
    );
}

#[test]
fn fresh_positive_span_fallback_reaches_and_accounts_second_walk() {
    let mut checked = 0;
    visit_fallback(|parent, descriptor| {
        let Parent::Domain(physical) = parent else {
            unreachable!()
        };
        let _ = fallback_case(physical, descriptor);
        checked += 1;
    });
    assert_eq!(checked, 2);
}
