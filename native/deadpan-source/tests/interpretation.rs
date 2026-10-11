use deadpan_source::{
    ChromaLocation, ColorRange, DecodeControl, DecodeLimits, DecodedRgbaFrame, SourceDecodeError,
    SourceDecoder, VideoInterpretation,
};
use std::{fs::File, path::PathBuf, sync::atomic::AtomicBool, time::Duration};

static CANCELLED: AtomicBool = AtomicBool::new(false);

#[test]
fn missing_yuv_range_is_recorded_correctable_and_never_export_evidence() {
    let mut missing = open("missing-range-yuv.mkv", VideoInterpretation::SOURCE, 1);
    let mut declared = open("limited709.mkv", VideoInterpretation::SOURCE, 1);
    assert!(missing.info().range_assumed);
    assert!(!declared.info().range_assumed);
    assert_eq!(missing.info().color, declared.info().color);
    assert_eq!(missing.info().color.range, ColorRange::Limited);
    assert!(matches!(
        missing.next_i420(control()),
        Err(SourceDecodeError::InvalidConfiguration(_))
    ));
    let expected = declared.next_rgba(control()).unwrap().unwrap();
    let frame = missing.next_rgba(control()).unwrap().unwrap();
    assert_eq!(frame, expected);
    let choice = VideoInterpretation {
        range: Some(ColorRange::Full),
        ..VideoInterpretation::SOURCE
    };
    let mut corrected = open("missing-range-yuv.mkv", choice, 1);
    assert!(corrected.info().range_assumed);
    let corrected = corrected.next_rgba(control()).unwrap().unwrap();
    let full = open("full709.mkv", VideoInterpretation::SOURCE, 1)
        .next_rgba(control())
        .unwrap()
        .unwrap();
    assert_eq!(corrected, full);
    assert_eq!(corrected.metadata, expected.metadata);
    assert_ne!(corrected.rgba, expected.rgba);
}

#[test]
fn receipt_choice_validation_agrees_with_native_conversion_admission() {
    let rgb = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../deadpan-media-worker/tests/fixtures/rgb25_24.mp4");
    for path in [
        fixture("hevc-sdr-8-limited.mp4"),
        fixture("av1-sdr-10-full.mp4"),
        fixture("hevc-pq.mp4"),
        fixture("prores-hq.mov"),
        fixture("limited709.mkv"),
        rgb,
    ] {
        let source = SourceDecoder::open(
            File::open(&path).unwrap(),
            DecodeLimits::default(),
            control(),
        )
        .unwrap();
        for range in [None, Some(ColorRange::Full), Some(ColorRange::Limited)] {
            for chroma_location in [
                None,
                Some(ChromaLocation::Left),
                Some(ChromaLocation::Center),
                Some(ChromaLocation::TopLeft),
                Some(ChromaLocation::Top),
                Some(ChromaLocation::BottomLeft),
                Some(ChromaLocation::Bottom),
            ] {
                let choice = VideoInterpretation {
                    range,
                    chroma_location,
                };
                let expected = choice.validate_for(source.info()).is_ok();
                let actual = SourceDecoder::open_interpreted(
                    File::open(&path).unwrap(),
                    DecodeLimits::default(),
                    control(),
                    choice,
                );
                assert_eq!(actual.is_ok(), expected, "{}: {choice:?}", path.display());
            }
        }
    }
}

fn control() -> DecodeControl<'static> {
    DecodeControl {
        timeout: Duration::from_secs(10),
        cancelled: &CANCELLED,
    }
}
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}
fn open(name: &str, choice: VideoInterpretation, threads: u32) -> SourceDecoder {
    SourceDecoder::open_interpreted(
        File::open(fixture(name)).unwrap(),
        DecodeLimits {
            threads,
            ..DecodeLimits::default()
        },
        control(),
        choice,
    )
    .unwrap()
}
fn pixel(frame: &DecodedRgbaFrame, x: usize, y: usize) -> [u16; 4] {
    let at = y * frame.row_stride_bytes + x * 8;
    std::array::from_fn(|c| {
        u16::from_le_bytes([frame.rgba[at + c * 2], frame.rgba[at + c * 2 + 1]])
    })
}
fn rgb(y: f64, cb: f64, cr: f64) -> [f64; 3] {
    let r = y + 1.5748 * cr;
    let b = y + 1.8556 * cb;
    let g = (y - 0.2126 * r - 0.0722 * b) / 0.7152;
    [r, g, b].map(|v| v.clamp(0.0, 1.0))
}

