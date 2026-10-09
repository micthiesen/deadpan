//! Real ProRes 422 packets, independent planar conversion and closed failures.
use deadpan_source::{
    ColorRange, ColorTransfer, DecodeControl, DecodeLimits, SourceDecodeError, SourceDecoder,
};
use std::{fs::File, io::Write, path::PathBuf, sync::atomic::AtomicBool, time::Duration};

static CANCELLED: AtomicBool = AtomicBool::new(false);
fn control() -> DecodeControl<'static> {
    DecodeControl {
        timeout: Duration::from_secs(10),
        cancelled: &CANCELLED,
    }
}
fn fixture(name: &str, suffix: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(format!("prores-{name}.{suffix}"))
}
fn open(name: &str, threads: u32) -> SourceDecoder {
    SourceDecoder::open(
        File::open(fixture(name, "mov")).unwrap(),
        DecodeLimits {
            threads,
            ..DecodeLimits::default()
        },
        control(),
    )
    .unwrap_or_else(|error| panic!("{name}: {error}"))
}
fn pixel(frame: &deadpan_source::DecodedRgbaFrame, x: usize, y: usize) -> [u16; 4] {
    let at = y * frame.row_stride_bytes + x * 8;
    std::array::from_fn(|c| {
        u16::from_le_bytes([frame.rgba[at + c * 2], frame.rgba[at + c * 2 + 1]])
    })
}

#[test]
fn all_422_profiles_preserve_precision_color_clocks_and_threaded_seeks() {
    for name in [
        "proxy",
        "lt",
        "standard",
        "hq",
        "anamorphic",
        "vfr",
        "apple-hq",
        "edges",
    ] {
        let (width, height) = if name == "edges" { (98, 66) } else { (96, 64) };
        let raw = std::fs::read(fixture(name, "yuv422p10le")).unwrap();
        let mut decoder = open(name, 1);
        let info = decoder.info();
        assert_eq!(info.codec, "prores");
        assert_eq!(info.pixel_format, "yuv422p10le");
        assert_eq!(info.color.range, ColorRange::Limited);
        assert_eq!(info.color.transfer, ColorTransfer::Bt709);
        assert_eq!((info.width as usize, info.height as usize), (width, height));
        assert_eq!((info.time_base_num, info.time_base_den), (1, 60000));
        assert_eq!(
            (info.sample_aspect_num, info.sample_aspect_den),
            if name == "anamorphic" { (4, 3) } else { (1, 1) }
        );
        assert!(!info.bwdif_fields);
        assert_eq!(info.audio_streams.len(), 1);
        let mut reference = Vec::new();
        let ordinals: Vec<i64> = (0..12)
            .filter(|n| name != "vfr" || ![4, 7].contains(n))
            .collect();
        assert_eq!(raw.len(), ordinals.len() * width * height * 4);
        for (index, &ordinal) in ordinals.iter().enumerate() {
            let frame = decoder.next_rgba16(control()).unwrap().unwrap();
            assert_eq!(frame.metadata.pts, ordinal * 2002);
            assert_eq!(
                frame.metadata.reported_duration,
                Some((ordinals.get(index + 1).copied().unwrap_or(12) - ordinal) * 2002)
            );
            assert!(frame.metadata.keyframe);
            assert_eq!(frame.sample_bits, 16);
            let at = index * width * height * 4;
            let code = |plane: usize, x: usize, y: usize| {
                let offset = if plane == 0 {
                    y * width + x
                } else {
                    width * height + (plane - 1) * (width / 2) * height + y * (width / 2) + x
                };
                f64::from(u16::from_le_bytes([
                    raw[at + offset * 2],
                    raw[at + offset * 2 + 1],
                ]))
            };
            for y in 0..height {
                for x in 0..width {
                    let luma = (code(0, x, y) - 64.0) / 876.0;
                    let chroma = |plane| {
                        let value = if x % 2 == 0 {
                            code(plane, x / 2, y)
                        } else {
                            (code(plane, x / 2, y) + code(plane, (x / 2 + 1).min(width / 2 - 1), y))
                                / 2.0
                        };
                        (value - 512.0) / 896.0
                    };
                    let red = luma + 1.5748 * chroma(2);
                    let blue = luma + 1.8556 * chroma(1);
                    let green = (luma - 0.2126 * red - 0.0722 * blue) / 0.7152;
                    let actual = pixel(&frame, x, y);
                    assert_eq!(actual[3], 65535);
                    for (c, expected) in [red, green, blue].into_iter().enumerate() {
                        let expected = (expected.clamp(0.0, 1.0) * 65535.0).round();
                        assert!(
                            (f64::from(actual[c]) - expected).abs() <= 1.0,
                            "{name} {ordinal} {x},{y} c{c}: {} vs {expected}",
                            actual[c]
                        );
                    }
                }
            }
            // Independent authored neutral patch, including adjacent low bits.
            let expected = ((324 + ordinal % 4 - 64) as f64 / 876.0 * 65535.0).round();
            let actual = pixel(&frame, 32, 8);
            assert!(
                (f64::from(actual[0]) - expected).abs() <= 150.0,
                "{name} patch {actual:?} vs {expected}"
            );
            reference.push(frame);
        }
        assert!(decoder.next_rgba16(control()).unwrap().is_none());
        assert_ne!(
            pixel(&reference[0], 32, 8),
            pixel(&reference[1], 32, 8),
            "{name} low bits"
        );
        for threads in [1, 8, 16] {
            let mut decoder = open(name, threads);
            for expected in &reference {
                assert_eq!(&decoder.next_rgba16(control()).unwrap().unwrap(), expected);
            }
            for expected in reference.iter().rev() {
                decoder.seek(expected.metadata.pts, control()).unwrap();
                assert_eq!(&decoder.next_rgba16(control()).unwrap().unwrap(), expected);
            }
        }
    }
}

