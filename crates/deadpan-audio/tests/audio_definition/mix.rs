use super::*;

use deadpan_plan::{
    AudioMixGate, AudioSignalMix, AudioSignalTape, AudioSignalTapeRun, AudioStageProjection,
};

pub(super) struct MixProvider {
    stereo: FixtureProvider,
    mono: PreparedSource,
    mono_calls: usize,
    revoke_mono_after: usize,
}

impl MixProvider {
    pub(super) fn new() -> Self {
        let bytes = std::fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../native/deadpan-source/tests/audio-fixtures/pcm-mono-44100.wav"),
        )
        .unwrap();
        let cancelled = AtomicBool::new(false);
        let session = AudioSession::open_verified(
            &mut Cursor::new(&bytes),
            SourceContentIdentity::new(Sha256::digest(&bytes).into(), bytes.len() as u64).unwrap(),
            0,
            AudioSessionLimits::default(),
            &cancelled,
        )
        .unwrap();
        let index = session.index().clone();
        let mono = PreparedSource::with_layout(
            session,
            &index,
            AudioChannelLayout::Native {
                channels: 1,
                mask: 4,
            },
            &cancelled,
        )
        .unwrap();
        Self {
            stereo: FixtureProvider::new(),
            mono,
            mono_calls: 0,
            revoke_mono_after: usize::MAX,
        }
    }
}

impl AudioSourceProvider for MixProvider {
    fn source(
        &mut self,
        project: &ProjectId,
        revision: &RevisionId,
        asset: &AssetId,
        cancelled: &AtomicBool,
    ) -> Result<&PreparedSource, PreparationError> {
        if asset.as_str() == "mono" {
            if self.mono_calls >= self.revoke_mono_after {
                return Err(PreparationError::SourceUnavailable(
                    "mono admission revoked".into(),
                ));
            }
            self.mono_calls += 1;
            Ok(&self.mono)
        } else {
            self.stereo.source(project, revision, asset, cancelled)
        }
    }
}

fn fixture(selected: i128) -> ProjectDocument {
    let rate = FrameRate::new(30, 1).unwrap();
    let mut a = source(rate, 3, 0..4800);
    let NodeKind::Source { source: a_source } = &mut a.kind else {
        unreachable!()
    };
    a_source.audio_mapping = SourceAudioMapping::SelectedPlacement {
        start: ExactRatio::ZERO,
        frames: ratio(3, 1),
        selection: ExactFrameRange::new(ExactRatio::ZERO, ratio(selected, 1600)).unwrap(),
    };
    let time_base = SourceTimeBase::new(1, 44_100).unwrap();
    let mono_audio = SourceAudio {
        asset: AssetId::new("mono").unwrap(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: 0,
                time_base,
            },
            SourceTimestamp {
                ticks: 4410,
                time_base,
            },
        )
        .unwrap(),
    };
    let mono_mapping = SourceAudioMapping::Placement {
        start: ratio(37, 1600),
        frames: ratio(3, 1),
    };
    let doc = document(
        rate,
        &["outer"],
        [
            ("a", a),
            ("b", source(rate, 3, 0..4800)),
            ("group", BeatNode::sequence("Group", vec![id("a"), id("b")])),
            ("inner", retime("group", 4, 0..6, PitchPolicy::Preserve)),
            ("outer", retime("inner", 2, 0..4, PitchPolicy::Preserve)),
        ],
        BTreeMap::new(),
    );
    let mut wire = serde_json::to_value(doc).unwrap();
    wire["nodes"]["b"]["kind"]["source"]["audio"] = serde_json::to_value(mono_audio).unwrap();
    wire["nodes"]["b"]["kind"]["source"]["audio_mapping"] =
        serde_json::to_value(mono_mapping).unwrap();
    let mut asset = wire["assets"]["media"].clone();
    asset["audio"] = serde_json::to_value(
        SourceSpan::new(
            SourceTimestamp {
                ticks: 0,
                time_base,
            },
            SourceTimestamp {
                ticks: 44117,
                time_base,
            },
        )
        .unwrap(),
    )
    .unwrap();
    asset["content_hash"] = serde_json::json!("b".repeat(64));
    wire["assets"]["mono"] = asset;
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn tape<'plan>(plan: &'plan RenderPlan, name: &str, extent: i128) -> AudioSignalTape<'plan> {
    let source_start = if name == "b" { 3 } else { 0 };
    let signal =
        projection::stage(projection::stage(plan.audio_signal()).input_signal()).input_signal();
    AudioSignalTape::new(
        plan,
        ExactRatio::ZERO..ratio(extent, 1),
        vec![AudioSignalTapeRun::new(
            ExactRatio::ZERO..ratio(extent, 1),
            ratio(source_start, 1)..ratio(source_start + 3, 1),
            signal,
        )],
    )
    .unwrap()
}

