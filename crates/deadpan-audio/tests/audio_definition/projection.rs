use super::*;

use deadpan_plan::{
    AudioProjectedRoot, AudioQueryLimits, AudioRootPlacement, AudioSignal, AudioSignalContent,
    AudioSignalTape, AudioSignalTapeRun, AudioStage, AudioStageProjection,
};

#[path = "projection_routed.rs"]
mod routed;

pub(super) fn stage(signal: AudioSignal<'_>) -> AudioStage<'_> {
    let query = signal
        .query(
            SignalSample(0)..SignalSample(1),
            AudioQueryLimits::default(),
        )
        .unwrap();
    let AudioSignalContent::Stage(stage) = query.spans.into_iter().next().unwrap().content else {
        panic!("expected scoped Preserve stage");
    };
    stage
}

fn fixture() -> ProjectDocument {
    let rate = FrameRate::new(48_000, 1).unwrap();
    document(
        rate,
        &["outer"],
        [
            ("a", source(rate, 1024, 0..1024)),
            ("pause", hold(768, HoldAudio::Silence)),
            ("b", source(rate, 2048, 4096..6144)),
            (
                "group",
                BeatNode::sequence("Group", vec![id("a"), id("pause"), id("b")]),
            ),
            (
                "inner",
                retime("group", 2560, 0..3840, PitchPolicy::Preserve),
            ),
            (
                "outer",
                retime("inner", 1280, 0..2560, PitchPolicy::Preserve),
            ),
        ],
        BTreeMap::new(),
    )
}

fn inner<'plan>(plan: &'plan RenderPlan, reverse: bool) -> Arc<AudioStageProjection<'plan>> {
    let outer = stage(plan.audio_signal());
    let inner = stage(outer.input_signal());
    let signal = inner.input_signal();
    let (first, second, split) = if reverse {
        (1792..3840, 0..1024, 2048)
    } else {
        (0..1024, 1792..3840, 1024)
    };
    let tape = |length: i128| {
        let seam = ratio(i128::from(split) * length, 3072);
        AudioSignalTape::new(
            plan,
            ExactRatio::ZERO..ratio(length, 1),
            vec![
                AudioSignalTapeRun::new(
                    ExactRatio::ZERO..seam,
                    ratio(first.start, 1)..ratio(first.end, 1),
                    signal.clone(),
                ),
                AudioSignalTapeRun::new(
                    seam..ratio(length, 1),
                    ratio(second.start, 1)..ratio(second.end, 1),
                    signal.clone(),
                ),
            ],
        )
        .unwrap()
    };
    AudioStageProjection::new(inner, tape(3072), tape(2048), frames(2048)).unwrap()
}

fn nested(plan: &RenderPlan) -> Arc<AudioStageProjection<'_>> {
    let child = inner(plan, false);
    let tape = |length: i128| {
        AudioSignalTape::new(
            plan,
            ExactRatio::ZERO..ratio(length, 1),
            vec![AudioSignalTapeRun::intrinsic(
                ExactRatio::ZERO..ratio(length, 1),
                ExactRatio::ZERO..ratio(2048, 1),
                Arc::clone(&child),
            )],
        )
        .unwrap()
    };
    AudioStageProjection::new(
        stage(plan.audio_signal()),
        tape(2048),
        tape(1024),
        frames(1024),
    )
    .unwrap()
}

pub(super) fn output<'plan>(
    plan: &'plan RenderPlan,
    projection: Arc<AudioStageProjection<'plan>>,
) -> AudioSignalTape<'plan> {
    let length = projection.output_policy().support().end;
    AudioSignalTape::new(
        plan,
        ExactRatio::ZERO..length,
        vec![AudioSignalTapeRun::intrinsic(
            ExactRatio::ZERO..length,
            ExactRatio::ZERO..length,
            projection,
        )],
    )
    .unwrap()
}

