use super::*;
use deadpan_plan::{
    AudioBoundaryRule, AudioRoutedRoot, AudioRoutedSignal, AudioSampleGrid, AudioSoundRoute,
};

fn gap_route() -> SoundRoute {
    SoundRoute::identity(ratio(1024, 1))
        .unwrap()
        .ripple(
            SoundRippleMap::new(
                ratio(1024, 1),
                0,
                vec![SoundRippleNode::Gap {
                    duration: ratio(1024, 1),
                }],
            )
            .unwrap(),
        )
        .unwrap()
}

fn signal_route(route: SoundRoute) -> AudioSoundRoute<SignalSample> {
    let grid = AudioSampleGrid::new(
        ExactRatio::ZERO,
        ExactRatio::ONE,
        AudioBoundaryRule::PointCeil,
    )
    .unwrap();
    let count = route.nodes().len();
    AudioSoundRoute::<SignalSample>::new(route, vec![grid; count]).unwrap()
}

fn root_route(route: SoundRoute) -> AudioSoundRoute<AudioSample> {
    let grid = AudioSampleGrid::new(
        ExactRatio::ZERO,
        ExactRatio::ONE,
        AudioBoundaryRule::RoundEven,
    )
    .unwrap();
    let count = route.nodes().len();
    AudioSoundRoute::<AudioSample>::new(route, vec![grid; count]).unwrap()
}

fn routed_root<'a>(
    projection: Arc<AudioStageProjection<'a>>,
    route: SoundRoute,
) -> AudioRoutedRoot<'a> {
    AudioRoutedRoot::new(
        AudioProjectedRoot::new(
            projection,
            AudioRootPlacement::new(
                ExactRatio::ZERO,
                ExactRatio::ONE,
                ExactRatio::ZERO..ratio(1024, 1),
            )
            .unwrap(),
        )
        .unwrap(),
        root_route(route),
    )
    .unwrap()
}

struct RevokedProvider {
    calls: usize,
}

impl AudioSourceProvider for RevokedProvider {
    fn source(
        &mut self,
        _: &ProjectId,
        _: &RevisionId,
        _: &AssetId,
        _: &AtomicBool,
    ) -> Result<&PreparedSource, PreparationError> {
        self.calls += 1;
        Err(PreparationError::SourceUnavailable(
            "revoked retained source".into(),
        ))
    }
}

