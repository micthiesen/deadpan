use super::*;
use deadpan_audio::DomainSignalTransfer;
use deadpan_plan::{AudioDefinitionSelector, AudioRootPlacement};

fn selected(
    rate: FrameRate,
    sample_rate: u32,
    frames: i64,
    start: ExactRatio,
    window: ExactFrameRange,
    offset: i64,
) -> BeatNode {
    let original = audio_at_rate(100, 2100, sample_rate);
    let mut leaf = source(rate, frames, 0..1);
    let NodeKind::Source { source } = &mut leaf.kind else {
        unreachable!()
    };
    source.audio_mapping = SourceAudioMapping::SelectedPlacement {
        start,
        frames: SourceAudioMapping::natural_rate(original.span, rate)
            .unwrap()
            .duration_frames(duration(frames))
            .unwrap(),
        selection: window,
    };
    source.audio = Some(original);
    source.audio_offset = AudioSample(offset);
    leaf
}

fn fixture(
    sample_rate: u32,
    leaf: BeatNode,
    rate: FrameRate,
) -> (ProjectDocument, FixtureProvider) {
    let (provider, end) = if sample_rate == 48_000 {
        (FixtureProvider::new(), 8197)
    } else {
        (
            FixtureProvider::from_fixture(
                "pcm-mono-44100.wav",
                AudioChannelLayout::Native {
                    channels: 1,
                    mask: 4,
                },
            ),
            44_117,
        )
    };
    (
        document_with_asset(
            rate,
            &["source"],
            [("source", leaf)],
            BTreeMap::new(),
            audio_at_rate(0, end, sample_rate).span,
        ),
        provider,
    )
}

