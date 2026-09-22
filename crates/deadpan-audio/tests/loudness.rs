use std::sync::atomic::{AtomicBool, Ordering};

use deadpan_audio::{LoudnessError, LoudnessMeter, LoudnessReport, MAX_LOUDNESS_FRAMES};

const RATE: u64 = 48_000;
const WINDOW: u64 = 19_200;

fn feed(meter: &mut LoudnessMeter, frames: u64, mut sample: impl FnMut(u64) -> [f32; 2]) {
    let cancel = AtomicBool::new(false);
    let mut block = [[0.0; 2]; 256];
    let mut position = 0;
    while position < frames {
        let length = usize::try_from((frames - position).min(256)).unwrap();
        for (offset, frame) in block[..length].iter_mut().enumerate() {
            *frame = sample(position + offset as u64);
        }
        meter.push(&block[..length], &cancel).unwrap();
        position += length as u64;
    }
}

fn sine_cycle(level_dbfs: f64) -> [f32; 48] {
    let amplitude = 10.0_f64.powf(level_dbfs / 20.0);
    std::array::from_fn(|index| {
        (amplitude * (std::f64::consts::TAU * index as f64 / 48.0).sin()) as f32
    })
}

fn tone(meter: &mut LoudnessMeter, frames: u64, level_dbfs: f64) {
    let cycle = sine_cycle(level_dbfs);
    feed(meter, frames, |index| [cycle[(index % 48) as usize]; 2]);
}

fn assert_lufs(report: &LoudnessReport, expected: f64, tolerance: f64) {
    let actual = report.integrated_lufs.unwrap();
    assert!(
        (actual - expected).abs() <= tolerance,
        "expected {expected} ± {tolerance} LUFS, measured {actual}; {report:?}"
    );
    assert!(report.relative_gate_lufs.unwrap().is_finite());
}

fn assert_report_bits(left: &LoudnessReport, right: &LoudnessReport) {
    assert_eq!(left.measured_frames, right.measured_frames);
    assert_eq!(left.complete_blocks, right.complete_blocks);
    assert_eq!(left.gated_blocks, right.gated_blocks);
    assert_eq!(
        left.integrated_lufs.map(f64::to_bits),
        right.integrated_lufs.map(f64::to_bits)
    );
    assert_eq!(
        left.relative_gate_lufs.map(f64::to_bits),
        right.relative_gate_lufs.map(f64::to_bits)
    );
}

#[test]
fn ebu_published_stereo_tone_and_gating_cases() {
    // Synthesized descriptions of EBU Tech 3341 (2023), Table 1, cases 1-5.
    // These are not the downloaded EBU conformance WAV files. Peak levels are
    // per channel; all segments are the same in-phase 1000 Hz sine.
    let cases: &[(&[(u64, f64)], f64)] = &[
        (&[(20 * RATE, -23.0)], -23.0),
        (&[(20 * RATE, -33.0)], -33.0),
        (
            &[(10 * RATE, -36.0), (60 * RATE, -23.0), (10 * RATE, -36.0)],
            -23.0,
        ),
        (
            &[
                (10 * RATE, -72.0),
                (10 * RATE, -36.0),
                (60 * RATE, -23.0),
                (10 * RATE, -36.0),
                (10 * RATE, -72.0),
            ],
            -23.0,
        ),
        (
            &[
                (20 * RATE, -26.0),
                (20 * RATE + RATE / 10, -20.0),
                (20 * RATE, -26.0),
            ],
            -23.0,
        ),
    ];
    for &(segments, expected) in cases {
        let frames = segments.iter().map(|(frames, _)| frames).sum();
        let mut meter = LoudnessMeter::new(frames).unwrap();
        for &(frames, level) in segments {
            tone(&mut meter, frames, level);
        }
        let report = meter.finish();
        assert_lufs(&report, expected, 0.1);
        assert_eq!(report.measured_frames, frames);
        assert_eq!(report.complete_blocks, (frames - WINDOW) / 4_800 + 1);
    }
}

#[test]
fn itu_997_hz_single_channel_reference_and_channel_energy() {
    // ITU-R BS.1770-5 Annex 1 specifies -3.01 LKFS for a single-channel,
    // full-scale 997 Hz sine. Stereo doubles energy regardless of polarity.
    let frames = 3 * RATE;
    let cycle: Vec<f32> = (0..RATE)
        .map(|index| (std::f64::consts::TAU * 997.0 * index as f64 / RATE as f64).sin() as f32)
        .collect();
    let mut left = LoudnessMeter::new(frames).unwrap();
    let mut right = LoudnessMeter::new(frames).unwrap();
    let mut stereo = LoudnessMeter::new(frames).unwrap();
    let mut inverted = LoudnessMeter::new(frames).unwrap();
    feed(&mut left, frames, |i| [cycle[(i % RATE) as usize], 0.0]);
    feed(&mut right, frames, |i| [0.0, cycle[(i % RATE) as usize]]);
    feed(&mut stereo, frames, |i| [cycle[(i % RATE) as usize]; 2]);
    feed(&mut inverted, frames, |i| {
        let value = cycle[(i % RATE) as usize];
        [value, -value]
    });
    let left = left.finish();
    let right = right.finish();
    let stereo = stereo.finish();
    let inverted = inverted.finish();
    assert_lufs(&left, -3.01, 0.01);
    assert_report_bits(&left, &right);
    assert_report_bits(&stereo, &inverted);
    assert!(
        (stereo.integrated_lufs.unwrap() - left.integrated_lufs.unwrap() - 10.0 * 2.0_f64.log10())
            .abs()
            < 1e-12
    );
}

