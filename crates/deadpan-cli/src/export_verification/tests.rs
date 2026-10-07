use super::metrics::{self, I420};
use super::*;

fn picture(width: u32, height: u32, luma: impl Fn(u32, u32) -> u8) -> I420 {
    let mut y = Vec::new();
    for row in 0..height {
        for column in 0..width {
            y.push(u16::from(luma(column, row)));
        }
    }
    let chroma = (width * height / 4) as usize;
    I420 {
        width,
        height,
        bits: 8,
        y,
        cb: vec![128; chroma],
        cr: vec![128; chroma],
    }
}

/// The same picture at ten bits: every limited-range code times four.
fn ten_bit(picture: &I420) -> I420 {
    let widen = |plane: &[u16]| plane.iter().map(|value| value * 4).collect();
    I420 {
        width: picture.width,
        height: picture.height,
        bits: 10,
        y: widen(&picture.y),
        cb: widen(&picture.cb),
        cr: widen(&picture.cr),
    }
}

fn square(at: u32) -> I420 {
    picture(64, 32, |x, y| {
        if (at..at + 8).contains(&x) && (8..16).contains(&y) {
            200
        } else {
            40
        }
    })
}

fn references(frames: &[(u64, I420)]) -> BTreeMap<u64, (I420, Provenance)> {
    frames
        .iter()
        .map(|(ordinal, picture)| (*ordinal, (picture.clone(), Provenance::Background)))
        .collect()
}

fn click(at: usize, length: usize) -> Vec<[f32; 2]> {
    let mut samples = vec![[0.0; 2]; length];
    for (offset, value) in [0.6_f32, -0.4, 0.25, -0.1].into_iter().enumerate() {
        samples[at + offset] = [value, value * 0.8];
    }
    samples
}

#[test]
fn tight_planes_and_identical_pictures_pass_with_capped_psnr() {
    let bytes: Vec<u8> = (0..(4 * 2 + 2 * 2)).map(|value| value as u8).collect();
    let planes = I420::from_tight(4, 2, &bytes).unwrap();
    assert_eq!(planes.y.len(), 8);
    assert_eq!(
        (planes.cb.as_slice(), planes.cr.as_slice()),
        (&[8_u16, 9][..], &[10_u16, 11][..])
    );
    assert!(I420::from_tight(4, 2, &bytes[1..]).is_none());
    assert!(I420::from_tight(3, 2, &bytes).is_none());
    assert_eq!(planes.bits, 8);
    let samples: Vec<u16> = (0..12).map(|value| value * 80).collect();
    let wide = I420::from_tight_p10(4, 2, &samples).unwrap();
    assert_eq!(
        (wide.bits, wide.peak(), wide.cr.as_slice()),
        (10, 1023, &[800_u16, 880][..])
    );
    let bytes_le: Vec<u8> = samples
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect();
    assert_eq!(I420::from_tight_p10_le(4, 2, &bytes_le).unwrap(), wide);
    let mut over = samples.clone();
    over[3] = 1024;
    assert!(I420::from_tight_p10(4, 2, &over).is_none());
    assert!(I420::from_tight_p10(4, 2, &samples[1..]).is_none());
    let reference = square(20);
    let mut stored = references(&[(4, square(18)), (5, reference.clone()), (6, square(22))]);
    let check = compare_picture(
        ProjectFrame(10),
        5,
        &reference,
        &mut stored,
        &Thresholds::default(),
    );
    assert!(check.passed, "{:?}", check.flags);
    assert_eq!(check.project_frame, 15);
    assert_eq!(check.planes[0].psnr_db, metrics::IDENTICAL_PSNR_DB);
    assert!(
        stored.contains_key(&5),
        "reference stays available as a neighbor"
    );
}

