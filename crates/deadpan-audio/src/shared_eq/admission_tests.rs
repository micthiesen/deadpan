use super::*;
use crate::stages::authored_eq::eq_admission_fixture::checks::{self, Harness, State};

struct Shared;

impl<'plan> Harness<'plan> for Shared {
    type Graph = Graph<'plan>;

    fn metadata_units(plan: &'plan Arc<RenderPlan>) -> u64 {
        original_metadata_units(plan)
    }

    fn graph(plan: &'plan RenderPlan) -> Self::Graph {
        Graph {
            plan,
            nodes: Vec::new(),
            resident: 0,
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

    fn node(
        renderer: &mut StageAudio,
        graph: &mut Self::Graph,
        owner: &AudioOwnerClock<'plan>,
        control: WorkControl<'_>,
    ) -> Result<usize, StageAudioError> {
        let context = Contexts {
            original: Context::Root,
            sounds: Sounds::None,
        };
        renderer.shared_eq_node(
            graph,
            Program {
                context: &context,
                stop: None,
                allowed: None,
                output_voice: None,
                parent: usize::MAX,
                depth: 0,
            },
            owner,
            AudioSample(128),
            Temporal::Equalizer(crate::stages::authored_eq::eq_admission_fixture::stage(
                owner,
            )),
            control,
        )
    }

    fn tile(
        renderer: &mut StageAudio,
        provider: &mut impl AudioSourceProvider,
        graph: &mut Self::Graph,
        id: usize,
        control: WorkControl<'_>,
    ) -> Result<(), StageAudioError> {
        let mut permissions = Permissions::default();
        permissions.insert(0)?;
        renderer
            .shared_eq_tile(provider, graph, id, (0, permissions, None), control, 1)
            .map(|_| ())
    }

    fn read(
        renderer: &mut StageAudio,
        provider: &mut impl AudioSourceProvider,
        control: WorkControl<'_>,
    ) -> Result<ReadBlock, StageAudioError> {
        renderer.read_shared_input(
            provider,
            AudioSample(128),
            1,
            control,
            InputSelection::default(),
        )
    }

    fn state(graph: &Self::Graph, id: Option<usize>) -> State {
        State {
            nodes: graph.nodes.len(),
            resident: graph.resident,
            filter: id.is_some_and(|id| graph.nodes[id].filter.is_some()),
            tiles_empty: id.is_none_or(|id| graph.nodes[id].tiles.is_empty()),
        }
    }

    fn take_filter(graph: &mut Self::Graph, id: usize) -> crate::Equalizer {
        graph.nodes[id].filter.take().unwrap()
    }

    fn restore_filter(graph: &mut Self::Graph, id: usize, filter: crate::Equalizer) {
        graph.nodes[id].filter = Some(filter);
    }
}

#[test]
fn actual_node_requires_full_eq_native_headroom_and_retains_it_once() {
    let plan = crate::stages::authored_eq::eq_admission_fixture::fixture();
    checks::actual_node_requires_full_eq_native_headroom_and_retains_it_once::<Shared>(&plan);
}

#[test]
fn numerical_read_refuses_small_native_budget_and_cleans_both_results() {
    let plan = crate::stages::authored_eq::eq_admission_fixture::fixture();
    checks::numerical_read_refuses_small_native_budget_and_cleans_both_results::<Shared>(&plan);
}

// Actual final-sharing constructor, independent of numerical Node publication.
// It measures only newly admitted Original metadata, with the full native bound
// independently asserted by the common Harness.
pub(super) fn original_metadata_units(plan: &Arc<RenderPlan>) -> u64 {
    let owner = crate::stages::authored_eq::eq_admission_fixture::owner(plan);
    let input = plan
        .audio_owner_input(&owner, AudioSample(128), Default::default())
        .unwrap();
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = crate::stages::authored_eq::eq_admission_fixture::control(&cancelled, &work);
    let mut renderer = StageAudio::new(Arc::clone(plan));
    let carrier = {
        let mut admission = sound_context_admission::Admission::new(&mut renderer, control);
        Context::finish_paid_owner_input_admitted(
            &input,
            input.owner(),
            AudioSample(128),
            control,
            &mut admission,
        )
        .unwrap()
    };
    let metadata = carrier.lease().unwrap().frames();
    assert!(metadata > 0);
    assert_eq!(work.borrow().sound_context_frames, metadata);
    drop(carrier);
    assert_eq!(work.borrow().sound_context_frames, 0);
    metadata
}