fn mix(plan: &RenderPlan, gates: Vec<AudioMixGate>) -> AudioSignalMix<'_> {
    AudioSignalMix::new(plan, vec![tape(plan, "a", 3), tape(plan, "b", 3)], gates).unwrap()
}

fn mono_sample(index: i64) -> [f32; 2] {
    let value = ((index * 73) % 65_536 - 32_768) as f32 / 32_768.0;
    [value; 2]
}

fn voice_reference(selected: i64) -> [Vec<[f32; 2]>; 2] {
    let a = sample_reference(
        0..selected,
        ExactRatio::ZERO,
        ExactRatio::ONE,
        4800,
        fixture_sample,
    );
    // The 37-sample placement leaves 4763 output samples inside this beat.
    // Its physical support ends at ceil(4763 * 147 / 160) = 4377 original
    // samples, even though the selected media span extends to sample 4410.
    let mut b = sample_reference(
        0..4377,
        ratio(-5439, 160),
        ratio(147, 160),
        4800,
        mono_sample,
    );
    b[..37].fill([0.0; 2]);
    [a, b]
}

fn sum(voices: &[Vec<[f32; 2]>]) -> Vec<[f32; 2]> {
    (0..voices[0].len())
        .map(|sample| {
            let mut total = [0.0_f64; 2];
            for voice in voices {
                total[0] += f64::from(voice[sample][0]);
                total[1] += f64::from(voice[sample][1]);
            }
            [total[0] as f32, total[1] as f32]
        })
        .collect()
}

#[test]
fn scoped_mix_sums_two_decoded_rates_and_keeps_exhaustion_and_gates_per_voice() {
    let doc = fixture(777);
    let plan = compile(&doc, false);
    // Arbitrary sample positions are fractional project frames at 30 fps.
    let gates = vec![
        AudioMixGate::new(ratio(1001, 1600)..ratio(1099, 1600), vec![1]),
        AudioMixGate::new(ratio(301, 1600)..ratio(317, 1600), vec![0]),
        AudioMixGate::new(ratio(509, 1600)..ratio(523, 1600), vec![0, 1]),
    ];
    let mix = mix(&plan, gates);
    let mut voices = voice_reference(777);
    voices[1][1001..1099].fill([0.0; 2]);
    voices[0][301..317].fill([0.0; 2]);
    for voice in &mut voices {
        voice[509..523].fill([0.0; 2]);
    }
    let expected = sum(&voices);
    assert!(expected[778..999].iter().any(|sample| *sample != [0.0; 2]));
    assert!(
        expected.iter().flatten().any(|value| value.abs() > 1.0),
        "raw sum must not clip"
    );
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let mut provider = MixProvider::new();
    for (start, count) in [
        (768, 256),
        (256, 256),
        (0, 256),
        (1000, 150),
        (4700, 100),
        (500, 40),
    ] {
        let actual = renderer
            .read_mix(
                &mut provider,
                &mix,
                SignalSample(start),
                count,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(
            actual.samples,
            expected[start as usize..start as usize + count as usize],
            "start={start}, count={count}"
        );
        let end = start + i64::from(count);
        let suppressed: Vec<_> = [509..523, 1001..1099]
            .into_iter()
            .filter_map(|range| {
                let range = range.start.max(start)..range.end.min(end);
                (range.start < range.end)
                    .then_some(SignalSample(range.start)..SignalSample(range.end))
            })
            .collect();
        assert_eq!(actual.suppressed, suppressed);
        assert_eq!(actual.stage, "scoped_mix_preparation_pcm_before_effects");
        let cold = StageAudio::new(Arc::clone(&plan))
            .read_mix(
                &mut MixProvider::new(),
                &mix,
                SignalSample(start),
                count,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(actual, cold);
    }
    assert!(provider.mono_calls > 0 && provider.stereo.calls > 0);
}

fn projected<'plan>(plan: &'plan RenderPlan) -> Arc<AudioStageProjection<'plan>> {
    AudioStageProjection::new_mixed_input(
        projection::stage(projection::stage(plan.audio_signal()).input_signal()),
        mix(plan, vec![]),
        tape(plan, "a", 2),
        frames(2),
    )
    .unwrap()
}