#[test]
fn picture_checks_flag_index_shift_black_frame_and_gross_framing_change() {
    let thresholds = Thresholds::default();
    // The decoded picture is the next frame's content: an index shift.
    let mut stored = references(&[(4, square(16)), (5, square(20)), (6, square(24))]);
    let shifted = compare_picture(ProjectFrame(0), 5, &square(24), &mut stored, &thresholds);
    assert!(shifted.flags.contains(&"frame_index_mismatch"));
    assert!(shifted.flags.contains(&"luma_psnr"));
    let index = shifted.next_index.unwrap();
    assert_eq!(index.status, IndexObservability::Observable);
    assert!(index.reference_luma_rms > 2.0 * index.max_error_luma_rms);
    assert!((index.max_error_luma_rms - 6.405_310_400_349_427).abs() < 1e-12);
    assert!(!shifted.passed);
    // A decoded black frame where the reference shows a bright picture.
    let bright = picture(64, 32, |_, _| 180);
    let black = picture(64, 32, |_, _| 16);
    let mut stored = references(&[(0, bright.clone())]);
    let check = compare_picture(ProjectFrame(0), 0, &black, &mut stored, &thresholds);
    for flag in [
        "unexpected_black_frame",
        "gross_structural_mismatch",
        "luma_psnr",
    ] {
        assert!(check.flags.contains(&flag), "{flag}: {:?}", check.flags);
    }
    // A 2x zoom of a gradient keeps similar means but moves structure.
    let gradient = picture(64, 32, |x, _| (16 + x * 3) as u8);
    let zoomed = picture(64, 32, |x, _| (16 + 48 + x * 3 / 2) as u8);
    let mut stored = references(&[(0, gradient)]);
    let check = compare_picture(ProjectFrame(0), 0, &zoomed, &mut stored, &thresholds);
    assert!(check.flags.contains(&"gross_structural_mismatch"));
    // Small coding noise passes every gate.
    let noisy = picture(64, 32, |x, y| {
        let base = if (20..28).contains(&x) && (8..16).contains(&y) {
            200
        } else {
            40
        };
        base + u8::from((x + y) % 7 == 0)
    });
    let mut stored = references(&[(1, square(16)), (2, square(20)), (3, square(24))]);
    let check = compare_picture(ProjectFrame(0), 2, &noisy, &mut stored, &thresholds);
    assert!(check.passed, "{:?}", check.flags);
}

#[test]
fn compression_can_erase_nearby_frame_identity_without_an_index_failure() {
    // Known-order compression witness: libx264 CRF 23 removes the middle
    // frame's one-code checkerboard in [flat 100, checker 99/101, flat 160].
    // Model that deterministic quantization directly, without a codec in this
    // unit test. The former nearest-reference rule falsely called it a shift.
    let flat = picture(64, 32, |_, _| 100);
    let checker = picture(64, 32, |x, y| 99 + 2 * ((x + y) % 2) as u8);
    let later = picture(64, 32, |_, _| 160);
    let thresholds = Thresholds::default();
    for bits in [8, 10] {
        let depth = |picture: &I420| {
            if bits == 10 {
                ten_bit(picture)
            } else {
                picture.clone()
            }
        };
        let mut stored = references(&[(0, depth(&flat)), (1, depth(&checker)), (2, depth(&later))]);
        let compressed =
            compare_picture(ProjectFrame(0), 1, &depth(&flat), &mut stored, &thresholds);
        assert!(compressed.passed, "{compressed:?}");
        assert!(
            compressed.previous_luma_psnr_db.unwrap()
                > compressed.planes[0].psnr_db + thresholds.neighbor_psnr_margin_db
        );
        let previous = compressed.previous_index.unwrap();
        assert_eq!(previous.status, IndexObservability::Unobservable);
        assert_eq!(
            previous.reference_luma_rms,
            if bits == 10 { 4.0 } else { 1.0 }
        );
        assert_eq!(
            compressed.next_index.unwrap().status,
            IndexObservability::Observable
        );
        let summary = summarize(std::slice::from_ref(&compressed), &[]);
        assert_eq!(summary.index_observable_comparisons, 1);
        assert_eq!(summary.index_unobservable_comparisons, 1);
        assert_eq!(summary.pictures_without_index_neighbors, 0);
        assert!(summary.min_neighbor_margin_db.unwrap() < -0.5);
        let json = serde_json::to_value(&compressed).unwrap();
        assert_eq!(json["previous_index"]["status"], "unobservable");
        assert_eq!(json["next_index"]["status"], "observable");

        // Observability depends on the references and declared fidelity alone.
        // A poor decode cannot increase its own allowance to hide a mismatch.
        let badly_decoded =
            compare_picture(ProjectFrame(0), 1, &depth(&later), &mut stored, &thresholds);
        assert_eq!(badly_decoded.previous_index, compressed.previous_index);
        assert_eq!(badly_decoded.next_index, compressed.next_index);
        assert!(badly_decoded.flags.contains(&"frame_index_mismatch"));
        assert!(badly_decoded.flags.contains(&"luma_psnr"));
    }
}

