//! Real catalog media through the shared PCM engine, without authored sound beats.

#[path = "source_voice/routed.rs"]
mod routed;

#[path = "source_voice/events.rs"]
mod events;

use super::*;

use deadpan_audio::{PcmWindow, ResampleRecipe, Resampler, StageAudio, StereoMatrix};
use deadpan_dsp::{CanonicalRecipe, CanonicalStretch, StereoPcm, StretchRate};
use deadpan_media::audio_index::AudioChannelLayout;
use deadpan_plan::{
    AudioDefinitionSelector, AudioHoldIssuer, AudioQueryLimits, AudioSignal, AudioSignalContent,
    AudioSignalTape, AudioSignalTapeRun, AudioSourceVoiceRecipe, AudioStage, AudioStageProjection,
    SignalSample,
};

const TIMEOUT: Duration = Duration::from_secs(60);
const MONO_LAYOUT: AudioChannelLayout = AudioChannelLayout::Native {
    channels: 1,
    mask: 4,
};

fn wave_bytes() -> Vec<u8> {
    let bytes = std::fs::read(
        fixture()
            .parent()
            .unwrap()
            .join("../audio-fixtures/pcm-mono-44100.wav"),
    )
    .unwrap();
    // This fixture has a plain PCM header and intentionally declares no speaker
    // positions. Keep its samples unchanged while declaring the test's explicit
    // mono-center interpretation in a standard extensible WAV container.
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(&bytes[8..16], b"WAVEfmt ");
    assert_eq!(u32::from_le_bytes(bytes[16..20].try_into().unwrap()), 16);
    assert_eq!(&bytes[20..24], &[1, 0, 1, 0]);
    assert_eq!(&bytes[36..40], b"data");
    assert_eq!(bytes.len(), 44 + 44_117 * 2);
    bytes
}

fn register_mono(store: &mut ProjectStore, directory: &std::path::Path) -> AssetId {
    let bytes = wave_bytes();
    let mut declared = Vec::with_capacity(bytes.len() + 24);
    declared.extend_from_slice(b"RIFF");
    declared.extend_from_slice(&u32::try_from(bytes.len() + 16).unwrap().to_le_bytes());
    declared.extend_from_slice(b"WAVEfmt ");
    declared.extend_from_slice(&40_u32.to_le_bytes());
    declared.extend_from_slice(&0xfffe_u16.to_le_bytes());
    declared.extend_from_slice(&bytes[22..36]);
    declared.extend_from_slice(&22_u16.to_le_bytes());
    declared.extend_from_slice(&16_u16.to_le_bytes());
    declared.extend_from_slice(&4_u32.to_le_bytes());
    // KSDATAFORMAT_SUBTYPE_PCM, encoded using the WAV GUID byte order.
    declared.extend_from_slice(&[
        1, 0, 0, 0, 0, 0, 0x10, 0, 0x80, 0, 0, 0xaa, 0, 0x38, 0x9b, 0x71,
    ]);
    declared.extend_from_slice(&bytes[36..]);
    assert_eq!(&declared[68..], &bytes[44..]);
    let path = directory.join("declared-mono-44100.wav");
    std::fs::write(&path, declared).unwrap();
    let original = store
        .retain_original(&path, OriginalOwnership::Managed, limits(), &cancelled())
        .unwrap()
        .record;
    let mut input = store
        .snapshot_original(original.object().content(), limits(), &cancelled())
        .unwrap();
    let audio = AudioSession::open_verified(
        &mut input,
        SourceContentIdentity::new(original.sha256(), original.object().byte_length()).unwrap(),
        0,
        AudioSessionLimits::default(),
        &cancelled(),
    )
    .unwrap();
    assert_eq!(audio.index().stream().channel_layout, MONO_LAYOUT);
    assert_eq!(audio.index().stream().sample_rate, 44_100);
    let decoded = DecodedSourceQualification::from_sessions(None, Some(&audio)).unwrap();
    let asset = AssetId::new("catalog-mono").unwrap();
    store
        .register_source(
            &SourceRegistration {
                expected_revision: store.snapshot().unwrap().revision_id().clone(),
                new_revision: revision("registered-catalog-mono"),
                original: original.object().content().clone(),
                new_asset_id: asset.clone(),
                label: "Explicit mono sound".into(),
                insertion: None,
            },
            &decoded,
            None,
            limits(),
            &cancelled(),
        )
        .unwrap();
    asset
}

