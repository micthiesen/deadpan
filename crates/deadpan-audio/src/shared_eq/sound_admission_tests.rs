use super::*;
use crate::stages::authored_eq::eq_admission_fixture as eq;
fn graph<'plan, 'work>(plan: &'plan RenderPlan, original_only: bool) -> Graph<'plan, 'work> {
    Graph {
        plan,
        nodes: Vec::new(),
        resident: 0,
        node_capacity_frames: 0,
        wet_filters: std::array::from_fn(|_| None),
        event_tiles: BTreeMap::new(),
        event_nodes: Default::default(),
        event_filters: BTreeMap::new(),
        voices: BTreeMap::new(),
        permission_families: BTreeMap::new(),
        sends: None,
        skip: Vec::new(),
        original_only,
    }
}
fn node<'plan, 'work>(
    renderer: &mut StageAudio,
    graph: &mut Graph<'plan, 'work>,
    owner: &AudioOwnerClock<'plan>,
    control: WorkControl<'work>,
) -> Result<usize, StageAudioError> {
    renderer.shared_eq_node(
        graph,
        Program {
            context: &Contexts {
                original: Context::Root,
                sounds: Sounds::None,
            },
            stop: None,
            allowed: None,
            output_voice: None,
            parent: usize::MAX,
            depth: 0,
        },
        owner,
        AudioSample(128),
        Temporal::Equalizer(eq::stage(owner)),
        control,
    )
}
#[test]
fn nested_native_entry_sees_outstanding_metadata_and_no_work_is_refunded() {
    let plan = eq::fixture();
    let owner = eq::owner(&plan);
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = eq::control(&cancelled, &work);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    renderer.active_frames = eq::BASE;
    let metadata = {
        let mut admission = sound_context_admission::Admission::new(&mut renderer, control);
        plan.audio_sound_instance_context_admitted(
            owner.instance(),
            control.query_limits().unwrap(),
            &mut admission,
        )
        .unwrap()
        .into_shared(&mut admission)
        .unwrap()
    };
    let original = super::admission_tests::original_metadata_units(&plan);
    let boundary_and_capacity = super::admission_tests::boundary_and_capacity_metadata_units(&plan);
    let m = work.borrow().sound_context_frames;
    assert!(m > 1);
    renderer.limits.maximum_resident_frames =
        u32::try_from(eq::BASE + eq::units() + original + boundary_and_capacity + m - 1).unwrap();
    let before_work = work.borrow().plan_work;
    let mut graph = graph(&plan, true);
    assert!(matches!(
        node(&mut renderer, &mut graph, &owner, control),
        Err(StageAudioError::Limit(_))
    ));
    assert!(graph.nodes.is_empty());
    assert_eq!(renderer.active_frames, eq::BASE);
    assert_eq!(work.borrow().sound_context_frames, m);
    assert!(work.borrow().plan_work > before_work);
    drop(metadata);
    assert_eq!(work.borrow().sound_context_frames, 0);
    node(&mut renderer, &mut graph, &owner, control).unwrap();
    assert_eq!(
        renderer.active_frames,
        eq::BASE + eq::units() + original + boundary_and_capacity
    );
    assert_eq!(
        graph.resident,
        eq::units() + original + boundary_and_capacity
    );
    renderer.release_shared_graph(graph, 0);
    assert_eq!(renderer.active_frames, eq::BASE);
}
#[test]
fn successful_graph_handoff_and_cancelled_tile_release_each_charge_once() {
    let plan = eq::fixture();
    let owner = eq::owner(&plan);
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = eq::control(&cancelled, &work);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    renderer.active_frames = eq::BASE;
    let sound_metadata = {
        let mut admission = sound_context_admission::Admission::new(&mut renderer, control);
        plan.audio_sound_instance_context_admitted(
            owner.instance(),
            control.query_limits().unwrap(),
            &mut admission,
        )
        .unwrap()
        .into_shared(&mut admission)
        .unwrap()
    };
    let sound_metadata_frames = work.borrow().sound_context_frames;
    drop(sound_metadata);
    assert_eq!(work.borrow().sound_context_frames, 0);
    let original = super::admission_tests::original_metadata_units(&plan);
    let boundary_and_capacity = super::admission_tests::boundary_and_capacity_metadata_units(&plan);
    let mut graph = graph(&plan, false);
    let id = node(&mut renderer, &mut graph, &owner, control).unwrap();
    assert_eq!(
        work.borrow().sound_context_frames,
        0,
        "request token transferred"
    );
    assert!(
        graph.resident > eq::units(),
        "Graph retains metadata plus native bound"
    );
    assert_eq!(
        graph.resident,
        eq::units() + original + boundary_and_capacity + sound_metadata_frames
    );
    assert_eq!(renderer.active_frames, eq::BASE + graph.resident);
    let Sounds::Owner(sound) = &graph.nodes[id].context.sounds else {
        panic!("current sound")
    };
    let weak = Arc::downgrade(sound);
    let retained = graph.resident;
    let filter = graph.nodes[id].filter.take().unwrap();
    assert_eq!(renderer.active_frames, eq::BASE + retained);
    graph.nodes[id].filter = Some(filter);
    cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
    let mut provider = eq::NoSources(0);
    assert!(
        renderer
            .shared_eq_tile(
                &mut provider,
                &mut graph,
                id,
                (0, Permissions::default(), None),
                control,
                1
            )
            .is_err()
    );
    assert_eq!(renderer.active_frames, eq::BASE + retained);
    renderer.release_shared_graph(graph, 0);
    assert!(weak.upgrade().is_none());
    assert_eq!(renderer.active_frames, eq::BASE);
    assert_eq!(work.borrow().sound_context_frames, 0);
    assert_eq!(provider.0, 0);
}
#[test]
fn denied_current_sound_construction_never_publishes_node_and_cancel_keeps_zero() {
    let plan = eq::fixture();
    let owner = eq::owner(&plan);
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = eq::control(&cancelled, &work);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    renderer.active_frames = eq::BASE;
    renderer.limits.maximum_resident_frames = u32::try_from(eq::BASE).unwrap();
    let mut graph = graph(&plan, false);
    assert!(matches!(
        node(&mut renderer, &mut graph, &owner, control),
        Err(StageAudioError::Limit(_))
    ));
    assert!(graph.nodes.is_empty());
    assert_eq!(graph.resident, 0);
    assert_eq!(work.borrow().sound_context_frames, 0);
    cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(node(&mut renderer, &mut graph, &owner, control).is_err());
    renderer.release_shared_graph(graph, 0);
    assert_eq!(renderer.active_frames, eq::BASE);
    assert_eq!(work.borrow().sound_context_frames, 0);
}