#[test]
fn index_observability_uses_strict_twice_bound_and_declared_depth_threshold() {
    // A mathematical peak of 100 and 40 dB give an exactly represented bound
    // of one code. Closed error balls still touch at separation two.
    for (distance, status) in [
        (0, IndexObservability::Unobservable),
        (1, IndexObservability::Unobservable),
        (2, IndexObservability::Unobservable),
        (3, IndexObservability::Observable),
    ] {
        let index = index_check::compare_references(&[40; 16], &[40 + distance; 16], 100, 40.0);
        assert_eq!(index.max_error_luma_rms, 1.0);
        assert_eq!(index.reference_luma_rms, f64::from(distance));
        assert_eq!(index.status, status);
    }
    let thresholds = Thresholds::default();
    for (peak, minimum, expected) in [
        (255, thresholds.min_luma_psnr_db, 6.405_310_400_349_427),
        (
            1023,
            thresholds.min_hdr_luma_psnr_db,
            32.350_100_463_522_516,
        ),
    ] {
        let index = index_check::compare_references(&[0; 16], &[peak; 16], peak, minimum);
        assert!((index.max_error_luma_rms - expected).abs() < 1e-12);
        assert_eq!(index.status, IndexObservability::Observable);
        let loose = index_check::compare_references(&[0; 16], &[peak; 16], peak, 0.0);
        assert_eq!(loose.max_error_luma_rms, f64::from(peak));
        assert_eq!(loose.status, IndexObservability::Unobservable);
        let strict = index_check::compare_references(&[0; 16], &[1; 16], peak, 100.0);
        assert_eq!(strict.status, IndexObservability::Observable);
        let identical = index_check::compare_references(&[1; 16], &[1; 16], peak, 100.0);
        assert_eq!(identical.reference_luma_rms, 0.0);
        assert_eq!(identical.status, IndexObservability::Unobservable);
    }
    // Choose separation 14 eight-bit-equivalent codes: above the SDR bound
    // (12.81) but below the HDR bound after scaling (64.70 ten-bit codes).
    let flat = picture(64, 32, |_, _| 100);
    let neighbor = picture(64, 32, |_, _| 114);
    let mut stored = references(&[(0, flat.clone()), (1, neighbor.clone())]);
    let sdr = compare_picture(ProjectFrame(0), 0, &flat, &mut stored, &thresholds);
    assert_eq!(
        sdr.next_index.unwrap().status,
        IndexObservability::Observable
    );
    let mut stored = references(&[(0, ten_bit(&flat)), (1, ten_bit(&neighbor))]);
    let hdr = compare_picture(
        ProjectFrame(0),
        0,
        &ten_bit(&flat),
        &mut stored,
        &thresholds,
    );
    assert_eq!(
        hdr.next_index.unwrap().status,
        IndexObservability::Unobservable
    );
}