fn recipe(
    document: &ProjectDocument,
    asset: &AssetId,
    end: i64,
    offset: i64,
) -> AudioSourceVoiceRecipe {
    let measured = document.assets()[asset].audio.unwrap();
    assert_eq!(measured.start().ticks, 0);
    assert_eq!(measured.end().ticks, 44_117);
    let source = SourceAudio {
        asset: asset.clone(),
        span: SourceSpan::new(
            measured.start(),
            SourceTimestamp {
                ticks: end,
                time_base: measured.start().time_base,
            },
        )
        .unwrap(),
    };
    AudioSourceVoiceRecipe {
        mapping: SourceAudioMapping::natural_rate(
            source.span,
            document.presentation_basis().frame_rate,
        )
        .unwrap(),
        source,
        offset: AudioSample(offset),
    }
}

fn tape<'plan>(plan: &'plan RenderPlan, signal: AudioSignal<'plan>) -> AudioSignalTape<'plan> {
    let support = signal.support();
    AudioSignalTape::new(
        plan,
        support.clone(),
        vec![AudioSignalTapeRun::new(support.clone(), support, signal)],
    )
    .unwrap()
}

fn reference(end: i64, offset: i64, count: usize) -> Vec<[f32; 2]> {
    let samples = wave_bytes()[44..]
        .chunks_exact(2)
        .map(|bytes| f32::from(i16::from_le_bytes(bytes.try_into().unwrap())) / 32768.0)
        .collect::<Vec<_>>();
    let sampler = Resampler::new(
        ResampleRecipe::new(
            0..end,
            ExactRatio::ZERO,
            AudioSample(offset),
            ExactRatio::new(147, 160).unwrap(),
            AudioSample(0)..AudioSample(i64::try_from(count).unwrap()),
        )
        .unwrap(),
        StereoMatrix::new(MONO_LAYOUT).unwrap(),
    );
    let mut result = Vec::with_capacity(count);
    while result.len() < count {
        let at = AudioSample(i64::try_from(result.len()).unwrap());
        let length = u32::try_from((count - result.len()).min(256)).unwrap();
        let window = sampler
            .required_source_range(at, length)
            .unwrap()
            .map(|range| PcmWindow {
                start: range.start,
                samples: samples
                    [usize::try_from(range.start).unwrap()..usize::try_from(range.end).unwrap()]
                    .to_vec(),
            });
        result.extend(
            sampler
                .render(at, length, window, &cancelled())
                .unwrap()
                .samples,
        );
    }
    // A placement's digital-silence mask is separate from the full sinc kernel.
    // At 44.1 kHz the final eligible sample is independently derived, not read
    // back from the plan or inferred from its enclosing project-frame duration.
    let audible_end = offset + (end * 160 + 146) / 147;
    for (index, sample) in result.iter_mut().enumerate() {
        let index = i64::try_from(index).unwrap();
        if index < offset || index >= audible_end {
            *sample = [0.0; 2];
        }
    }
    result
}

fn stretch(input: &[[f32; 2]], output_frames: u32) -> Vec<[f32; 2]> {
    let pcm = StereoPcm::new(
        input.iter().map(|frame| frame[0]).collect(),
        input.iter().map(|frame| frame[1]).collect(),
    )
    .unwrap();
    let recipe = CanonicalRecipe::with_rate(
        pcm.frames(),
        output_frames,
        StretchRate::new(3, 2).unwrap(),
        0,
    )
    .unwrap();
    let mut engine = CanonicalStretch::new(recipe, pcm).unwrap();
    let mut result = Vec::new();
    while result.len() < usize::try_from(output_frames).unwrap() {
        let count = (usize::try_from(output_frames).unwrap() - result.len()).min(256);
        let mut left = vec![0.0; count];
        let mut right = vec![0.0; count];
        assert_eq!(
            engine.read(&mut left, &mut right, &cancelled()).unwrap(),
            count
        );
        result.extend(
            left.into_iter()
                .zip(right)
                .map(|(left, right)| [left, right]),
        );
    }
    result
}

fn commit(store: &mut ProjectStore, name: &str, command: Command) {
    let current = store.snapshot().unwrap();
    store
        .commit(&CommandRequest {
            project_id: current.project_id().clone(),
            expected_revision: current.revision_id().clone(),
            new_revision: revision(name),
            command,
        })
        .unwrap();
}