#[test]
fn both_interlaced_orders_use_exact_field_times_and_separate_content() {
    for name in ["tff", "bff"] {
        let mut decoder = open(name, 1);
        assert!(decoder.info().bwdif_fields);
        assert_eq!(decoder.info().time_base_den, 360000);
        let mut reference = Vec::new();
        for ordinal in 0..24 {
            let frame = decoder.next_rgba16(control()).unwrap().unwrap();
            assert_eq!(frame.metadata.pts, ordinal * 6006);
            assert_eq!(frame.metadata.reported_duration, Some(6006));
            let bottom = (ordinal % 2 == 1) != (name == "bff");
            let code = 200 + 16 * (ordinal / 2) + if bottom { 160 } else { 0 };
            let expected = ((code - 64) as f64 / 876.0 * 65535.0).round();
            for y in 4..28 {
                let actual = pixel(&frame, 32, y);
                assert!(
                    (f64::from(actual[0]) - expected).abs() <= 150.0,
                    "{name} field {ordinal} row{y}: {actual:?} vs {expected}"
                );
            }
            reference.push(frame);
        }
        assert!(decoder.next_rgba16(control()).unwrap().is_none());
        for threads in [8, 16] {
            let mut decoder = open(name, threads);
            for expected in &reference {
                assert_eq!(&decoder.next_rgba16(control()).unwrap().unwrap(), expected);
            }
        }
    }
}

fn copied(bytes: &[u8]) -> File {
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(bytes).unwrap();
    file
}
fn code(error: SourceDecodeError) -> String {
    match error {
        SourceDecodeError::Native { code, .. } => code,
        other => panic!("{other}"),
    }
}

#[test]
fn packet_envelopes_profiles_and_interpretation_fail_closed() {
    let original = std::fs::read(fixture("hq", "mov")).unwrap();
    let first = original
        .windows(4)
        .position(|bytes| bytes == b"icpf")
        .unwrap()
        - 4;
    let h = first + 8;
    let picture = h + usize::from(u16::from_be_bytes([original[h], original[h + 1]]));
    let cases: Vec<(usize, Vec<u8>, &str)> = vec![
        (first, vec![0, 0, 0, 28], "unsupported_codec"),
        (h, vec![0, 19], "unsupported_codec"),
        (h + 3, vec![2], "unsupported_codec"),
        (h + 8, vec![0, 95], "stream_changed"),
        (h + 8, vec![0xff, 0xff], "resource_limit"),
        (h + 12, vec![0xc0], "unsupported_codec"),
        (h + 12, vec![0x8c], "unsupported_codec"),
        (h + 12, vec![0x84], "unsupported_interlace"),
        (h + 13, vec![0x40], "unsupported_transform"),
        (h + 14, vec![2], "unsupported_primaries"),
        (h + 15, vec![16], "unsupported_transfer"),
        (h + 16, vec![0], "unsupported_matrix"),
        (h + 17, vec![1], "unsupported_codec"),
        (h + 19, vec![4], "unsupported_codec"),
        (h + 20, vec![0], "unsupported_codec"),
        (picture, vec![0x38], "unsupported_codec"),
        (picture + 1, vec![0xff; 4], "unsupported_codec"),
        (picture + 7, vec![0x40], "unsupported_codec"),
        (picture + 8, vec![0xff, 0xff], "unsupported_codec"),
    ];
    for (at, replacement, expected) in cases {
        let mut bytes = original.clone();
        bytes[at..at + replacement.len()].copy_from_slice(&replacement);
        let result = SourceDecoder::open(copied(&bytes), DecodeLimits::default(), control());
        assert_eq!(
            code(
                result
                    .err()
                    .unwrap_or_else(|| panic!("accepted offset{at}"))
            ),
            expected,
            "offset{at}"
        );
    }
    for tag in [b"ap4h", b"ap4x", b"aprn"] {
        let mut bytes = original.clone();
        let at = bytes.windows(4).position(|b| b == b"apch").unwrap();
        bytes[at..at + 4].copy_from_slice(tag);
        assert_eq!(
            code(
                SourceDecoder::open(copied(&bytes), DecodeLimits::default(), control())
                    .err()
                    .unwrap()
            ),
            "unsupported_codec"
        );
    }
    // A later bad packet cannot be concealed by a successful first picture.
    let mut bytes = original;
    let last = bytes.windows(4).rposition(|b| b == b"icpf").unwrap() + 4;
    bytes[last + 14] = 9;
    let mut decoder = SourceDecoder::open(
        copied(&bytes),
        DecodeLimits {
            threads: 1,
            ..DecodeLimits::default()
        },
        control(),
    )
    .unwrap();
    loop {
        match decoder.next_metadata(control()) {
            Ok(Some(_)) => (),
            Ok(None) => panic!("late changed color was ignored"),
            Err(error) => {
                assert_eq!(code(error), "stream_changed");
                break;
            }
        }
    }
    assert!(decoder.next_metadata(control()).is_err());
}

