use super::*;

const WIDTH: u32 = 64;
const HEIGHT: u32 = 36;

/// A picture filled by `color(x, y)`.
fn picture(color: impl Fn(u32, u32) -> [u8; 3]) -> PictureSignature {
    let rgba: Vec<u8> = (0..HEIGHT)
        .flat_map(|y| (0..WIDTH).map(move |x| (x, y)))
        .flat_map(|(x, y)| {
            let [r, g, b] = color(x, y);
            [r, g, b, 255]
        })
        .collect();
    PictureSignature::from_rgba(&rgba, WIDTH, HEIGHT, WIDTH as usize * 4).unwrap()
}

/// A warm scene with a soft horizontal gradient, shifted by `offset` pixels.
fn scene_a(offset: u32) -> PictureSignature {
    picture(|x, _| {
        let shade = ((x + offset) * 3 % 192) as u8;
        [200, 120 + shade / 4, 40]
    })
}

/// A dark blue scene with a vertical gradient.
fn scene_b() -> PictureSignature {
    picture(|_, y| [10, 20, 90 + (y * 4) as u8])
}

#[test]
fn a_cut_between_scenes_is_one_boundary() {
    let signatures: Vec<_> = (0..20)
        .map(|index| {
            if index < 12 {
                scene_a(index)
            } else {
                scene_b()
            }
        })
        .collect();
    let analysis = ShotAnalysis::from_signatures(&signatures).unwrap();
    assert_eq!(analysis.boundaries(), [12]);
    let [cell, histogram, skip] = analysis.change(12).unwrap();
    assert!(analysis.transitions().is_empty());
    assert!(
        cell >= 24 && histogram >= 48 && skip >= 24,
        "{cell} {histogram} {skip}"
    );
}

#[test]
fn a_slow_pan_within_one_scene_is_not_a_cut() {
    let signatures: Vec<_> = (0..30).map(|index| scene_a(index * 2)).collect();
    let analysis = ShotAnalysis::from_signatures(&signatures).unwrap();
    assert!(
        analysis.boundaries().is_empty(),
        "{:?}",
        analysis.measures()
    );
}

#[test]
fn a_flash_frame_returning_to_its_scene_is_not_a_cut() {
    let mut signatures: Vec<_> = (0..20).map(scene_a).collect();
    signatures[10] = picture(|_, _| [255, 255, 255]);
    let analysis = ShotAnalysis::from_signatures(&signatures).unwrap();
    assert!(
        analysis.boundaries().is_empty(),
        "{:?}",
        analysis.measures()
    );
}

#[test]
fn cuts_closer_than_six_pictures_keep_the_first() {
    let signatures: Vec<_> = (0..24)
        .map(|index| match index {
            0..10 => scene_a(index),
            10..13 => scene_b(),
            _ => picture(|_, _| [255, 255, 255]),
        })
        .collect();
    let analysis = ShotAnalysis::from_signatures(&signatures).unwrap();
    assert_eq!(analysis.boundaries(), [10]);
}

#[test]
fn identical_pictures_do_not_change() {
    assert_eq!(scene_b().change(&scene_b(), Some(&scene_b())), [0, 0, 0]);
    let black = picture(|_, _| [0, 0, 0]);
    let white = picture(|_, _| [255, 255, 255]);
    assert_eq!(black.change(&white, None)[1], 255, "disjoint histograms");
    assert_eq!(
        black.change(&white, Some(&white))[2],
        0,
        "skip compares with the picture two before"
    );
}

#[test]
fn stored_changes_and_buffers_are_validated() {
    assert!(ShotAnalysis::from_changes(vec![[1, 0, 0]]).is_err());
    assert!(ShotAnalysis::new(vec![]).unwrap().boundaries().is_empty());
    assert!(PictureSignature::from_rgba(&[0; 16], 2, 2, 8).is_err());
    assert!(PictureSignature::from_rgba(&[0; 64 * 36 * 4 - 1], 64, 36, 256).is_err());
}

#[test]
fn padded_rows_are_read_by_stride() {
    let tight = scene_b();
    let stride = WIDTH as usize * 4 + 12;
    let mut padded = vec![7_u8; stride * HEIGHT as usize];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let offset = y as usize * stride + x as usize * 4;
            padded[offset..offset + 4].copy_from_slice(&[10, 20, 90 + (y * 4) as u8, 255]);
        }
    }
    let signature = PictureSignature::from_rgba(&padded, WIDTH, HEIGHT, stride).unwrap();
    assert_eq!(signature, tight);
}