fn capture(document: &ProjectDocument) -> ProjectDocument {
    let mut wire = serde_json::to_value(document).unwrap();
    wire["audio_bindings"] = serde_json::to_value(
        capture_unbound_audio_bindings(
            document,
            AudioTimingId {
                allocation: RevisionId::new("selected-capture").unwrap(),
                ordinal: 0,
            },
        )
        .unwrap(),
    )
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

#[test]
fn selected_empty_host_intersections_and_zero_point_windows_do_not_read_media() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    for window in [
        ExactFrameRange::new(ExactRatio::integer(-2), ExactRatio::integer(-1)).unwrap(),
        ExactFrameRange::new(ExactRatio::integer(1000), ExactRatio::integer(1200)).unwrap(),
        ExactFrameRange::new(ratio(1, 10), ratio(1, 5)).unwrap(),
    ] {
        let (unbound, mut provider) = fixture(
            48_000,
            selected(rate, 48_000, 400, ExactRatio::integer(-3), window, 0),
            rate,
        );
        for doc in [unbound.clone(), capture(&unbound)] {
            let plan = Arc::new(RenderPlan::compile(&doc).unwrap());
            let definition = plan
                .audio_definition(AudioDefinitionSelector::Node { node: id("source") })
                .unwrap();
            let point = definition
                .in_point_clock(
                    AudioRootPlacement::new(
                        ExactRatio::ZERO,
                        ExactRatio::ONE,
                        ExactRatio::ZERO..ExactRatio::integer(400),
                    )
                    .unwrap(),
                    ExactRatio::ZERO,
                )
                .unwrap();
            let mut renderer = StageAudio::new(Arc::clone(&plan));
            assert!(
                read_all(&mut renderer, &mut provider, &[199, 1])
                    .iter()
                    .all(|v| *v == [0.; 2])
            );
            let block = renderer
                .read_point_domain(
                    &mut provider,
                    &point,
                    deadpan_plan::ReferenceSample(0),
                    256,
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap();
            assert!(block.samples.iter().all(|v| *v == [0.; 2]));
            assert_eq!(
                provider.calls, 0,
                "empty selected operand must not resolve source media"
            );
        }
    }
}

#[test]
fn changing_a_selected_window_keeps_retained_phase_and_uses_current_support() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let (original, mut provider) = fixture(
        48_000,
        selected(
            rate,
            48_000,
            400,
            ratio(-3, 7),
            ExactFrameRange::new(ExactRatio::integer(100), ExactRatio::integer(150)).unwrap(),
            0,
        ),
        rate,
    );
    let bound = capture(&original);
    let edited = apply(
        &bound,
        &CommandRequest {
            project_id: bound.project_id().clone(),
            expected_revision: bound.revision_id().clone(),
            new_revision: RevisionId::new("selected-window-change").unwrap(),
            command: Command::SetSourceAudioMapping {
                node: id("source"),
                offset: AudioSample(0),
                mapping: SourceAudioMapping::SelectedPlacement {
                    start: ratio(-3, 7),
                    frames: ExactRatio::integer(2000),
                    selection: ExactFrameRange::new(
                        ExactRatio::integer(200),
                        ExactRatio::integer(250),
                    )
                    .unwrap(),
                },
            },
        },
    )
    .unwrap();
    let current = edited.forward.apply(&bound).unwrap();
    assert_eq!(edited.inverse.apply(&current).unwrap(), bound);
    provider.revisions.insert(current.revision_id().clone());
    let mut renderer = StageAudio::new(Arc::new(RenderPlan::compile(&current).unwrap()));
    let mut actual = Vec::new();
    for (start, count) in [(0, 251), (251, 149)] {
        let block = renderer
            .read(
                &mut provider,
                AudioSample(start),
                count,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(&block.revision_id, current.revision_id());
        assert_eq!(&block.project_id, current.project_id());
        assert_eq!(block.start, AudioSample(start));
        actual.extend(block.samples);
    }
    let expected = provider
        .source
        .prepare(
            ResampleRecipe::new(
                301..351,
                ratio(2103, 7),
                AudioSample(200),
                ExactRatio::ONE,
                AudioSample(200)..AudioSample(250),
            )
            .unwrap(),
            AudioSample(200),
            50,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap()
        .samples;
    assert_eq!(actual[200..250], expected);
    assert!(
        actual[..200]
            .iter()
            .chain(&actual[250..])
            .all(|v| *v == [0.; 2])
    );
    assert!(actual[200..250].iter().any(|v| *v != [0.; 2]));
}

#[test]
fn selected_source_follow_speed_uses_exact_window_on_its_retained_physical_grid() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let start = ratio(-3, 7);
    let window = ExactFrameRange::new(ratio(101, 3), ratio(511, 3)).unwrap();
    let leaf = selected(rate, 48_000, 256, start, window, 0);
    let doc = document_with_asset(
        rate,
        &["tape"],
        [
            ("source", leaf),
            (
                "tape",
                retime("source", 384, 0..256, PitchPolicy::FollowSpeed),
            ),
        ],
        BTreeMap::new(),
        audio(0, 8197).span,
    );
    let mut provider = FixtureProvider::new();
    let mut renderer = StageAudio::new(Arc::new(RenderPlan::compile(&doc).unwrap()));
    let actual = read_all(&mut renderer, &mut provider, &[61, 3, 256]);
    let source_at = |local: ExactRatio| {
        ExactRatio::integer(100)
            .checked_add(local.checked_sub(start).unwrap())
            .unwrap()
    };
    let support = i64::try_from(source_at(window.start).ceil().unwrap()).unwrap()
        ..i64::try_from(source_at(window.end).ceil().unwrap()).unwrap();
    let expected = provider
        .source
        .prepare(
            ResampleRecipe::new(
                support,
                source_at(ratio(100, 3)),
                AudioSample(50),
                ratio(2, 3),
                AudioSample(50)..AudioSample(256),
            )
            .unwrap(),
            AudioSample(50),
            206,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap()
        .samples;
    assert_eq!(actual[50..256], expected);
    assert!(
        actual[..50]
            .iter()
            .chain(&actual[256..])
            .all(|v| *v == [0.; 2])
    );
    let mut wire = serde_json::to_value(&doc).unwrap();
    wire["audio_bindings"] = serde_json::to_value(
        capture_unbound_audio_bindings(
            &doc,
            AudioTimingId {
                allocation: RevisionId::new("selected-tape-clock").unwrap(),
                ordinal: 0,
            },
        )
        .unwrap(),
    )
    .unwrap();
    let bound = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let mut renderer = StageAudio::new(Arc::new(RenderPlan::compile(&bound).unwrap()));
    assert_eq!(read_all(&mut renderer, &mut provider, &[7, 127]), actual);
}

#[test]
fn fractional_selected_source_preserves_original_phase_and_excludes_outside_taps() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    for sample_rate in [44_100, 48_000] {
        let start = ratio(-111, 7);
        let selection = ExactFrameRange::new(ratio(1001, 7), ratio(1901, 7)).unwrap();
        let (doc, mut provider) = fixture(
            sample_rate,
            selected(rate, sample_rate, 400, start, selection, 3),
            rate,
        );
        let plan = Arc::new(RenderPlan::compile(&doc).unwrap());
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        let actual = read_all(&mut renderer, &mut provider, &[19, 251, 1]);
        // Independent output allocation and affine source origin. Only source
        // sample positions in the exact fractional crop may reach the reader.
        let left = 146;
        let right = 275;
        let speed = ratio(i128::from(sample_rate), 48_000);
        let point = |local: ExactRatio| {
            ExactRatio::integer(100)
                .checked_add(
                    local
                        .checked_sub(start)
                        .unwrap()
                        .checked_mul(speed)
                        .unwrap(),
                )
                .unwrap()
        };
        let support = i64::try_from(point(selection.start).ceil().unwrap()).unwrap()
            ..i64::try_from(point(selection.end).ceil().unwrap()).unwrap();
        let recipe = ResampleRecipe::new(
            support.clone(),
            point(ExactRatio::integer(left - 3)),
            AudioSample(left),
            speed,
            AudioSample(left)..AudioSample(right),
        )
        .unwrap();
        let required = Resampler::new(
            recipe.clone(),
            StereoMatrix::new(provider.source.matrix_layout()).unwrap(),
        )
        .required_source_range(AudioSample(left), (right - left) as u32)
        .unwrap()
        .unwrap();
        assert!(required.start >= support.start && required.end <= support.end);
        let expected = provider
            .source
            .prepare(
                recipe,
                AudioSample(left),
                (right - left) as u32,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap()
            .samples;
        assert_eq!(actual[left as usize..right as usize], expected);
        assert!(
            actual[..left as usize]
                .iter()
                .chain(&actual[right as usize..])
                .all(|v| *v == [0.; 2])
        );
        let leaked = provider
            .source
            .prepare(
                ResampleRecipe::new(
                    100..2100,
                    point(ExactRatio::integer(left - 3)),
                    AudioSample(left),
                    speed,
                    AudioSample(left)..AudioSample(right),
                )
                .unwrap(),
                AudioSample(left),
                (right - left) as u32,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap()
            .samples;
        assert_ne!(
            expected, leaked,
            "full measured span must not supply excluded filter taps"
        );
        assert_eq!(
            read_all(&mut StageAudio::new(plan), &mut provider, &[256, 7]),
            actual
        );
    }
}

#[test]
fn selection_masks_physical_transfer_but_preserve_retains_its_decay() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let selection = ExactFrameRange::new(ratio(1025, 2), ratio(4095, 2)).unwrap();
    let leaf = selected(rate, 48_000, 3072, ratio(1, 3), selection, 0);
    // This selected source has a longer full stream so both fractional ends
    // are contained without fitting the 3072-frame authored beat.
    let mut leaf = leaf;
    let NodeKind::Source { source } = &mut leaf.kind else {
        unreachable!()
    };
    source.audio = Some(audio(100, 3100));
    source.audio_mapping = SourceAudioMapping::SelectedPlacement {
        start: ratio(1, 3),
        frames: ExactRatio::integer(3000),
        selection,
    };
    let (doc, mut provider) = fixture(48_000, leaf.clone(), rate);
    let plan = Arc::new(RenderPlan::compile(&doc).unwrap());
    let definition = plan
        .audio_definition(AudioDefinitionSelector::Node { node: id("source") })
        .unwrap();
    let domain = definition
        .in_root_clock(
            AudioRootPlacement::new(
                ExactRatio::ZERO,
                ExactRatio::ONE,
                ExactRatio::ZERO..ExactRatio::integer(3072),
            )
            .unwrap(),
        )
        .unwrap();
    let transfer = DomainSignalTransfer::new(
        domain,
        ratio(4093, 2),
        SignalSample(0),
        ratio(1, 2),
        SignalSample(0)..SignalSample(20),
    )
    .unwrap();
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let actual = renderer
        .read_domain_transferred(
            &mut provider,
            &transfer,
            SignalSample(0),
            20,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    // RootRoundEven allocates the fractional end to 2048. At/after that
    // physical endpoint interpolation must not ring from the selected voice.
    assert!(actual.samples[3..].iter().all(|v| *v == [0.; 2]));
    assert!(actual.samples[..3].iter().any(|v| *v != [0.; 2]));
    let input_definition = definition.signal();
    let mut input = Vec::new();
    while input.len() < input_definition.sample_count().unwrap().0 as usize {
        let n = (3072 - input.len()).min(256) as u32;
        input.extend(
            renderer
                .read_definition(
                    &mut provider,
                    &definition,
                    SignalSample(input.len() as i64),
                    n,
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap()
                .samples,
        );
    }
    let expected = stretch_reference(&input, 4608, 2, 3);
    let stretched = document_with_asset(
        rate,
        &["stretch"],
        [
            ("source", leaf),
            (
                "stretch",
                retime("source", 4608, 0..3072, PitchPolicy::Preserve),
            ),
        ],
        BTreeMap::new(),
        audio(0, 8197).span,
    );
    let mut renderer = StageAudio::new(Arc::new(RenderPlan::compile(&stretched).unwrap()));
    let actual = read_all(&mut renderer, &mut provider, &[151, 1, 256]);
    for (index, (actual, expected)) in actual.iter().zip(&expected).enumerate() {
        assert_eq!(actual, expected, "Preserve output sample {index}");
    }
    assert!(
        actual[3072..].iter().any(|v| *v != [0.; 2]),
        "a selection endpoint must not become a silent Hold after Preserve"
    );
    let bindings = capture_unbound_audio_bindings(
        &stretched,
        AudioTimingId {
            allocation: RevisionId::new("selection-capture").unwrap(),
            ordinal: 0,
        },
    )
    .unwrap();
    let mut wire = serde_json::to_value(&stretched).unwrap();
    wire["audio_bindings"] = serde_json::to_value(bindings).unwrap();
    let captured = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let mut captured_renderer = StageAudio::new(Arc::new(RenderPlan::compile(&captured).unwrap()));
    let captured_pcm = read_all(&mut captured_renderer, &mut provider, &[17, 256]);
    for (index, (actual, expected)) in captured_pcm.iter().zip(&expected).enumerate() {
        assert_eq!(actual, expected, "captured Preserve output sample {index}");
    }
}
