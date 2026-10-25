use super::*;
use proptest::prelude::*;

const RATE: u32 = 48_000;

fn audio(samples: u64) -> ActivityAudio {
    ActivityAudio {
        origin: 1_024,
        sample_rate: RATE,
        samples,
    }
}

/// Analysis PCM: a 440 Hz tone at `speech_level` where `speaking` says so,
/// otherwise a quiet noise floor.
fn pcm(samples: u64, speaking: impl Fn(u64) -> bool) -> Vec<f32> {
    (0..samples)
        .map(|index| {
            if speaking(index) {
                0.3 * (index as f32 * 440.0 * std::f32::consts::TAU / 16_000.0).sin()
            } else {
                // Deterministic low-level "room" at roughly -66 dBFS.
                if index % 2 == 0 { 0.0005 } else { -0.0005 }
            }
        })
        .collect()
}

/// Detector output that agrees with `speaking` at hop resolution.
fn probabilities(samples: u64, speaking: impl Fn(u64) -> bool) -> Vec<f32> {
    (0..samples.div_ceil(VAD_HOP))
        .map(|hop| {
            let middle = (hop * VAD_HOP + VAD_HOP / 2).min(samples - 1);
            if speaking(middle) { 0.95 } else { 0.02 }
        })
        .collect()
}

#[test]
fn a_pause_between_words_is_refined_to_the_quiet_frames() {
    // Speech 0–1.0 s and 1.6–2.0 s: the pause is 16,000..25,600.
    let samples = 32_000;
    let speaking = |index: u64| !(16_000..25_600).contains(&index);
    let activity = SpeechActivity::measure(
        audio(samples),
        &pcm(samples, speaking),
        &probabilities(samples, speaking),
    )
    .unwrap();
    assert_eq!(
        activity.pauses(),
        vec![Pause {
            start: 16_000,
            end: 25_600
        }],
        "the hop edges 16,384 and 25,600 move to the energy edges"
    );
}

#[test]
fn short_gaps_are_not_pauses_and_quiet_ends_are() {
    // A 100 ms gap inside speech, then trailing silence from 1.88 s.
    let samples = 48_000;
    let speaking = |index: u64| index < 8_000 || (9_600..30_080).contains(&index);
    let activity = SpeechActivity::measure(
        audio(samples),
        &pcm(samples, speaking),
        &probabilities(samples, speaking),
    )
    .unwrap();
    let pauses = activity.pauses();
    assert_eq!(
        pauses,
        vec![Pause {
            start: 30_080,
            end: samples
        }]
    );
}

#[test]
fn hysteresis_keeps_a_dip_inside_speech() {
    let samples: u64 = 16_000;
    let mut speech = vec![200_u8; usize::try_from(samples.div_ceil(VAD_HOP)).unwrap()];
    // A dip to 0.4 never ends speech once it has started.
    for value in &mut speech[10..20] {
        *value = 102;
    }
    let energy = vec![180; usize::try_from(samples.div_ceil(ENERGY_HOP)).unwrap()];
    let activity = SpeechActivity::new(audio(samples), speech, energy).unwrap();
    assert!(activity.pauses().is_empty());
}

#[test]
fn stored_values_are_validated() {
    let samples = 1_000;
    let speech = vec![0; 2];
    let energy = vec![0; 7];
    assert!(SpeechActivity::new(audio(samples), speech.clone(), energy.clone()).is_ok());
    assert!(SpeechActivity::new(audio(samples), vec![0; 3], energy.clone()).is_err());
    assert!(SpeechActivity::new(audio(samples), speech.clone(), vec![0; 6]).is_err());
    assert!(SpeechActivity::new(audio(samples), speech, vec![241; 7]).is_err());
    let nan = [f32::NAN, 0.0];
    assert!(SpeechActivity::measure(audio(samples), &vec![0.0; 1_000], &nan).is_err());
}

#[test]
fn analysis_samples_map_exactly_to_original_time() {
    let activity = SpeechActivity::new(audio(16_000), vec![0; 32], vec![0; 100]).unwrap();
    // origin 1,024 / 48,000 s plus 8,000 / 16,000 s.
    assert_eq!(
        activity.seconds(8_000).unwrap(),
        ExactRatio::new(1_024 + 24_000, 48_000).unwrap()
    );
}

#[test]
fn digital_silence_quantizes_to_the_floor_and_full_scale_to_the_top() {
    assert_eq!(quantized_energy(&[0.0; 160]).unwrap(), 0);
    assert_eq!(quantized_energy(&[1.0; 160]).unwrap(), 240);
    // -6.02 dBFS rounds to 228 half-decibel steps.
    assert_eq!(quantized_energy(&[0.5; 160]).unwrap(), 228);
}

proptest! {
    #[test]
    fn pauses_are_ordered_disjoint_and_long_enough(
        speech in proptest::collection::vec(any::<u8>(), 1..200),
        seed in any::<u64>(),
    ) {
        let samples = speech.len() as u64 * VAD_HOP - (seed % VAD_HOP);
        let energy = (0..samples.div_ceil(ENERGY_HOP))
            .map(|index| ((index.wrapping_mul(seed | 1) >> 3) % 241) as u8)
            .collect();
        let activity = SpeechActivity::new(audio(samples), speech, energy).unwrap();
        let pauses = activity.pauses();
        for pair in pauses.windows(2) {
            prop_assert!(pair[0].end <= pair[1].start);
        }
        for pause in pauses {
            prop_assert!(pause.end - pause.start >= MIN_PAUSE_SAMPLES);
            prop_assert!(pause.end <= samples);
        }
    }
}
