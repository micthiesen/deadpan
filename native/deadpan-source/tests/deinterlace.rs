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
    assert_eq!(decoder.info().time_base_den, 120000);
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
    assert_eq!(decoder.next_metadata(control()).unwrap().unwrap().pts, 2400);
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
