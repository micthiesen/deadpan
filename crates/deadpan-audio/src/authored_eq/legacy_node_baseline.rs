//! Reusable observations and fixture reconstruction after the independent OLD
//! native oracle was frozen. The immutable golden is never generated here.
//! Public Plan snapshots cannot expose private provider/play fields. The
//! independent Plan direct-input baseline records those private fields.
use super::*;
use serde_json::{Value, json};

fn range(value: Range<AudioSample>) -> Value {
    json!([value.start.0, value.end.0])
}

pub(super) fn clock(plan: &RenderPlan, owner: &AudioOwnerClock<'_>, at: AudioSample) -> Value {
    assert!(owner.belongs_to(plan));
    json!({
        "belongs_to_expected_plan": true,
        "definition": owner.definition(),
        "instance": owner.instance(),
        "kind": format!("{:?}", owner.kind()),
        "gap_after": owner.gap_after(),
        "origin": format!("{:?}", owner.origin()),
        "sampling": {"anchor": owner.sampling().anchor().0,
            "local_at_anchor": owner.sampling().local_at_anchor(),
            "local_frames_per_sample": owner.sampling().local_frames_per_sample()},
        "local_at_observation": format!("{:?}", owner.sampling().local_at(at)),
        "local_frames_per_sample": owner.sampling().local_frames_per_sample(),
        "treatments": owner.treatments(),
        "escalation_millidecibels": owner.escalation_millidecibels(),
        "explicit_tail_provider": owner.is_explicit_tail_provider(),
    })
}

fn domain(plan: &RenderPlan, value: &AudioDomain<'_>) -> Value {
    assert!(value.belongs_to(plan));
    json!({
        "belongs_to_expected_plan": true, "definition": value.definition(),
        "placement": value.placement(), "root_samples": range(value.root_samples()),
        "root_extent": value.root_extent(), "visible_samples": range(value.visible_samples()),
        "instance": value.instance(), "gap_after": value.gap_after(),
    })
}

pub(super) fn context(plan: &RenderPlan, value: &Context<'_>, at: AudioSample) -> Value {
    match value {
        Context::Root => json!({"kind": "Root"}),
        Context::Domain(value) => json!({"kind": "Domain", "value": domain(plan, value)}),
        Context::Retained(value) => {
            assert!(value.belongs_to(plan));
            let raw_domain = match value.domain() {
                deadpan_plan::AudioBoundDomain::Root(value) => {
                    json!({"kind": "Root", "value": domain(plan, value)})
                }
                deadpan_plan::AudioBoundDomain::Point(_) => {
                    json!({"kind": "Point", "private_fields": "Plan oracle only"})
                }
                deadpan_plan::AudioBoundDomain::Empty => json!({"kind": "Empty"}),
            };
            json!({
                "kind": "Retained", "belongs_to_expected_plan": true,
                "bound": value.bound(), "raw_domain": raw_domain,
                "reference_at": value.reference_at(at).unwrap(),
                "reference_step": value.reference_samples_per_output_sample(),
                "retained_lookup_work": value.work(),
            })
        }
    }
}

