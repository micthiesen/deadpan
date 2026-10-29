use super::*;
use crate::stages::authored_eq::eq_admission_fixture::checks::{self, Harness, State};

struct Authored;

impl<'plan> Harness<'plan> for Authored {
    type Graph<'work> = Graph<'plan, 'plan, 'work>;

    fn metadata_units(plan: &'plan Arc<RenderPlan>) -> u64 {
        super::node_admission_tests::metadata_units(plan)
    }

    fn expected_tile_prepared_frames() -> u64 {
        // The independently admitted temporary boundary refuses before the
        // filter is taken or any tile PCM workspace is reserved.
        0
    }

    fn graph<'work>(plan: &'plan RenderPlan) -> Self::Graph<'work> {
        Graph {
            plan,
            nodes: Vec::new(),
            resident: 0,
            node_capacity_frames: 0,
            skip: &[],
        }
    }

    fn node<'work>(
        renderer: &mut StageAudio,
        graph: &mut Self::Graph<'work>,
        owner: &AudioOwnerClock<'plan>,
        control: WorkControl<'work>,
    ) -> Result<usize, StageAudioError> {
        renderer.eq_node(
            graph,
            Program {
                context: &Context::Root,
                stop: None,
                parent: usize::MAX,
                depth: 0,
            },
            owner,
            AudioSample(128),
            crate::stages::authored_eq::eq_admission_fixture::stage(owner),
            control,
        )
    }

    fn tile<'work>(
        renderer: &mut StageAudio,
        provider: &mut impl AudioSourceProvider,
        graph: &mut Self::Graph<'work>,
        id: usize,
        control: WorkControl<'work>,
    ) -> Result<(), StageAudioError> {
        renderer
            .eq_tile(provider, graph, id, 0, control, 1)
            .map(|_| ())
    }

    fn read(
        renderer: &mut StageAudio,
        provider: &mut impl AudioSourceProvider,
        control: WorkControl<'_>,
    ) -> Result<ReadBlock, StageAudioError> {
        renderer.read_authored_equalizers(provider, AudioSample(128), 1, control, 0, &[])
    }

    fn state<'work>(graph: &Self::Graph<'work>, id: Option<usize>) -> State {
        State {
            nodes: graph.nodes.len(),
            resident: graph.resident,
            filter: id.is_some_and(|id| graph.nodes[id].filter.is_some()),
            tiles_empty: id.is_none_or(|id| graph.nodes[id].tiles.is_empty()),
        }
    }

    fn take_filter<'work>(graph: &mut Self::Graph<'work>, id: usize) -> crate::Equalizer {
        graph.nodes[id].filter.take().unwrap()
    }

    fn restore_filter<'work>(graph: &mut Self::Graph<'work>, id: usize, filter: crate::Equalizer) {
        graph.nodes[id].filter = Some(filter);
    }
}

#[test]
fn actual_node_requires_full_eq_native_headroom_and_retains_it_once() {
    let plan = crate::stages::authored_eq::eq_admission_fixture::fixture();
    checks::actual_node_requires_full_eq_native_headroom_and_retains_it_once::<Authored>(&plan);
}

#[test]
fn numerical_read_refuses_small_native_budget_and_cleans_both_results() {
    let plan = crate::stages::authored_eq::eq_admission_fixture::fixture();
    checks::numerical_read_refuses_small_native_budget_and_cleans_both_results::<Authored>(&plan);
}