#[test]
fn low_and_high_frequency_weighting_matches_analytic_filter_gain() {
    // Independent steady-state expectations from H(z) evaluated on the unit
    // circle using the published coefficients. Ten seconds makes the admitted
    // zero-history transient negligible at this tolerance. The low tones catch
    // a missing high-pass stage that a nominal 1 kHz calibration would miss.
    let expectations = [
        (20.0, -36.96636779238207),
        (100.0, -24.82449809269376),
        (1000.0, -22.993295603910457),
        (10_000.0, -19.649117777429872),
    ];
    let amplitude = 10.0_f64.powf(-23.0 / 20.0);
    for (frequency, expected) in expectations {
        let cycle: Vec<f32> = (0..RATE)
            .map(|index| {
                (amplitude * (std::f64::consts::TAU * frequency * index as f64 / RATE as f64).sin())
                    as f32
            })
            .collect();
        let mut meter = LoudnessMeter::new(10 * RATE).unwrap();
        feed(&mut meter, 10 * RATE, |i| [cycle[(i % RATE) as usize]; 2]);
        assert_lufs(&meter.finish(), expected, 0.02);
    }
}

#[test]
fn no_complete_or_audible_window_has_no_numeric_loudness() {
    for frames in [0, WINDOW - 1, WINDOW, WINDOW + 4_800 - 1, WINDOW + 4_800] {
        let mut meter = LoudnessMeter::new(frames.max(1)).unwrap();
        feed(&mut meter, frames, |_| [0.0; 2]);
        let report = meter.finish();
        assert_eq!(report.measured_frames, frames);
        assert_eq!(
            report.complete_blocks,
            frames
                .checked_sub(WINDOW)
                .map_or(0, |tail| tail / 4_800 + 1)
        );
        assert_eq!(report.gated_blocks, 0);
        assert_eq!(report.integrated_lufs, None);
        assert_eq!(report.relative_gate_lufs, None);
        let json = serde_json::to_value(&report).unwrap();
        assert!(json["integrated_lufs"].is_null());
        assert!(json["relative_gate_lufs"].is_null());
    }
    let mut short = LoudnessMeter::new(WINDOW - 1).unwrap();
    tone(&mut short, WINDOW - 1, -12.0);
    assert_eq!(short.finish().integrated_lufs, None);

    let mut below = LoudnessMeter::new(RATE).unwrap();
    tone(&mut below, RATE, -71.0);
    assert_eq!(below.finish().integrated_lufs, None);
    let mut above = LoudnessMeter::new(RATE).unwrap();
    tone(&mut above, RATE, -69.0);
    assert_lufs(&above.finish(), -69.0, 0.1);
}

#[test]
fn incomplete_final_hop_neither_pads_nor_changes_existing_blocks() {
    let mut exact = LoudnessMeter::new(WINDOW).unwrap();
    tone(&mut exact, WINDOW, -23.0);
    let exact = exact.finish();
    let mut tail = LoudnessMeter::new(WINDOW + 4_799).unwrap();
    tone(&mut tail, WINDOW, -23.0);
    tone(&mut tail, 4_799, 12.0);
    let tail = tail.finish();
    assert_eq!(exact.complete_blocks, 1);
    assert_eq!(tail.complete_blocks, 1);
    assert_eq!(tail.measured_frames, WINDOW + 4_799);
    assert_eq!(exact.gated_blocks, tail.gated_blocks);
    assert_eq!(exact.integrated_lufs, tail.integrated_lufs);
    assert_eq!(exact.relative_gate_lufs, tail.relative_gate_lufs);
}

#[test]
fn a_long_silent_gap_does_not_dilute_the_gated_programme() {
    fn programme(gap: u64) -> LoudnessReport {
        let mut meter = LoudnessMeter::new(4 * RATE + gap).unwrap();
        tone(&mut meter, 2 * RATE, -23.0);
        feed(&mut meter, gap, |_| [0.0; 2]);
        tone(&mut meter, 2 * RATE, -23.0);
        meter.finish()
    }
    // Both gaps align to all gating boundaries. Their onset/offset windows
    // match; extra silence must not enter either mean or leave subtractive drift.
    let short = programme(WINDOW);
    let long = programme(30 * RATE);
    assert_eq!(short.gated_blocks, long.gated_blocks);
    assert_eq!(long.complete_blocks - short.complete_blocks, 296);
    assert!((short.integrated_lufs.unwrap() - long.integrated_lufs.unwrap()).abs() < 1e-10);
    assert!((short.relative_gate_lufs.unwrap() - long.relative_gate_lufs.unwrap()).abs() < 1e-10);
}