pub(super) fn measured<'plan>(
    plan: &'plan Arc<RenderPlan>,
    incoming: &Context<'plan>,
    owner: &AudioOwnerClock<'plan>,
    at: AudioSample,
    stage: usize,
    parent: usize,
    owner_limit: Option<u32>,
) -> Value {
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = eq_admission_fixture::control(&cancelled, &work);
    let mut renderer = StageAudio::new(Arc::clone(plan));
    if let Some(limit) = owner_limit {
        renderer.limits.maximum_prepared_stages = limit;
    }
    let mut graph = Graph {
        plan,
        nodes: Vec::new(),
        resident: 0,
        node_capacity_frames: 0,
        skip: &[],
    };
    let program = Program {
        context: incoming,
        stop: None,
        parent,
        depth: 0,
    };
    // Exactly one construction attempt per case. The separate second call below
    // is explicitly a cache-hit lookup and must perform no new canonical work.
    let result = renderer.eq_node(&mut graph, program, owner, at, stage, control);
    let construction_work = work.borrow().plan_work;
    let mut output = json!({
        "actual_eq_node_calls": 1,
        "incoming_context": context(plan, incoming, at),
        "source_owner": clock(plan, owner, at),
        "observation": at.0, "requested_stage": stage, "requested_parent": parent,
        "controller_plan_work": construction_work,
        "native_filter_taps": deadpan_dsp::FINITE_FILTER_TAPS,
        "accounting": {
            "graph_resident": graph.resident, "active_frames": renderer.active_frames,
            "request_metadata": work.borrow().sound_context_frames,
            "native_bound_units": eq_admission_fixture::units(),
            "node_count": graph.nodes.len(),
        }
    });
    match result {
        Err(error) => output["result"] = json!({"error": format!("{error:?}")}),
        Ok(id) => {
            let node = &graph.nodes[id];
            let retained_same_arc = match (incoming, &node.context) {
                (Context::Retained(a), Context::Retained(b)) => Some(a.ptr_eq(b)),
                _ => None,
            };
            if retained_same_arc.is_some() {
                assert_eq!(retained_same_arc, Some(true));
            }
            output["result"] = json!({
                "id": id, "parent": node.parent,
                "context": context(plan, &node.context, at),
                "boundary": {
                    "before_stage": node.boundary.before_stage(),
                    "owner": clock(plan, node.boundary.owner(), at),
                    "matches_source_owner": node.boundary.matches(owner),
                    "matches_source_clock_at": node.boundary.matches_clock_at(owner, at),
                },
                "filter_present": node.filter.is_some(),
                "tiles_empty": node.tiles.is_empty(),
                "incoming_retained_same_arc": retained_same_arc,
            });
            let resident = graph.resident;
            let active = renderer.active_frames;
            let filter_pointer = graph.nodes[id].filter.as_ref().map(std::ptr::from_ref);
            let again = renderer
                .eq_node(&mut graph, program, owner, at, stage, control)
                .unwrap();
            assert_eq!(again, id);
            assert_eq!(graph.resident, resident);
            assert_eq!(renderer.active_frames, active);
            assert_eq!(work.borrow().plan_work, construction_work);
            assert_eq!(
                graph.nodes[id].filter.as_ref().map(std::ptr::from_ref),
                filter_pointer
            );
            output["cache_lookup"] = json!({
                "actual_eq_node_calls": 1, "id": again, "same_node": again == id,
                "same_filter": true, "new_controller_work": 0, "new_resident": 0,
                "nodes": graph.nodes.len(),
            });
        }
    }
    // The ordinary renderer uses the same physical-drop-before-release order.
    let resident = graph.resident;
    drop(graph);
    renderer.active_frames -= resident;
    output["after_graph_drop"] = json!({
        "active_frames": renderer.active_frames,
        "request_metadata": work.borrow().sound_context_frames,
    });
    output
}

pub(super) fn observed<'plan>(
    plan: &'plan RenderPlan,
    name: &str,
    at: i64,
    gap: bool,
) -> AudioOwnerClock<'plan> {
    let query = plan
        .audio_gain_owners(AudioSample(at)..AudioSample(at + 1), Default::default())
        .unwrap();
    query.spans()[0]
        .owners()
        .iter()
        .find(|owner| {
            owner.instance().node.as_str() == name
                && (owner.kind() == deadpan_plan::AudioOwnerKind::DefaultGap) == gap
        })
        .unwrap()
        .clone()
}

pub(super) fn frozen(name: &str, eq_owner: &str) -> Arc<RenderPlan> {
    let snapshot: Value = serde_json::from_str(include_str!(
        "../../../deadpan-plan/tests/fixtures/owner-capture-baseline-v1/snapshot.json"
    ))
    .unwrap();
    let mut document = snapshot["fixtures"]
        .as_array()
        .unwrap()
        .iter()
        .find(|fixture| fixture["fixture"] == name)
        .unwrap()["document"]
        .clone();
    // Explicitly derived fixture: retain the original timing/binding documents;
    // only give this live owner the established native EQ-admission recipe.
    let recipe_plan = eq_admission_fixture::fixture();
    let recipe_owner = eq_admission_fixture::owner(&recipe_plan);
    document["nodes"][eq_owner]["audio_treatments"] =
        serde_json::to_value(recipe_owner.treatments().unwrap()).unwrap();
    Arc::new(
        RenderPlan::compile(
            &deadpan_core::ProjectDocument::from_json(&document.to_string()).unwrap(),
        )
        .unwrap(),
    )
}

pub(super) fn render(plan: &Arc<RenderPlan>, at: i64, frames: u32) -> Value {
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = eq_admission_fixture::control(&cancelled, &work);
    let mut renderer = StageAudio::new(Arc::clone(plan));
    let mut provider = eq_admission_fixture::NoSources(0);
    let result =
        renderer.read_authored_equalizers(&mut provider, AudioSample(at), frames, control, 0, &[]);
    assert_eq!(provider.0, 0, "Tone witness must never open external media");
    let value = match result {
        Ok(block) => json!({
            "start": block.start.0,
            "pcm_bits": block.samples.iter().map(|frame|
                [frame[0].to_bits(), frame[1].to_bits()]).collect::<Vec<_>>(),
            "suppressed": block.suppressed.into_iter().map(range).collect::<Vec<_>>(),
            "exhausted": block.exhausted.into_iter().map(range).collect::<Vec<_>>(),
            "dependencies": block.dependencies, "relative_depth": block.relative_depth,
        }),
        Err(error) => json!({"error": format!("{error:?}")}),
    };
    json!({
        "output": value, "provider_reads": provider.0,
        "accounting": { "controller_plan_work": work.borrow().plan_work,
            "prepared_frames": work.borrow().prepared_frames,
            "prepared_stages": work.borrow().prepared_stages,
            "request_metadata": work.borrow().sound_context_frames,
            "active_frames": renderer.active_frames }
    })
}