/// Scene colours as functions, so pictures can be blended per pixel.
fn warm(x: u32, _: u32) -> [u8; 3] {
    let shade = (x * 3 % 192) as u8;
    [200, 120 + shade / 4, 40]
}

fn checks(x: u32, y: u32) -> [u8; 3] {
    if (x / 8 + y / 6).is_multiple_of(2) {
        [20, 60, 160]
    } else {
        [230, 230, 210]
    }
}

fn blue(_: u32, y: u32) -> [u8; 3] {
    [10, 20, 90 + (y * 4) as u8]
}

fn black(_: u32, _: u32) -> [u8; 3] {
    [0, 0, 0]
}

/// `from` blended `amount / steps` of the way to `to`, per pixel.
fn blend(
    from: fn(u32, u32) -> [u8; 3],
    to: fn(u32, u32) -> [u8; 3],
    amount: u32,
    steps: u32,
) -> PictureSignature {
    picture(|x, y| {
        let (a, b) = (from(x, y), to(x, y));
        std::array::from_fn(|channel| {
            ((u32::from(a[channel]) * (steps - amount)
                + u32::from(b[channel]) * amount
                + steps / 2)
                / steps) as u8
        })
    })
}

/// `before` pictures of `from`, `blended` pictures blending into `to`, then
/// `after` pictures of `to`.
fn transition(
    from: fn(u32, u32) -> [u8; 3],
    to: fn(u32, u32) -> [u8; 3],
    before: usize,
    blended: usize,
    after: usize,
) -> Vec<PictureSignature> {
    let steps = blended as u32 + 1;
    (0..before)
        .map(|_| picture(from))
        .chain((1..=blended as u32).map(|amount| blend(from, to, amount, steps)))
        .chain((0..after).map(|_| picture(to)))
        .collect()
}

#[test]
fn a_dissolve_is_one_transition_with_one_middle_boundary() {
    for blended in [3, 12, 24, 40, 48] {
        let signatures = transition(warm, checks, 60, blended, 60);
        let analysis = ShotAnalysis::from_signatures(&signatures).unwrap();
        assert!(
            analysis.cuts().is_empty(),
            "{blended}: {:?}",
            analysis.cuts()
        );
        let transitions = analysis.transitions();
        assert_eq!(transitions.len(), 1, "{blended}: {transitions:?}");
        let found = &transitions[0];
        assert_eq!(found.kind, TransitionKind::Dissolve);
        // The estimated extent is within a few pictures of the truth.
        let tolerance = 1 + blended / 8;
        assert!(
            found.range.start.abs_diff(60) <= tolerance
                && found.range.end.abs_diff(60 + blended) <= tolerance,
            "{blended}: {found:?}"
        );
        assert!(found.boundary.abs_diff(60 + blended / 2) <= 1, "{found:?}");
        assert_eq!(analysis.boundaries(), [found.boundary]);
    }
}

#[test]
fn a_fade_to_black_ends_at_the_first_black_picture() {
    let signatures = transition(checks, black, 40, 16, 40);
    let analysis = ShotAnalysis::from_signatures(&signatures).unwrap();
    let transitions = analysis.transitions();
    assert_eq!(transitions.len(), 1, "{transitions:?}");
    assert_eq!(transitions[0].kind, TransitionKind::FadeOut);
    // The last blend, 1/17 of the picture, may already read as black.
    let end = transitions[0].range.end;
    assert!(end.abs_diff(56) <= 1, "{transitions:?}");
    assert!(
        transitions[0].range.start.abs_diff(40) <= 2,
        "{transitions:?}"
    );
    assert_eq!(analysis.boundaries(), [end]);
}

#[test]
fn a_fade_from_black_begins_at_the_first_lit_picture() {
    let signatures = transition(black, warm, 40, 10, 40);
    let analysis = ShotAnalysis::from_signatures(&signatures).unwrap();
    let transitions = analysis.transitions();
    assert_eq!(transitions.len(), 1, "{transitions:?}");
    assert_eq!(transitions[0].kind, TransitionKind::FadeIn);
    assert_eq!(transitions[0].range.start, 40);
    assert!(
        transitions[0].range.end.abs_diff(50) <= 2,
        "{transitions:?}"
    );
    assert_eq!(analysis.boundaries(), [40]);
}