fn original_inner(reverse: bool) -> Vec<[f32; 2]> {
    let a = sample_reference(
        0..1024,
        ExactRatio::ZERO,
        ExactRatio::ONE,
        1024,
        fixture_sample,
    );
    let b = sample_reference(
        4096..6144,
        ratio(4096, 1),
        ExactRatio::ONE,
        2048,
        fixture_sample,
    );
    let input: Vec<_> = if reverse {
        b.into_iter().chain(a).collect()
    } else {
        a.into_iter().chain(b).collect()
    };
    stretch(&input, 2048, 3, 2)
}

#[test]
fn nested_projection_prepares_each_intrinsic_once_and_places_pause_after_stretch() {
    let doc = fixture();
    let plan = compile(&doc, false);
    let projection = nested(&plan);
    let raw = stage(stage(plan.audio_signal()).input_signal()).input_signal();
    let cut = ratio(1024, 3);
    let resume = cut.checked_add(ratio(256, 1)).unwrap();
    let schedule = AudioSignalTape::new(
        &plan,
        ExactRatio::ZERO..ratio(1280, 1),
        vec![
            AudioSignalTapeRun::intrinsic(
                ExactRatio::ZERO..cut,
                ExactRatio::ZERO..cut,
                Arc::clone(&projection),
            ),
            AudioSignalTapeRun::new(cut..resume, ratio(1024, 1)..ratio(1792, 1), raw),
            AudioSignalTapeRun::intrinsic(resume..ratio(1280, 1), cut..ratio(1024, 1), projection),
        ],
    )
    .unwrap();
    let intrinsic = stretch(&original_inner(false), 1024, 2, 1);
    let mut expected = intrinsic[..342].to_vec();
    expected.extend(vec![[0.0; 2]; 256]);
    expected.extend_from_slice(&intrinsic[342..]);
    let mut renderer = StageAudio::with_limits(
        Arc::clone(&plan),
        StageLimits {
            maximum_prepared_stages: 2,
            ..Default::default()
        },
    )
    .unwrap();
    let mut provider = FixtureProvider::new();
    // Out-of-order reads must all use the complete canonical processing history.
    for (start, count) in [(1024, 256), (256, 256), (512, 256), (0, 256)] {
        let actual = renderer
            .read_tape(
                &mut provider,
                &schedule,
                SignalSample(start),
                count,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(
            actual.samples,
            expected[start as usize..start as usize + count as usize]
        );
        assert_eq!(
            renderer.cached_stage_count(),
            0,
            "projections must not enter descriptor cache"
        );
    }
    provider.unavailable = true;
    assert!(matches!(
        renderer.read_tape(
            &mut provider,
            &schedule,
            SignalSample(0),
            1,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Preparation(
            PreparationError::SourceUnavailable(_)
        ))
    ));
}

#[test]
fn two_routes_through_one_stage_do_not_alias_prepared_pcm() {
    let doc = fixture();
    let plan = compile(&doc, false);
    let first = inner(&plan, false);
    let second = inner(&plan, true);
    assert_eq!(first.stage().descriptor(), second.stage().descriptor());
    assert_ne!(first.identity(), second.identity());
    let tape = AudioSignalTape::new(
        &plan,
        ExactRatio::ZERO..ratio(4096, 1),
        vec![
            AudioSignalTapeRun::intrinsic(
                ExactRatio::ZERO..ratio(2048, 1),
                ExactRatio::ZERO..ratio(2048, 1),
                first,
            ),
            AudioSignalTapeRun::intrinsic(
                ratio(2048, 1)..ratio(4096, 1),
                ExactRatio::ZERO..ratio(2048, 1),
                second,
            ),
        ],
    )
    .unwrap();
    let normal = original_inner(false);
    let reversed = original_inner(true);
    assert_ne!(&normal[..128], &reversed[..128]);
    let actual = StageAudio::new(Arc::clone(&plan))
        .read_tape(
            &mut FixtureProvider::new(),
            &tape,
            SignalSample(1920),
            256,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    let expected: Vec<_> = normal[1920..]
        .iter()
        .chain(&reversed[..128])
        .copied()
        .collect();
    assert_eq!(actual.samples, expected);
    let mut provider = FixtureProvider::new();
    let mut bounded = StageAudio::with_limits(
        Arc::clone(&plan),
        StageLimits {
            maximum_resident_frames: 10_239,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(matches!(
        bounded.read_tape(
            &mut provider,
            &tape,
            SignalSample(1920),
            256,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Limit("resident PCM or stage cache"))
    ));
    assert_eq!(
        provider.calls, 12,
        "second preparation must include the first memo's 2048 resident frames"
    );
}

#[test]
fn nested_projection_enforces_depth_and_residency_then_recovers() {
    let doc = fixture();
    let plan = compile(&doc, false);
    let tape = output(&plan, nested(&plan));
    let mut provider = FixtureProvider::new();
    let mut shallow = StageAudio::with_limits(
        Arc::clone(&plan),
        StageLimits {
            maximum_depth: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(matches!(
        shallow.read_tape(
            &mut provider,
            &tape,
            SignalSample(0),
            1,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Limit("nested stage depth"))
    ));
    assert_eq!(provider.calls, 0);
    // Parent reservation5120 + child8192 requires13312 frames. Memo residency
    // must stay charged while subsequent input chunks consume the child result.
    let mut bounded = StageAudio::with_limits(
        Arc::clone(&plan),
        StageLimits {
            maximum_resident_frames: 13_311,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(matches!(
        bounded.read_tape(
            &mut provider,
            &tape,
            SignalSample(0),
            1,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Limit("resident PCM or stage cache"))
    ));
    let child = output(&plan, inner(&plan, false));
    let recovered = bounded
        .read_tape(
            &mut provider,
            &child,
            SignalSample(0),
            16,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(recovered.samples, original_inner(false)[..16]);
}

#[test]
fn projected_input_is_preflighted_even_when_output_policy_omits_unsupported_tail() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let doc = document(
        rate,
        &["stage"],
        [
            ("a", source(rate, 1024, 512..1536)),
            (
                "tail",
                hold(
                    1024,
                    HoldAudio::Tail {
                        source: audio(512..1536),
                        maximum: frames(1024),
                    },
                ),
            ),
            (
                "group",
                BeatNode::sequence("Group", vec![id("a"), id("tail")]),
            ),
            (
                "stage",
                retime("group", 1024, 0..2048, PitchPolicy::Preserve),
            ),
        ],
        BTreeMap::new(),
    );
    let plan = compile(&doc, false);
    let current = stage(plan.audio_signal());
    let signal = current.input_signal();
    let tape = |length: i128, end: i128| {
        AudioSignalTape::new(
            &plan,
            ExactRatio::ZERO..ratio(length, 1),
            vec![AudioSignalTapeRun::new(
                ExactRatio::ZERO..ratio(length, 1),
                ExactRatio::ZERO..ratio(end, 1),
                signal.clone(),
            )],
        )
        .unwrap()
    };
    let projected =
        AudioStageProjection::new(current, tape(2048, 2048), tape(1024, 1024), frames(1024))
            .unwrap();
    let tape = output(&plan, projected);
    let mut provider = FixtureProvider::new();
    assert!(matches!(
        StageAudio::new(Arc::clone(&plan)).read_tape(
            &mut provider,
            &tape,
            SignalSample(0),
            1,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Unsupported("effect tails"))
    ));
    assert_eq!(provider.calls, 0);
}

#[test]
fn projected_output_preserves_processed_decay_past_the_source_selection() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let mut selected = source(rate, 1024, 512..1536);
    let NodeKind::Source { source } = &mut selected.kind else {
        unreachable!();
    };
    source.audio_mapping = SourceAudioMapping::SelectedPlacement {
        start: ExactRatio::ZERO,
        frames: ratio(1024, 1),
        selection: ExactFrameRange::new(ExactRatio::ZERO, ratio(256, 1)).unwrap(),
    };
    let doc = document(
        rate,
        &["stage"],
        [
            ("source", selected),
            (
                "stage",
                retime("source", 2048, 0..1024, PitchPolicy::Preserve),
            ),
        ],
        BTreeMap::new(),
    );
    let plan = compile(&doc, false);
    let current = stage(plan.audio_signal());
    let signal = current.input_signal();
    let tape = |length: i128| {
        AudioSignalTape::new(
            &plan,
            ExactRatio::ZERO..ratio(length, 1),
            vec![AudioSignalTapeRun::new(
                ExactRatio::ZERO..ratio(length, 1),
                ExactRatio::ZERO..ratio(1024, 1),
                signal.clone(),
            )],
        )
        .unwrap()
    };
    let output_policy = tape(2048);
    assert_eq!(
        output_policy
            .policy(
                SignalSample(512)..SignalSample(768),
                AudioQueryLimits::default()
            )
            .unwrap()
            .suppressed,
        vec![SignalSample(512)..SignalSample(768)],
        "raw Source selection policy would erase this processed decay"
    );
    let projection =
        AudioStageProjection::new(current, tape(1024), output_policy, frames(2048)).unwrap();
    let input = sample_reference(
        512..768,
        ratio(512, 1),
        ExactRatio::ONE,
        1024,
        fixture_sample,
    );
    let expected = stretch(&input, 2048, 1, 2);
    assert!(
        expected[512..768].iter().any(|frame| *frame != [0.0; 2]),
        "fixture must expose processed decay after the physical Source endpoint"
    );
    let actual = StageAudio::new(Arc::clone(&plan))
        .read_tape(
            &mut FixtureProvider::new(),
            &output(&plan, Arc::clone(&projection)),
            SignalSample(512),
            256,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(actual.samples, expected[512..768]);
    assert!(actual.suppressed.is_empty());
    let root = AudioProjectedRoot::new(
        projection,
        AudioRootPlacement::new(
            ratio(-17, 1),
            ExactRatio::ONE,
            ExactRatio::ZERO..ratio(2048, 1),
        )
        .unwrap(),
    )
    .unwrap();
    let actual = StageAudio::new(Arc::clone(&plan))
        .read_projected_root(
            &mut FixtureProvider::new(),
            &root,
            AudioSample(495),
            256,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(actual.samples, expected[512..768]);
    assert!(actual.suppressed.is_empty());
}

fn unchanged_projection<'plan>(
    plan: &'plan RenderPlan,
    stage: AudioStage<'plan>,
) -> Arc<AudioStageProjection<'plan>> {
    let signal = stage.input_signal();
    let duration = stage.descriptor().duration;
    let input_extent = signal.support();
    let tape = |length: ExactRatio| {
        AudioSignalTape::new(
            plan,
            ExactRatio::ZERO..length,
            vec![AudioSignalTapeRun::new(
                ExactRatio::ZERO..length,
                input_extent.clone(),
                signal.clone(),
            )],
        )
        .unwrap()
    };
    let input = tape(input_extent.end.checked_sub(input_extent.start).unwrap());
    let policy = tape(ExactRatio::integer(duration.frames()));
    AudioStageProjection::new(stage, input, policy, duration).unwrap()
}

#[test]
fn projected_root_ntsc_resume_keeps_phase_and_later_domain_origin_independent() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let doc = document(
        rate,
        &["a-stage", "b-stage"],
        [
            ("a", source(rate, 3, 0..4805)),
            ("b", source(rate, 3, 3072..7877)),
            ("a-stage", retime("a", 2, 0..3, PitchPolicy::Preserve)),
            ("b-stage", retime("b", 2, 0..3, PitchPolicy::Preserve)),
        ],
        BTreeMap::new(),
    );
    let plan = compile(&doc, false);
    let domain = |name: &str, origin: i128| {
        let stage = stage(plan.audio_definition(node(name)).unwrap().signal());
        AudioProjectedRoot::new(
            unchanged_projection(&plan, stage),
            AudioRootPlacement::new(
                ratio(origin, 1),
                ExactRatio::ONE,
                ExactRatio::ZERO..ratio(2, 1),
            )
            .unwrap(),
        )
        .unwrap()
    };
    let a = domain("a-stage", 0);
    let b = domain("b-stage", 2);
    assert_eq!(a.samples(), AudioSample(0)..AudioSample(3203));
    assert_eq!(b.samples(), AudioSample(3203)..AudioSample(6406));
    let a_left = a.crop(ExactRatio::ZERO..ExactRatio::ONE).unwrap();
    let a_resume = a
        .resume(AudioSample(1602), ratio(2, 1)..ratio(3, 1))
        .unwrap();
    let b_moved = b
        .resume(AudioSample(3203), ratio(3, 1)..ratio(5, 1))
        .unwrap();
    let a_twice = a_resume
        .resume(AudioSample(4004), ratio(3, 1)..ratio(7, 2))
        .unwrap();
    let canonical = |offset: i64| {
        stretch(
            &sample_reference(
                offset..offset + 4805,
                ratio(i128::from(offset), 1),
                ExactRatio::ONE,
                4805,
                fixture_sample,
            ),
            3204,
            3,
            2,
        )
    };
    let original_a = canonical(0);
    let original_b = canonical(3072);
    let b_root = sample_reference(0..3204, ratio(-1, 5), ExactRatio::ONE, 3203, |sample| {
        original_b[sample as usize]
    });
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let mut provider = FixtureProvider::new();
    // Includes disordered requests, independent B placement, and a second pause
    // composed from A's already-resumed phase rather than original frame time.
    for (domain, start, expected, suppression) in [
        (
            &a_resume,
            4741,
            [&original_a[3140..3203], &[[0.0; 2]]].concat(),
            vec![AudioSample(4804)..AudioSample(4805)],
        ),
        (&a_left, 1538, original_a[1538..1602].to_vec(), vec![]),
        (&b_moved, 4805, b_root[..127].to_vec(), vec![]),
        (&a_resume, 3203, original_a[1602..1693].to_vec(), vec![]),
        (&a_twice, 4805, original_a[2403..2467].to_vec(), vec![]),
        (&a_left, 0, original_a[..64].to_vec(), vec![]),
    ] {
        let actual = renderer
            .read_projected_root(
                &mut provider,
                domain,
                AudioSample(start),
                expected.len() as u32,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(actual.samples, expected);
        assert_eq!(actual.suppressed, suppression);
        assert_eq!(actual.stage, "projected_root_pcm_before_effects");
        assert_eq!(actual.allocation, domain.samples());
    }
    assert_eq!(renderer.cached_stage_count(), 0);
    provider.unavailable = true;
    assert!(matches!(
        renderer.read_projected_root(
            &mut provider,
            &a_resume,
            AudioSample(3203),
            1,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Preparation(
            PreparationError::SourceUnavailable(_)
        ))
    ));
}

#[test]
fn projected_root_regrids_a_hold_with_no_intrinsic_point_before_rounding() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let doc = document(
        rate,
        &["stage"],
        [
            ("a", source(rate, 1025, 0..1025)),
            ("pause", hold(1, HoldAudio::Silence)),
            ("b", source(rate, 3070, 4096..7166)),
            (
                "group",
                BeatNode::sequence("Group", vec![id("a"), id("pause"), id("b")]),
            ),
            (
                "stage",
                retime("group", 2048, 0..4096, PitchPolicy::Preserve),
            ),
        ],
        BTreeMap::new(),
    );
    let plan = compile(&doc, false);
    let projection = unchanged_projection(&plan, stage(plan.audio_signal()));
    assert!(
        projection
            .output_policy()
            .policy_after_preserve(SignalSample(512)..SignalSample(514), Default::default())
            .unwrap()
            .suppressed
            .is_empty()
    );
    let root = AudioProjectedRoot::new(
        projection,
        AudioRootPlacement::new(ratio(-3, 2), ratio(4, 1), ExactRatio::ZERO..ratio(2048, 1))
            .unwrap(),
    )
    .unwrap();
    let mut input = sample_reference(
        0..1025,
        ExactRatio::ZERO,
        ExactRatio::ONE,
        1025,
        fixture_sample,
    );
    input.push([0.0; 2]);
    input.extend(sample_reference(
        4096..7166,
        ratio(4096, 1),
        ExactRatio::ONE,
        3070,
        fixture_sample,
    ));
    let canonical = stretch(&input, 2048, 2, 1);
    let mut expected = sample_reference(0..2048, ratio(4083, 8), ratio(1, 4), 32, |sample| {
        canonical[sample as usize]
    });
    assert!(expected[8..10].iter().any(|frame| *frame != [0.0; 2]));
    expected[8..10].fill([0.0; 2]);
    let block = StageAudio::new(Arc::clone(&plan))
        .read_projected_root(
            &mut FixtureProvider::new(),
            &root,
            AudioSample(2040),
            32,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(block.samples, expected);
    assert_eq!(block.suppressed, vec![AudioSample(2048)..AudioSample(2050)]);
}

#[test]
fn projected_root_admission_precedes_source_reads_and_failures_recover() {
    let plan = compile(&fixture(), false);
    let projection = nested(&plan);
    let root = AudioProjectedRoot::new(
        Arc::clone(&projection),
        AudioRootPlacement::new(
            ratio(-17, 1),
            ExactRatio::ONE,
            ExactRatio::ZERO..ratio(1024, 1),
        )
        .unwrap(),
    )
    .unwrap();
    let mut provider = FixtureProvider::new();
    let foreign = compile(&fixture(), false);
    let mut other = StageAudio::new(foreign);
    assert!(matches!(
        other.read_projected_root(
            &mut provider,
            &root,
            AudioSample(0),
            1,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::ForeignDomain)
    ));
    let mut shallow = StageAudio::with_limits(
        Arc::clone(&plan),
        StageLimits {
            maximum_depth: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(matches!(
        shallow.read_projected_root(
            &mut provider,
            &root,
            AudioSample(0),
            1,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Limit("nested stage depth"))
    ));
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let too_fast = AudioProjectedRoot::new(
        projection,
        AudioRootPlacement::new(
            ExactRatio::ZERO,
            ratio(1, 100),
            ExactRatio::ZERO..ratio(1024, 1),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(matches!(
        renderer.read_projected_root(
            &mut provider,
            &too_fast,
            AudioSample(0),
            1,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Preparation(
            PreparationError::InvalidRecipe(_)
        ))
    ));
    for (start, count) in [(-18, 1), (-17, 0), (-17, 257), (1006, 2)] {
        assert!(matches!(
            renderer.read_projected_root(
                &mut provider,
                &root,
                AudioSample(start),
                count,
                TIMEOUT,
                &AtomicBool::new(false)
            ),
            Err(StageAudioError::Range)
        ));
    }
    assert!(
        renderer
            .read_projected_root(
                &mut provider,
                &root,
                AudioSample(-17),
                1,
                TIMEOUT,
                &AtomicBool::new(true)
            )
            .unwrap_err()
            .is_cancelled()
    );
    assert_eq!(provider.calls, 0);
    let block = renderer
        .read_projected_root(
            &mut provider,
            &root,
            AudioSample(-17),
            64,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(
        block.samples,
        stretch(&original_inner(false), 1024, 2, 1)[..64]
    );
}

#[test]
fn projected_memo_rechecks_source_admission_within_one_read() {
    struct RevokingProvider {
        fixture: FixtureProvider,
        remaining: usize,
    }
    impl AudioSourceProvider for RevokingProvider {
        fn source(
            &mut self,
            project: &ProjectId,
            revision: &RevisionId,
            asset: &AssetId,
            cancelled: &AtomicBool,
        ) -> Result<&PreparedSource, PreparationError> {
            if self.remaining == 0 {
                return Err(PreparationError::SourceUnavailable(
                    "revoked during read".into(),
                ));
            }
            self.remaining -= 1;
            self.fixture.source(project, revision, asset, cancelled)
        }
    }
    let doc = fixture();
    let plan = compile(&doc, false);
    let tape = output(&plan, nested(&plan));
    // Twelve 256-point source blocks prepare the child's complete 3072-point input.
    // The next source request is a memo-hit admission check in the parent.
    let mut provider = RevokingProvider {
        fixture: FixtureProvider::new(),
        remaining: 12,
    };
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    assert!(matches!(
        renderer.read_tape(
            &mut provider,
            &tape,
            SignalSample(0),
            1,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Preparation(
            PreparationError::SourceUnavailable(_)
        ))
    ));
    assert_eq!(provider.fixture.calls, 12);
    provider.remaining = usize::MAX;
    let recovered = renderer
        .read_tape(
            &mut provider,
            &tape,
            SignalSample(0),
            1,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(
        recovered.samples,
        stretch(&original_inner(false), 1024, 2, 1)[..1]
    );
}

#[test]
fn projected_input_preflights_hidden_history_of_an_ordinary_nested_stage() {
    assert_hidden_history_preflight(false);
    assert_hidden_history_preflight(true);
}

fn assert_hidden_history_preflight(bound_child: bool) {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let doc = document(
        rate,
        &["outer"],
        [
            ("prefix", source(rate, 256, 0..256)),
            ("a", source(rate, 1024, 512..1536)),
            (
                "tail",
                hold(
                    1024,
                    HoldAudio::Tail {
                        source: audio(512..1536),
                        maximum: frames(1024),
                    },
                ),
            ),
            (
                "inside",
                BeatNode::sequence("Inside", vec![id("a"), id("tail")]),
            ),
            (
                "inner",
                retime("inside", 1024, 0..2048, PitchPolicy::Preserve),
            ),
            (
                "group",
                BeatNode::sequence("Group", vec![id("prefix"), id("inner")]),
            ),
            (
                "outer",
                retime("group", 640, 0..1280, PitchPolicy::Preserve),
            ),
        ],
        BTreeMap::new(),
    );
    let doc = if bound_child {
        let captured = capture_unbound_audio_bindings(
            &doc,
            AudioTimingId {
                allocation: RevisionId::new("projection-binding").unwrap(),
                ordinal: 0,
            },
        )
        .unwrap();
        let binding = captured.bindings()[&id("inner")].clone();
        let bindings = AudioBindingState::new(
            captured
                .timings()
                .iter()
                .map(|(id, layout)| AudioTimingRecord {
                    id: id.clone(),
                    layout: layout.clone(),
                })
                .collect(),
            BTreeMap::from([(id("inner"), binding)]),
        )
        .unwrap();
        let mut wire = serde_json::to_value(doc).unwrap();
        wire["audio_bindings"] = serde_json::to_value(bindings).unwrap();
        ProjectDocument::from_json(&wire.to_string()).unwrap()
    } else {
        doc
    };
    let plan = compile(&doc, false);
    let current = stage(plan.audio_signal());
    let signal = current.input_signal();
    let input = AudioSignalTape::new(
        &plan,
        ExactRatio::ZERO..ratio(1280, 1),
        vec![
            AudioSignalTapeRun::new(
                ExactRatio::ZERO..ratio(256, 1),
                ExactRatio::ZERO..ratio(256, 1),
                signal.clone(),
            ),
            AudioSignalTapeRun::new(
                ratio(256, 1)..ratio(1280, 1),
                ratio(256, 1)..ratio(512, 1),
                signal.clone(),
            ),
        ],
    )
    .unwrap();
    let policy = AudioSignalTape::new(
        &plan,
        ExactRatio::ZERO..ratio(640, 1),
        vec![AudioSignalTapeRun::new(
            ExactRatio::ZERO..ratio(640, 1),
            ExactRatio::ZERO..ratio(256, 1),
            signal,
        )],
    )
    .unwrap();
    let projection = AudioStageProjection::new(current, input, policy, frames(640)).unwrap();
    let mut provider = FixtureProvider::new();
    assert!(matches!(
        StageAudio::new(Arc::clone(&plan)).read_tape(
            &mut provider,
            &output(&plan, projection),
            SignalSample(0),
            1,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Unsupported("effect tails"))
    ));
    assert_eq!(
        provider.calls, 0,
        "hidden child history must preflight before the earlier raw prefix"
    );
}