#[test]
fn fully_masked_routes_admit_hidden_depth_before_source_io() {
    let plan = compile(&fixture(), false);
    let projection = nested(&plan);
    let signal =
        AudioRoutedSignal::projected(Arc::clone(&projection), signal_route(gap_route())).unwrap();
    let root = routed_root(projection, gap_route());
    let mut provider = FixtureProvider::new();
    let mut renderer = StageAudio::with_limits(
        Arc::clone(&plan),
        StageLimits {
            maximum_depth: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(matches!(
        renderer.read_routed_signal(
            &mut provider,
            &signal,
            SignalSample(700),
            32,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Limit("nested stage depth"))
    ));
    assert!(matches!(
        renderer.read_routed_root(
            &mut provider,
            &root,
            AudioSample(700),
            32,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Limit("nested stage depth"))
    ));
    assert_eq!(provider.calls, 0);
}

#[test]
fn fully_masked_routes_reject_revoked_history_and_recover() {
    let plan = compile(&fixture(), false);
    let projection = nested(&plan);
    let signal =
        AudioRoutedSignal::projected(Arc::clone(&projection), signal_route(gap_route())).unwrap();
    let root = routed_root(projection, gap_route());
    let mut revoked = RevokedProvider { calls: 0 };
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    assert!(matches!(
        renderer.read_routed_signal(
            &mut revoked,
            &signal,
            SignalSample(700),
            32,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Preparation(
            PreparationError::SourceUnavailable(_)
        ))
    ));
    assert!(matches!(
        renderer.read_routed_root(
            &mut revoked,
            &root,
            AudioSample(700),
            32,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Preparation(
            PreparationError::SourceUnavailable(_)
        ))
    ));
    assert_eq!(revoked.calls, 2);
    let mut provider = FixtureProvider::new();
    let block = renderer
        .read_routed_signal(
            &mut provider,
            &signal,
            SignalSample(700),
            32,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(block.samples, vec![[0.0; 2]; 32]);
    assert_eq!(block.suppressed, vec![SignalSample(700)..SignalSample(732)]);
    let block = renderer
        .read_routed_root(
            &mut provider,
            &root,
            AudioSample(700),
            32,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(block.samples, vec![[0.0; 2]; 32]);
    assert_eq!(block.suppressed, vec![AudioSample(700)..AudioSample(732)]);
    assert!(provider.calls > 0);
    assert_eq!(renderer.cached_stage_count(), 0);
}

#[test]
fn routed_spans_share_one_complete_projection_preparation() {
    let plan = compile(&fixture(), false);
    let route = SoundRoute::identity(ratio(1024, 1))
        .unwrap()
        .ripple(
            SoundRippleMap::new(
                ratio(1024, 1),
                3,
                vec![
                    SoundRippleNode::Keep {
                        range: ExactFrameRange {
                            start: ratio(0, 1),
                            end: ratio(32, 1),
                        },
                    },
                    SoundRippleNode::Gap {
                        duration: ratio(16, 1),
                    },
                    SoundRippleNode::Keep {
                        range: ExactFrameRange {
                            start: ratio(32, 1),
                            end: ratio(1024, 1),
                        },
                    },
                    SoundRippleNode::Sequence {
                        parts: vec![0, 1, 2],
                    },
                ],
            )
            .unwrap(),
        )
        .unwrap();
    let projection = nested(&plan);
    let signal =
        AudioRoutedSignal::projected(Arc::clone(&projection), signal_route(route.clone())).unwrap();
    let mut baseline_provider = FixtureProvider::new();
    StageAudio::new(Arc::clone(&plan))
        .read_tape(
            &mut baseline_provider,
            &output(&plan, Arc::clone(&projection)),
            SignalSample(0),
            1,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    let root = routed_root(projection, route);
    let mut renderer = StageAudio::with_limits(
        Arc::clone(&plan),
        StageLimits {
            maximum_prepared_stages: 2,
            maximum_prepared_frames: 8192,
            ..Default::default()
        },
    )
    .unwrap();
    let mut provider = FixtureProvider::new();
    let intrinsic = stretch(&original_inner(false), 1024, 2, 1);
    let expected: Vec<_> = intrinsic[..32]
        .iter()
        .copied()
        .chain(vec![[0.0; 2]; 16])
        .chain(intrinsic[32..112].iter().copied())
        .collect();
    let signal = renderer
        .read_routed_signal(
            &mut provider,
            &signal,
            SignalSample(0),
            128,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(signal.samples, expected);
    assert_eq!(signal.suppressed, vec![SignalSample(32)..SignalSample(48)]);
    let signal_calls = provider.calls;
    let root = renderer
        .read_routed_root(
            &mut provider,
            &root,
            AudioSample(0),
            128,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(root.samples, expected);
    assert_eq!(root.suppressed, vec![AudioSample(32)..AudioSample(48)]);
    let root_calls = provider.calls - signal_calls;
    // A renewed budget that rebuilt the full nested history per audible span
    // would perform at least two preparations. Allow dependency re-admission
    // on shared memo hits, but require fewer calls than that repeated history.
    assert!(
        signal_calls.max(root_calls) < baseline_provider.calls * 2,
        "routed calls (signal {signal_calls}, root {root_calls}) must stay below two complete preparations ({} calls)",
        baseline_provider.calls * 2,
    );
    assert_eq!(renderer.cached_stage_count(), 0);
}