#[test]
fn slow_pans_and_flashes_are_not_transitions() {
    let pan: Vec<_> = (0..120).map(|index| scene_a(index * 2)).collect();
    let analysis = ShotAnalysis::from_signatures(&pan).unwrap();
    assert!(analysis.transitions().is_empty());
    assert!(analysis.boundaries().is_empty());

    let mut flashes: Vec<_> = (0..120).map(scene_a).collect();
    flashes[30] = picture(|_, _| [255, 255, 255]);
    for flash in &mut flashes[70..73] {
        *flash = picture(|_, _| [255, 255, 255]);
    }
    let analysis = ShotAnalysis::from_signatures(&flashes).unwrap();
    assert!(
        analysis.transitions().is_empty(),
        "{:?}",
        analysis.transitions()
    );
}

#[test]
fn a_cut_is_not_also_a_transition_and_keeps_its_picture() {
    let signatures = transition(warm, checks, 60, 0, 60);
    let analysis = ShotAnalysis::from_signatures(&signatures).unwrap();
    assert_eq!(analysis.cuts(), [60]);
    assert!(analysis.transitions().is_empty());
    assert_eq!(analysis.boundaries(), [60]);
}

#[test]
fn transitions_and_cuts_merge_in_order() {
    let mut signatures = transition(warm, checks, 40, 12, 40);
    signatures.extend(transition(blue, blue, 40, 0, 0));
    signatures.extend(transition(blue, black, 0, 8, 30));
    let analysis = ShotAnalysis::from_signatures(&signatures).unwrap();
    assert_eq!(analysis.cuts(), [92]);
    let boundaries = analysis.boundaries();
    assert_eq!(
        boundaries.len(),
        3,
        "{boundaries:?} {:?}",
        analysis.transitions()
    );
    assert_eq!(boundaries[1], 92);
    assert!(boundaries[2].abs_diff(140) <= 1, "{boundaries:?}");
    assert!(boundaries[0].abs_diff(46) <= 1, "{boundaries:?}");
}

#[test]
fn a_resumed_measurement_equals_an_uninterrupted_one() {
    let mut signatures = transition(warm, checks, 30, 20, 30);
    signatures.extend((0..40).map(|index| scene_a(index * 3)));
    signatures.extend(transition(blue, black, 10, 30, 10));
    let whole = ShotAnalysis::from_signatures(&signatures).unwrap();
    assert!(!whole.transitions().is_empty());
    for stop in [
        0,
        1,
        2,
        7,
        49,
        50,
        51,
        77,
        100,
        signatures.len() - 1,
        signatures.len(),
    ] {
        let mut first = ShotMeasurer::new(signatures.len()).unwrap();
        for (ordinal, signature) in signatures.iter().enumerate().take(stop) {
            first.push(ordinal, signature.clone()).unwrap();
        }
        let saved = first.progress();
        assert_eq!(saved.next(), stop);
        // Through stored bytes, as the store keeps them.
        let restored = ShotProgress::new(
            saved.pictures(),
            decode_measures(&encode_measures(saved.measures())).unwrap(),
        )
        .unwrap();
        let mut resumed = ShotMeasurer::resume(restored).unwrap();
        assert_eq!(resumed.next_ordinal(), stop.saturating_sub(REPLAY_PICTURES));
        assert!(resumed.push(stop + 1, signatures[0].clone()).is_err() || stop == 0);
        let from = resumed.next_ordinal();
        for (ordinal, signature) in signatures.iter().enumerate().skip(from) {
            resumed.push(ordinal, signature.clone()).unwrap();
        }
        assert_eq!(resumed.finish().unwrap(), whole, "stopped at {stop}");
    }
}