#[test]
fn arbitrary_chunk_partitions_are_bit_identical() {
    let cycle = sine_cycle(-13.0);
    let samples: Vec<[f32; 2]> = (0..137_219)
        .map(|index| {
            let envelope = match index / 12_000 % 4 {
                0 => 0.0,
                1 => 0.001,
                2 => 0.25,
                _ => 1.0,
            };
            [
                cycle[index % 48] * envelope,
                cycle[(index * 5 + 7) % 48] * envelope * 0.61,
            ]
        })
        .collect();
    fn measure(samples: &[[f32; 2]], partition: &[usize]) -> LoudnessReport {
        let mut meter = LoudnessMeter::new(samples.len() as u64).unwrap();
        let mut position = 0;
        let mut chunk = 0;
        while position < samples.len() {
            let end = (position + partition[chunk % partition.len()]).min(samples.len());
            meter
                .push(&samples[position..end], &AtomicBool::new(false))
                .unwrap();
            position = end;
            chunk += 1;
        }
        meter.finish()
    }
    let expected = measure(&samples, &[256]);
    for partition in [&[1][..], &[7, 255, 19, 1, 256, 43], &[255]] {
        assert_report_bits(&expected, &measure(&samples, partition));
    }
}

#[test]
fn rejected_blocks_and_cancellation_leave_all_state_unchanged() {
    const FRAMES: u64 = 21_000;
    let cycle = sine_cycle(-23.0);
    let samples: Vec<[f32; 2]> = (0..FRAMES)
        .map(|index| {
            let value = cycle[(index % 48) as usize];
            [value, -value * 0.3]
        })
        .collect();
    let cancel = AtomicBool::new(false);
    let mut actual = LoudnessMeter::new(FRAMES).unwrap();
    for block in samples[..19_000].chunks(256) {
        actual.push(block, &cancel).unwrap();
    }
    assert_eq!(actual.push(&[], &cancel), Err(LoudnessError::InvalidBlock));
    assert_eq!(
        actual.push(&[[0.0; 2]; 257], &cancel),
        Err(LoudnessError::InvalidBlock)
    );
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 16.001, -16.001] {
        let mut block = [[0.25; 2]; 256];
        // A late invalid value would contaminate the first completed window if
        // validation were interleaved with filter or ring updates.
        block[255][1] = invalid;
        assert_eq!(
            actual.push(&block, &cancel),
            Err(LoudnessError::InvalidSamples)
        );
    }
    cancel.store(true, Ordering::Release);
    assert_eq!(
        actual.push(&samples[19_000..19_256], &cancel),
        Err(LoudnessError::Cancelled)
    );
    cancel.store(false, Ordering::Release);
    for block in samples[19_000..20_800].chunks(256) {
        actual.push(block, &cancel).unwrap();
    }
    assert_eq!(
        actual.push(&[[0.5; 2]; 201], &cancel),
        Err(LoudnessError::FrameBudgetExceeded)
    );
    actual.push(&samples[20_800..], &cancel).unwrap();
    assert_eq!(
        actual.push(&[[0.0; 2]], &cancel),
        Err(LoudnessError::FrameBudgetExceeded)
    );
    let mut expected = LoudnessMeter::new(FRAMES).unwrap();
    for block in samples.chunks(256) {
        expected.push(block, &cancel).unwrap();
    }
    assert_report_bits(&actual.finish(), &expected.finish());
}

#[test]
fn admission_is_bounded_and_preserves_full_scale_pcm() {
    for limit in [0, MAX_LOUDNESS_FRAMES + 1, u64::MAX] {
        assert!(matches!(
            LoudnessMeter::new(limit),
            Err(LoudnessError::InvalidLimit)
        ));
    }
    assert_eq!(MAX_LOUDNESS_FRAMES, 4_147_200_000);
    assert_eq!(
        LoudnessMeter::new(MAX_LOUDNESS_FRAMES)
            .unwrap()
            .finish()
            .measured_frames,
        0
    );
    let samples: Vec<[f32; 2]> = (0..WINDOW)
        .map(|index| {
            let value = if index % 2 == 0 { 16.0 } else { -16.0 };
            [value; 2]
        })
        .collect();
    let original = samples.clone();
    let mut meter = LoudnessMeter::new(WINDOW).unwrap();
    for block in samples.chunks(256) {
        meter.push(block, &AtomicBool::new(false)).unwrap();
    }
    let report = meter.finish();
    assert_eq!(samples, original);
    assert!(report.integrated_lufs.unwrap() > 24.0);
    assert!(report.integrated_lufs.unwrap().is_finite());
    assert!(report.relative_gate_lufs.unwrap().is_finite());
}