#[test]
fn enclosing_preserve_processes_the_complete_sum_once_and_retains_source_decay() {
    let doc = fixture(777);
    let plan = compile(&doc, false);
    let output = projection::output(&plan, projected(&plan));
    let voices = voice_reference(777);
    let expected = stretch(&sum(&voices), 3200, 3, 2);
    let independent = sum(&voices
        .iter()
        .map(|voice| stretch(voice, 3200, 3, 2))
        .collect::<Vec<_>>());
    assert!(
        expected
            .iter()
            .zip(independent)
            .any(|(a, b)| a.iter().zip(b).any(|(a, b)| (*a - b).abs() > 0.00001)),
        "fixture must distinguish mix-before-Preserve from per-voice Preserve"
    );
    assert!(expected[600..856].iter().any(|sample| *sample != [0.0; 2]));
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let mut provider = MixProvider::new();
    for (start, count) in [(2944, 256), (600, 256), (0, 256), (256, 91)] {
        let actual = renderer
            .read_tape(
                &mut provider,
                &output,
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
        assert!(
            actual.suppressed.is_empty(),
            "Source exhaustion must not truncate the mixed stage's decay"
        );
        assert_eq!(renderer.cached_stage_count(), 0);
    }
}

#[test]
fn mixed_projection_memo_rechecks_every_voice_and_shares_nested_limits() {
    let doc = fixture(4800);
    let plan = compile(&doc, false);
    let inner = projected(&plan);
    let voice = || projection::output(&plan, Arc::clone(&inner));
    let mixed = AudioSignalMix::new(&plan, vec![voice(), voice()], vec![]).unwrap();
    let outer = AudioStageProjection::new_mixed_input(
        projection::stage(plan.audio_signal()),
        mixed,
        tape(&plan, "a", 1),
        frames(1),
    )
    .unwrap();
    let output = projection::output(&plan, outer);
    let mut provider = MixProvider::new();
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
            &output,
            SignalSample(0),
            1,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Limit("nested stage depth"))
    ));
    assert_eq!(provider.stereo.calls + provider.mono_calls, 0);
    let mut bounded = StageAudio::with_limits(
        Arc::clone(&plan),
        StageLimits {
            maximum_resident_frames: 21_000,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(matches!(
        bounded.read_tape(
            &mut provider,
            &output,
            SignalSample(0),
            1,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Limit("resident PCM or stage cache"))
    ));
    // First full inner preparation consumes nineteen source chunks. Its next
    // use is the second overlapping voice's request-local memo admission.
    let mut provider = MixProvider::new();
    provider.revoke_mono_after = 19;
    let mut renderer = StageAudio::with_limits(
        Arc::clone(&plan),
        StageLimits {
            maximum_prepared_stages: 2,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(matches!(
        renderer.read_tape(
            &mut provider,
            &output,
            SignalSample(0),
            1,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Preparation(
            PreparationError::SourceUnavailable(_)
        ))
    ));
    assert_eq!(provider.mono_calls, 19);
    provider.revoke_mono_after = usize::MAX;
    let actual = renderer
        .read_tape(
            &mut provider,
            &output,
            SignalSample(127),
            129,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    let inner = stretch(&sum(&voice_reference(4800)), 3200, 3, 2);
    let expected = stretch(&sum(&[inner.clone(), inner]), 1600, 2, 1);
    assert_eq!(actual.samples, expected[127..256]);
}

#[test]
fn gated_voice_still_requires_source_admission_and_invalid_requests_read_nothing() {
    let doc = fixture(4800);
    let plan = compile(&doc, false);
    let mix = mix(
        &plan,
        vec![AudioMixGate::new(ExactRatio::ZERO..ratio(3, 1), vec![1])],
    );
    let mut provider = MixProvider::new();
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    for (start, count) in [(-1, 1), (0, 0), (0, 257), (4799, 2), (i64::MAX, 1)] {
        assert!(matches!(
            renderer.read_mix(
                &mut provider,
                &mix,
                SignalSample(start),
                count,
                TIMEOUT,
                &AtomicBool::new(false)
            ),
            Err(StageAudioError::Range)
        ));
    }
    assert!(
        renderer
            .read_mix(
                &mut provider,
                &mix,
                SignalSample(0),
                1,
                TIMEOUT,
                &AtomicBool::new(true)
            )
            .unwrap_err()
            .is_cancelled()
    );
    assert_eq!(provider.stereo.calls + provider.mono_calls, 0);
    provider.revoke_mono_after = 0;
    assert!(matches!(
        renderer.read_mix(
            &mut provider,
            &mix,
            SignalSample(50),
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
fn mix_residency_includes_the_voice_reader_and_its_span_buffer() {
    let doc = fixture(4800);
    let plan = compile(&doc, false);
    let mix = mix(&plan, vec![]);
    let mut provider = MixProvider::new();
    let mut limited = StageAudio::with_limits(
        Arc::clone(&plan),
        StageLimits {
            maximum_resident_frames: 3,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(matches!(
        limited.read_mix(
            &mut provider,
            &mix,
            SignalSample(50),
            1,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Limit("resident PCM or stage cache"))
    ));
    assert_eq!(provider.stereo.calls + provider.mono_calls, 0);

    let mut admitted = StageAudio::with_limits(
        Arc::clone(&plan),
        StageLimits {
            maximum_resident_frames: 4,
            ..Default::default()
        },
    )
    .unwrap();
    let actual = admitted
        .read_mix(
            &mut provider,
            &mix,
            SignalSample(50),
            1,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(actual.samples, sum(&voice_reference(4800))[50..51]);
}

#[test]
fn a_silent_hold_voice_does_not_suppress_the_other_voice() {
    let mut wire = serde_json::to_value(fixture(4800)).unwrap();
    wire["nodes"]["a"] = serde_json::to_value(hold(3, HoldAudio::Silence)).unwrap();
    let doc = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = compile(&doc, false);
    let mix = mix(&plan, vec![]);
    let [_, expected] = voice_reference(4800);
    let mut provider = MixProvider::new();
    let actual = StageAudio::new(Arc::clone(&plan))
        .read_mix(
            &mut provider,
            &mix,
            SignalSample(20),
            256,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(actual.samples, expected[20..276]);
    assert_eq!(actual.suppressed, vec![SignalSample(20)..SignalSample(37)]);
    assert_eq!(provider.stereo.calls, 0);
    assert_eq!(provider.mono_calls, 1);
}

#[test]
fn a_tail_hidden_inside_a_preserve_voice_is_an_invalid_document() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    assert_tail_in_speed_change_is_invalid(try_document(
        rate,
        &["a", "inner"],
        [
            ("a", source(rate, 1024, 0..1024)),
            ("b", source(rate, 1024, 2048..3072)),
            (
                "tail",
                hold(
                    1024,
                    HoldAudio::Tail {
                        maximum: frames(1024),
                        effect: Default::default(),
                    },
                ),
            ),
            (
                "group",
                BeatNode::sequence("Group", vec![id("b"), id("tail")]),
            ),
            (
                "inner",
                retime("group", 1024, 0..2048, PitchPolicy::Preserve),
            ),
        ],
        BTreeMap::new(),
    ));
}
