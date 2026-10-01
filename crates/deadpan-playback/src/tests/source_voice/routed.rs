//! Retained edit routes over complete, independently rendered qualified PCM.

use super::*;

use deadpan_plan::{
    AudioBoundaryRule, AudioProjectedRoot, AudioRootPlacement, AudioRoutedRoot, AudioRoutedSignal,
    AudioSampleGrid, AudioSoundRoute,
};

fn q(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}

fn frames(start: i64, end: i64) -> ExactFrameRange {
    ExactFrameRange {
        start: ExactRatio::integer(start),
        end: ExactRatio::integer(end),
    }
}

fn insertion(extent: i64, at: i64) -> SoundRippleMap {
    SoundRippleMap::new(
        ExactRatio::integer(extent),
        3,
        vec![
            SoundRippleNode::Keep {
                range: frames(0, at),
            },
            SoundRippleNode::Gap {
                duration: ExactRatio::ONE,
            },
            SoundRippleNode::Keep {
                range: frames(at, extent),
            },
            SoundRippleNode::Sequence {
                parts: vec![0, 1, 2],
            },
        ],
    )
    .unwrap()
}

fn twice_inserted() -> SoundRoute {
    SoundRoute::identity(ExactRatio::integer(6))
        .unwrap()
        .ripple(insertion(6, 1))
        .unwrap()
        .ripple(insertion(7, 3))
        .unwrap()
}

fn point_route(route: SoundRoute) -> AudioSoundRoute<SignalSample> {
    let grids =
        vec![
            AudioSampleGrid::new(ExactRatio::ZERO, q(5, 8008), AudioBoundaryRule::PointCeil)
                .unwrap();
            route.nodes().len()
        ];
    AudioSoundRoute::<SignalSample>::new(route, grids).unwrap()
}

fn root_route(route: SoundRoute) -> AudioSoundRoute<AudioSample> {
    let grids =
        vec![
            AudioSampleGrid::new(ExactRatio::ZERO, q(5, 8008), AudioBoundaryRule::RoundEven)
                .unwrap();
            route.nodes().len()
        ];
    AudioSoundRoute::<AudioSample>::new(route, grids).unwrap()
}

fn registered(
    input_frames: i64,
    preserve_frames: Option<i64>,
) -> (tempfile::TempDir, ProjectStore, AssetId) {
    let directory = tempfile::tempdir().unwrap();
    let initial = ProjectDocument::new(
        ProjectId::new("routed-pcm").unwrap(),
        revision("initial"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    let mut store =
        ProjectStore::create(&directory.path().join("routed.deadpan"), &initial).unwrap();
    register(&mut store);
    let asset = register_mono(&mut store, directory.path());
    commit(
        &mut store,
        "split",
        Command::Split {
            node: node("source"),
            at: FrameDuration::new(input_frames).unwrap(),
            identities: SplitIdentities {
                nodes: ["left", "right", "right-source"].map(node).to_vec(),
            },
        },
    );
    if let Some(output) = preserve_frames {
        commit(
            &mut store,
            "preserve",
            Command::WrapRetime {
                node: node("left"),
                id: node("preserve"),
                duration: FrameDuration::new(output).unwrap(),
                pitch: PitchPolicy::Preserve,
            },
        );
    }
    (directory, store, asset)
}

fn six_frame_projection<'plan>(
    plan: &'plan RenderPlan,
    document: &ProjectDocument,
    asset: &AssetId,
) -> Arc<AudioStageProjection<'plan>> {
    let current = stage(
        plan.audio_definition(AudioDefinitionSelector::Node {
            node: node("preserve"),
        })
        .unwrap()
        .signal(),
    );
    assert_eq!(current.descriptor().rate, q(3, 2));
    let voice = current
        .input_signal()
        .source_voice(recipe(document, asset, 44_117, 137))
        .unwrap();
    let input = tape(plan, voice.input_signal());
    let policy = AudioSignalTape::new(
        plan,
        ExactRatio::ZERO..ExactRatio::integer(6),
        vec![AudioSignalTapeRun::new(
            ExactRatio::ZERO..ExactRatio::integer(6),
            ExactRatio::ZERO..ExactRatio::integer(9),
            voice.output_signal(),
        )],
    )
    .unwrap();
    AudioStageProjection::new(current, input, policy, FrameDuration::new(6).unwrap()).unwrap()
}

