use super::*;
use crate::stages::authored_eq::eq_admission_fixture as fixture;
use deadpan_core::{AudioTreatments, EqualizerRecipe, GainDb, ProjectDocument};
use std::alloc::Layout;
use std::sync::atomic::Ordering;

const OWNER_QUERY_SNAPSHOT: &str =
    include_str!("../../../deadpan-plan/tests/fixtures/owner-query-baseline-v1/snapshot.json");
const BASE: u64 = 17;

fn repeated_eq_plan() -> Arc<RenderPlan> {
    let snapshot: serde_json::Value = serde_json::from_str(OWNER_QUERY_SNAPSHOT).unwrap();
    let mut document = snapshot["fixtures"]
        .as_array()
        .unwrap()
        .iter()
        .find(|fixture| fixture["fixture"] == "stable_repeat_plays_default_gap_and_overrides")
        .unwrap()["document"]
        .clone();
    // Keep the frozen repeat topology, and add the actual EQ treatment needed
    // to enter the shared-node path. The modified document is validated by the
    // normal ProjectDocument and RenderPlan constructors.
    let eq = EqualizerRecipe::new(
        [3000, -6000, 1500].map(|gain| GainDb::new(gain).unwrap()),
        None,
        None,
    )
    .unwrap();
    let treatments = AudioTreatments::default().with_equalizer(Some(eq)).unwrap();
    document["nodes"]["voice"]["audio_treatments"] = serde_json::to_value(treatments).unwrap();
    let document = ProjectDocument::from_json(&document.to_string()).unwrap();
    Arc::new(RenderPlan::compile(&document).unwrap())
}

fn repeated_voice<'plan>(plan: &'plan RenderPlan) -> AudioOwnerClock<'plan> {
    let query = plan
        .audio_gain_owners(AudioSample(0)..AudioSample(1), Default::default())
        .unwrap();
    let owner = query.spans()[0]
        .owners()
        .iter()
        .find(|owner| owner.instance().node.as_str() == "voice")
        .unwrap()
        .clone();
    assert!(!owner.instance().repeats.is_empty());
    assert!(owner.treatments().unwrap().equalizer().is_some());
    owner
}

fn graph<'plan, 'work>(plan: &'plan RenderPlan) -> Graph<'plan, 'work> {
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
        original_only: true,
    }
}

fn capacity_frames<T>(capacity: usize) -> u64 {
    u64::try_from(Layout::array::<T>(capacity).unwrap().size())
        .unwrap()
        .div_ceil(std::mem::size_of::<[f32; 2]>() as u64)
}

fn add_node<'plan, 'work>(
    renderer: &mut StageAudio,
    graph: &mut Graph<'plan, 'work>,
    owner: &AudioOwnerClock<'plan>,
    parent: usize,
    control: WorkControl<'work>,
) -> Result<usize, StageAudioError> {
    let context = Contexts {
        original: Context::Root,
        sounds: Sounds::None,
    };
    let stage = fixture::stage(owner);
    renderer.shared_eq_node(
        graph,
        Program {
            context: &context,
            stop: None,
            allowed: None,
            output_voice: None,
            parent,
            depth: 0,
        },
        owner,
        AudioSample(0),
        Temporal::Equalizer(stage),
        control,
    )
}

fn fill_four_nodes<'plan, 'work>(
    renderer: &mut StageAudio,
    graph: &mut Graph<'plan, 'work>,
    owner: &AudioOwnerClock<'plan>,
    control: WorkControl<'work>,
) {
    for parent in 0..4 {
        assert_eq!(
            add_node(renderer, graph, owner, parent, control).unwrap(),
            parent
        );
    }
}

fn renderer(plan: &Arc<RenderPlan>, maximum_resident_frames: u32) -> StageAudio {
    StageAudio::with_limits(
        Arc::clone(plan),
        StageLimits {
            maximum_resident_frames,
            ..Default::default()
        },
    )
    .unwrap()
}