#[test]
fn static_and_single_frame_reports_distinguish_unobservable_from_absent_neighbors() {
    let flat = picture(64, 32, |_, _| 100);
    let thresholds = Thresholds::default();
    let mut stored = references(&[(0, flat.clone()), (1, flat.clone())]);
    let first = compare_picture(ProjectFrame(0), 0, &flat, &mut stored, &thresholds);
    let last = compare_picture(ProjectFrame(0), 1, &flat, &mut stored, &thresholds);
    assert!(first.passed && last.passed);
    assert!(first.previous_index.is_none());
    assert!(last.next_index.is_none());
    assert_eq!(first.next_index.unwrap().reference_luma_rms, 0.0);
    let mut stored = references(&[(0, flat.clone())]);
    let only = compare_picture(ProjectFrame(0), 0, &flat, &mut stored, &thresholds);
    assert!(only.passed && only.previous_index.is_none() && only.next_index.is_none());
    let summary = summarize(&[first, last, only], &[]);
    assert_eq!(summary.pictures_checked, 3);
    assert_eq!(summary.index_observable_comparisons, 0);
    assert_eq!(summary.index_unobservable_comparisons, 2);
    assert_eq!(summary.pictures_without_index_neighbors, 1);
    let json = serde_json::to_value(&summary).unwrap();
    assert_eq!(json["index_unobservable_comparisons"], 2);
    assert_eq!(json["pictures_without_index_neighbors"], 1);
}

/// Deterministic aperiodic noise (LCG), optionally one-pole lowpassed.
fn noise(seed: u64, length: usize, smoothing: f32, level: f32) -> Vec<[f32; 2]> {
    let mut state = seed;
    let mut filtered = [0.0_f32; 2];
    (0..length)
        .map(|_| {
            let mut next = || {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                ((state >> 40) as f32 / (1 << 24) as f32) * 2.0 - 1.0
            };
            let raw = [next(), next()];
            for channel in 0..2 {
                filtered[channel] =
                    filtered[channel] * smoothing + raw[channel] * (1.0 - smoothing);
            }
            [filtered[0] * level, filtered[1] * level]
        })
        .collect()
}

/// Decoded buffer for window [0, length): `content` shifted by `lag` (decoded
/// sample t + lag holds reference t), padded by the alignment margin, starting
/// at output sample -MAX_ALIGNMENT_LAG with the reported coverage.
fn shifted(content: &[[f32; 2]], lag: i64) -> (i64, Vec<[f32; 2]>, Vec<bool>) {
    let start = -MAX_ALIGNMENT_LAG;
    let length = content.len() + 2 * MAX_ALIGNMENT_LAG as usize;
    let mut decoded = vec![[0.0; 2]; length];
    for (index, value) in content.iter().enumerate() {
        let at = index as i64 + lag - start;
        if (0..length as i64).contains(&at) {
            decoded[at as usize] = *value;
        }
    }
    (start, decoded, vec![true; length])
}

fn check(reference: &[[f32; 2]], lag: i64) -> AudioCheck {
    let (start, decoded, covered) = shifted(reference, lag);
    compare_audio(
        0..reference.len() as i64,
        AudioSample(1000),
        reference,
        start,
        &decoded,
        &covered,
        &Thresholds::default(),
    )
}

#[test]
fn audio_offsets_are_measured_reported_and_never_compensated() {
    let reference = click(3000, 9600);
    let exact = check(&reference, 0);
    assert!(exact.passed, "{:?}", exact.flags);
    assert_eq!(exact.offset_status, OffsetStatus::VerifiedZero);
    assert_eq!(exact.measured_offset_samples, Some(0));
    assert_eq!(exact.project_samples, [1000, 10_600]);
    // The documented AAC failures: 1,024 samples late and 1,088 early.
    for lag in [1024, -1088, 1, -1] {
        let shifted = check(&reference, lag);
        assert_eq!(shifted.offset_status, OffsetStatus::Offset, "{lag}");
        assert_eq!(shifted.measured_offset_samples, Some(lag));
        assert!(shifted.flags.contains(&"audio_offset"));
        assert!(!shifted.passed);
    }
    // An early shift at the very start of the output remains observable: the
    // margin below zero reads as silence instead of being skipped.
    let opening = click(1500, 9600);
    let early = check(&opening, -1088);
    assert_eq!(
        early.measured_offset_samples,
        Some(-1088),
        "{:?}",
        early.segments
    );
    assert!(!early.passed);
}

