//! Independent moving-field truth, including temporal context across GOP seeks.
use deadpan_source::{DecodeControl, DecodeLimits, DecodedRgbaFrame, SourceDecoder};
use std::{fs::File, path::PathBuf, sync::atomic::AtomicBool, time::Duration};

static CANCELLED: AtomicBool = AtomicBool::new(false);
fn control() -> DecodeControl<'static> {
    DecodeControl {
        timeout: Duration::from_secs(10),
        cancelled: &CANCELLED,
    }
}
fn open(name: &str, threads: u32) -> SourceDecoder {
    SourceDecoder::open(
        File::open(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(name),
        )
        .unwrap(),
        DecodeLimits {
            threads,
            ..DecodeLimits::default()
        },
        control(),
    )
    .unwrap()
}
fn decode(name: &str, threads: u32) -> Vec<DecodedRgbaFrame> {
    let mut decoder = open(name, threads);
    assert!(decoder.info().bwdif_fields, "{name}");
    assert_eq!(decoder.info().time_base_den, 360000);
    let mut frames = Vec::new();
    while let Some(frame) = decoder.next_rgba(control()).unwrap() {
        frames.push(frame);
    }
    frames
}

#[test]
fn both_field_orders_retain_motion_at_field_cadence_including_single_picture_eof() {
    for (name, count, ticks) in [
        ("fields-tff.mp4", 24, 2400),
        ("fields-bff.mp4", 24, 2002),
        ("fields-single.mp4", 2, 2400),
        ("fields-single-bff.mp4", 2, 2400),
        ("fields-tff-bframes.mp4", 24, 2400),
        ("fields-100.mp4", 24, 1200),
    ] {
        let ticks = ticks * 3;
        let frames = decode(name, 1);
        assert_eq!(frames.len(), count, "{name}");
        for (ordinal, frame) in frames.iter().enumerate() {
            assert_eq!(
                frame.metadata.pts,
                i64::try_from(ordinal).unwrap() * ticks,
                "{name} {ordinal}"
            );
            assert_eq!(frame.metadata.reported_duration, Some(ticks));
            // Every row within the rectangle, including rows synthesized from
            // the other field, must track this field's independently known time.
            let expected = 13.5 + ordinal as f64 * 2.0;
            for row in 16..48 {
                let lit: Vec<_> = (0..96)
                    .filter(|x| frame.rgba[(row * 96 + x) * 4] > 128)
                    .collect();
                assert!(!lit.is_empty(), "{name} {ordinal} row {row}");
                let center = lit.iter().sum::<usize>() as f64 / lit.len() as f64;
                assert!(
                    (center - expected).abs() <= 1.0,
                    "{name} field {ordinal} row {row}: center {center}, expected {expected}"
                );
            }
        }
    }
}

#[test]
fn threaded_fields_and_backward_seeks_equal_sequential_pixels() {
    for name in [
        "fields-tff.mp4",
        "fields-bff.mp4",
        "fields-single.mp4",
        "fields-variable.mp4",
        "fields-single-bff.mp4",
        "fields-tff-bframes.mp4",
        "fields-100.mp4",
        "telecine-tff.mp4",
        "telecine-bff.mp4",
        "telecine-progressive.mp4",
        "telecine-variable.mp4",
        "telecine-single.mp4",
        "telecine-bframes.mp4",
        "progressive-timing-hrd.mp4",
        "progressive-repeats.mp4",
    ] {
        let expected = decode(name, 1);
        for threads in [1, 8, 16] {
            let linear = decode(name, threads);
            for (actual, reference) in linear.iter().zip(&expected) {
                assert_eq!(actual.metadata, reference.metadata);
                assert_eq!(actual.rgba, reference.rgba, "{name} threads {threads}");
            }
            let mut decoder = open(name, threads);
            for reference in expected.iter().rev() {
                decoder.seek(reference.metadata.pts, control()).unwrap();
                let actual = loop {
                    let next = decoder.next_rgba(control()).unwrap().unwrap();
                    if next.metadata.pts >= reference.metadata.pts {
                        break next;
                    }
                };
                assert_eq!(
                    actual.metadata, reference.metadata,
                    "{name} threads {threads}"
                );
                assert_eq!(
                    actual.rgba, reference.rgba,
                    "{name} at {} threads {threads}",
                    reference.metadata.pts
                );
            }
        }
    }
}