fn description_lease_frames<'plan>(
    plan: &'plan Arc<RenderPlan>,
    owner: &AudioOwnerClock<'plan>,
) -> (u64, u64, u64, u64) {
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = fixture::control(&cancelled, &work);
    let mut renderer = renderer(plan, 16 * 1024 * 1024);
    renderer.active_frames = BASE;
    let incoming = Contexts {
        original: Context::Root,
        sounds: Sounds::None,
    };
    let description = {
        let mut admission = sound_context_admission::Admission::new(&mut renderer, control);
        node_description::describe_admitted(
            plan,
            node_description::Request {
                incoming: &incoming,
                owner,
                at: AudioSample(0),
                kind: Temporal::Equalizer(fixture::stage(owner)),
                stage: fixture::stage(owner),
                original_only: true,
            },
            control,
            &mut admission,
        )
        .unwrap()
    };
    let stop = description
        .stop
        .lease()
        .map_or(0, sound_context_admission::Lease::frames);
    let boundary = description
        .boundary
        .lease()
        .map_or(0, sound_context_admission::Lease::frames);
    let original = description
        .original
        .as_ref()
        .unwrap()
        .leases()
        .into_iter()
        .flatten()
        .map(sound_context_admission::Lease::frames)
        .sum();
    let total = stop + boundary + original;
    drop(description);
    assert_eq!(work.borrow().sound_context_frames, 0);
    (stop, boundary, original, total)
}

#[test]
fn actual_repeated_owner_graph_hands_off_stop_boundary_and_capacity_charges() {
    let plan = repeated_eq_plan();
    let owner = repeated_voice(&plan);
    let (stop_frames, boundary_frames, _, metadata_frames) =
        description_lease_frames(&plan, &owner);
    assert!(stop_frames > 0, "the actual repeat-path key owns a lease");
    assert!(boundary_frames > 0, "the actual boundary owns a lease");
    assert!(metadata_frames >= stop_frames + boundary_frames);

    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = fixture::control(&cancelled, &work);
    let mut renderer = renderer(&plan, 16 * 1024 * 1024);
    renderer.active_frames = BASE;
    let mut graph = graph(&plan);

    let id = add_node(&mut renderer, &mut graph, &owner, usize::MAX, control).unwrap();
    let node = &graph.nodes[id];
    assert!(node.stop.owner.matches(&owner));
    assert!(node.stop.lease().is_some());
    assert!(node.boundary.lease().is_some());
    assert_eq!(
        node.stop.lease().unwrap().frames(),
        0,
        "key charge moved to Graph"
    );
    assert_eq!(
        node.boundary.lease().unwrap().frames(),
        0,
        "boundary charge moved to Graph"
    );
    assert_eq!(work.borrow().sound_context_frames, 0);

    let native = fixture::units();
    let capacity = capacity_frames::<Node<'_, '_>>(graph.nodes.capacity());
    assert_eq!(graph.node_capacity_frames, capacity);
    assert_eq!(graph.resident, native + capacity + metadata_frames);
    assert_eq!(renderer.active_frames, BASE + graph.resident);
    assert!(node.filter.is_some());

    let retained = graph.resident;
    renderer.release_shared_graph(graph, 0);
    assert_eq!(renderer.active_frames, BASE);
    assert_eq!(retained, native + capacity + metadata_frames);
}

#[test]
fn graph_node_capacity_uses_spare_space_then_accounts_replacement_growth() {
    let plan = repeated_eq_plan();
    let owner = repeated_voice(&plan);
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = fixture::control(&cancelled, &work);
    let mut renderer = renderer(&plan, 16 * 1024 * 1024);
    renderer.active_frames = BASE;
    let mut graph = graph(&plan);

    let id0 = add_node(&mut renderer, &mut graph, &owner, 0, control).unwrap();
    assert_eq!(id0, 0);
    assert_eq!(graph.nodes.capacity(), 4);
    let first_capacity = graph.node_capacity_frames;
    assert_eq!(first_capacity, capacity_frames::<Node<'_, '_>>(4));
    let first_resident = graph.resident;
    assert_eq!(work.borrow().sound_context_frames, 0);

    for parent in 1..4 {
        let id = add_node(&mut renderer, &mut graph, &owner, parent, control).unwrap();
        assert_eq!(id, parent);
        assert_eq!(graph.nodes.capacity(), 4);
        assert_eq!(graph.node_capacity_frames, first_capacity);
        assert_eq!(work.borrow().sound_context_frames, 0);
    }
    assert_eq!(graph.nodes.len(), 4);
    assert!(graph.resident > first_resident);
    let before_growth = graph.resident;
    let before_active = renderer.active_frames;

    let id4 = add_node(&mut renderer, &mut graph, &owner, 4, control).unwrap();
    assert_eq!(id4, 4);
    assert_eq!(graph.nodes.len(), 5);
    assert_eq!(graph.nodes.capacity(), 8);
    assert_eq!(
        graph.node_capacity_frames,
        capacity_frames::<Node<'_, '_>>(8)
    );
    assert!(graph.node_capacity_frames > first_capacity);
    assert!(graph.resident > before_growth);
    assert_eq!(renderer.active_frames, BASE + graph.resident);
    assert!(renderer.active_frames > before_active);
    assert_eq!(work.borrow().sound_context_frames, 0);
    assert!(graph.nodes.iter().all(|node| node.filter.is_some()));

    let retained = graph.resident;
    renderer.release_shared_graph(graph, 0);
    assert_eq!(renderer.active_frames, BASE);
    assert!(retained > before_growth);
}

