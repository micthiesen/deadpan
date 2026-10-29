use super::eq_admission_fixture as fixture;
use super::*;

#[test]
fn root_and_physical_domain_descriptions_remove_only_the_actual_point_query() {
    let plan = fixture::fixture();
    let owner = fixture::owner(&plan);
    let input = plan
        .audio_owner_input(&owner, AudioSample(128), Default::default())
        .unwrap();
    let domain = Context::Domain(Arc::new(input.context().clone()));
    let cancelled = AtomicBool::new(false);
    for incoming in [&Context::Root, &domain] {
        span_start::reset_trace();
        let old_work = RefCell::new(ReadWork::default());
        let old_control = fixture::control(&cancelled, &old_work);
        let mut old_renderer = StageAudio::new(Arc::clone(&plan));
        let direct = {
            let mut admission =
                sound_context_admission::Admission::new(&mut old_renderer, old_control);
            node_description::describe_admitted(
                &plan,
                node_description::Request {
                    incoming,
                    owner: &owner,
                    at: AudioSample(128),
                    stage: fixture::stage(&owner),
                },
                old_control,
                &mut admission,
            )
            .unwrap()
        };
        let point_query_work = span_start::trace().direct_point_work;
        assert!(point_query_work > 0);
        let work = RefCell::new(ReadWork::default());
        let control = fixture::control(&cancelled, &work);
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        let candidates = {
            let mut admission = sound_context_admission::Admission::new(&mut renderer, control);
            incoming
                .span_start_owners_admitted(
                    &plan,
                    AudioSample(128)..AudioSample(129),
                    control,
                    &mut admission,
                )
                .unwrap()
        };
        let wide_work = work.borrow().plan_work;
        let certified = {
            let mut admission = sound_context_admission::Admission::new(&mut renderer, control);
            node_description::describe_selected_admitted(
                &plan,
                node_description::Selection {
                    request: node_description::Request {
                        incoming,
                        owner: &owner,
                        at: AudioSample(128),
                        stage: fixture::stage(&owner),
                    },
                    source: candidates.source(0),
                },
                control,
                &mut admission,
            )
            .unwrap()
        };
        assert_eq!(
            legacy_node_baseline::context(&plan, &certified.context, AudioSample(128)),
            legacy_node_baseline::context(&plan, &direct.context, AudioSample(128)),
        );
        assert_eq!(certified.recipe, direct.recipe);
        assert_eq!(
            certified.boundary.before_stage(),
            direct.boundary.before_stage()
        );
        assert_eq!(
            legacy_node_baseline::clock(&plan, certified.boundary.owner(), AudioSample(128)),
            legacy_node_baseline::clock(&plan, direct.boundary.owner(), AudioSample(128)),
        );
        let fresh_work = work.borrow().plan_work.checked_sub(wide_work).unwrap();
        assert_eq!(
            fresh_work.checked_add(point_query_work).unwrap(),
            old_work.borrow().plan_work
        );
        assert_eq!(span_start::trace().direct_point_queries, 1);
        assert_eq!(span_start::trace().certified_captures, 1);
        drop(certified);
        assert!(
            work.borrow().sound_context_frames > 0,
            "wide query is still guarded"
        );
        drop(candidates);
        drop(direct);
        assert_eq!(work.borrow().sound_context_frames, 0);
        assert_eq!(old_work.borrow().sound_context_frames, 0);
    }
}

#[test]
fn cache_hit_and_owner_count_precede_certificate_validation() {
    let plan = fixture::fixture();
    let incoming = Context::Root;
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = fixture::control(&cancelled, &work);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let query = {
        let mut admission = sound_context_admission::Admission::new(&mut renderer, control);
        incoming
            .span_start_owners_admitted(
                &plan,
                AudioSample(128)..AudioSample(129),
                control,
                &mut admission,
            )
            .unwrap()
    };
    let owner = &query.query().spans()[0].owners()[0];
    let stage = fixture::stage(owner);
    let program = Program {
        context: &incoming,
        stop: None,
        parent: usize::MAX,
        depth: 0,
    };
    let mut graph = Graph {
        plan: &plan,
        nodes: Vec::new(),
        resident: 0,
        node_capacity_frames: 0,
        skip: &[],
    };
    let id = renderer
        .eq_node(&mut graph, program, owner, AudioSample(128), stage, control)
        .unwrap();
    let spent = work.borrow().plan_work;
    let again = renderer
        .eq_node_selected(
            &mut graph,
            program,
            node_description::Selection {
                request: node_description::Request {
                    incoming: &incoming,
                    owner,
                    at: AudioSample(128),
                    stage,
                },
                source: query.source(usize::MAX),
            },
            control,
        )
        .unwrap();
    assert_eq!(again, id);
    assert_eq!(work.borrow().plan_work, spent);
    let resident = graph.resident;
    drop(graph);
    renderer.active_frames -= resident;
    renderer.limits.maximum_prepared_stages = 0;
    let mut graph = Graph {
        plan: &plan,
        nodes: Vec::new(),
        resident: 0,
        node_capacity_frames: 0,
        skip: &[],
    };
    let result = renderer.eq_node_selected(
        &mut graph,
        program,
        node_description::Selection {
            request: node_description::Request {
                incoming: &incoming,
                owner,
                at: AudioSample(128),
                stage,
            },
            source: query.source(usize::MAX),
        },
        control,
    );
    assert!(matches!(
        result,
        Err(StageAudioError::Limit("EQ owner count"))
    ));
    assert_eq!(work.borrow().plan_work, spent);
    drop(graph);
    drop(query);
    assert_eq!(work.borrow().sound_context_frames, 0);
    assert_eq!(renderer.active_frames, 0);
}