fn stage(signal: AudioSignal<'_>) -> AudioStage<'_> {
    let query = signal
        .query(
            SignalSample(0)..SignalSample(1),
            AudioQueryLimits::default(),
        )
        .unwrap();
    let AudioSignalContent::Stage(stage) = query.spans.into_iter().next().unwrap().content else {
        panic!("expected Preserve stage");
    };
    stage
}

#[test]
fn catalog_voice_uses_exact_44100_phase_at_arbitrary_48000_onset_without_editing() {
    let _permit = crate::tests::resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let mut store =
        ProjectStore::create(&directory.path().join("voice.deadpan"), &empty()).unwrap();
    register(&mut store);
    let before_catalog = store.snapshot().unwrap();
    let asset = register_mono(&mut store, directory.path());
    let saved = store.snapshot().unwrap();
    assert_eq!(saved.nodes(), before_catalog.nodes());
    let history = store.history_availability().unwrap();
    let captured = snapshot(&store, 1);
    let plan = Arc::new(RenderPlan::compile(&saved).unwrap());
    let voice = plan
        .audio_signal()
        .source_voice(recipe(&saved, &asset, 44_117, 137))
        .unwrap();
    let input = tape(&plan, voice.input_signal());
    let output = tape(&plan, voice.output_signal());
    let expected = reference(44_117, 137, 48_300);
    assert!(expected[137..393].iter().any(|frame| frame[0].abs() > 0.1));
    let mut renderer = StageAudio::new(plan.clone());
    let mut sources = Sources::new(captured);
    let original = renderer
        .read(&mut sources, AudioSample(0), 256, TIMEOUT, &cancelled())
        .unwrap()
        .samples;
    assert_ne!(original, expected[..256]);
    for (start, count) in [
        (48_019, 256),
        (0, 256),
        (137, 256),
        (337, 113),
        (12_003, 71),
    ] {
        for selected in [&input, &output] {
            let actual = renderer
                .read_tape(
                    &mut sources,
                    selected,
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
    assert_eq!(
        renderer
            .read(&mut sources, AudioSample(0), 256, TIMEOUT, &cancelled())
            .unwrap()
            .samples,
        original,
        "catalog voice reads must not replace or contaminate Original PCM"
    );
    assert_eq!(store.snapshot().unwrap(), saved);
    assert_eq!(store.history_availability().unwrap(), history);
    assert_eq!(
        saved.nodes().len(),
        2,
        "only the Original belongs to the timeline"
    );
}

#[test]
fn silent_hold_suppresses_output_but_keeps_catalog_pcm_available_to_processing() {
    let _permit = crate::tests::resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let mut store =
        ProjectStore::create(&directory.path().join("voice.deadpan"), &hold(40)).unwrap();
    let asset = register_mono(&mut store, directory.path());
    let saved = store.snapshot().unwrap();
    let history = store.history_availability().unwrap();
    let captured = snapshot(&store, 1);
    let plan = Arc::new(RenderPlan::compile(&saved).unwrap());
    let voice = plan
        .audio_signal()
        .source_voice(recipe(&saved, &asset, 44_117, 0))
        .unwrap();
    let input = tape(&plan, voice.input_signal());
    let output_signal = voice.output_signal();
    let rules = output_signal
        .hold_policy(
            SignalSample(0)..SignalSample(256),
            AudioQueryLimits::default(),
        )
        .unwrap();
    assert_eq!(rules.rules.len(), 1);
    assert_eq!(rules.rules[0].samples, SignalSample(0)..SignalSample(256));
    assert!(
        matches!(&rules.rules[0].issuer, AudioHoldIssuer::Node { definition: None, instance } if instance.node == node("pause") && instance.repeats.is_empty())
    );
    let output = tape(&plan, output_signal);
    let mut renderer = StageAudio::new(plan.clone());
    let mut sources = Sources::new(captured);
    let raw = renderer
        .read_tape(
            &mut sources,
            &input,
            SignalSample(0),
            256,
            TIMEOUT,
            &cancelled(),
        )
        .unwrap();
    assert_eq!(raw.samples, reference(44_117, 0, 256));
    assert!(raw.samples.iter().any(|frame| frame[0].abs() > 0.1));
    assert!(raw.suppressed.is_empty());
    let muted = renderer
        .read_tape(
            &mut sources,
            &output,
            SignalSample(0),
            256,
            TIMEOUT,
            &cancelled(),
        )
        .unwrap();
    assert!(muted.samples.iter().all(|frame| *frame == [0.0; 2]));
    assert_eq!(muted.suppressed, vec![SignalSample(0)..SignalSample(256)]);
    assert_eq!(store.snapshot().unwrap(), saved);
    assert_eq!(store.history_availability().unwrap(), history);
}

#[test]
fn independent_voice_preserve_uses_complete_intrinsic_pcm_for_cold_and_shuffled_reads() {
    let _permit = crate::tests::resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let mut store =
        ProjectStore::create(&directory.path().join("voice.deadpan"), &empty()).unwrap();
    register(&mut store);
    let asset = register_mono(&mut store, directory.path());
    commit(
        &mut store,
        "split",
        Command::Split {
            node: node("source"),
            at: FrameDuration::new(3).unwrap(),
            identities: SplitIdentities {
                nodes: ["left", "right", "right-source"].map(node).to_vec(),
            },
        },
    );
    commit(
        &mut store,
        "preserve",
        Command::WrapRetime {
            node: node("left"),
            id: node("preserve"),
            duration: FrameDuration::new(2).unwrap(),
            pitch: PitchPolicy::Preserve,
        },
    );
    let saved = store.snapshot().unwrap();
    let history = store.history_availability().unwrap();
    let captured = snapshot(&store, 1);
    let plan = Arc::new(RenderPlan::compile(&saved).unwrap());
    let current = stage(
        plan.audio_definition(AudioDefinitionSelector::Node {
            node: node("preserve"),
        })
        .unwrap()
        .signal(),
    );
    assert_eq!(current.descriptor().rate, ExactRatio::new(3, 2).unwrap());
    let voice = current
        .input_signal()
        .source_voice(recipe(&saved, &asset, 2205, 137))
        .unwrap();
    let input = tape(&plan, voice.input_signal());
    let policy = AudioSignalTape::new(
        &plan,
        ExactRatio::ZERO..ExactRatio::integer(2),
        vec![AudioSignalTapeRun::new(
            ExactRatio::ZERO..ExactRatio::integer(2),
            ExactRatio::ZERO..ExactRatio::integer(3),
            voice.output_signal(),
        )],
    )
    .unwrap();
    let projection = AudioStageProjection::new(
        current.clone(),
        input,
        policy,
        FrameDuration::new(2).unwrap(),
    )
    .unwrap();
    let output = AudioSignalTape::new(
        &plan,
        ExactRatio::ZERO..ExactRatio::integer(2),
        vec![AudioSignalTapeRun::intrinsic(
            ExactRatio::ZERO..ExactRatio::integer(2),
            ExactRatio::ZERO..ExactRatio::integer(2),
            projection.clone(),
        )],
    )
    .unwrap();
    let expected = stretch(&reference(2205, 137, 4800), 3200);
    assert!(expected[300..1200].iter().any(|frame| frame[0].abs() > 0.1));
    assert!(
        expected[1692..1856]
            .iter()
            .any(|frame| frame[0].abs() > 0.000_001),
        "the reference must retain processed decay after the scaled source endpoint"
    );
    let mut renderer = StageAudio::new(plan.clone());
    let mut sources = Sources::new(captured.clone());
    for (start, count) in [(1600, 256), (3047, 153), (0, 256), (811, 173), (1479, 256)] {
        let expected = &expected
            [usize::try_from(start).unwrap()..usize::try_from(start + i64::from(count)).unwrap()];
        let warm = renderer
            .read_tape(
                &mut sources,
                &output,
                SignalSample(start),
                count,
                TIMEOUT,
                &cancelled(),
            )
            .unwrap();
        let cold = StageAudio::new(plan.clone())
            .read_tape(
                &mut Sources::new(captured.clone()),
                &output,
                SignalSample(start),
                count,
                TIMEOUT,
                &cancelled(),
            )
            .unwrap();
        assert_eq!(warm.samples, expected);
        assert_eq!(cold.samples, expected);
    }
    // A later physical fragment selects an already prepared output interval.
    // It must keep the complete original DSP history, including when read cold.
    let cropped = AudioSignalTape::new(
        &plan,
        ExactRatio::ZERO..ExactRatio::new(3, 2).unwrap(),
        vec![AudioSignalTapeRun::intrinsic(
            ExactRatio::ZERO..ExactRatio::new(3, 2).unwrap(),
            ExactRatio::new(1, 4).unwrap()..ExactRatio::new(7, 4).unwrap(),
            projection,
        )],
    )
    .unwrap();
    for (start, count) in [(2137_i64, 256_u32), (0, 256), (397, 73)] {
        let cold = StageAudio::new(plan.clone())
            .read_tape(
                &mut Sources::new(captured.clone()),
                &cropped,
                SignalSample(start),
                count,
                TIMEOUT,
                &cancelled(),
            )
            .unwrap();
        let offset = usize::try_from(start + 400).unwrap();
        assert_eq!(
            cold.samples,
            expected[offset..offset + usize::try_from(count).unwrap()]
        );
    }
    // An owner outside the Preserve child retains its real scope. Constructing
    // a catalog voice must not turn it into an admissible descendant by fiat.
    let outside = plan
        .audio_signal()
        .source_voice(recipe(&saved, &asset, 2205, 137))
        .unwrap();
    let outside_tape = |end: i64| {
        AudioSignalTape::new(
            &plan,
            ExactRatio::ZERO..ExactRatio::integer(end),
            vec![AudioSignalTapeRun::new(
                ExactRatio::ZERO..ExactRatio::integer(end),
                ExactRatio::ZERO..ExactRatio::integer(3),
                outside.input_signal(),
            )],
        )
        .unwrap()
    };
    assert!(
        AudioStageProjection::new(
            current,
            outside_tape(3),
            outside_tape(2),
            FrameDuration::new(2).unwrap()
        )
        .is_err()
    );
    assert_eq!(store.snapshot().unwrap(), saved);
    assert_eq!(store.history_availability().unwrap(), history);
}

#[test]
fn catalog_voice_keeps_plan_receipt_layout_and_live_original_admission() {
    let _permit = crate::tests::resources::pcm();
    let directory = tempfile::tempdir().unwrap();
    let mut store =
        ProjectStore::create(&directory.path().join("voice.deadpan"), &hold(40)).unwrap();
    let unspecified = super::sound::register_sound(&mut store, "pcm-mono-44100.wav", "unspecified");
    let asset = register_mono(&mut store, directory.path());
    let captured = snapshot(&store, 1);
    let plan = Arc::new(RenderPlan::compile(&captured.document).unwrap());
    let voice = plan
        .audio_signal()
        .source_voice(recipe(&captured.document, &asset, 2205, 0))
        .unwrap();
    let selected = tape(&plan, voice.input_signal());
    let foreign = Arc::new(RenderPlan::compile(&captured.document).unwrap());
    assert!(
        AudioSignalTape::new(
            &foreign,
            voice.input_signal().support(),
            vec![AudioSignalTapeRun::new(
                voice.input_signal().support(),
                voice.input_signal().support(),
                voice.input_signal(),
            )]
        )
        .is_err()
    );
    assert!(matches!(
        StageAudio::new(foreign).read_tape(
            &mut Sources::new(captured.clone()),
            &selected,
            SignalSample(0),
            1,
            TIMEOUT,
            &cancelled()
        ),
        Err(deadpan_audio::StageAudioError::ForeignDomain)
    ));
    let mut missing = Snapshot::committed(
        captured.session,
        captured.document.clone(),
        captured.sources.clone(),
        store.original_import_handle().unwrap(),
    );
    missing.sources.remove(&asset);
    assert!(
        StageAudio::new(plan.clone())
            .read_tape(
                &mut Sources::new(Arc::new(missing)),
                &selected,
                SignalSample(0),
                1,
                TIMEOUT,
                &cancelled()
            )
            .is_err()
    );
    let unqualified_layout = plan
        .audio_signal()
        .source_voice(recipe(&captured.document, &unspecified, 2205, 0))
        .unwrap();
    let error = StageAudio::new(plan.clone())
        .read_tape(
            &mut Sources::new(captured.clone()),
            &tape(&plan, unqualified_layout.input_signal()),
            SignalSample(0),
            1,
            TIMEOUT,
            &cancelled(),
        )
        .unwrap_err();
    assert!(error.to_string().contains("layout"), "{error}");
    drop(store);
    assert!(
        StageAudio::new(plan.clone())
            .read_tape(
                &mut Sources::new(captured),
                &selected,
                SignalSample(0),
                1,
                TIMEOUT,
                &cancelled()
            )
            .is_err(),
        "closing the writer revokes cold original snapshots"
    );
}