#[test]
fn small_offsets_of_smooth_aperiodic_content_fail_instead_of_looking_ambiguous() {
    // A heavily lowpassed noise has a broad, flat correlation lobe.
    let smooth = noise(7, 24_000, 0.995, 8.0);
    assert!(check(&smooth, 0).passed, "{:?}", check(&smooth, 0).segments);
    for lag in [3, -5, 17] {
        let shifted = check(&smooth, lag);
        assert_eq!(
            shifted.offset_status,
            OffsetStatus::Offset,
            "{lag}: {:?}",
            shifted.segments
        );
        assert_eq!(shifted.measured_offset_samples, Some(lag));
        assert!(
            shifted
                .segments
                .iter()
                .all(|segment| segment.status == metrics::AlignmentStatus::Offset)
        );
    }
}

#[test]
fn periodic_content_is_unobservable_not_offset_zero() {
    // An endless tone: decoded sample t holds tone(t - lag) everywhere.
    let tone = |period: i64, at: i64| -> [f32; 2] {
        let value = (std::f32::consts::TAU * at.rem_euclid(period) as f32 / period as f32).sin();
        [value * 0.5, value * -0.4]
    };
    let run = |period: i64, lag: i64, mark: Option<i64>| {
        let sample = |at: i64| {
            // A 32-sample burst, a few percent of a segment's energy.
            if mark.is_some_and(|mark| (mark..mark + 32).contains(&at)) {
                [0.99, -0.99]
            } else {
                tone(period, at)
            }
        };
        let reference: Vec<[f32; 2]> = (0..24_000).map(sample).collect();
        let start = -MAX_ALIGNMENT_LAG;
        let decoded: Vec<[f32; 2]> = (start..24_000 + MAX_ALIGNMENT_LAG)
            .map(|at| sample(at - lag))
            .collect();
        let covered = vec![true; decoded.len()];
        compare_audio(
            0..24_000,
            AudioSample(0),
            &reference,
            start,
            &decoded,
            &covered,
            &Thresholds::default(),
        )
    };
    // A 1,024-sample shift of a period-64 tone is indistinguishable from none.
    for lag in [0, 1024] {
        let periodic = run(64, lag, None);
        assert_eq!(periodic.offset_status, OffsetStatus::Unobservable, "{lag}");
        assert_eq!(periodic.measured_offset_samples, None);
        assert!(periodic.flags.contains(&"audio_offset_unobservable"));
        assert!(!periodic.passed);
    }
    // A shift that is not a multiple of the period is still an offset.
    let shifted = run(100, 1088, None);
    assert_eq!(
        shifted.offset_status,
        OffsetStatus::Offset,
        "{:?}",
        shifted.segments
    );
    assert!(!shifted.passed);
    // One short transient makes the window observable again, in both directions.
    let marked = run(64, 0, Some(5000));
    assert_eq!(
        marked.offset_status,
        OffsetStatus::VerifiedZero,
        "{:?}",
        marked.segments
    );
    let marked = run(64, 1024, Some(5000));
    assert_eq!(
        marked.offset_status,
        OffsetStatus::Offset,
        "{:?}",
        marked.segments
    );
    assert_eq!(marked.measured_offset_samples, Some(1024));
}