#[test]
fn quicktime_aac_keeps_identical_decoded_samples_and_priming() {
    use deadpan_source::audio::{AudioDecodeLimits, AudioDecoder};
    let collect = |path: PathBuf| {
        let mut decoder = AudioDecoder::open_first(
            File::open(path).unwrap(),
            AudioDecodeLimits::default(),
            control(),
        )
        .unwrap();
        let mut result = Vec::new();
        while let Some(metadata) = decoder.next_metadata(control()).unwrap() {
            result.push((
                metadata,
                decoder
                    .copy_current_interleaved_f32(control())
                    .unwrap()
                    .samples,
            ));
        }
        result
    };
    let expected = collect(fixture("hq", "mov").with_file_name("fields-bff.mp4"));
    for name in [
        "proxy",
        "lt",
        "standard",
        "hq",
        "anamorphic",
        "vfr",
        "tff",
        "bff",
        "apple-hq",
    ] {
        assert_eq!(collect(fixture(name, "mov")), expected, "{name}");
    }
}

#[test]
fn quicktime_metadata_is_closed_and_cannot_override_packet_color_or_geometry() {
    let original = std::fs::read(fixture("anamorphic", "mov")).unwrap();
    let at = |tag: &[u8; 4]| original.windows(4).position(|b| b == tag).unwrap();
    let cases = [
        (at(b"clef") + 8, vec![0, 0x81, 0, 0], "invalid_input"),
        (at(b"prof"), b"clef".to_vec(), "invalid_input"),
        (at(b"enof") + 8, vec![0, 0x61, 0, 0], "invalid_input"),
        (at(b"nclc") + 4, vec![0, 9], "stream_changed"),
        (at(b"nclc"), b"prof".to_vec(), "invalid_input"),
        (at(b"mp4a") + 12, vec![0, 2], "invalid_input"),
        (at(b"frma") + 4, b"alac".to_vec(), "invalid_input"),
        (at(b"chan") + 8, vec![0, 0x65, 0, 2], "invalid_input"),
        (at(b"chan") + 12, vec![0, 0, 0, 4], "invalid_input"),
        (at(b"dhlr"), b"mhlr".to_vec(), "invalid_input"),
    ];
    for (at, replacement, expected) in cases {
        let mut bytes = original.clone();
        bytes[at..at + replacement.len()].copy_from_slice(&replacement);
        assert_eq!(
            code(
                SourceDecoder::open(copied(&bytes), DecodeLimits::default(), control())
                    .err()
                    .unwrap_or_else(|| panic!("accepted offset{at}"))
            ),
            expected,
            "offset{at}"
        );
    }
}

#[test]
fn apple_global_description_must_match_outer_geometry_color_and_field_order() {
    let original = std::fs::read(fixture("apple-hq", "mov")).unwrap();
    let start = original.windows(4).position(|b| b == b"glbl").unwrap() + 4;
    for (relative, value) in [
        (0, 1),
        (7, b'n'),
        (32, 1),
        (83, 16),
        (99, 9),
        (113, 1),
        (117, 1),
    ] {
        let mut bytes = original.clone();
        bytes[start + relative] = value;
        assert_eq!(
            code(
                SourceDecoder::open(copied(&bytes), DecodeLimits::default(), control())
                    .err()
                    .unwrap_or_else(|| panic!("accepted global offset{relative}"))
            ),
            "invalid_input"
        );
    }
}
