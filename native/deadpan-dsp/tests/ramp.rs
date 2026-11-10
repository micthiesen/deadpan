//! The pitch-preserving ramp schedule through the native streaming renderer.
use deadpan_dsp::{
    DspError, MAX_STREAM_WINDOW_FRAMES, QUANTUM, RAMP_ENGINE_ID, RampSchedule, StreamRecipe,
    StreamingStretch,
};
use std::sync::atomic::AtomicBool;

/// Rates 1/2 at the first output sample to 2 at the last, over `output`
/// samples; the input is `5·output/4` samples.
fn half_to_double(output: u32) -> RampSchedule {
    RampSchedule::new(2 * i128::from(output), 3, 4 * i128::from(output), output).unwrap()
}

fn tone(frequency: f64, frames: u32) -> Vec<[f32; 2]> {
    (0..frames)
        .map(|at| {
            let value =
                (0.5 * (std::f64::consts::TAU * frequency * f64::from(at) / 48_000.0).sin()) as f32;
            [value, -value]
        })
        .collect()
}

struct Run {
    output: Vec<[f32; 2]>,
    ranges: Vec<std::ops::Range<u32>>,
}

fn render(recipe: StreamRecipe, input: &[[f32; 2]], pattern: &[usize]) -> Run {
    let mut stream = StreamingStretch::new(recipe).unwrap();
    let cancel = AtomicBool::new(false);
    let mut run = Run {
        output: Vec::new(),
        ranges: Vec::new(),
    };
    for &requested in pattern.iter().cycle() {
        let count = requested.min((recipe.output_frames() - stream.position()) as usize);
        if count == 0 {
            break;
        }
        let range = stream.required_input(count).unwrap();
        assert!((range.end - range.start) as usize <= MAX_STREAM_WINDOW_FRAMES);
        let window = |channel: usize| -> Vec<f32> {
            range
                .clone()
                .map(|at| input.get(at as usize).map_or(0.0, |frame| frame[channel]))
                .collect()
        };
        let (left, right) = (window(0), window(1));
        let (mut first, mut second) = ([0.0; QUANTUM], [0.0; QUANTUM]);
        let read = stream
            .read(
                range.start,
                [&left, &right],
                [&mut first[..count], &mut second[..count]],
                &cancel,
            )
            .unwrap();
        assert_eq!(read, count);
        if !range.is_empty() {
            run.ranges.push(range);
        }
        run.output.extend(
            first[..read]
                .iter()
                .zip(&second[..read])
                .map(|(&l, &r)| [l, r]),
        );
    }
    run
}

#[test]
fn every_call_consumes_the_exact_quadratic_between_quantum_boundaries() {
    let output = 48_000;
    let schedule = half_to_double(output);
    let recipe = StreamRecipe::ramp(60_000, schedule).unwrap();
    assert_eq!(recipe.engine_id(), RAMP_ENGINE_ID);
    assert_eq!(recipe.output_frames(), output);
    let input = tone(440.0, 60_000);
    let run = render(recipe, &input, &[256]);
    // Contiguous windows, each ending at analysis latency plus the rounded
    // exact position one output latency past the call's last sample. Both
    // latencies are fixed by the pinned analysis window; find them once.
    for pair in run.ranges.windows(2) {
        assert_eq!(pair[0].end, pair[1].start);
    }
    let ends: Vec<i64> = run
        .ranges
        .iter()
        .map(|range| i64::from(range.end))
        .collect();
    let (input_latency, output_latency) = (0..8192)
        .find_map(|latency| {
            let offset = ends[1] - schedule.boundary(512 + latency).unwrap();
            ends[1..]
                .iter()
                .enumerate()
                .take_while(|&(_, &end)| end < 60_000)
                .all(|(index, &end)| {
                    end == offset
                        + schedule
                            .boundary((index as i64 + 2) * 256 + latency)
                            .unwrap()
                })
                .then_some((offset, latency))
        })
        .expect("one fixed latency pair explains every boundary");
    assert!(input_latency > 0 && output_latency > 0);
    assert!(ends.len() > 150, "{}", ends.len());
}

#[test]
fn a_steady_tone_keeps_its_pitch_while_the_duration_follows_the_ramp() {
    let output = 96_000;
    let schedule = half_to_double(output);
    let input = tone(441.0, 120_000);
    let run = render(
        StreamRecipe::ramp(120_000, schedule).unwrap(),
        &input,
        &[256],
    );
    assert_eq!(run.output.len(), output as usize);
    // Frequency from interpolated rising zero crossings in 4,096-sample windows
    // away from both padded ends.
    for window in (8_192..(output as usize - 8_192)).step_by(4_096) {
        let samples: Vec<f64> = run.output[window..window + 4_096]
            .iter()
            .map(|frame| f64::from(frame[0]))
            .collect();
        let crossings: Vec<f64> = samples
            .windows(2)
            .enumerate()
            .filter(|(_, pair)| pair[0] < 0.0 && pair[1] >= 0.0)
            .map(|(at, pair)| at as f64 + pair[0] / (pair[0] - pair[1]))
            .collect();
        let period = (crossings[crossings.len() - 1] - crossings[0]) / (crossings.len() - 1) as f64;
        let frequency = 48_000.0 / period;
        assert!(
            (frequency - 441.0).abs() < 441.0 * 0.005,
            "{frequency} Hz at output {window}"
        );
    }
}