#[test]
fn exact_combined_native_metadata_capacity_headroom_succeeds_and_one_short_refuses() {
    let plan = repeated_eq_plan();
    let owner = repeated_voice(&plan);
    let cancelled = AtomicBool::new(false);
    let probe_work = RefCell::new(ReadWork::default());
    let probe_control = fixture::control(&cancelled, &probe_work);
    let mut probe_renderer = renderer(&plan, 16 * 1024 * 1024);
    probe_renderer.active_frames = BASE;
    let mut probe_graph = graph(&plan);
    add_node(
        &mut probe_renderer,
        &mut probe_graph,
        &owner,
        usize::MAX,
        probe_control,
    )
    .unwrap();
    let combined = probe_graph.resident;
    let capacity = probe_graph.node_capacity_frames;
    let native = fixture::units();
    assert!(combined > native + capacity);
    let (_, _, _, metadata) = description_lease_frames(&plan, &owner);
    assert_eq!(combined, native + capacity + metadata);
    probe_renderer.release_shared_graph(probe_graph, 0);

    let mut exact_renderer = renderer(&plan, u32::try_from(BASE + combined).unwrap());
    exact_renderer.active_frames = BASE;
    let exact_work = RefCell::new(ReadWork::default());
    let exact_control = fixture::control(&cancelled, &exact_work);
    let mut exact_graph = graph(&plan);
    add_node(
        &mut exact_renderer,
        &mut exact_graph,
        &owner,
        usize::MAX,
        exact_control,
    )
    .unwrap();
    assert_eq!(exact_graph.resident, combined);
    assert_eq!(exact_renderer.active_frames, BASE + combined);
    assert_eq!(exact_work.borrow().sound_context_frames, 0);
    exact_renderer.release_shared_graph(exact_graph, 0);
    assert_eq!(exact_renderer.active_frames, BASE);

    let mut short_renderer = renderer(&plan, u32::try_from(BASE + combined - 1).unwrap());
    short_renderer.active_frames = BASE;
    let short_work = RefCell::new(ReadWork::default());
    let short_control = fixture::control(&cancelled, &short_work);
    let mut short_graph = graph(&plan);
    assert!(matches!(
        add_node(
            &mut short_renderer,
            &mut short_graph,
            &owner,
            usize::MAX,
            short_control
        ),
        Err(StageAudioError::Limit("resident PCM or stage cache"))
    ));
    assert!(short_graph.nodes.is_empty());
    assert_eq!(short_graph.resident, 0);
    assert_eq!(short_graph.node_capacity_frames, 0);
    assert_eq!(short_renderer.active_frames, BASE);
    assert_eq!(short_work.borrow().sound_context_frames, 0);
}

#[test]
fn failed_replacement_capacity_overlap_preserves_published_graph_state() {
    let plan = repeated_eq_plan();
    let owner = repeated_voice(&plan);
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = fixture::control(&cancelled, &work);
    let mut renderer = renderer(&plan, 16 * 1024 * 1024);
    renderer.active_frames = BASE;
    let mut graph = graph(&plan);
    for parent in 0..4 {
        assert_eq!(
            add_node(&mut renderer, &mut graph, &owner, parent, control).unwrap(),
            parent
        );
    }
    let old_nodes = graph.nodes.len();
    let old_capacity = graph.nodes.capacity();
    let old_capacity_frames = graph.node_capacity_frames;
    let old_resident = graph.resident;
    let old_active = renderer.active_frames;
    let node_metadata = old_resident - (old_nodes as u64 * fixture::units()) - old_capacity_frames;
    let next_metadata = node_metadata / old_nodes as u64;
    let replacement_frames = capacity_frames::<Node<'_, '_>>(8);
    let overlapped = old_active + next_metadata + replacement_frames;
    renderer.limits.maximum_resident_frames = u32::try_from(overlapped - 1).unwrap();

    assert!(matches!(
        add_node(&mut renderer, &mut graph, &owner, 4, control),
        Err(StageAudioError::Limit("resident PCM or stage cache"))
    ));
    assert_eq!(graph.nodes.len(), old_nodes);
    assert_eq!(graph.nodes.capacity(), old_capacity);
    assert_eq!(graph.node_capacity_frames, old_capacity_frames);
    assert_eq!(graph.resident, old_resident);
    assert_eq!(renderer.active_frames, old_active);
    assert!(graph.nodes.iter().all(|node| node.filter.is_some()));
    assert!(graph.nodes.iter().all(|node| node.tiles.is_empty()));
    assert_eq!(work.borrow().sound_context_frames, 0);

    let retained = graph.resident;
    renderer.release_shared_graph(graph, 0);
    assert_eq!(renderer.active_frames, BASE);
    assert_eq!(retained, old_resident);
}

