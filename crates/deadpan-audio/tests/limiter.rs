#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::{
    ops::Range,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

use deadpan_audio::{LimitedTile, LimiterContext, LimiterError, TruePeakMeter};
use deadpan_core::AudioSample;

fn interval(start: i64, end: i64) -> Range<AudioSample> {
    AudioSample(start)..AudioSample(end)
}

fn prepare(source: Vec<[f32; 2]>) -> LimitedTile {
    let count = source.len() as i64;
    LimitedTile::prepare(
        LimiterContext {
            project_samples: interval(0, count),
            start: AudioSample(0),
            samples: source,
        },
        interval(0, count),
        Instant::now() + Duration::from_secs(60),
        &AtomicBool::new(false),
    )
    .unwrap()
}

fn bits(samples: &[[f32; 2]]) -> Vec<[u32; 2]> {
    samples
        .iter()
        .map(|sample| sample.map(f32::to_bits))
        .collect()
}

fn tone(count: usize) -> Vec<[f32; 2]> {
    (0..count)
        .map(|i| {
            let value = (0.8
                * (std::f64::consts::TAU * 1_000.0 * (i as f64 + 0.5) / 48_000.0).sin())
                as f32;
            [value, value / 4.0]
        })
        .collect()
}

#[test]
fn ordinary_tones_and_tiny_hard_fragments_keep_their_exact_samples() {
    let ordinary = tone(8_192);
    let limited = prepare(ordinary.clone());
    assert_eq!(bits(&limited.samples), bits(&ordinary));
    assert_eq!(limited.min_gain(), 1.0);

    for count in [1, 2] {
        let input = vec![[0.5, -0.25]; count];
        let limited = prepare(input.clone());
        assert_eq!(bits(&limited.samples), bits(&input));
        assert!(limited.gain.iter().all(|gain| *gain == 1.0));
    }
}