#[test]
fn wrong_content_in_part_of_a_window_fails_per_block() {
    let program = noise(11, 48_000, 0.6, 0.3);
    assert!(check(&program, 0).passed);
    // Replace 100 ms in the middle with a different program at the same level.
    let mut wrong = program.clone();
    wrong[20_000..24_800].copy_from_slice(&noise(99, 4800, 0.6, 0.3));
    let (start, decoded, covered) = shifted(&wrong, 0);
    let result = compare_audio(
        0..48_000,
        AudioSample(0),
        &program,
        start,
        &decoded,
        &covered,
        &Thresholds::default(),
    );
    assert!(
        result.flags.contains(&"audio_block_snr"),
        "{:?}",
        result.blocks
    );
    // 4,800 samples straddle eleven 480-sample blocks.
    assert_eq!(result.blocks.low_snr_blocks, 11);
    // Whole-window SNR alone would have passed this.
    assert!(result.snr_db.unwrap() > 6.0);
    assert_eq!(result.offset_status, OffsetStatus::VerifiedZero);
}

#[test]
fn level_silence_and_gap_checks_fail_explicitly() {
    let thresholds = Thresholds::default();
    let reference = click(3000, 9600);
    let quiet: Vec<[f32; 2]> = reference
        .iter()
        .map(|[left, right]| [left * 0.25, right * 0.25])
        .collect();
    let (start, decoded, covered) = shifted(&quiet, 0);
    let result = compare_audio(
        0..9600,
        AudioSample(0),
        &reference,
        start,
        &decoded,
        &covered,
        &thresholds,
    );
    assert_eq!(result.measured_offset_samples, Some(0));
    // A 12 dB loss keeps 2.5 dB SNR; the level gate reports it.
    assert!(result.flags.contains(&"audio_level"));
    let silence = vec![[0.0; 2]; 4800];
    let (start, decoded, covered) = shifted(&silence, 0);
    let result = compare_audio(
        0..4800,
        AudioSample(0),
        &silence,
        start,
        &decoded,
        &covered,
        &thresholds,
    );
    assert!(result.passed);
    assert_eq!(result.offset_status, OffsetStatus::NotApplicable);
    let (start, decoded, covered) = shifted(&click(100, 4800), 0);
    let result = compare_audio(
        0..4800,
        AudioSample(0),
        &silence,
        start,
        &decoded,
        &covered,
        &thresholds,
    );
    assert!(result.flags.contains(&"unexpected_sound_in_silence"));
    // A silent-RMS window whose reference itself carries a -48 dBFS
    // pre-echo sample: the matching decoded sample is not new sound, but a
    // decoded peak 3 dB above the reference peak still is.
    let mut echo = vec![[0.0_f32; 2]; 48_000];
    echo[47_900] = [0.004, -0.004];
    let (start, decoded, covered) = shifted(&echo, 0);
    let result = compare_audio(
        0..48_000,
        AudioSample(0),
        &echo,
        start,
        &decoded,
        &covered,
        &thresholds,
    );
    assert_eq!(result.kind, "silent");
    assert!(result.passed, "{:?}", result.flags);
    let mut louder = echo.clone();
    louder[47_900] = [0.0057, -0.0057];
    let (start, decoded, covered) = shifted(&louder, 0);
    let result = compare_audio(
        0..48_000,
        AudioSample(0),
        &echo,
        start,
        &decoded,
        &covered,
        &thresholds,
    );
    assert!(result.flags.contains(&"unexpected_sound_in_silence"));
    let (start, decoded, mut covered) = shifted(&reference, 0);
    covered[4000..4100].fill(false);
    let result = compare_audio(
        0..9600,
        AudioSample(0),
        &reference,
        start,
        &decoded,
        &covered,
        &thresholds,
    );
    assert!(result.flags.contains(&"decoded_audio_gap"));
    assert_eq!(result.uncovered_samples, 100);
}

