//! Shared nonlinear owners after a one-frame move on a fractional sample grid.
use super::*;

const START: i64 = 4805;
const FRAMES: u32 = 6406;

fn moved(with_original: bool) -> ProjectDocument {
    let mut wire = serde_json::to_value(ntsc(AudioEdgePolicy::Hard)).unwrap();
    if with_original {
        let original = audio(0..6407);
        wire["nodes"]["owner"]["kind"]["source"]["audio"] =
            serde_json::to_value(&original).unwrap();
        wire["nodes"]["owner"]["kind"]["source"]["audio_mapping"] = serde_json::to_value(
            SourceAudioMapping::natural_rate(original.span, FrameRate::new(30000, 1001).unwrap())
                .unwrap(),
        )
        .unwrap();
    }
    insert_prefix(&ProjectDocument::from_json(&wire.to_string()).unwrap())
}

fn treated(document: &ProjectDocument, gain: ClipGain) -> ProjectDocument {
    let mut wire = serde_json::to_value(document).unwrap();
    wire["nodes"]["owner"]["audio_treatments"] = serde_json::to_value(
        AudioTreatments::from_clip_gain(gain)
            .with_saturation(Some(Saturation::new(GainDb::new(12000).unwrap()).unwrap()))
            .unwrap(),
    )
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn read(document: &ProjectDocument, start: i64, count: u32) -> Vec<[f32; 2]> {
    bus(
        &mut StageAudio::new(compile(document, false)),
        &mut ClockProvider::new(document),
        start,
        count,
    )
    .samples
}

fn original(document: &ProjectDocument, start: i64, count: u32) -> Vec<[f32; 2]> {
    let mut renderer = StageAudio::new(compile(document, false));
    let mut provider = ClockProvider::new(document);
    (0..count)
        .step_by(256)
        .flat_map(|offset| {
            renderer
                .read_edge_faded(
                    &mut provider,
                    AudioSample(start + i64::from(offset)),
                    256.min(count - offset),
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap()
                .samples
        })
        .collect()
}

fn ramp() -> ClipGain {
    ClipGain::new(
        GainDb::new(3000).unwrap(),
        false,
        vec![
            GainEnvelope::new(
                GainClock::OwnerOutput,
                GainRange::new(ExactRatio::ZERO, ExactRatio::integer(4)).unwrap(),
                GainDb::new(-12000).unwrap(),
                vec![
                    GainSegment::new(
                        ExactRatio::integer(4),
                        GainDb::new(6000).unwrap(),
                        GainCurve::Linear,
                    )
                    .unwrap(),
                ],
            )
            .unwrap(),
        ],
        vec![],
    )
    .unwrap()
}

fn amplitude(gain: &ClipGain, clock: ExactRatio) -> f64 {
    let value = gain.evaluate(clock).unwrap();
    if value.muted {
        0.0
    } else {
        10.0_f64.powf(
            value.millidecibels.numerator() as f64
                / value.millidecibels.denominator() as f64
                / 20000.0,
        )
    }
}

#[test]
fn shared_eq_independent_input_automation_keeps_gain_filter_and_saturation_order() {
    use deadpan_core::{AudioTreatmentStage as S, EqualizerRecipe};
    let source = moved(true);
    let gain = ramp();
    let eq = EqualizerRecipe::new(
        [6000, -3000, 1500].map(|db| GainDb::new(db).unwrap()),
        None,
        None,
    )
    .unwrap();
    let kernel = deadpan_audio::equalizer_coefficients(eq, &AtomicBool::new(false)).unwrap();
    let start = START + 2700;
    let count = 128u32;
    let halo_start = start - 2048;
    let halo_count = count + 4096;
    let original = original(&source, halo_start, halo_count);
    let sound = read(&moved(false), halo_start, halo_count);
    let input: Vec<[f64; 2]> = original
        .iter()
        .zip(&sound)
        .enumerate()
        .map(|(offset, (original, sound))| {
            let sample = i128::from(halo_start + offset as i64);
            let a = amplitude(&gain, ratio(sample * 5 - 24026, 8008));
            let b = amplitude(&gain, ratio(sample * 5 - 24024, 8008));
            std::array::from_fn(|ch| f64::from(original[ch]) * a + f64::from(sound[ch]) * b)
        })
        .collect();
    let drive = Saturation::new(GainDb::new(12000).unwrap()).unwrap();
    for saturation_first in [false, true] {
        let treatments = AudioTreatments::from_clip_gain(gain.clone())
            .with_equalizer(Some(eq))
            .unwrap()
            .with_saturation(Some(drive))
            .unwrap();
        let mut wire = serde_json::to_value(&source).unwrap();
        wire["nodes"]["owner"]["audio_treatments"] = serde_json::to_value(treatments).unwrap();
        if saturation_first {
            wire["nodes"]["owner"]["audio_treatments"]["order"] =
                serde_json::to_value([S::ClipGain, S::Saturation, S::Equalizer]).unwrap();
        }
        let treated = ProjectDocument::from_json(&wire.to_string()).unwrap();
        let expected: Vec<[f32; 2]> = (0..count as usize)
            .map(|offset| {
                std::array::from_fn(|ch| {
                    let filtered: f64 = kernel
                        .iter()
                        .enumerate()
                        .map(|(k, weight)| {
                            let input = input[offset + k][ch];
                            weight
                                * if saturation_first {
                                    drive.shape(input)
                                } else {
                                    input
                                }
                        })
                        .sum();
                    (if saturation_first {
                        filtered
                    } else {
                        drive.shape(filtered)
                    }) as f32
                })
            })
            .collect();
        let actual = read(&treated, start, count);
        let error = actual
            .iter()
            .zip(&expected)
            .flat_map(|(a, b)| (0..2).map(move |ch| (a[ch] - b[ch]).abs()))
            .fold(0.0_f32, f32::max);
        // The oracle's raw inspection samples are f32; the temporal graph
        // retains f64 intermediates. This covers four f32 ulps at unity.
        assert!(
            error <= 4.0 * f32::EPSILON,
            "saturation_first={saturation_first}, error={error}"
        );
        assert!(actual.iter().flatten().any(|sample| sample.abs() > 0.01));
        for (offset, frames) in [(73, 55), (0, 37), (37, 36)] {
            assert_eq!(
                read(&treated, start + offset, frames),
                actual[offset as usize..offset as usize + frames as usize]
            );
        }
    }
}

#[test]
fn independent_post_saturation_mute_preserves_the_shared_drive() {
    let gain = ClipGain::new(
        GainDb::UNITY,
        false,
        vec![],
        vec![GainRange::new(ratio(975, 8008), ratio(977, 8008)).unwrap()],
    )
    .unwrap();
    compare_independent_suffix(gain);
}

fn compare_independent_suffix(gain: ClipGain) {
    let source = moved(true);
    let original = original(&source, START, FRAMES);
    let sound = read(&moved(false), START, FRAMES);
    let drive = Saturation::new(GainDb::new(12000).unwrap()).unwrap();
    let expected: Vec<[f32; 2]> = original
        .iter()
        .zip(&sound)
        .enumerate()
        .map(|(offset, (a, b))| {
            let sample = i128::from(START + offset as i64);
            let ga = amplitude(&gain, ratio(sample * 5 - 24026, 8008));
            let gb = amplitude(&gain, ratio(sample * 5 - 24024, 8008));
            std::array::from_fn(|ch| {
                let sum = f64::from(a[ch]) + f64::from(b[ch]);
                let factor = if sum == 0.0 {
                    drive.drive_factor()
                } else {
                    drive.shape(sum) / sum
                };
                (f64::from(a[ch]) * factor * ga + f64::from(b[ch]) * factor * gb) as f32
            })
        })
        .collect();
    let mut wire = serde_json::to_value(treated(&source, gain)).unwrap();
    wire["nodes"]["owner"]["audio_treatments"]["order"] =
        serde_json::json!(["saturation", "clip_gain"]);
    let treated = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let actual = read(&treated, START, FRAMES);
    assert_eq!(actual, expected);
    for (offset, count) in [(193, 7), (195, 61), (6011, 395), (131, 509)] {
        assert_eq!(
            read(&treated, START + offset, count),
            actual[offset as usize..offset as usize + count as usize]
        );
    }
}

#[test]
fn independent_post_saturation_envelope_preserves_the_shared_drive() {
    compare_independent_suffix(ramp());
}

#[test]
fn independent_clocks_share_one_eq_processing_context() {
    let mut wire = serde_json::to_value(post_treated(ramp())).unwrap();
    wire["nodes"]["owner"]["audio_treatments"] = serde_json::to_value(
        AudioTreatments::from_clip_gain(ramp())
            .with_equalizer(Some(
                deadpan_core::EqualizerRecipe::new(
                    [6000, -3000, 1500].map(|db| GainDb::new(db).unwrap()),
                    None,
                    None,
                )
                .unwrap(),
            ))
            .unwrap(),
    )
    .unwrap();
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let expected = read(&document, START + 2700, 128);
    let mut renderer = StageAudio::with_limits(
        compile(&document, false),
        StageLimits {
            maximum_prepared_stages: 1,
            ..StageLimits::default()
        },
    )
    .unwrap();
    assert_eq!(
        bus(
            &mut renderer,
            &mut ClockProvider::new(&document),
            START + 2700,
            128
        )
        .samples,
        expected
    );
}

#[test]
fn independent_automation_is_invariant_across_owner_activation_boundaries() {
    let document = post_treated(ramp());
    let total = u32::try_from(compile(&document, false).audio_duration().unwrap().0).unwrap();
    let complete = read(&document, 0, total);
    for (start, count) in [
        (0, 1),
        (START - 257, 513),
        (START - 1, 2),
        (START, 1),
        (START + FRAMES as i64 - 257, 513),
        (START + FRAMES as i64 - 1, 2),
    ] {
        assert_eq!(
            read(&document, start, count),
            complete[start as usize..start as usize + count as usize]
        );
    }
}

#[test]
fn independent_eq_automation_keeps_rebound_input_history_across_a_new_pause() {
    use deadpan_core::{AudioTreatmentStage as S, EqualizerRecipe};
    let mut wire = serde_json::to_value(post_treated(ramp())).unwrap();
    let recipe = AudioTreatments::with_equalizer_stages(
        vec![S::Saturation, S::Equalizer, S::ClipGain],
        Some(ramp()),
        Some(Saturation::new(GainDb::new(12000).unwrap()).unwrap()),
        None,
        Some(
            EqualizerRecipe::new(
                [6000, -3000, 1500].map(|db| GainDb::new(db).unwrap()),
                None,
                None,
            )
            .unwrap(),
        ),
    )
    .unwrap();
    wire["nodes"]["owner"]["audio_treatments"] = serde_json::to_value(recipe).unwrap();
    let attached = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let sound: BeatSound =
        serde_json::from_value(wire["beat_sounds"]["owner"]["effect"].clone()).unwrap();
    wire["sounds"] = serde_json::json!({"effect": SoundEvent {
        owner: id("root"), label: sound.label, source: sound.source, mapping: sound.mapping,
        offset: AudioSample(START), gain_millidecibels: sound.gain_millidecibels,
        start_edge: sound.start_edge, end_edge: sound.end_edge, overflow: sound.overflow,
    }});
    wire["beat_sounds"] = serde_json::json!({});
    wire["audio_bindings"]["sound_clocks"] = serde_json::json!({});
    wire["nodes"]["root"]["audio_treatments"] = wire["nodes"]["owner"]["audio_treatments"].take();
    wire["nodes"]["owner"]
        .as_object_mut()
        .unwrap()
        .remove("audio_treatments");
    let source = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let at = ProjectFrame(4);
    let count = source
        .insert_time_target(at)
        .unwrap()
        .split
        .map_or(0, |split| split.required_ids);
    let NodeKind::Hold { recipe } = hold(1, HoldAudio::Silence).kind else {
        unreachable!()
    };
    let command = Command::InsertTime {
        at,
        hold: recipe,
        id: id("inner-pause"),
        identities: SplitIdentities {
            nodes: (0..count)
                .map(|n| NodeId::new(format!("inner-split-{n}")).unwrap())
                .collect(),
        },
        timing: AudioTimingId {
            allocation: RevisionId::new("held-again").unwrap(),
            ordinal: 0,
        },
    };
    // Attached sounds still require an owner-local interval route before their
    // processing scope can be split. Preserve this existing atomic refusal;
    // the root event above has that route and exercises the actual EQ boundary.
    let snapshot = attached.to_json().unwrap();
    let error = apply(
        &attached,
        &CommandRequest {
            project_id: attached.project_id().clone(),
            expected_revision: attached.revision_id().clone(),
            new_revision: RevisionId::new("held-again").unwrap(),
            command: command.clone(),
        },
    )
    .unwrap_err();
    assert!(
        error
            .message
            .contains("sound clock scope omits an enclosing processor"),
        "{error}"
    );
    assert_eq!(attached.to_json().unwrap(), snapshot);
    let edited = edit(&source, "held-again", command);
    let total = u32::try_from(compile(&edited, false).audio_duration().unwrap().0).unwrap();
    let complete = read(&edited, 0, total);
    for (start, count) in [(START - 17, 129), (6405, 129), (8007, 129), (11210, 129)] {
        let actual = read(&edited, start, count);
        for (offset, actual) in actual.iter().enumerate() {
            assert_eq!(
                *actual,
                complete[start as usize + offset],
                "sample={}",
                start + offset as i64
            );
        }
    }
}

fn post_treated(gain: ClipGain) -> ProjectDocument {
    let mut wire = serde_json::to_value(treated(&moved(true), gain)).unwrap();
    wire["nodes"]["owner"]["audio_treatments"]["order"] =
        serde_json::json!(["saturation", "clip_gain"]);
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

#[test]
fn independent_post_saturation_inputs_rejoin_the_parent_bus() {
    let child = post_treated(ramp());
    let reference = read(&child, START, FRAMES);
    let drive = Saturation::new(GainDb::new(9000).unwrap()).unwrap();
    let mut wire = serde_json::to_value(&child).unwrap();
    wire["nodes"]["root"]["audio_treatments"] = serde_json::to_value(
        AudioTreatments::from_clip_gain(
            ClipGain::new(GainDb::new(-3000).unwrap(), false, vec![], vec![]).unwrap(),
        )
        .with_saturation(Some(drive))
        .unwrap(),
    )
    .unwrap();
    let nested = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let actual = read(&nested, START, FRAMES);
    let factor = 10_f64.powf(-3.0 / 20.0);
    for (actual, reference) in actual.iter().zip(reference) {
        for ch in 0..2 {
            assert!(
                (f64::from(actual[ch]) - drive.shape(f64::from(reference[ch]) * factor)).abs()
                    <= 4.0 * f64::from(f32::EPSILON)
            );
        }
    }
    for (offset, count) in [(193, 7), (195, 61), (6011, 395), (131, 509)] {
        assert_eq!(
            read(&nested, START + offset, count),
            actual[offset as usize..offset as usize + count as usize]
        );
    }
}

#[test]
fn contribution_pcm_is_admitted_before_media_and_cancellation_recovers() {
    let treated = post_treated(ramp());
    let plan = compile(&treated, false);
    let mut provider = ClockProvider::new(&treated);
    let mut small = StageAudio::with_limits(
        Arc::clone(&plan),
        StageLimits {
            maximum_resident_frames: 1024,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(matches!(
        small.prepare_authored_bus(
            &mut provider,
            AudioSample(START + 193),
            64,
            TIMEOUT,
            &AtomicBool::new(false),
        ),
        Err(StageAudioError::Limit(_))
    ));
    assert_eq!(provider.fixture.calls, 0);
    assert_eq!(provider.fixture.context_calls, 0);
    let mut renderer = StageAudio::new(plan);
    assert!(
        renderer
            .prepare_authored_bus(
                &mut provider,
                AudioSample(START + 193),
                64,
                TIMEOUT,
                &AtomicBool::new(true),
            )
            .is_err()
    );
    assert_eq!(provider.fixture.calls, 0);
    let expected = read(&treated, START + 193, 64);
    assert_eq!(
        bus(&mut renderer, &mut provider, START + 193, 64).samples,
        expected
    );
    provider.fixture.unavailable = true;
    assert!(
        renderer
            .prepare_authored_bus(
                &mut provider,
                AudioSample(START + 193),
                64,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .is_err()
    );
    provider.fixture.unavailable = false;
    assert_eq!(
        bus(&mut renderer, &mut provider, START + 193, 64).samples,
        expected
    );
}

fn constant_source(value: [i16; 2]) -> PreparedSource {
    let mut bytes = std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav"),
    )
    .unwrap();
    let mut at = 12;
    while &bytes[at..at + 4] != b"data" {
        let length = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
        at += 8 + length + length % 2;
    }
    let length = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
    for frame in bytes[at + 8..at + 8 + length].chunks_exact_mut(4) {
        frame[..2].copy_from_slice(&value[0].to_le_bytes());
        frame[2..].copy_from_slice(&value[1].to_le_bytes());
    }
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
    PreparedSource::with_layout(session, &index, layout(), &cancelled).unwrap()
}

#[test]
fn cancelling_inputs_keep_the_shared_saturation_zero_limit_before_mute() {
    struct Opposing {
        positive: ClockProvider,
        negative: ClockProvider,
    }
    impl AudioSourceProvider for Opposing {
        fn source(
            &mut self,
            project: &ProjectId,
            revision: &RevisionId,
            asset: &AssetId,
            cancelled: &AtomicBool,
        ) -> Result<&PreparedSource, PreparationError> {
            let provider = if asset.as_str() == "negative" {
                &mut self.negative
            } else {
                &mut self.positive
            };
            provider.source(
                project,
                revision,
                &AssetId::new("media").unwrap(),
                cancelled,
            )
        }
        fn source_for_context(
            &mut self,
            project: &ProjectId,
            revision: &RevisionId,
            asset: &AssetId,
            expected: &AssetRecord,
            cancelled: &AtomicBool,
        ) -> Result<&PreparedSource, PreparationError> {
            let provider = if asset.as_str() == "negative" {
                &mut self.negative
            } else {
                &mut self.positive
            };
            provider.source_for_context(
                project,
                revision,
                &AssetId::new("media").unwrap(),
                expected,
                cancelled,
            )
        }
    }
    let gain = ClipGain::new(
        GainDb::UNITY,
        false,
        vec![],
        vec![GainRange::new(ratio(975, 8008), ratio(977, 8008)).unwrap()],
    )
    .unwrap();
    let mut wire = serde_json::to_value(post_treated(gain)).unwrap();
    wire["assets"]["negative"] = wire["assets"]["media"].clone();
    wire["beat_sounds"]["owner"]["effect"]["source"]["asset"] = serde_json::json!("negative");
    let treated = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let mut provider = Opposing {
        positive: ClockProvider::new(&treated),
        negative: ClockProvider::new(&treated),
    };
    provider.positive.fixture.prepared = constant_source([4096, -2048]);
    provider.negative.fixture.prepared = constant_source([-4096, 2048]);
    let actual = bus(
        &mut StageAudio::new(compile(&treated, false)),
        &mut provider,
        4936,
        128,
    )
    .samples;
    let drive = Saturation::new(GainDb::new(12000).unwrap())
        .unwrap()
        .drive_factor();
    for (offset, frame) in actual.iter().enumerate() {
        let expected = if offset == 64 {
            [(0.125 * drive) as f32, (-0.0625 * drive) as f32]
        } else {
            [0.0; 2]
        };
        assert_eq!(*frame, expected, "sample={}", 4936 + offset);
    }
}

#[test]
fn independent_post_shared_automation_preserves_every_eq_order() {
    use deadpan_core::{AudioTreatmentStage as S, EqualizerRecipe};
    let source = moved(true);
    let gain = ramp();
    let eq = EqualizerRecipe::new(
        [6000, -3000, 1500].map(|db| GainDb::new(db).unwrap()),
        None,
        None,
    )
    .unwrap();
    let kernel = deadpan_audio::equalizer_coefficients(eq, &AtomicBool::new(false)).unwrap();
    let drive = Saturation::new(GainDb::new(12000).unwrap()).unwrap();
    let start = START + 2700;
    let count = 96u32;
    let begin = start - 2048;
    let a = original(&source, begin, count + 4096);
    let b = read(&moved(false), begin, count + 4096);
    let common = |a: f64, b: f64| {
        let sum = a + b;
        if sum == 0.0 {
            drive.drive_factor()
        } else {
            drive.shape(sum) / sum
        }
    };
    let factors = |at: i64| {
        let sample = i128::from(at);
        [
            amplitude(&gain, ratio(sample * 5 - 24026, 8008)),
            amplitude(&gain, ratio(sample * 5 - 24024, 8008)),
        ]
    };
    for order in [
        [S::Equalizer, S::ClipGain, S::Saturation],
        [S::Saturation, S::Equalizer, S::ClipGain],
        [S::Saturation, S::ClipGain, S::Equalizer],
        [S::Equalizer, S::Saturation, S::ClipGain],
    ] {
        let mut input = [Vec::new(), Vec::new()];
        for (offset, (a, b)) in a.iter().zip(&b).enumerate() {
            let factor = factors(begin + offset as i64);
            let mut values = [a.map(f64::from), b.map(f64::from)];
            if order[0] == S::Saturation {
                let [a, b] = &mut values;
                for (a, b) in a.iter_mut().zip(b) {
                    let c = common(*a, *b);
                    *a *= c;
                    *b *= c;
                }
            }
            if order[2] == S::Equalizer {
                for voice in 0..2 {
                    values[voice] = values[voice].map(|v| v * factor[voice]);
                }
            }
            for voice in 0..2 {
                input[voice].push(values[voice]);
            }
        }
        let expected: Vec<[f32; 2]> = (0..count as usize)
            .map(|offset| {
                let factor = factors(start + offset as i64);
                let mut value: [[f64; 2]; 2] = std::array::from_fn(|voice| {
                    std::array::from_fn(|ch| {
                        kernel
                            .iter()
                            .enumerate()
                            .map(|(k, weight)| weight * input[voice][offset + k][ch])
                            .sum()
                    })
                });
                if order[1] == S::Saturation {
                    let [a, b] = &mut value;
                    for (a, b) in a.iter_mut().zip(b) {
                        let c = common(*a, *b);
                        *a *= c;
                        *b *= c;
                    }
                }
                if order[2] != S::Equalizer {
                    for voice in 0..2 {
                        value[voice] = value[voice].map(|v| v * factor[voice]);
                    }
                }
                std::array::from_fn(|ch| {
                    let sum = value[0][ch] + value[1][ch];
                    (if order[2] == S::Saturation {
                        drive.shape(sum)
                    } else {
                        sum
                    }) as f32
                })
            })
            .collect();
        let treatments = AudioTreatments::with_equalizer_stages(
            order.to_vec(),
            Some(gain.clone()),
            Some(drive),
            None,
            Some(eq),
        )
        .unwrap();
        let mut wire = serde_json::to_value(&source).unwrap();
        wire["nodes"]["owner"]["audio_treatments"] = serde_json::to_value(treatments).unwrap();
        let treated = ProjectDocument::from_json(&wire.to_string()).unwrap();
        let actual = read(&treated, start, count);
        let error = actual
            .iter()
            .zip(&expected)
            .flat_map(|(a, b)| (0..2).map(move |ch| (a[ch] - b[ch]).abs()))
            .fold(0.0_f32, f32::max);
        assert!(
            error <= 4.0 * f32::EPSILON,
            "order={order:?}, error={error}"
        );
        assert!(actual.iter().flatten().any(|v| v.abs() > 0.01));
        for (offset, frames) in [(73, 23), (0, 37), (37, 36)] {
            assert_eq!(
                read(&treated, start + offset, frames),
                actual[offset as usize..offset as usize + frames as usize],
                "order={order:?}"
            );
        }
    }
}

#[test]
fn nonlinear_owner_mixes_ntsc_sound_after_retained_clock_move() {
    let sound = read(&moved(false), START, FRAMES);
    let drive = Saturation::new(GainDb::new(12000).unwrap()).unwrap();
    for with_original in [false, true] {
        let moved = moved(with_original);
        let mut renderer = StageAudio::new(compile(&moved, false));
        let mut provider = ClockProvider::new(&moved);
        let original: Vec<_> = (0..FRAMES)
            .step_by(256)
            .flat_map(|offset| {
                renderer
                    .read_edge_faded(
                        &mut provider,
                        AudioSample(START + i64::from(offset)),
                        256.min(FRAMES - offset),
                        TIMEOUT,
                        &AtomicBool::new(false),
                    )
                    .unwrap()
                    .samples
            })
            .collect();
        if with_original {
            assert!(original.iter().flatten().any(|sample| sample.abs() > 0.01));
        }
        let expected: Vec<_> = original
            .iter()
            .zip(&sound)
            .map(|(original, sound)| {
                std::array::from_fn(|channel| {
                    drive.shape(f64::from(original[channel]) + f64::from(sound[channel])) as f32
                })
            })
            .collect();
        let treated = treated(
            &moved,
            ClipGain::new(GainDb::UNITY, false, vec![], vec![]).unwrap(),
        );
        assert_eq!(read(&treated, START, FRAMES), expected);
        for (offset, count) in [(3, 37), (195, 61), (6011, 395)] {
            assert_eq!(
                read(&treated, START + offset, count),
                expected[offset as usize..offset as usize + count as usize]
            );
        }
    }
}

#[test]
fn absent_original_does_not_supply_a_nonlinear_automation_clock() {
    let moved = moved(false);
    let sound = read(&moved, START, FRAMES);
    let gain = ClipGain::new(
        GainDb::UNITY,
        false,
        vec![
            GainEnvelope::new(
                GainClock::OwnerOutput,
                GainRange::new(ExactRatio::ZERO, ExactRatio::integer(4)).unwrap(),
                GainDb::new(-12000).unwrap(),
                vec![
                    GainSegment::new(ExactRatio::integer(4), GainDb::UNITY, GainCurve::Linear)
                        .unwrap(),
                ],
            )
            .unwrap(),
        ],
        vec![],
    )
    .unwrap();
    let treated = treated(&moved, gain.clone());
    let drive = Saturation::new(GainDb::new(12000).unwrap()).unwrap();
    let expected: Vec<_> = sound
        .iter()
        .enumerate()
        .map(|(offset, sample)| {
            // Owner starts at project frame 3, on the current 30000/1001 clock.
            let local = ratio(i128::from(START + offset as i64) * 5 - 24024, 8008);
            let db = gain.evaluate(local).unwrap().millidecibels;
            let factor = 10.0_f64.powf(db.numerator() as f64 / db.denominator() as f64 / 20000.0);
            sample.map(|value| drive.shape(f64::from(value) * factor) as f32)
        })
        .collect();
    assert_eq!(read(&treated, START, FRAMES), expected);
    assert_eq!(read(&treated, START + 131, 509), expected[131..640]);
}

#[test]
fn independent_pre_saturation_mute_preserves_each_input_clock() {
    // The two clocks agree outside this tiny mute range, including both ends
    // of the read. At output sample 5000 only the current sound clock is muted.
    let gain = ClipGain::new(
        GainDb::UNITY,
        false,
        vec![],
        vec![GainRange::new(ratio(975, 8008), ratio(977, 8008)).unwrap()],
    )
    .unwrap();
    compare_independent_prefix(gain);
}

fn compare_independent_prefix(gain: ClipGain) {
    let sound = read(&moved(false), START, FRAMES);
    let moved = moved(true);
    let mut renderer = StageAudio::new(compile(&moved, false));
    let mut provider = ClockProvider::new(&moved);
    let original: Vec<_> = (0..FRAMES)
        .step_by(256)
        .flat_map(|offset| {
            renderer
                .read_edge_faded(
                    &mut provider,
                    AudioSample(START + i64::from(offset)),
                    256.min(FRAMES - offset),
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap()
                .samples
        })
        .collect();
    let drive = Saturation::new(GainDb::new(12000).unwrap()).unwrap();
    let factor = |clock| {
        let value = gain.evaluate(clock).unwrap();
        if value.muted {
            0.0
        } else {
            10.0_f64.powf(
                value.millidecibels.numerator() as f64
                    / value.millidecibels.denominator() as f64
                    / 20000.0,
            )
        }
    };
    let expected: Vec<_> = original
        .iter()
        .zip(&sound)
        .enumerate()
        .map(|(offset, (original, sound))| {
            let sample = i128::from(START + offset as i64);
            // Inserting one NTSC frame translates Original's rounded origin by
            // 1602 samples. The sound's owner stays on the exact current grid.
            let original_gain = factor(ratio(sample * 5 - 24026, 8008));
            let sound_gain = factor(ratio(sample * 5 - 24024, 8008));
            std::array::from_fn(|ch| {
                drive.shape(
                    f64::from(original[ch]) * original_gain + f64::from(sound[ch]) * sound_gain,
                ) as f32
            })
        })
        .collect();
    let treated = treated(&moved, gain);
    assert_eq!(read(&treated, START, FRAMES), expected);
    for (offset, count) in [(193, 7), (195, 61), (6011, 395), (131, 509)] {
        assert_eq!(
            read(&treated, START + offset, count),
            expected[offset as usize..offset as usize + count as usize]
        );
    }
}

#[test]
fn independent_pre_saturation_envelope_preserves_each_input_clock() {
    compare_independent_prefix(
        ClipGain::new(
            GainDb::new(3000).unwrap(),
            false,
            vec![
                GainEnvelope::new(
                    GainClock::OwnerOutput,
                    GainRange::new(ExactRatio::ZERO, ExactRatio::integer(4)).unwrap(),
                    GainDb::new(-12000).unwrap(),
                    vec![
                        GainSegment::new(
                            ExactRatio::integer(4),
                            GainDb::new(6000).unwrap(),
                            GainCurve::Linear,
                        )
                        .unwrap(),
                    ],
                )
                .unwrap(),
            ],
            vec![],
        )
        .unwrap(),
    );
}

#[test]
fn muted_independent_clocks_keep_dependency_admission() {
    let treated = treated(
        &moved(true),
        ClipGain::new(
            GainDb::UNITY,
            true,
            vec![
                GainEnvelope::new(
                    GainClock::OwnerOutput,
                    GainRange::new(ExactRatio::ZERO, ExactRatio::integer(4)).unwrap(),
                    GainDb::new(-12000).unwrap(),
                    vec![
                        GainSegment::new(ExactRatio::integer(4), GainDb::UNITY, GainCurve::Linear)
                            .unwrap(),
                    ],
                )
                .unwrap(),
            ],
            vec![],
        )
        .unwrap(),
    );
    let mut provider = ClockProvider::new(&treated);
    let result = bus(
        &mut StageAudio::new(compile(&treated, false)),
        &mut provider,
        START,
        256,
    );
    assert_eq!(result.samples, vec![[0.0; 2]; 256]);
    assert!(provider.fixture.calls > 0);
    assert!(provider.fixture.context_calls > 0);
}
