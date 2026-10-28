use super::*;

pub(in crate::stages) struct State {
    pub(in crate::stages) nodes: usize,
    pub(in crate::stages) resident: u64,
    pub(in crate::stages) filter: bool,
    pub(in crate::stages) tiles_empty: bool,
}

pub(in crate::stages) trait Harness<'plan> {
    type Graph;

    fn graph(plan: &'plan RenderPlan) -> Self::Graph;
    fn node(
        renderer: &mut StageAudio,
        graph: &mut Self::Graph,
        owner: &AudioOwnerClock<'plan>,
        control: WorkControl<'_>,
    ) -> Result<usize, StageAudioError>;
    fn tile(
        renderer: &mut StageAudio,
        provider: &mut impl AudioSourceProvider,
        graph: &mut Self::Graph,
        id: usize,
        control: WorkControl<'_>,
    ) -> Result<(), StageAudioError>;
    fn read(
        renderer: &mut StageAudio,
        provider: &mut impl AudioSourceProvider,
        control: WorkControl<'_>,
    ) -> Result<ReadBlock, StageAudioError>;
    fn state(graph: &Self::Graph, id: Option<usize>) -> State;
    fn take_filter(graph: &mut Self::Graph, id: usize) -> crate::Equalizer;
    fn restore_filter(graph: &mut Self::Graph, id: usize, filter: crate::Equalizer);
}

pub(in crate::stages) fn actual_node_requires_full_eq_native_headroom_and_retains_it_once<
    'plan,
    H: Harness<'plan>,
>(
    plan: &'plan Arc<RenderPlan>,
) {
    let owner = owner(plan);
    let cancelled = AtomicBool::new(false);
    let native = units();
    assert_eq!(native, 266_241);
    for headroom in [native - 1, native] {
        let mut renderer = StageAudio::with_limits(
            Arc::clone(plan),
            StageLimits {
                maximum_resident_frames: u32::try_from(BASE + headroom).unwrap(),
                ..Default::default()
            },
        )
        .unwrap();
        renderer.active_frames = BASE;
        let mut graph = H::graph(plan);
        let work = RefCell::new(ReadWork::default());
        let control = control(&cancelled, &work);
        let result = H::node(&mut renderer, &mut graph, &owner, control);
        if headroom < native {
            assert!(matches!(
                result,
                Err(StageAudioError::Limit("resident PCM or stage cache"))
            ));
            assert_eq!(H::state(&graph, None).nodes, 0);
            assert_eq!(H::state(&graph, None).resident, 0);
            assert_eq!(renderer.active_frames, BASE);
            assert!(work.borrow().plan_work >= deadpan_dsp::FINITE_FILTER_TAPS);
            continue;
        }
        let id = result.unwrap();
        assert_eq!(H::state(&graph, None).nodes, 1);
        assert!(H::state(&graph, Some(id)).filter);
        assert_eq!(H::state(&graph, None).resident, native);
        assert_eq!(renderer.active_frames, BASE + native);
        let spent = work.borrow().plan_work;
        assert_eq!(
            H::node(&mut renderer, &mut graph, &owner, control).unwrap(),
            id
        );
        assert_eq!(H::state(&graph, None).nodes, 1);
        assert_eq!(H::state(&graph, None).resident, native);
        assert_eq!(renderer.active_frames, BASE + native);
        assert_eq!(work.borrow().plan_work, spent);

        let filter = H::take_filter(&mut graph, id);
        assert!(matches!(
            renderer.make_room(1, false, control),
            Err(StageAudioError::Limit("resident PCM or stage cache"))
        ));
        assert_eq!(renderer.active_frames, BASE + native);
        H::restore_filter(&mut graph, id, filter);

        // The actual tile entry takes the filter, then refuses its separately
        // admitted PCM workspace. It must restore the same retained processor.
        let mut provider = NoSources(0);
        assert!(matches!(
            H::tile(&mut renderer, &mut provider, &mut graph, id, control),
            Err(StageAudioError::Limit("resident PCM or stage cache"))
        ));
        assert!(H::state(&graph, Some(id)).filter);
        assert!(H::state(&graph, Some(id)).tiles_empty);
        assert_eq!(H::state(&graph, None).resident, native);
        assert_eq!(renderer.active_frames, BASE + native);
        assert_eq!(
            work.borrow().prepared_frames,
            (deadpan_dsp::FINITE_FILTER_INPUT_FRAMES + deadpan_dsp::FINITE_FILTER_OUTPUT_FRAMES)
                as u64
        );
        assert_eq!(provider.0, 0);
        renderer.active_frames -= H::state(&graph, None).resident;
        drop(graph);
        assert_eq!(renderer.active_frames, BASE);
    }
}

pub(in crate::stages) fn numerical_read_refuses_small_native_budget_and_cleans_both_results<
    'plan,
    H: Harness<'plan>,
>(
    plan: &'plan Arc<RenderPlan>,
) {
    let cancelled = AtomicBool::new(false);
    let native = units();
    let mut provider = NoSources(0);
    let mut normal = StageAudio::new(Arc::clone(plan));
    normal.active_frames = BASE;
    let normal_work = RefCell::new(ReadWork::default());
    let expected = H::read(
        &mut normal,
        &mut provider,
        control(&cancelled, &normal_work),
    )
    .unwrap();
    assert!(expected.samples.iter().any(|frame| *frame != [0.0; 2]));
    assert!(
        expected
            .samples
            .iter()
            .flatten()
            .all(|sample| sample.is_finite())
    );
    assert_eq!(normal.active_frames, BASE);

    let mut renderer = StageAudio::with_limits(
        Arc::clone(plan),
        StageLimits {
            maximum_resident_frames: u32::try_from(BASE + native / 2).unwrap(),
            ..Default::default()
        },
    )
    .unwrap();
    renderer.active_frames = BASE;
    for _ in 0..2 {
        let work = RefCell::new(ReadWork::default());
        assert!(matches!(
            H::read(&mut renderer, &mut provider, control(&cancelled, &work)),
            Err(StageAudioError::Limit("resident PCM or stage cache"))
        ));
        assert_eq!(renderer.active_frames, BASE);
        assert!(work.borrow().plan_work >= deadpan_dsp::FINITE_FILTER_TAPS);
    }
    // A fresh request after ordinary graph failure must not inherit a leaked
    // native charge. This test adjustment is not a production error fallback.
    renderer.limits.maximum_resident_frames = u32::try_from(BASE + native + 65_536).unwrap();
    let work = RefCell::new(ReadWork::default());
    let actual = H::read(&mut renderer, &mut provider, control(&cancelled, &work)).unwrap();
    assert_eq!(actual.start, expected.start);
    assert_eq!(
        actual
            .samples
            .iter()
            .map(|frame| frame.map(f32::to_bits))
            .collect::<Vec<_>>(),
        expected
            .samples
            .iter()
            .map(|frame| frame.map(f32::to_bits))
            .collect::<Vec<_>>()
    );
    assert_eq!(actual.suppressed, expected.suppressed);
    assert_eq!(actual.exhausted, expected.exhausted);
    assert_eq!(actual.dependencies, expected.dependencies);
    assert_eq!(actual.relative_depth, expected.relative_depth);
    assert_eq!(renderer.active_frames, BASE);
    assert_eq!(provider.0, 0);
}