#[test]
fn selections_are_bounded_and_explicit() {
    assert_eq!(
        select_frames(&FrameSelection::Automatic, 3).unwrap(),
        [0, 1, 2]
    );
    assert_eq!(
        select_frames(&FrameSelection::Automatic, 1201)
            .unwrap()
            .len(),
        401
    );
    assert_eq!(
        select_frames(&FrameSelection::Every(4), 10).unwrap(),
        [0, 4, 8]
    );
    assert!(select_frames(&FrameSelection::Every(0), 10).is_err());
    assert_eq!(
        select_frames(&FrameSelection::List(vec![5, 1, 5]), 10).unwrap(),
        [1, 5]
    );
    assert!(select_frames(&FrameSelection::List(vec![10]), 10).is_err());
    assert_eq!(
        select_windows(&AudioSelection::Automatic, 100_000).unwrap(),
        [0..48_000, 48_000..100_000]
    );
    // Long outputs stride instead of refusing.
    let long = select_windows(&AudioSelection::Automatic, 48_000 * 1000).unwrap();
    assert_eq!(long.len(), MAX_AUDIO_WINDOWS);
    assert_eq!(long[0], 0..48_000);
    assert_eq!(long.last().unwrap().end, 48_000 * 1000);
    assert!(
        select_windows(
            &AudioSelection::Windows(std::iter::once(5..5).collect()),
            10
        )
        .is_err()
    );
    assert!(
        select_windows(
            &AudioSelection::Windows(std::iter::once(0..11).collect()),
            10
        )
        .is_err()
    );
    let (request, report) = cli::parse(&[
        "p.deadpan",
        "--movie",
        "m.mp4",
        "--every",
        "3",
        "--samples",
        "0:48000,96000:100000",
    ])
    .unwrap();
    assert_eq!(request.frames, FrameSelection::Every(3));
    assert_eq!(
        request.audio,
        AudioSelection::Windows(vec![0..48_000, 96_000..100_000])
    );
    assert!(report.is_none());
    assert!(cli::parse(&["p.deadpan"]).is_err());
    assert!(cli::parse(&["p.deadpan", "--movie", "m", "--frames", "1", "--every", "2"]).is_err());
}

#[test]
fn ten_bit_pictures_use_peak_1023_and_eight_bit_equivalent_structure_gates() {
    // An error of one ten-bit code everywhere: PSNR uses the 1023 peak.
    let reference: Vec<u16> = vec![500; 64];
    let actual: Vec<u16> = vec![501; 64];
    let metrics = metrics::plane_metrics(&reference, &actual, 1023);
    assert!((metrics.psnr_db - 20.0 * 1023_f64.log10()).abs() < 1e-9);
    assert_eq!(metrics.max_abs_error, 1);
    // The same eight-bit error is 20*log10(255).
    let eight = metrics::plane_metrics(&[100; 8], &[101; 8], 255);
    assert!((eight.psnr_db - 20.0 * 255_f64.log10()).abs() < 1e-9);

    // Ten-bit copies of the eight-bit fixtures keep thumbnail and mean-luma
    // values in eight-bit-equivalent codes, so the same gates apply.
    let base = square(20);
    let wide = ten_bit(&base);
    assert_eq!(wide.mean_luma(), base.mean_luma());
    assert_eq!(wide.thumbnail(), base.thumbnail());
    let thresholds = Thresholds::default();
    let mut stored = references(&[
        (4, ten_bit(&square(16))),
        (5, wide.clone()),
        (6, ten_bit(&square(24))),
    ]);
    let shifted = compare_picture(
        ProjectFrame(0),
        5,
        &ten_bit(&square(24)),
        &mut stored,
        &thresholds,
    );
    assert!(
        shifted.flags.contains(&"frame_index_mismatch"),
        "{:?}",
        shifted.flags
    );
    assert_eq!(
        shifted.next_index.unwrap().status,
        IndexObservability::Observable
    );
    assert!(shifted.flags.contains(&"luma_psnr"));
    let bright = ten_bit(&picture(64, 32, |_, _| 180));
    let black = ten_bit(&picture(64, 32, |_, _| 16));
    let mut stored = references(&[(0, bright)]);
    let check = compare_picture(ProjectFrame(0), 0, &black, &mut stored, &thresholds);
    for flag in [
        "unexpected_black_frame",
        "gross_structural_mismatch",
        "luma_psnr",
    ] {
        assert!(check.flags.contains(&flag), "{flag}: {:?}", check.flags);
    }
    // HDR thresholds gate ten-bit pictures independently of the SDR ones.
    let mut noisy = wide.clone();
    for (index, value) in noisy.y.iter_mut().enumerate() {
        if index % 3 == 0 {
            *value += 6;
        }
    }
    let strict = Thresholds {
        min_hdr_luma_psnr_db: 60.0,
        ..Thresholds::default()
    };
    let mut stored = references(&[(0, wide.clone())]);
    assert!(compare_picture(ProjectFrame(0), 0, &noisy, &mut stored, &thresholds).passed);
    let mut stored = references(&[(0, wide)]);
    let check = compare_picture(ProjectFrame(0), 0, &noisy, &mut stored, &strict);
    assert_eq!(check.flags, vec!["luma_psnr"]);
}