#[test]
fn explicit_range_changes_pixels_without_relabelling_metadata_or_clocks() {
    for bits in [8, 10] {
        for source_full in [false, true] {
            let name = format!(
                "hevc-sdr-{bits}-{}.mp4",
                if source_full { "full" } else { "limited" }
            );
            let choice = VideoInterpretation {
                range: Some(if source_full {
                    ColorRange::Limited
                } else {
                    ColorRange::Full
                }),
                ..VideoInterpretation::SOURCE
            };
            let mut ordinary = open(&name, VideoInterpretation::SOURCE, 1);
            let mut source = open(&name, choice, 1);
            assert_eq!(source.info(), ordinary.info());
            assert_eq!(source.interpretation(), choice);
            let scale = f64::from(1 << (bits - 8));
            let maximum = f64::from((1 << bits) - 1);
            let (offset, range, c_range) = if source_full {
                (16.0 * scale, 219.0 * scale, 224.0 * scale)
            } else {
                (0.0, maximum, maximum)
            };
            let mut reference = Vec::new();
            for ordinal in 0..12 {
                let frame = source.next_rgba16(control()).unwrap().unwrap();
                let baseline = ordinary.next_rgba16(control()).unwrap().unwrap();
                assert_eq!(frame.metadata, baseline.metadata);
                assert_ne!(frame.rgba, baseline.rgba);
                let eight = source.copy_current_rgba(control()).unwrap();
                // The retained lossless HEVC fixtures have independently
                // authored neutral/color patches at these exact code values.
                for (patch, luma) in [
                    if source_full { 0.0 } else { 16.0 * scale },
                    81.0 * scale + f64::from(ordinal % 4),
                    120.0 * scale + f64::from(ordinal % 4),
                    if source_full { maximum } else { 235.0 * scale },
                ]
                .into_iter()
                .enumerate()
                {
                    let (cb, cr) = if patch == 2 {
                        (-23.0 * scale / c_range, 17.0 * scale / c_range)
                    } else {
                        (0.0, 0.0)
                    };
                    let expected = rgb((luma - offset) / range, cb, cr);
                    let x = patch * 24 + 8;
                    let actual = pixel(&frame, x, 8);
                    assert_eq!(actual[3], 65535);
                    for (channel, value) in expected.into_iter().enumerate() {
                        assert!((f64::from(actual[channel]) - value * 65535.0).abs() <= 1.0);
                        let byte = eight.rgba[8 * eight.row_stride_bytes + x * 4 + channel];
                        assert!((f64::from(byte) - value * 255.0).abs() <= 2.0);
                    }
                }
                reference.push(frame);
            }
            assert!(source.next_rgba16(control()).unwrap().is_none());
            let mut threaded = open(&name, choice, 8);
            for _ in 0..2 {
                for expected in &reference {
                    assert_eq!(&threaded.next_rgba16(control()).unwrap().unwrap(), expected);
                }
                threaded.seek(0, control()).unwrap();
            }
        }
    }
}

fn centered_sample(plane: &[u8], width: usize, height: usize, x: usize, y: usize) -> f64 {
    let column = (x as f64 - 0.5) / 2.0;
    let row = (y as f64 - 0.5) / 2.0;
    let x0 = column.floor();
    let y0 = row.floor();
    let dx = column - x0;
    let dy = row - y0;
    let sample = |x: f64, y: f64| {
        f64::from(
            plane[y.clamp(0.0, (height - 1) as f64) as usize * width
                + x.clamp(0.0, (width - 1) as f64) as usize],
        )
    };
    (1.0 - dy) * ((1.0 - dx) * sample(x0, y0) + dx * sample(x0 + 1.0, y0))
        + dy * ((1.0 - dx) * sample(x0, y0 + 1.0) + dx * sample(x0 + 1.0, y0 + 1.0))
}

#[test]
fn chroma_override_matches_retained_planar_codes_and_keeps_source_evidence() {
    let name = "av1-sdr-8-limited.mp4";
    let choice = VideoInterpretation {
        chroma_location: Some(ChromaLocation::Center),
        ..VideoInterpretation::SOURCE
    };
    let mut source = open(name, choice, 8);
    let mut ordinary = open(name, VideoInterpretation::SOURCE, 1);
    assert_eq!(source.info(), ordinary.info());
    let actual = source.next_rgba16(control()).unwrap().unwrap();
    let baseline = ordinary.next_rgba16(control()).unwrap().unwrap();
    assert_eq!(actual.metadata, baseline.metadata);
    assert_ne!(actual.rgba, baseline.rgba);
    assert_eq!((actual.width, actual.height), (128, 96));
    let raw = std::fs::read(fixture("av1-sdr-8-limited.yuv420p")).unwrap();
    let luma = &raw[..128 * 96];
    let cb = &raw[128 * 96..128 * 96 * 5 / 4];
    let cr = &raw[128 * 96 * 5 / 4..128 * 96 * 3 / 2];
    for y in 0..96 {
        for x in 0..128 {
            let expected = rgb(
                (f64::from(luma[y * 128 + x]) - 16.0) / 219.0,
                (centered_sample(cb, 64, 48, x, y) - 128.0) / 224.0,
                (centered_sample(cr, 64, 48, x, y) - 128.0) / 224.0,
            );
            let pixel = pixel(&actual, x, y);
            for (channel, value) in expected.into_iter().enumerate() {
                assert!((f64::from(pixel[channel]) - value * 65535.0).abs() <= 1.0);
            }
        }
    }
    source.seek(0, control()).unwrap();
    assert_eq!(source.next_rgba16(control()).unwrap().unwrap(), actual);
}

#[test]
fn unsupported_choices_and_raw_export_inspection_are_refused() {
    for (name, choice) in [
        (
            "cfr-bframes.mp4",
            VideoInterpretation {
                chroma_location: Some(ChromaLocation::Unspecified),
                ..VideoInterpretation::SOURCE
            },
        ),
        (
            "av1-pq.mp4",
            VideoInterpretation {
                range: Some(ColorRange::Full),
                ..VideoInterpretation::SOURCE
            },
        ),
    ] {
        assert!(
            SourceDecoder::open_interpreted(
                File::open(fixture(name)).unwrap(),
                DecodeLimits::default(),
                control(),
                choice,
            )
            .is_err()
        );
    }
    let mut source = open(
        "cfr-bframes.mp4",
        VideoInterpretation {
            range: Some(ColorRange::Full),
            ..VideoInterpretation::SOURCE
        },
        1,
    );
    assert!(matches!(
        source.next_i420(control()),
        Err(SourceDecodeError::InvalidConfiguration(_))
    ));
    assert!(matches!(
        source.next_yuv420p10(control()),
        Err(SourceDecodeError::InvalidConfiguration(_))
    ));
    assert!(source.next_rgba(control()).unwrap().is_some());
}