#[test]
fn work_limit_after_native_construction_and_capacity_allocation_keeps_old_graph() {
    let plan = repeated_eq_plan();
    let owner = repeated_voice(&plan);
    let cancelled = AtomicBool::new(false);

    let measured_work = RefCell::new(ReadWork::default());
    let measured_control = fixture::control(&cancelled, &measured_work);
    let mut measured_renderer = renderer(&plan, 16 * 1024 * 1024);
    measured_renderer.active_frames = BASE;
    let mut measured_graph = graph(&plan);
    fill_four_nodes(
        &mut measured_renderer,
        &mut measured_graph,
        &owner,
        measured_control,
    );
    let before_growth_work = measured_work.borrow().plan_work;
    add_node(
        &mut measured_renderer,
        &mut measured_graph,
        &owner,
        4,
        measured_control,
    )
    .unwrap();
    let full_growth_work = measured_work.borrow().plan_work - before_growth_work;
    let move_work = measured_graph.nodes.len() - 1;
    assert!(full_growth_work > move_work);
    measured_renderer.release_shared_graph(measured_graph, 0);

    let work = RefCell::new(ReadWork::default());
    let control = fixture::control(&cancelled, &work);
    let mut renderer = renderer(&plan, 16 * 1024 * 1024);
    renderer.active_frames = BASE;
    let mut graph = graph(&plan);
    fill_four_nodes(&mut renderer, &mut graph, &owner, control);
    let old_nodes = graph.nodes.len();
    let old_capacity = graph.nodes.capacity();
    let old_capacity_frames = graph.node_capacity_frames;
    let old_resident = graph.resident;
    let old_active = renderer.active_frames;

    // The fifth node spends `full_growth_work - move_work` units before the
    // capacity move prepayment. Leave exactly one fewer than the four moves.
    let pre_move_work = full_growth_work - move_work;
    let remaining_before_move = move_work - 1;
    let start_work = MAX_PLAN_WORK_PER_READ - pre_move_work - remaining_before_move;
    work.borrow_mut().plan_work = start_work;

    assert!(matches!(
        add_node(&mut renderer, &mut graph, &owner, 4, control),
        Err(StageAudioError::Limit("plan work per read"))
    ));
    assert_eq!(
        work.borrow().plan_work,
        MAX_PLAN_WORK_PER_READ - remaining_before_move
    );
    assert_eq!(graph.nodes.len(), old_nodes);
    assert_eq!(graph.nodes.capacity(), old_capacity);
    assert_eq!(graph.node_capacity_frames, old_capacity_frames);
    assert_eq!(graph.resident, old_resident);
    assert_eq!(renderer.active_frames, old_active);
    assert!(graph.nodes.iter().all(|node| node.filter.is_some()));
    assert_eq!(work.borrow().sound_context_frames, 0);

    renderer.release_shared_graph(graph, 0);
    assert_eq!(renderer.active_frames, BASE);
}

#[test]
fn already_cancelled_control_does_not_publish_or_leak_preparation_leases() {
    let plan = repeated_eq_plan();
    let owner = repeated_voice(&plan);
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = fixture::control(&cancelled, &work);
    let mut renderer = renderer(&plan, 16 * 1024 * 1024);
    renderer.active_frames = BASE;
    let mut graph = graph(&plan);
    cancelled.store(true, Ordering::Relaxed);

    assert!(add_node(&mut renderer, &mut graph, &owner, 0, control).is_err());
    assert!(graph.nodes.is_empty());
    assert_eq!(graph.resident, 0);
    assert_eq!(graph.node_capacity_frames, 0);
    assert_eq!(renderer.active_frames, BASE);
    assert_eq!(work.borrow().sound_context_frames, 0);
}