/// A small graphic missing from a grainy ten-bit picture: whole-picture
/// PSNR stays above the HDR gate, the 4x4-cell local gate fails it, and the
/// grain alone passes. Eight-bit pictures report the value ungated.
#[test]
fn small_missing_graphic_fails_the_local_gate_regardless_of_psnr() {
    let (width, height) = (320_u32, 180_u32);
    // Deterministic uniform grain of -22..=23 ten-bit codes (LCG).
    let mut state = 0x2545_f491_u32;
    let mut grain = || {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        i32::try_from(state >> 28).unwrap_or(0) * 3 - 22
    };
    let mut clean = Vec::new();
    let mut grainy = Vec::new();
    let mut captioned = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let base = 300 + i32::try_from(x).unwrap_or(0);
            clean.push(u16::try_from(base).unwrap_or(0));
            grainy.push(u16::try_from(base + grain()).unwrap_or(0));
            let graphic = (40..56).contains(&x) && (100..108).contains(&y);
            captioned.push(if graphic {
                800
            } else {
                u16::try_from(base).unwrap_or(0)
            });
        }
    }
    let chroma = (width * height / 4) as usize;
    let ten = |y: Vec<u16>| I420 {
        width,
        height,
        bits: 10,
        y,
        cb: vec![512; chroma],
        cr: vec![512; chroma],
    };
    let thresholds = Thresholds::default();
    let mut stored = references(&[(0, ten(captioned.clone()))]);
    let missing = compare_picture(
        ProjectFrame(0),
        0,
        &ten(clean.clone()),
        &mut stored,
        &thresholds,
    );
    assert!(
        missing.planes[0].psnr_db > thresholds.min_hdr_luma_psnr_db,
        "{missing:?}"
    );
    assert_eq!(missing.flags, vec!["local_structure_mismatch"]);
    assert!(
        missing.local_luma_error > 400.0,
        "{}",
        missing.local_luma_error
    );
    let mut stored = references(&[(0, ten(clean.clone()))]);
    let noisy = compare_picture(ProjectFrame(0), 0, &ten(grainy), &mut stored, &thresholds);
    assert!(noisy.passed, "{noisy:?}");
    assert!(noisy.local_luma_error < 20.0, "{}", noisy.local_luma_error);
    // Eight-bit: the same geometry at a quarter of the codes is reported only.
    let quarter = |plane: &[u16]| plane.iter().map(|value| value / 4).collect::<Vec<_>>();
    let eight = |y: Vec<u16>| I420 {
        bits: 8,
        cb: vec![128; chroma],
        cr: vec![128; chroma],
        ..ten(y)
    };
    let mut stored = references(&[(0, eight(quarter(&captioned)))]);
    let sdr = compare_picture(
        ProjectFrame(0),
        0,
        &eight(quarter(&clean)),
        &mut stored,
        &thresholds,
    );
    assert!(sdr.local_luma_error > 100.0);
    assert!(
        !sdr.flags.contains(&"local_structure_mismatch"),
        "{:?}",
        sdr.flags
    );
}