#[test]
fn read_sizes_and_replays_never_change_the_output() {
    let output = 20_011;
    // A falling ramp, 3x to 1/3x, over an odd count.
    let schedule =
        RampSchedule::new(9 * i128::from(output), -4, 3 * i128::from(output), output).unwrap();
    let input: Vec<[f32; 2]> = (0..40_000_u32)
        .map(|at| {
            [
                ((at * 97 % 16384) as f32 - 8192.0) / 32768.0,
                ((at * 43 % 8192) as f32 - 4096.0) / 32768.0,
            ]
        })
        .collect();
    let recipe = StreamRecipe::ramp(40_000, schedule).unwrap();
    let whole = render(recipe, &input, &[256]).output;
    assert_eq!(whole.len(), output as usize);
    for pattern in [&[1, 17, 253, 127, 256][..], &[64][..], &[255, 2][..]] {
        assert_eq!(render(recipe, &input, pattern).output, whole, "{pattern:?}");
    }
    // A fresh renderer replays canonically to any position: discard a prefix
    // and the suffix matches.
    let mut stream = StreamingStretch::new(recipe).unwrap();
    let cancel = AtomicBool::new(false);
    let mut suffix = Vec::new();
    while stream.position() < output {
        let start = stream.position();
        let count = (output - start).min(256) as usize;
        let range = stream.required_input(count).unwrap();
        let window = |channel: usize| -> Vec<f32> {
            range
                .clone()
                .map(|at| input.get(at as usize).map_or(0.0, |frame| frame[channel]))
                .collect()
        };
        let (mut left, mut right) = ([0.0; QUANTUM], [0.0; QUANTUM]);
        stream
            .read(
                range.start,
                [&window(0), &window(1)],
                [&mut left[..count], &mut right[..count]],
                &cancel,
            )
            .unwrap();
        if start >= 9_000 {
            suffix.extend(
                left[..count]
                    .iter()
                    .zip(&right[..count])
                    .map(|(&l, &r)| [l, r]),
            );
        }
    }
    assert_eq!(suffix, whole[9_216..]);
}

#[test]
fn ramp_recipes_refuse_outside_the_stretch_range() {
    // 1/2 to 10x at the end is outside the admitted slope.
    assert_eq!(
        RampSchedule::new(1, 19, 2, 1),
        Err(DspError::Rate),
        "end slope 19.5"
    );
    assert_eq!(
        StreamRecipe::ramp(0, half_to_double(16)),
        Err(DspError::InputLength)
    );
}

/// Half-amplitude onset times of 1 kHz bursts (attack 96 samples), from a
/// 256-sample moving RMS.
fn onsets(samples: &[[f32; 2]], threshold: f64) -> Vec<f64> {
    let mut found = Vec::new();
    let mut energy = 0.0_f64;
    let mut above = false;
    for (at, frame) in samples.iter().enumerate() {
        energy += f64::from(frame[0]).powi(2);
        if at >= 256 {
            energy -= f64::from(samples[at - 256][0]).powi(2);
        }
        let rms = (energy.max(0.0) / 256.0).sqrt();
        if !above && rms > threshold {
            // The moving window's centre.
            found.push(at as f64 - 128.0);
        }
        above = rms > threshold;
    }
    found
}

#[test]
fn burst_onsets_land_at_the_exact_mapping() {
    let output = 96_000;
    let schedule = half_to_double(output);
    let input_frames = 120_000;
    let input: Vec<[f32; 2]> = (0..input_frames)
        .map(|at| {
            let phase = at % 9_600;
            let gate = if phase < 4_800 {
                (f64::from(phase) / 96.0).min(1.0)
            } else {
                0.0
            };
            let value =
                (0.5 * gate * (std::f64::consts::TAU * 1_000.0 * f64::from(at) / 48_000.0).sin())
                    as f32;
            [value, value]
        })
        .collect();
    let run = render(
        StreamRecipe::ramp(input_frames, schedule).unwrap(),
        &input,
        &[256],
    );
    let source = onsets(&input, 0.5 * 0.5 / 2.0_f64.sqrt());
    let heard = onsets(&run.output, 0.5 * 0.5 / 2.0_f64.sqrt());
    let (linear, quadratic, denominator) = schedule.coefficients();
    let q = |n: f64| (linear as f64 * n + quadratic as f64 * n * n) / denominator as f64;
    // Invert the increasing quadratic by bisection.
    let inverse = |x: f64| {
        let (mut low, mut high) = (0.0, f64::from(output));
        for _ in 0..80 {
            let middle = 0.5 * (low + high);
            if q(middle) < x {
                low = middle
            } else {
                high = middle
            }
        }
        low
    };
    let mut worst = 0.0_f64;
    for &at in source.iter().skip(1) {
        let expected = inverse(at);
        if !(4_096.0..f64::from(output) - 4_096.0).contains(&expected) {
            continue;
        }
        let nearest = heard
            .iter()
            .map(|&heard| heard - expected)
            .min_by(|a, b| a.abs().total_cmp(&b.abs()))
            .unwrap();
        worst = worst.max(nearest.abs());
    }
    eprintln!("worst onset deviation {worst} samples");
    // Measured 97 samples (2 ms). This detector reads constant-rate stretches
    // of the same bursts with a bias of -104 (2x) to +476 (1/2x) samples: the
    // phase vocoder smears a 2 ms attack. The schedule itself stays within a
    // fraction of a sample (see RampSchedule::linearisation_bound).
    assert!(worst < 240.0, "{worst}");
}