/// Recreate the exact thirteen independently frozen inputs. Observation queries
/// and the actual incoming retained Arc are established outside measured work.
pub(super) fn cases(
    mut observe: impl for<'plan> FnMut(
        &'plan Arc<RenderPlan>,
        &Context<'plan>,
        &AudioOwnerClock<'plan>,
        AudioSample,
        usize,
        usize,
        Option<u32>,
    ) -> Value,
) -> Vec<Value> {
    let plan = eq_admission_fixture::fixture();
    let owner = eq_admission_fixture::owner(&plan);
    let stage = eq_admission_fixture::stage(&owner);
    let at = AudioSample(128);
    let input = plan
        .audio_owner_input(&owner, at, Default::default())
        .unwrap();
    let actual_domain = Context::Domain(Arc::new(input.context().clone()));
    let mut cases = Vec::new();
    let mut push = |name, value| cases.push(json!({"name":name,"observation":value}));
    push(
        "root_tone",
        observe(&plan, &Context::Root, &owner, at, stage, usize::MAX, None),
    );
    push(
        "actual_captured_domain",
        observe(&plan, &actual_domain, &owner, at, stage, 7, None),
    );
    push(
        "owner_limit_before_resolution",
        observe(&plan, &Context::Root, &owner, at, stage, 0, Some(0)),
    );
    push(
        "invalid_stage_after_resolution",
        observe(&plan, &Context::Root, &owner, at, usize::MAX, 0, None),
    );
    let untreated = observed(&plan, "tone", 128, false);
    push(
        "tone_missing_recipe_after_context",
        observe(&plan, &Context::Root, &untreated, at, 0, 0, None),
    );
    push(
        "owner_absent_at_observation",
        observe(
            &plan,
            &Context::Root,
            &owner,
            AudioSample(600),
            stage,
            0,
            None,
        ),
    );
    push(
        "observation_end_overflow",
        observe(
            &plan,
            &Context::Root,
            &owner,
            AudioSample(i64::MAX),
            stage,
            0,
            None,
        ),
    );

    let repeat = frozen("stable_repeat_plays_default_gap_and_overrides", "voice");
    for (name, sample) in [("repeat_voice_first", 0), ("repeat_voice_last", 10)] {
        let owner = observed(&repeat, "voice", sample, false);
        push(
            name,
            observe(
                &repeat,
                &Context::Root,
                &owner,
                AudioSample(sample),
                eq_admission_fixture::stage(&owner),
                3,
                None,
            ),
        );
    }
    let gap = observed(&repeat, "repeat", 8, true);
    push(
        "default_gap_missing_recipe",
        observe(&repeat, &Context::Root, &gap, AudioSample(8), 0, 0, None),
    );

    let retained_plan = frozen("hidden_resumed_retained_domain_vs_live_root", "group");
    let live = observed(&retained_plan, "group", 1, false);
    push(
        "live_owner_actual_retained_probe_selection",
        observe(
            &retained_plan,
            &Context::Root,
            &live,
            AudioSample(1),
            eq_admission_fixture::stage(&live),
            5,
            None,
        ),
    );
    let input = retained_plan
        .audio_owner_input(&live, AudioSample(1), Default::default())
        .unwrap();
    let probe = input
        .retained_probe_at(AudioSample(1), Default::default())
        .unwrap();
    let retained = probe
        .into_input()
        .expect("frozen actual raw retained probe")
        .into_shared();
    let owners = retained
        .owners(AudioSample(1)..AudioSample(2), Default::default())
        .unwrap();
    let retained_owner = owners.spans()[0]
        .owners()
        .iter()
        .find(|owner| owner.instance().node.as_str() == "group")
        .unwrap();
    push(
        "actual_incoming_retained",
        observe(
            &retained_plan,
            &Context::Retained(retained.clone()),
            retained_owner,
            AudioSample(1),
            eq_admission_fixture::stage(retained_owner),
            5,
            None,
        ),
    );
    let deadpan_plan::AudioBoundDomain::Root(hidden) = retained.domain() else {
        panic!("frozen hidden Root domain")
    };
    let query = hidden
        .gain_owners(AudioSample(3)..AudioSample(4), Default::default())
        .unwrap();
    let hidden_owner = query.spans()[0]
        .owners()
        .iter()
        .find(|owner| owner.instance().node.as_str() == "group")
        .unwrap();
    push(
        "actual_hidden_domain",
        observe(
            &retained_plan,
            &Context::Domain(Arc::new(hidden.clone())),
            hidden_owner,
            AudioSample(3),
            eq_admission_fixture::stage(hidden_owner),
            5,
            None,
        ),
    );

    cases
}