#[test]
fn measures_round_trip_and_progress_is_validated() {
    let signatures = transition(warm, checks, 30, 12, 30);
    let analysis = ShotAnalysis::from_signatures(&signatures).unwrap();
    let bytes = encode_measures(analysis.measures());
    assert_eq!(bytes.len(), analysis.pictures() * MEASURE_BYTES);
    assert_eq!(
        ShotAnalysis::new(decode_measures(&bytes).unwrap()).unwrap(),
        analysis
    );
    assert!(decode_measures(&bytes[1..]).is_err());
    // A span that reaches outside the pictures is refused.
    let mut measures = analysis.measures().to_vec();
    measures[1].spans[0] = [1, 0];
    assert!(ShotAnalysis::new(measures.clone()).is_err());
    // Progress may not claim spans beyond its measured pictures.
    let mut stopped = ShotMeasurer::new(72).unwrap();
    for (ordinal, signature) in signatures.iter().enumerate().take(40) {
        stopped.push(ordinal, signature.clone()).unwrap();
    }
    let mut partial = stopped.progress().measures().to_vec();
    assert!(ShotProgress::new(72, partial.clone()).is_ok());
    assert!(ShotProgress::new(39, partial.clone()).is_err());
    partial[38].spans[0] = [3, 0];
    assert!(ShotProgress::new(72, partial).is_err());
    let mut measurer = ShotMeasurer::new(3).unwrap();
    assert!(measurer.push(1, scene_b()).is_err());
    measurer.push(0, scene_b()).unwrap();
    assert!(measurer.clone().finish().is_err());
}

/// The direct per-pixel reduction the optimized loop must equal.
fn reference_signature(
    rgba: &[u8],
    width: usize,
    height: usize,
    stride: usize,
) -> PictureSignature {
    let mut sums = vec![[0_u64; 3]; GRID_COLUMNS * GRID_ROWS];
    let mut counts = vec![0_u64; GRID_COLUMNS * GRID_ROWS];
    let mut histogram = [0_u64; HISTOGRAM_BINS];
    for y in 0..height {
        for x in 0..width {
            let pixel = &rgba[y * stride + x * 4..y * stride + x * 4 + 4];
            let (r, g, b) = (
                i32::from(pixel[0]),
                i32::from(pixel[1]),
                i32::from(pixel[2]),
            );
            let luma = (77 * r + 150 * g + 29 * b + 128) >> 8;
            let blue = ((-43 * r - 85 * g + 128 * b + 128) >> 8) + 128;
            let red = ((128 * r - 107 * g - 21 * b + 128) >> 8) + 128;
            let cell = y * GRID_ROWS / height * GRID_COLUMNS + x * GRID_COLUMNS / width;
            sums[cell][0] += luma as u64;
            sums[cell][1] += blue.clamp(0, 255) as u64;
            sums[cell][2] += red.clamp(0, 255) as u64;
            counts[cell] += 1;
            histogram[luma.clamp(0, 255) as usize * HISTOGRAM_BINS / 256] += 1;
        }
    }
    let cells = sums
        .iter()
        .zip(&counts)
        .map(|(sum, count)| sum.map(|value| ((value + count / 2) / count) as u8))
        .collect();
    let pixels = (width * height) as u64;
    let histogram = histogram.map(|count| ((count * 65_536 + pixels / 2) / pixels) as u32);
    PictureSignature { cells, histogram }
}

#[test]
fn the_signature_equals_the_direct_per_pixel_reduction() {
    // Odd sizes put uneven pixel counts in cells; a simple generator covers
    // every channel value, including the extremes.
    let mut state = 0x2545_f491_u32;
    for (width, height) in [(32, 18), (33, 19), (101, 57), (640, 360), (1920, 1080)] {
        let stride = width * 4 + 8;
        let rgba: Vec<u8> = (0..stride * height)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                (state >> 24) as u8
            })
            .collect();
        assert_eq!(
            PictureSignature::from_rgba(&rgba, width as u32, height as u32, stride).unwrap(),
            reference_signature(&rgba, width, height, stride),
            "{width}x{height}"
        );
    }
    let extremes: Vec<u8> = [
        [0, 0, 255, 0],
        [255, 255, 0, 0],
        [255, 0, 0, 0],
        [0, 255, 255, 0],
    ]
    .into_iter()
    .cycle()
    .take(64 * 36)
    .flatten()
    .collect();
    assert_eq!(
        PictureSignature::from_rgba(&extremes, 64, 36, 256).unwrap(),
        reference_signature(&extremes, 64, 36, 256)
    );
}

