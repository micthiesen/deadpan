//! Independently authored SDR code values, clocks and threaded VP9 seeks.
use deadpan_source::{ColorRange, ColorTransfer, DecodeControl, DecodeLimits, SourceDecoder};
use std::io::{Seek, SeekFrom, Write};
use std::{fs::File, path::PathBuf, sync::atomic::AtomicBool, time::Duration};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

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
    .unwrap_or_else(|error| panic!("{name}: {error}"))
}
fn pixel(frame: &deadpan_source::DecodedRgbaFrame, x: usize, y: usize) -> [u16; 4] {
    let at = y * frame.row_stride_bytes + x * 8;
    std::array::from_fn(|c| {
        u16::from_le_bytes([frame.rgba[at + c * 2], frame.rgba[at + c * 2 + 1]])
    })
}

#[test]
fn profiles_zero_and_two_preserve_color_precision_timing_and_threaded_seeks() {
    for bits in [8, 10] {
        for full in [false, true] {
            let name = format!(
                "vp9-sdr-{bits}-{}.mp4",
                if full { "full" } else { "limited" }
            );
            let mut decoder = open(&name, 1);
            assert_eq!(decoder.info().codec, "vp9");
            assert_eq!(decoder.info().color.transfer, ColorTransfer::Bt709);
            assert_eq!(
                decoder.info().color.range,
                if full {
                    ColorRange::Full
                } else {
                    ColorRange::Limited
                }
            );
            assert_eq!(
                (decoder.info().time_base_num, decoder.info().time_base_den),
                (1, 60000)
            );
            assert!(!decoder.info().bwdif_fields);
            let scale = f64::from(1 << (bits - 8));
            let maximum = f64::from((1 << bits) - 1);
            let (offset, range, chroma_range) = if full {
                (0.0, maximum, maximum)
            } else {
                (16.0 * scale, 219.0 * scale, 224.0 * scale)
            };
            let mut reference = Vec::new();
            for ordinal in 0..12 {
                let frame = decoder.next_rgba16(control()).unwrap().unwrap();
                assert_eq!(frame.metadata.pts, ordinal * 2002);
                assert_eq!(frame.metadata.reported_duration, Some(2002));
                assert_eq!(frame.sample_bits, 16);
                let levels = [
                    offset,
                    81.0 * scale + (ordinal % 4) as f64,
                    120.0 * scale + (ordinal % 4) as f64,
                    if full { maximum } else { 235.0 * scale },
                ];
                for (patch, level) in levels.into_iter().enumerate() {
                    let y = (level - offset) / range;
                    let (cb, cr) = if patch == 2 {
                        (-23.0 * scale / chroma_range, 17.0 * scale / chroma_range)
                    } else {
                        (0.0, 0.0)
                    };
                    let r = y + 2.0 * (1.0 - 0.2126) * cr;
                    let b = y + 2.0 * (1.0 - 0.0722) * cb;
                    let g = (y - 0.2126 * r - 0.0722 * b) / (1.0 - 0.2126 - 0.0722);
                    for row in 4..28 {
                        for x in patch * 24 + 4..patch * 24 + 20 {
                            let actual = pixel(&frame, x, row);
                            assert_eq!(actual[3], 65535);
                            for (channel, expected) in [r, g, b].into_iter().enumerate() {
                                let expected = expected.clamp(0.0, 1.0) * 65535.0;
                                assert!(
                                    (f64::from(actual[channel]) - expected).abs() <= 1.0,
                                    "{name} {ordinal}/{patch} channel {channel}: {} vs {expected}",
                                    actual[channel]
                                );
                            }
                        }
                    }
                }
                reference.push(frame);
            }
            assert!(decoder.next_rgba16(control()).unwrap().is_none());
            if bits == 10 {
                assert_ne!(
                    pixel(&reference[0], 30, 8),
                    pixel(&reference[1], 30, 8),
                    "ten-bit low bits survive"
                );
            }
            for threads in [1, 8, 16] {
                let mut decoder = open(&name, threads);
                for expected in &reference {
                    assert_eq!(&decoder.next_rgba16(control()).unwrap().unwrap(), expected);
                }
                for expected in reference.iter().rev() {
                    decoder.seek(expected.metadata.pts, control()).unwrap();
                    loop {
                        let actual = decoder.next_rgba16(control()).unwrap().unwrap();
                        if actual.metadata.pts == expected.metadata.pts {
                            assert_eq!(&actual, expected, "{name} {threads}");
                            break;
                        }
                        assert!(
                            actual.metadata.pts < expected.metadata.pts,
                            "{name} threads={threads} sought {} but got {}",
                            expected.metadata.pts,
                            actual.metadata.pts
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn altref_superframes_keep_one_picture_per_sample_and_exact_reverse_seeks() {
    let name = "vp9-altref.mp4";
    let data = std::fs::read(fixture(name)).unwrap();
    let mut packets = deadpan_source::Mp4PacketReader::open(
        File::open(fixture(name)).unwrap(),
        DecodeLimits::default(),
        control(),
    )
    .unwrap();
    let mut hidden = 0;
    while let Some(packet) = packets.next_packet(control()).unwrap() {
        let last = data[usize::try_from(packet.offset).unwrap() + packet.length as usize - 1];
        if last & 0xe0 == 0xc0 {
            hidden += last & 7;
        }
    }
    assert_eq!(hidden, 6, "fixture must actually contain hidden pictures");
    let mut source = open(name, 1);
    let mut reference = Vec::new();
    while let Some(frame) = source.next_rgba(control()).unwrap() {
        assert_eq!(
            frame.metadata.pts,
            i64::try_from(reference.len()).unwrap() * 2002
        );
        assert_eq!(frame.metadata.reported_duration, Some(2002));
        // Independent authored motion, not another decode of the same file:
        // lower-half neutral luma advances seven code values per picture.
        let error = |ordinal: usize| {
            let mut squared = 0.0;
            let mut samples = 0;
            for y in (36..60).step_by(2) {
                for x in (4..92).step_by(2) {
                    let level = 40 + (2 * x + y + 7 * ordinal) % 180;
                    let expected = (level as f64 - 16.0) * 255.0 / 219.0;
                    let actual = f64::from(frame.rgba[y * frame.row_stride_bytes + x * 4]);
                    squared += (actual - expected).powi(2);
                    samples += 1;
                }
            }
            squared / f64::from(samples)
        };
        let ordinal = reference.len();
        let actual_error = error(ordinal);
        assert!(
            actual_error < 16.0,
            "authored VP9 picture {ordinal}: {actual_error}"
        );
        for neighbor in [
            ordinal.checked_sub(1),
            (ordinal < 59).then_some(ordinal + 1),
        ]
        .into_iter()
        .flatten()
        {
            assert!(
                actual_error * 4.0 < error(neighbor),
                "VP9 picture {ordinal} resembles {neighbor}"
            );
        }
        reference.push(frame);
    }
    assert_eq!(reference.len(), 60);
    assert!(source.work().decoded_pictures >= 66);
    for threads in [1, 8, 16] {
        let mut source = open(name, threads);
        for expected in &reference {
            assert_eq!(&source.next_rgba(control()).unwrap().unwrap(), expected);
        }
        for expected in reference.iter().rev() {
            source.seek(expected.metadata.pts, control()).unwrap();
            loop {
                let actual = source.next_rgba(control()).unwrap().unwrap();
                if actual.metadata.pts == expected.metadata.pts {
                    assert_eq!(&actual, expected, "threads {threads}");
                    break;
                }
                assert!(actual.metadata.pts < expected.metadata.pts);
            }
        }
    }
}

fn refusal(data: &[u8]) -> String {
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(data).unwrap();
    file.seek(SeekFrom::Start(0)).unwrap();
    let error = match SourceDecoder::open(file, DecodeLimits::default(), control()) {
        Err(error) => error,
        Ok(mut source) => loop {
            match source.next_metadata(control()) {
                Err(error) => break error,
                Ok(Some(_)) => (),
                Ok(None) => panic!("forged VP9 was admitted"),
            }
        },
    };
    match error {
        deadpan_source::SourceDecodeError::Native { code, .. } => code,
        other => panic!("unexpected error: {other}"),
    }
}

fn set_bits(data: &mut [u8], start: usize, count: usize, value: u32) {
    for bit in 0..count {
        let at = start + bit;
        let mask = 1 << (7 - at % 8);
        data[at / 8] =
            (data[at / 8] & !mask) | (u8::from(value & (1 << (count - 1 - bit)) != 0) * mask);
    }
}

#[test]
fn malformed_configuration_and_packet_geometry_fail_before_unbounded_decode() {
    let path = fixture("vp9-sdr-8-limited.mp4");
    let original = std::fs::read(&path).unwrap();
    let vpcc = original
        .windows(4)
        .position(|bytes| bytes == b"vpcC")
        .unwrap()
        + 4;
    for (offset, value) in [
        (0, 0),
        (1, 1),
        (4, 1),
        (5, 99),
        (6, 0xc0),
        (6, 0x84),
        (10, 1),
    ] {
        let mut data = original.clone();
        data[vpcc + offset] = value;
        assert_eq!(refusal(&data), "invalid_input", "vpcC {offset}/{value}");
    }
    let mut reader = deadpan_source::Mp4PacketReader::open(
        File::open(&path).unwrap(),
        DecodeLimits::default(),
        control(),
    )
    .unwrap();
    let first = reader.next_packet(control()).unwrap().unwrap();
    let start = usize::try_from(first.offset).unwrap();
    // Profile 0 key header: 8 control, 24 sync, 4 color, 16+16 raster bits.
    for (bit, count, value, code) in [
        (0, 2, 0, "invalid_input"),
        (2, 2, 1, "unsupported_codec"),
        (32, 3, 1, "stream_changed"),
        (35, 1, 1, "stream_changed"),
        (36, 16, 65535, "resource_limit"),
        (52, 16, 65535, "resource_limit"),
        (36, 16, 95 - 1, "stream_changed"),
        (68, 1, 1, "unsupported_transform"),
    ] {
        let mut data = original.clone();
        set_bits(&mut data[start..], bit, count, value);
        assert_eq!(refusal(&data), code, "packet bit {bit}");
    }
    let mut full_config = original.clone();
    full_config[vpcc + 6] |= 1;
    assert_eq!(
        refusal(&full_config),
        "invalid_input",
        "contradictory vpcC/colr"
    );
}

#[test]
fn malformed_superframes_and_multiple_visible_pictures_are_refused() {
    let path = fixture("vp9-altref.mp4");
    let original = std::fs::read(&path).unwrap();
    let mut reader = deadpan_source::Mp4PacketReader::open(
        File::open(path).unwrap(),
        DecodeLimits::default(),
        control(),
    )
    .unwrap();
    reader.next_packet(control()).unwrap().unwrap();
    let packet = reader.next_packet(control()).unwrap().unwrap();
    let start = usize::try_from(packet.offset).unwrap();
    let end = start + packet.length as usize;
    let marker = original[end - 1];
    let frames = usize::from(marker & 7) + 1;
    let bytes = usize::from((marker >> 3) & 3) + 1;
    let index = end - 2 - frames * bytes;
    assert_eq!(frames, 2);
    assert_eq!(original[index], marker);
    for at in [index, index + 1, end - 1] {
        let mut data = original.clone();
        data[at] = 0;
        assert!(matches!(
            refusal(&data).as_str(),
            "invalid_input" | "unsupported_timing"
        ));
    }
    let mut shown = original.clone();
    shown[start] |= 2; // first hidden frame must not claim its own picture PTS
    assert_eq!(refusal(&shown), "unsupported_timing");
}

#[test]
fn show_existing_frame_has_its_own_pts_and_seeks_to_the_same_pixels() {
    for bits in [8, 10] {
        let name = format!("vp9-existing-{bits}.mp4");
        for threads in [1, 8, 16] {
            let mut source = open(&name, threads);
            let first = source.next_rgba16(control()).unwrap().unwrap();
            let second = source.next_rgba16(control()).unwrap().unwrap();
            assert_eq!(first.rgba, second.rgba);
            assert_eq!(first.metadata.pts, 0);
            assert_eq!(second.metadata.pts, 2002);
            assert_eq!(second.metadata.reported_duration, Some(2002));
            assert!(source.next_rgba16(control()).unwrap().is_none());
            for _ in 0..3 {
                source.seek(2002, control()).unwrap();
                assert_eq!(source.next_rgba16(control()).unwrap().unwrap(), first);
                assert_eq!(source.next_rgba16(control()).unwrap().unwrap(), second);
            }
        }
    }
}

#[test]
fn vpcc_chroma_siting_changes_only_the_declared_sample_phase() {
    let mut left = open("vp9-sdr-10-limited.mp4", 1);
    let left = left.next_rgba16(control()).unwrap().unwrap();
    let mut data = std::fs::read(fixture("vp9-sdr-10-limited.mp4")).unwrap();
    let vpcc = data.windows(4).position(|bytes| bytes == b"vpcC").unwrap() + 4;
    data[vpcc + 6] |= 2; // top-left chroma, identical coded picture bytes
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(&data).unwrap();
    let mut top = SourceDecoder::open(file, DecodeLimits::default(), control()).unwrap();
    let top = top.next_rgba16(control()).unwrap().unwrap();
    assert_eq!(pixel(&left, 60, 8), pixel(&top, 60, 8));
    // At y=31 the colored chroma has weight 3/4 for left and 1/2 for
    // top-left siting. Check the authored BT.709 matrix independently.
    for (frame, colored_weight) in [(&left, 0.75), (&top, 0.5)] {
        let y = (120.0 - 16.0) / 219.0;
        let cr = 17.0 / 224.0 * colored_weight;
        let expected = (y + 2.0 * (1.0 - 0.2126) * cr) * 65535.0;
        assert!((f64::from(pixel(frame, 60, 31)[0]) - expected).abs() <= 1.0);
    }
}