#[test]
fn variable_intervals_keep_exact_half_ticks_and_the_measured_final_duration() {
    let frames = decode("fields-variable.mp4", 1);
    let intervals = [
        2401, 4801, 2401, 2401, 7201, 2401, 2401, 4801, 2401, 2401, 2401, 6001,
    ];
    assert_eq!(frames.len(), intervals.len() * 2);
    let mut pts = 0;
    for (pair, interval) in frames.chunks_exact(2).zip(intervals) {
        let interval = interval * 3;
        assert_eq!(pair[0].metadata.pts, pts);
        assert_eq!(pair[1].metadata.pts, pts + interval);
        for field in pair {
            assert_eq!(field.metadata.reported_duration, Some(interval));
        }
        pts += 2 * interval;
    }
}

#[test]
fn fields_obey_output_count_bounds_and_preflight_cancellation_preserves_pending_field() {
    let mut decoder = open("fields-tff.mp4", 1);
    assert_eq!(decoder.next_metadata(control()).unwrap().unwrap().pts, 0);
    let cancelled = AtomicBool::new(true);
    assert!(
        decoder
            .next_metadata(DecodeControl {
                cancelled: &cancelled,
                ..control()
            })
            .is_err()
    );
    assert_eq!(decoder.next_metadata(control()).unwrap().unwrap().pts, 7200);
    assert!(
        !decoder
            .copy_current_i420(control())
            .unwrap()
            .metadata
            .interlaced
    );
    assert!(matches!(decoder.restart_at_keyframe(0, control()),
        Err(deadpan_source::SourceDecodeError::Native {code, ..}) if code == "unsupported_interlace"));
    let file =
        File::open(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fields-tff.mp4"))
            .unwrap();
    let mut bounded = SourceDecoder::open(
        file,
        DecodeLimits {
            max_frames: 16,
            ..DecodeLimits::default()
        },
        control(),
    )
    .unwrap();
    for _ in 0..16 {
        assert!(bounded.next_metadata(control()).unwrap().is_some());
    }
    assert!(matches!(bounded.next_metadata(control()),
        Err(deadpan_source::SourceDecodeError::Native {code, ..}) if code == "resource_limit"));
}

#[test]
fn telecine_preserves_two_three_fields_and_repeats_exactly_the_first_field() {
    for name in [
        "telecine-tff.mp4",
        "telecine-bff.mp4",
        "telecine-variable.mp4",
        "telecine-single.mp4",
        "telecine-bframes.mp4",
    ] {
        let frames = decode(name, 1);
        let single = name == "telecine-single.mp4";
        let reordered = name == "telecine-bframes.mp4";
        assert_eq!(
            frames.len(),
            if single {
                3
            } else if reordered {
                36
            } else {
                30
            },
            "{name}"
        );
        let mut offset = 0;
        let mut pts = 0;
        for coded in 0..if single { 1 } else { 12 } {
            let count = if single || reordered || coded % 2 == 1 {
                3
            } else {
                2
            };
            let interval = if reordered {
                2400
            } else if name == "telecine-variable.mp4" {
                if count == 2 { 2003 } else { 3001 }
            } else {
                count * 1001
            };
            let duration = interval * 6 / count;
            for phase in 0..count {
                let frame = &frames[offset + phase as usize];
                assert_eq!(frame.metadata.pts, pts + phase * duration);
                assert_eq!(frame.metadata.reported_duration, Some(duration));
                let expected = 13.5 + (coded * 2 + phase % 2) as f64 * 2.0;
                for row in 16..48 {
                    let lit: Vec<_> = (0..96)
                        .filter(|x| frame.rgba[(row * 96 + x) * 4] > 128)
                        .collect();
                    assert!(!lit.is_empty());
                    let center = lit.iter().sum::<usize>() as f64 / lit.len() as f64;
                    assert!(
                        (center - expected).abs() <= 1.0,
                        "{name} {coded}/{phase} row {row}: {center} vs {expected}"
                    );
                }
            }
            if count == 3 {
                assert_eq!(frames[offset].rgba, frames[offset + 2].rgba);
            }
            offset += count as usize;
            pts += interval * 6;
        }
    }
}

#[test]
fn progressive_hrd_picture_timing_is_valid_output_and_retains_identical_source_pixels() {
    let reference = decode("progressive-timing-hrd.mp4", 1);
    assert_eq!(reference.len(), 12);
    let file = File::open(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/progressive-timing-hrd.mp4"),
    )
    .unwrap();
    let mut decoder = SourceDecoder::open(
        file,
        DecodeLimits {
            progressive_only: true,
            ..DecodeLimits::default()
        },
        control(),
    )
    .unwrap();
    assert!(!decoder.info().bwdif_fields);
    assert_eq!(decoder.info().time_base_den, 60000);
    for expected in &reference {
        let actual = decoder.next_rgba(control()).unwrap().unwrap();
        assert_eq!(actual.metadata.pts * 6, expected.metadata.pts);
        assert_eq!(actual.rgba, expected.rgba);
    }
    assert!(decoder.next_metadata(control()).unwrap().is_none());
    let file = File::open(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/progressive-timing-hrd.mp4"),
    )
    .unwrap();
    let mut fresh =
        SourceDecoder::open_at_keyframe(file, DecodeLimits::default(), control(), 0).unwrap();
    assert!(!fresh.info().bwdif_fields);
    assert_eq!(
        fresh.next_rgba(control()).unwrap().unwrap().rgba,
        reference[0].rgba
    );
}

#[test]
fn progressive_telecine_declares_field_timing_before_its_first_repeated_picture() {
    let frames = decode("telecine-progressive.mp4", 1);
    assert_eq!(frames.len(), 30);
    let mut offset = 0;
    let mut pts = 0;
    for coded in 0..12 {
        let count = if coded % 2 == 0 { 2 } else { 3 };
        let duration = 6006;
        for phase in 0..count {
            let frame = &frames[offset + phase];
            assert_eq!(frame.metadata.pts, pts + phase as i64 * duration);
            assert_eq!(frame.metadata.reported_duration, Some(duration));
            let expected = 13.5 + coded as f64 * 2.0;
            for row in 16..48 {
                let lit: Vec<_> = (0..96)
                    .filter(|x| frame.rgba[(row * 96 + x) * 4] > 128)
                    .collect();
                let center = lit.iter().sum::<usize>() as f64 / lit.len() as f64;
                assert!((center - expected).abs() <= 1.0);
            }
        }
        offset += count;
        pts += duration * count as i64;
    }
    // Output verification cannot silently apply this source interpretation.
    let file = File::open(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/telecine-progressive.mp4"),
    )
    .unwrap();
    assert!(
        matches!(SourceDecoder::open(file, DecodeLimits { progressive_only: true, ..DecodeLimits::default() }, control()), Err(deadpan_source::SourceDecodeError::Native {code, ..}) if code == "unsupported_interlace")
    );
}

#[test]
fn progressive_repeat_hints_do_not_add_time_beyond_container_intervals() {
    let frames = decode("progressive-repeats.mp4", 1);
    assert_eq!(frames.len(), 12);
    let mut pts = 0;
    for (ordinal, frame) in frames.iter().enumerate() {
        let duration = [2400, 4800, 7200][ordinal % 3] * 6;
        assert_eq!(frame.metadata.pts, pts);
        assert_eq!(frame.metadata.reported_duration, Some(duration));
        pts += duration;
    }
    let file = File::open(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/progressive-repeats.mp4"),
    )
    .unwrap();
    let mut strict = SourceDecoder::open(
        file,
        DecodeLimits {
            progressive_only: true,
            ..DecodeLimits::default()
        },
        control(),
    )
    .unwrap();
    assert_eq!(strict.next_metadata(control()).unwrap().unwrap().pts, 0);
    assert!(
        matches!(strict.next_metadata(control()), Err(deadpan_source::SourceDecodeError::Native {code, ..}) if code == "unsupported_interlace")
    );
}