fn projected_root(projection: Arc<AudioStageProjection<'_>>) -> AudioProjectedRoot<'_> {
    AudioProjectedRoot::new(
        projection,
        AudioRootPlacement::new(
            ExactRatio::ZERO,
            ExactRatio::ONE,
            ExactRatio::ZERO..ExactRatio::integer(6),
        )
        .unwrap(),
    )
    .unwrap()
}

// These boundaries are arithmetic on the fixture's 8008/5 samples per frame,
// independent of every routed query, sampling map and recipe accessor.
fn point_boundary(frame: usize) -> usize {
    (frame * 8008).div_ceil(5)
}

fn root_boundary(frame: usize) -> usize {
    (frame * 8008 + 2) / 5
}

fn copy_insertion(previous: &[[f32; 2]], cut: usize, resume: usize, end: usize) -> Vec<[f32; 2]> {
    let mut next = previous[..cut].to_vec();
    next.resize(resume, [0.0; 2]);
    next.extend(previous[cut..].iter().copied().take(end - resume));
    next.resize(end, [0.0; 2]);
    next
}

fn dense_twice(raw: &[[f32; 2]], boundary: fn(usize) -> usize) -> Vec<[f32; 2]> {
    let first = copy_insertion(raw, boundary(1), boundary(2), boundary(7));
    copy_insertion(&first, boundary(3), boundary(4), boundary(8))
}

fn shuffled_blocks(length: usize, first: usize) -> Vec<(i64, u32)> {
    std::iter::once(first)
        .chain((0..length).step_by(256).rev())
        .map(|start| {
            (
                i64::try_from(start).unwrap(),
                u32::try_from((length - start).min(256)).unwrap(),
            )
        })
        .collect()
}

#[test]
fn point_ceil_route_copies_real_pcm_across_two_ntsc_insertions_without_changing_original() {
    let _permit = crate::tests::resources::pcm();
    let (_directory, store, asset) = registered(6, None);
    let saved = store.snapshot().unwrap();
    let history = store.history_availability().unwrap();
    let captured = snapshot(&store, 1);
    let plan = Arc::new(RenderPlan::compile(&saved).unwrap());
    let voice = plan
        .audio_definition(AudioDefinitionSelector::Node { node: node("left") })
        .unwrap()
        .signal()
        .source_voice(recipe(&saved, &asset, 44_117, 137))
        .unwrap();
    let routed = AudioRoutedSignal::source(voice, point_route(twice_inserted())).unwrap();
    let raw = reference(44_117, 137, point_boundary(6));
    let expected = dense_twice(&raw, point_boundary);
    assert_eq!(expected[point_boundary(4)], raw[3203]);
    assert_ne!(
        raw[3203], raw[3204],
        "the seam witness must distinguish adjacent PCM"
    );
    assert!(raw[3203][0].abs() > 0.000_001);
    let mut renderer = StageAudio::new(plan.clone());
    let mut sources = Sources::new(captured);
    let original = renderer
        .read(&mut sources, AudioSample(0), 256, TIMEOUT, &cancelled())
        .unwrap()
        .samples;
    // The first routed read starts in the second suffix, with no routed prefix
    // preparation. Every later block is visited in reverse physical order.
    for (start, count) in shuffled_blocks(expected.len(), point_boundary(4)) {
        let actual = renderer
            .read_routed_signal(
                &mut sources,
                &routed,
                SignalSample(start),
                count,
                TIMEOUT,
                &cancelled(),
            )
            .unwrap();
        let start = usize::try_from(start).unwrap();
        assert_eq!(
            actual.samples,
            expected[start..start + usize::try_from(count).unwrap()]
        );
    }
    assert_eq!(
        renderer
            .read(&mut sources, AudioSample(0), 256, TIMEOUT, &cancelled())
            .unwrap()
            .samples,
        original
    );
    assert_eq!(store.snapshot().unwrap(), saved);
    assert_eq!(store.history_availability().unwrap(), history);
}

#[test]
fn round_even_route_keeps_complete_preserve_history_when_the_second_suffix_is_read_cold() {
    let _permit = crate::tests::resources::pcm();
    let (_directory, store, asset) = registered(9, Some(6));
    let saved = store.snapshot().unwrap();
    let captured = snapshot(&store, 1);
    let plan = Arc::new(RenderPlan::compile(&saved).unwrap());
    let projection = six_frame_projection(&plan, &saved, &asset);
    let routed = AudioRoutedRoot::new(
        projected_root(projection.clone()),
        root_route(twice_inserted()),
    )
    .unwrap();
    // Prepare the complete nine-frame input directly from decoded fixture PCM,
    // then run the canonical processor once before doing any route copies.
    // The declared input selection constrains its filter taps at frame9:
    // ceil((9 * 8008/5 - 137) * 147/160) = 13118 source samples.
    // This support boundary is distinct from both the selected catalog span
    // and the later route fragments; only the complete input selection owns it.
    let input_support_end = ((9 * 8008 - 137 * 5) * 147 + 5 * 160 - 1) / (5 * 160);
    assert_eq!(input_support_end, 13_118);
    let raw = stretch(
        &reference(input_support_end, 137, point_boundary(9)),
        u32::try_from(point_boundary(6)).unwrap(),
    );
    let expected = dense_twice(&raw, root_boundary);
    assert_eq!(root_boundary(4), 6406);
    assert_eq!(expected[6406], raw[3204]);
    assert_ne!(
        raw[3204], raw[3203],
        "the processed seam must distinguish adjacent PCM"
    );
    assert!(raw[3204][0].abs() > 0.000_001);
    let mut renderer = StageAudio::new(plan.clone());
    let mut sources = Sources::new(captured.clone());
    for (start, count) in shuffled_blocks(expected.len(), 6406) {
        let actual = renderer
            .read_routed_root(
                &mut sources,
                &routed,
                AudioSample(start),
                count,
                TIMEOUT,
                &cancelled(),
            )
            .unwrap();
        let start = usize::try_from(start).unwrap();
        assert_eq!(
            actual.samples,
            expected[start..start + usize::try_from(count).unwrap()]
        );
    }

    // The same complete projection can be consumed on the distinct intrinsic
    // PointCeil clock. Start its second suffix cold in a separate PCM engine.
    let intrinsic =
        AudioRoutedSignal::projected(projection.clone(), point_route(twice_inserted())).unwrap();
    let intrinsic_expected = dense_twice(&raw, point_boundary);
    assert_eq!(point_boundary(4), 6407);
    assert_eq!(intrinsic_expected[6407], raw[3203]);
    let mut renderer = StageAudio::new(plan.clone());
    let mut sources = Sources::new(captured.clone());
    for (start, count) in shuffled_blocks(intrinsic_expected.len(), 6407) {
        let actual = renderer
            .read_routed_signal(
                &mut sources,
                &intrinsic,
                SignalSample(start),
                count,
                TIMEOUT,
                &cancelled(),
            )
            .unwrap();
        let start = usize::try_from(start).unwrap();
        assert_eq!(
            actual.samples,
            intrinsic_expected[start..start + usize::try_from(count).unwrap()]
        );
    }

    // A fractional crop must select the already processed full output, never
    // prepare a shortened input history when its interior is requested cold.
    let cropped = AudioRoutedRoot::new(
        projected_root(projection),
        root_route(
            SoundRoute::identity(ExactRatio::integer(6))
                .unwrap()
                .window(ExactFrameRange {
                    start: q(1, 3),
                    end: q(8, 3),
                })
                .unwrap(),
        ),
    )
    .unwrap();
    let expected_crop = &raw[534..4271];
    let mut renderer = StageAudio::new(plan.clone());
    let mut sources = Sources::new(captured);
    for (start, count) in shuffled_blocks(expected_crop.len(), 3000) {
        let actual = renderer
            .read_routed_root(
                &mut sources,
                &cropped,
                AudioSample(start),
                count,
                TIMEOUT,
                &cancelled(),
            )
            .unwrap();
        let start = usize::try_from(start).unwrap();
        assert_eq!(
            actual.samples,
            expected_crop[start..start + usize::try_from(count).unwrap()]
        );
    }
}

fn wrongly_cropped_kernel(end: i64) -> Vec<[f32; 2]> {
    let samples = wave_bytes()[44..]
        .chunks_exact(2)
        .map(|bytes| f32::from(i16::from_le_bytes(bytes.try_into().unwrap())) / 32768.0)
        .collect::<Vec<_>>();
    // Window starts at old sample534, whose source position is364.74375.
    // A broken fragment reader might retain its phase but drop prior sinc taps.
    let sampler = Resampler::new(
        ResampleRecipe::new(
            365..end,
            ExactRatio::ZERO,
            AudioSample(137),
            q(147, 160),
            AudioSample(534)..AudioSample(550),
        )
        .unwrap(),
        StereoMatrix::new(MONO_LAYOUT).unwrap(),
    );
    let range = sampler
        .required_source_range(AudioSample(534), 16)
        .unwrap()
        .unwrap();
    sampler
        .render(
            AudioSample(534),
            16,
            Some(PcmWindow {
                start: range.start,
                samples: samples
                    [usize::try_from(range.start).unwrap()..usize::try_from(range.end).unwrap()]
                    .to_vec(),
            }),
            &cancelled(),
        )
        .unwrap()
        .samples
}

#[test]
fn fractional_window_keeps_filter_support_old_source_masks_and_prior_gap_silence() {
    let _permit = crate::tests::resources::pcm();
    let (_directory, store, asset) = registered(6, None);
    let saved = store.snapshot().unwrap();
    let captured = snapshot(&store, 1);
    let plan = Arc::new(RenderPlan::compile(&saved).unwrap());
    let route = SoundRoute::identity(ExactRatio::integer(6))
        .unwrap()
        .ripple(insertion(6, 1))
        .unwrap()
        .window(ExactFrameRange {
            start: q(1, 3),
            end: q(8, 3),
        })
        .unwrap();
    for end in [44_117, 2205] {
        let voice = plan
            .audio_definition(AudioDefinitionSelector::Node { node: node("left") })
            .unwrap()
            .signal()
            .source_voice(recipe(&saved, &asset, end, 137))
            .unwrap();
        let routed = AudioRoutedSignal::source(voice, point_route(route.clone())).unwrap();
        let raw = reference(end, 137, point_boundary(6));
        assert_ne!(&raw[534..550], wrongly_cropped_kernel(end));
        let first = copy_insertion(
            &raw,
            point_boundary(1),
            point_boundary(2),
            point_boundary(7),
        );
        // ceil((1/3)*8008/5)=534; ceil((8/3)*8008/5)=4271.
        // The new zero-origin 7/3-frame Window has3738 slots, one more than
        // that old selected output. Its final slot must remain silent.
        let mut expected = first[534..4271].to_vec();
        expected.resize(3738, [0.0; 2]);
        assert_eq!(expected[3737], [0.0; 2]);
        if end == 44_117 {
            assert_ne!(
                first[4271], [0.0; 2],
                "adjacent full-support PCM must not leak past the old cut"
            );
        } else {
            assert!(expected[3605..].iter().all(|sample| *sample == [0.0; 2]));
        }
        assert!(
            expected[1068..2670]
                .iter()
                .all(|sample| *sample == [0.0; 2])
        );
        let mut renderer = StageAudio::new(plan.clone());
        let mut sources = Sources::new(captured.clone());
        for (start, count) in shuffled_blocks(expected.len(), 0) {
            let actual = renderer
                .read_routed_signal(
                    &mut sources,
                    &routed,
                    SignalSample(start),
                    count,
                    TIMEOUT,
                    &cancelled(),
                )
                .unwrap();
            let start = usize::try_from(start).unwrap();
            assert_eq!(
                actual.samples,
                expected[start..start + usize::try_from(count).unwrap()]
            );
        }
    }
}

fn gap_route() -> SoundRoute {
    SoundRoute::identity(ExactRatio::integer(6))
        .unwrap()
        .ripple(
            SoundRippleMap::new(
                ExactRatio::integer(6),
                0,
                vec![SoundRippleNode::Gap {
                    duration: ExactRatio::integer(6),
                }],
            )
            .unwrap(),
        )
        .unwrap()
}

#[test]
fn masked_routes_still_admit_complete_live_dependencies_and_reject_foreign_plans() {
    let _permit = crate::tests::resources::pcm();
    let (_directory, store, asset) = registered(9, Some(6));
    let saved = store.snapshot().unwrap();
    let captured = snapshot(&store, 1);
    let plan = Arc::new(RenderPlan::compile(&saved).unwrap());
    let projection = six_frame_projection(&plan, &saved, &asset);
    let root = AudioRoutedRoot::new(projected_root(projection), root_route(gap_route())).unwrap();
    let voice = plan
        .audio_definition(AudioDefinitionSelector::Node {
            node: node("preserve"),
        })
        .unwrap()
        .signal()
        .source_voice(recipe(&saved, &asset, 44_117, 137))
        .unwrap();
    let signal = AudioRoutedSignal::source(voice, point_route(gap_route())).unwrap();
    let mut missing = Snapshot::committed(
        captured.session,
        captured.document.clone(),
        captured.sources.as_ref().clone(),
        store.original_import_handle().unwrap(),
    );
    Arc::make_mut(&mut missing.sources).remove(&asset);
    let missing = Arc::new(missing);
    assert!(
        StageAudio::new(plan.clone())
            .read_routed_signal(
                &mut Sources::new(missing.clone()),
                &signal,
                SignalSample(0),
                1,
                TIMEOUT,
                &cancelled()
            )
            .is_err()
    );
    assert!(
        StageAudio::new(plan.clone())
            .read_routed_root(
                &mut Sources::new(missing),
                &root,
                AudioSample(0),
                1,
                TIMEOUT,
                &cancelled()
            )
            .is_err()
    );
    let foreign = Arc::new(RenderPlan::compile(&saved).unwrap());
    assert!(matches!(
        StageAudio::new(foreign.clone()).read_routed_signal(
            &mut Sources::new(captured.clone()),
            &signal,
            SignalSample(0),
            1,
            TIMEOUT,
            &cancelled()
        ),
        Err(deadpan_audio::StageAudioError::ForeignDomain)
    ));
    assert!(matches!(
        StageAudio::new(foreign).read_routed_root(
            &mut Sources::new(captured.clone()),
            &root,
            AudioSample(0),
            1,
            TIMEOUT,
            &cancelled()
        ),
        Err(deadpan_audio::StageAudioError::ForeignDomain)
    ));
    assert_eq!(
        StageAudio::new(plan.clone())
            .read_routed_root(
                &mut Sources::new(captured.clone()),
                &root,
                AudioSample(0),
                1,
                TIMEOUT,
                &cancelled()
            )
            .unwrap()
            .samples,
        vec![[0.0; 2]]
    );
    drop(store);
    assert!(
        StageAudio::new(plan.clone())
            .read_routed_signal(
                &mut Sources::new(captured.clone()),
                &signal,
                SignalSample(0),
                1,
                TIMEOUT,
                &cancelled()
            )
            .is_err(),
        "a silent route cannot hide a revoked source capture"
    );
    assert!(
        StageAudio::new(plan.clone())
            .read_routed_root(
                &mut Sources::new(captured),
                &root,
                AudioSample(0),
                1,
                TIMEOUT,
                &cancelled()
            )
            .is_err(),
        "a silent route cannot hide revoked full Preserve input"
    );
}
