use super::*;
use crate::stages::authored_eq::eq_admission_fixture as fixture;
const BASE: u64 = 17;

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
        original_inputs: original_inputs::Cache::default(),
    }
}

#[test]
fn actual_wide_root_query_blocks_one_short_native_node_then_releases_for_retry() {
    let plan = fixture::fixture();
    let owner = fixture::owner(&plan);
    let cancelled = AtomicBool::new(false);

    // Measure the complete actual node charge (native processor plus every
    // real metadata and initial-capacity lease) through the existing harness.
    let probe_work = RefCell::new(ReadWork::default());
    let probe_control = fixture::control(&cancelled, &probe_work);
    let mut probe_renderer = StageAudio::new(Arc::clone(&plan));
    probe_renderer.active_frames = BASE;
    let mut probe_graph = graph(&plan);
    let probe_context = Contexts {
        original: Context::Root,
        sounds: Sounds::None,
    };
    let probe_id = probe_renderer
        .shared_eq_node(
            &mut probe_graph,
            Program {
                context: &probe_context,
                stop: None,
                allowed: None,
                output_voice: None,
                parent: usize::MAX,
                depth: 0,
            },
            &owner,
            AudioSample(128),
            Temporal::Equalizer(fixture::stage(&owner)),
            probe_control,
        )
        .unwrap();
    assert_eq!(probe_id, 0);
    let node_charge = probe_graph.resident;
    assert!(node_charge >= fixture::units());
    probe_renderer.release_shared_graph(probe_graph, 0);

    let work = RefCell::new(ReadWork::default());
    let control = fixture::control(&cancelled, &work);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    renderer.active_frames = BASE;
    let guard = {
        let mut admission = sound_context_admission::Admission::new(&mut renderer, control);
        Context::Root
            .owners_admitted(
                &plan,
                AudioSample(128)..AudioSample(129),
                control,
                &mut admission,
            )
            .unwrap()
    };
    let query_frames = work.borrow().sound_context_frames;
    assert!(query_frames > 0);
    let spent = work.borrow().plan_work;
    let mut graph = graph(&plan);
    renderer.limits.maximum_resident_frames =
        u32::try_from(BASE + query_frames + node_charge - 1).unwrap();
    let context = Contexts {
        original: Context::Root,
        sounds: Sounds::None,
    };
    assert!(matches!(
        renderer.shared_eq_node(
            &mut graph,
            Program {
                context: &context,
                stop: None,
                allowed: None,
                output_voice: None,
                parent: usize::MAX,
                depth: 0,
            },
            &owner,
            AudioSample(128),
            Temporal::Equalizer(fixture::stage(&owner)),
            control
        ),
        Err(StageAudioError::Limit("resident PCM or stage cache"))
    ));
    assert!(graph.nodes.is_empty());
    assert_eq!(graph.resident, 0);
    assert_eq!(renderer.active_frames, BASE);
    assert_eq!(work.borrow().sound_context_frames, query_frames);
    let after_refusal = work.borrow().plan_work;
    assert!(after_refusal > spent);

    drop(guard);
    assert_eq!(work.borrow().sound_context_frames, 0);
    assert_eq!(work.borrow().plan_work, after_refusal);
    renderer.limits.maximum_resident_frames = u32::try_from(BASE + node_charge).unwrap();
    renderer
        .shared_eq_node(
            &mut graph,
            Program {
                context: &context,
                stop: None,
                allowed: None,
                output_voice: None,
                parent: usize::MAX,
                depth: 0,
            },
            &owner,
            AudioSample(128),
            Temporal::Equalizer(fixture::stage(&owner)),
            control,
        )
        .unwrap();
    assert_eq!(graph.resident, node_charge);
    assert_eq!(renderer.active_frames, BASE + node_charge);
    renderer.release_shared_graph(graph, 0);
    assert_eq!(renderer.active_frames, BASE);
}