#[test]
fn a_fade_whose_range_reaches_an_accepted_transition_is_not_also_accepted() {
    // Lit pictures with one black picture at 26. A fade out with half-span 3
    // in the middle of picture 20 has its window at 17..24 but its range
    // reaches the black picture, 19..26; a dissolve with half-span 2 in the
    // middle of picture 26 (taken first) has its window at 24..29 and range
    // 25..28. The windows are disjoint; the ranges are not.
    let mut measures = vec![
        PictureMeasure {
            luma: 100,
            spread: 20,
            ..PictureMeasure::default()
        };
        60
    ];
    measures[0].change = [0, 0, 0];
    measures[20].spans[1] = [60, 0];
    measures[26] = PictureMeasure::default();
    measures[26].spans[0] = [60, 0];
    let transitions = gradual::transitions(&measures, &[]);
    assert_eq!(transitions.len(), 1, "{transitions:?}");
    assert_eq!(transitions[0].kind, TransitionKind::Dissolve);
    assert_eq!(transitions[0].range, 25..28);
    // Without the dissolve the fade out alone is accepted with that range.
    measures[26].spans[0] = [0, 0];
    let transitions = gradual::transitions(&measures, &[]);
    assert_eq!(transitions.len(), 1, "{transitions:?}");
    assert_eq!(transitions[0].kind, TransitionKind::FadeOut);
    assert_eq!(transitions[0].range, 19..26);
}

#[test]
fn appended_progress_tails_equal_the_whole_progress() {
    let mut signatures = transition(warm, checks, 30, 20, 30);
    signatures.extend(transition(blue, black, 10, 30, 10));
    let mut measurer = ShotMeasurer::new(signatures.len()).unwrap();
    let mut saved: Vec<PictureMeasure> = Vec::new();
    let apply = |saved: &mut Vec<PictureMeasure>, tail: &ShotProgressTail| {
        assert!(tail.start() <= saved.len());
        // A tail starts at most the widest half-span before the saved end.
        assert!(
            saved.len() - tail.start() <= 25,
            "{} {}",
            tail.start(),
            saved.len()
        );
        saved.truncate(tail.start());
        saved.extend_from_slice(tail.measures());
    };
    let mut pending: Option<ShotProgressTail> = None;
    for (ordinal, signature) in signatures.iter().enumerate() {
        measurer.push(ordinal, signature.clone()).unwrap();
        if ordinal % 7 == 3 {
            let tail = measurer.take_tail();
            ShotProgressTail::new(tail.pictures(), tail.start(), tail.measures().to_vec()).unwrap();
            apply(&mut saved, &tail);
            assert_eq!(saved, measurer.progress().measures());
        } else if ordinal % 7 == 5 {
            // A tail that waits merges with the next one.
            let tail = measurer.take_tail();
            pending = Some(match pending.take() {
                Some(older) => older.merge(tail).unwrap(),
                None => tail,
            });
        } else if ordinal % 7 == 6
            && let Some(tail) = pending.take()
        {
            let newer = measurer.take_tail();
            let merged = tail.merge(newer).unwrap();
            apply(&mut saved, &merged);
            assert_eq!(saved, measurer.progress().measures());
        }
    }
    // A tail that was not saved is offered again.
    let tail = measurer.take_tail();
    measurer.unsave_from(tail.start());
    let again = measurer.take_tail();
    assert_eq!(again, tail);
    apply(&mut saved, &again);
    assert_eq!(saved, measurer.finish().unwrap().measures());

    // A resumed measurer's first tail joins its saved progress.
    let mut first = ShotMeasurer::new(signatures.len()).unwrap();
    for (ordinal, signature) in signatures.iter().enumerate().take(77) {
        first.push(ordinal, signature.clone()).unwrap();
    }
    let mut saved = first.progress().measures().to_vec();
    let mut resumed = ShotMeasurer::resume(first.progress()).unwrap();
    assert_eq!(resumed.take_tail().measures(), []);
    for (ordinal, signature) in signatures.iter().enumerate().skip(resumed.next_ordinal()) {
        resumed.push(ordinal, signature.clone()).unwrap();
    }
    apply(&mut saved, &resumed.take_tail());
    assert_eq!(saved, resumed.finish().unwrap().measures());

    // Tails that leave a gap do not merge, and tails are validated.
    let gap = ShotProgressTail::new(120, 10, Vec::new()).unwrap();
    assert!(
        ShotProgressTail::new(120, 0, Vec::new())
            .unwrap()
            .merge(gap)
            .is_err()
    );
    assert!(ShotProgressTail::new(120, 119, vec![PictureMeasure::default(); 2]).is_err());
    let mut spanned = PictureMeasure::default();
    spanned.spans[0] = [1, 0];
    assert!(ShotProgressTail::new(120, 10, vec![spanned]).is_err());
}