#[test]
fn the_long_ordinary_tone_ending_is_not_attenuated_by_the_guard() {
    let original = tone(65_536);
    let project = interval(0, original.len() as i64);
    let requested = interval(57_344, 65_536);
    let required = LimitedTile::required_context(&project, &requested).unwrap();
    let limited = LimitedTile::prepare(
        LimiterContext {
            project_samples: project,
            start: required.start,
            samples: original[required.start.0 as usize..required.end.0 as usize].to_vec(),
        },
        requested,
        Instant::now() + Duration::from_secs(60),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(bits(&limited.samples), bits(&original[57_344..]));
    assert_eq!(limited.min_gain(), 1.0);
}

#[test]
fn complete_zero_context_preserves_signed_bits_and_still_requires_its_halo() {
    let project = interval(0, 100_000);
    let requested = interval(50_000, 50_257);
    let required = LimitedTile::required_context(&project, &requested).unwrap();
    let count = (required.end.0 - required.start.0) as usize;
    let source: Vec<_> = (0..count)
        .map(|i| if i % 2 == 0 { [0.0, -0.0] } else { [-0.0, 0.0] })
        .collect();
    let context = || LimiterContext {
        project_samples: project.clone(),
        start: required.start,
        samples: source.clone(),
    };
    let deadline = Instant::now() + Duration::from_secs(60);
    let active = AtomicBool::new(false);
    let output = LimitedTile::prepare(context(), requested.clone(), deadline, &active).unwrap();
    let offset = (requested.start.0 - required.start.0) as usize;
    assert_eq!(bits(&output.samples), bits(&source[offset..offset + 257]));
    assert_eq!(output.gain, vec![1.0; 257]);
    assert_eq!(output.peak, 0.0);
    let mut missing = context();
    missing.samples.pop();
    assert!(matches!(
        LimitedTile::prepare(missing, requested.clone(), deadline, &active),
        Err(LimiterError::MissingContext { .. })
    ));
    assert!(matches!(
        LimitedTile::prepare(context(), requested, deadline, &AtomicBool::new(true)),
        Err(LimiterError::Cancelled)
    ));
}

#[test]
fn hot_linked_stereo_preserves_silent_islands_and_passes_the_independent_meter() {
    let original: Vec<_> = (0..8_192)
        .map(|n| {
            if n % 2 == 0 {
                [16.0, -8.0]
            } else {
                [0.0, -0.0]
            }
        })
        .collect();
    let limited = prepare(original.clone());
    assert!(limited.min_gain() > 0.0 && limited.max_gain() < 1.0);
    for ((input, output), gain) in original.iter().zip(&limited.samples).zip(&limited.gain) {
        assert_eq!(
            output.map(f32::to_bits),
            input.map(|value| ((f64::from(value) * gain) as f32).to_bits())
        );
        if input[0] == 0.0 {
            assert_eq!(output.map(f32::to_bits), input.map(f32::to_bits));
        }
    }
    let cancelled = AtomicBool::new(false);
    let mut meter = TruePeakMeter::new(8_192).unwrap();
    for chunk in limited.samples.chunks(256) {
        meter.push(chunk, &cancelled).unwrap();
    }
    assert!(
        meter
            .finish()
            .true_peak
            .iter()
            .all(|peak| *peak <= 10_f64.powf(-1.0 / 20.0))
    );
}

#[test]
fn exact_retained_failure_inputs_are_not_replaced_with_approximations() {
    for wav in [
        include_bytes!("../../../tools/audio-limiter-qualification/evidence/2026-09-22-postmask/producer/outputs/mask-alternating-dc-0.8-hard.wav").as_slice(),
        include_bytes!("../../../tools/audio-limiter-qualification/evidence/2026-09-22-postmask/producer/outputs/mask-alternating-dc-16-hard.wav").as_slice(),
    ] {
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(wav.len(), 44 + 8_192 * 8);
        let samples: Vec<_> = wav[44..]
            .chunks_exact(8)
            .map(|bytes| {
                [
                    f32::from_le_bytes(bytes[..4].try_into().unwrap()),
                    f32::from_le_bytes(bytes[4..].try_into().unwrap()),
                ]
            })
            .collect();
        let output = prepare(samples.clone());
        assert_eq!(bits(&output.samples), bits(&samples));
    }
}

#[test]
fn cold_unaligned_crops_match_the_same_owned_project_context() {
    let source: Vec<_> = (0..100_000)
        .map(|i| {
            let value = (1.8
                * (std::f64::consts::TAU * 12_345.0 * (i as f64 + 0.31) / 48_000.0).sin())
                as f32;
            if (44_000..44_512).contains(&i) && i % 2 == 0 {
                [0.0, -0.0]
            } else {
                [value, -0.7 * value]
            }
        })
        .collect();
    let project = interval(0, source.len() as i64);
    let whole_request = interval(45_056, 53_248);
    let whole = LimitedTile::prepare(
        LimiterContext {
            project_samples: project.clone(),
            start: AudioSample(0),
            samples: source.clone(),
        },
        whole_request,
        Instant::now() + Duration::from_secs(60),
        &AtomicBool::new(false),
    )
    .unwrap();
    for requested in [interval(49_153, 49_154), interval(45_993, 46_250)] {
        let required = LimitedTile::required_context(&project, &requested).unwrap();
        let cropped = LimitedTile::prepare(
            LimiterContext {
                project_samples: project.clone(),
                start: required.start,
                samples: source[required.start.0 as usize..required.end.0 as usize].to_vec(),
            },
            requested.clone(),
            Instant::now() + Duration::from_secs(60),
            &AtomicBool::new(false),
        )
        .unwrap();
        let begin = (requested.start.0 - whole.start.0) as usize;
        let end = begin + cropped.samples.len();
        assert_eq!(bits(&cropped.samples), bits(&whole.samples[begin..end]));
        assert_eq!(cropped.gain, whole.gain[begin..end]);
    }
}

#[test]
fn root_end_near_i64_max_keeps_signed_filter_context_in_wide_coordinates() {
    let project = interval(0, i64::MAX);
    let requested = interval(i64::MAX - 2, i64::MAX);
    let required = LimitedTile::required_context(&project, &requested).unwrap();
    let count = (required.end.0 - required.start.0) as usize;
    let output = LimitedTile::prepare(
        LimiterContext {
            project_samples: project,
            start: required.start,
            samples: vec![[0.25, -0.0]; count],
        },
        requested,
        Instant::now() + Duration::from_secs(60),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(output.start, AudioSample(i64::MAX - 2));
    assert_eq!(bits(&output.samples), bits(&[[0.25, -0.0]; 2]));
}

#[test]
fn range_context_samples_and_work_fail_before_returning_output() {
    assert!(matches!(
        LimitedTile::required_context(&interval(0, 10_000), &interval(0, 8_193)),
        Err(LimiterError::OutputBudget)
    ));
    assert!(matches!(
        LimitedTile::required_context(&interval(1, 10), &interval(1, 2)),
        Err(LimiterError::Range)
    ));
    let context = || LimiterContext {
        project_samples: interval(0, 100_000),
        start: AudioSample(50_000),
        samples: vec![[0.5; 2]],
    };
    let requested = interval(50_000, 50_001);
    let deadline = Instant::now() + Duration::from_secs(60);
    let active = AtomicBool::new(false);
    assert!(matches!(
        LimitedTile::prepare(context(), requested.clone(), deadline, &active),
        Err(LimiterError::MissingContext { .. })
    ));
    assert!(matches!(
        LimitedTile::prepare(context(), requested.clone(), Instant::now(), &active),
        Err(LimiterError::Deadline)
    ));
    assert!(matches!(
        LimitedTile::prepare(context(), requested, deadline, &AtomicBool::new(true)),
        Err(LimiterError::Cancelled)
    ));
    for sample in [f32::NAN, f32::INFINITY, 16.01] {
        let result = LimitedTile::prepare(
            LimiterContext {
                project_samples: interval(0, 1),
                start: AudioSample(0),
                samples: vec![[sample, 0.0]],
            },
            interval(0, 1),
            deadline,
            &active,
        );
        assert!(matches!(result, Err(LimiterError::InvalidSamples)));
    }
    let result = LimitedTile::prepare(
        LimiterContext {
            project_samples: interval(0, 131_073),
            start: AudioSample(0),
            samples: vec![[0.0; 2]; 131_073],
        },
        interval(0, 1),
        deadline,
        &active,
    );
    assert!(matches!(result, Err(LimiterError::ContextBudget)));
}

#[test]
fn an_empty_project_has_no_native_preparation_or_fabricated_samples() {
    let output = prepare(Vec::new());
    assert!(output.samples.is_empty() && output.gain.is_empty());
    assert_eq!(output.peak, 0.0);
    assert_eq!(output.maximum_reduction_db(), None);
}
