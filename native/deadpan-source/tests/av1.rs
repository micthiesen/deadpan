//! Real AV1 temporal units through the bounded software decoder.
use deadpan_source::{ColorRange, ColorTransfer, DecodeControl, DecodeLimits, SourceDecoder};
use std::io::Write;
use std::{fs::File, path::PathBuf, sync::atomic::AtomicBool, time::Duration};

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
        .join(format!("av1-{name}.{suffix}"))
}
fn open(name: &str, suffix: &str, threads: u32) -> SourceDecoder {
    SourceDecoder::open(
        File::open(fixture(name, suffix)).unwrap(),
        DecodeLimits {
            threads,
            ..DecodeLimits::default()
        },
        control(),
    )
    .unwrap_or_else(|error| panic!("{name}.{suffix}: {error}"))
}

#[test]
fn temporal_units_preserve_pictures_pts_and_bound_decode_work() {
    let mut decoder = open("sdr-10-limited", "mp4", 1);
    assert_eq!(decoder.info().codec, "av1");
    assert_eq!(decoder.info().pixel_format, "yuv420p10le");
    let mut reference = Vec::new();
    for ordinal in 0..12 {
        let frame = decoder.next_rgba16(control()).unwrap().unwrap();
        assert_eq!(frame.metadata.pts, ordinal * 2002);
        assert_eq!(frame.metadata.reported_duration, Some(2002));
        assert_eq!(frame.metadata.keyframe, ordinal == 0);
        reference.push(frame);
    }
    assert!(decoder.next_rgba16(control()).unwrap().is_none());
    assert!(decoder.work().decoded_pictures >= 12);
    for threads in [8, 16] {
        let mut decoder = open("sdr-10-limited", "mp4", threads);
        for expected in &reference {
            assert_eq!(&decoder.next_rgba16(control()).unwrap().unwrap(), expected);
        }
        decoder.seek(0, control()).unwrap();
        for expected in &reference {
            assert_eq!(&decoder.next_rgba16(control()).unwrap().unwrap(), expected);
        }
    }
}

const CASES: &[&str] = &[
    "sdr-8-limited",
    "sdr-8-full",
    "sdr-10-limited",
    "sdr-10-full",
    "anamorphic",
    "topleft",
    "vfr",
    "multigop",
    "grain",
    "superres",
    "pq",
    "hlg",
];

fn copied(bytes: &[u8]) -> File {
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(bytes).unwrap();
    file
}
fn refusal(bytes: &[u8]) -> String {
    let error = match SourceDecoder::open(copied(bytes), DecodeLimits::default(), control()) {
        Err(error) => error,
        Ok(mut decoder) => loop {
            match decoder.next_metadata(control()) {
                Err(error) => break error,
                Ok(Some(_)) => (),
                Ok(None) => return "accepted".into(),
            }
        },
    };
    match error {
        deadpan_source::SourceDecodeError::Native { code, .. } => code,
        other => panic!("{other}"),
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
fn packets(name: &str) -> Vec<(usize, usize)> {
    let mut reader = deadpan_source::Mp4PacketReader::open(
        File::open(fixture(name, "mp4")).unwrap(),
        DecodeLimits::default(),
        control(),
    )
    .unwrap();
    let mut result = Vec::new();
    while let Some(p) = reader.next_packet(control()).unwrap() {
        if p.track_index == 0 {
            result.push((usize::try_from(p.offset).unwrap(), p.length as usize));
        }
    }
    result
}
fn padded(mut prefix: Vec<u8>, length: usize) -> Vec<u8> {
    let remaining = length.checked_sub(prefix.len()).unwrap();
    for size in 0..remaining {
        let mut n = size;
        let mut leb = Vec::new();
        loop {
            let byte = (n & 127) as u8;
            n >>= 7;
            leb.push(byte | if n == 0 { 0 } else { 128 });
            if n == 0 {
                break;
            }
        }
        if 1 + leb.len() + size == remaining {
            prefix.push(0x7a);
            prefix.extend(leb);
            prefix.resize(length, 0);
            return prefix;
        }
    }
    panic!("cannot pad AV1 packet");
}

#[test]
fn sequence_allocation_color_and_packet_grammar_fail_closed() {
    let original = std::fs::read(fixture("sdr-10-limited", "mp4")).unwrap();
    let config = original.windows(4).position(|b| b == b"av1C").unwrap() + 4;
    assert_eq!(&original[config..config + 6], &[0x81, 0, 0x4d, 0, 0x0a, 14]);
    for (offset, value) in [
        (0, 1),
        (1, 32),
        (1, 24),
        (2, 0x6d),
        (2, 0x5d),
        (2, 0x49),
        (2, 0x4c),
        (2, 0x4f),
        (2, 0xcd),
        (3, 0x80),
        (3, 1),
    ] {
        let mut data = original.clone();
        data[config + offset] = value;
        assert_eq!(
            refusal(&data),
            "invalid_input",
            "av1C prefix {offset}/{value}"
        );
    }
    // Pinned generated sequence: one operating point, seven-bit raster fields,
    // explicit BT.709. Mutate both configuration and in-band sequence headers.
    let (first, length) = packets("sdr-10-limited")[0];
    assert_eq!(&original[first..first + 2], &[0x0a, 14]);
    for start in [config + 6, first + 2] {
        for (bit, count, value, expected) in [
            (0, 3, 1, "unsupported_codec"),
            (7, 5, 1, "unsupported_codec"),
            (12, 12, 1, "unsupported_codec"),
            (24, 5, 31, "stream_changed"),
            (34, 4, 15, "resource_limit"),
            (38, 4, 15, "resource_limit"),
            (42, 7, 126, "stream_changed"),
            (49, 7, 94, "stream_changed"),
            (75, 1, 0, "stream_changed"),
            (76, 1, 1, "unsupported_pixel_format"),
            (78, 8, 9, "stream_changed"),
            (86, 8, 16, "stream_changed"),
            (94, 8, 9, "stream_changed"),
            (102, 1, 1, "stream_changed"),
            (103, 2, 2, "stream_changed"),
            (107, 1, 0, "invalid_input"),
            (111, 1, 1, "invalid_input"),
        ] {
            let mut data = original.clone();
            set_bits(&mut data[start..start + 14], bit, count, value);
            assert_eq!(refusal(&data), expected, "sequence {start} bit {bit}");
        }
    }
    let mut cases = vec![
        (vec![0x8a, 0], "invalid_input"),
        (vec![0x08, 0], "invalid_input"),
        (vec![0x0b, 0], "invalid_input"),
        (vec![0x0e, 0x20, 0], "unsupported_codec"),
        (
            vec![0x0a, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff],
            "invalid_input",
        ),
        (vec![0x0a, 0], "resource_limit"),
        (vec![0x2a, 2, 4, 0x80], "unsupported_metadata"),
        (vec![0x2a, 2, 3, 0x80], "unsupported_metadata"),
        (vec![0x2a, 2, 1, 0x80], "invalid_input"),
        (vec![0x22, 1, 0], "invalid_input"),
        (vec![0x3a, 1, 0], "unsupported_codec"),
        (vec![0x32, 1, 0x10, 0x32, 1, 0x10], "unsupported_timing"),
        (vec![0x7a, 1, 1], "invalid_input"),
        (vec![0x12, 1, 0], "invalid_input"),
    ];
    cases.push(([0x32, 1, 0x20].repeat(33), "resource_limit"));
    cases.push(([0x7a, 0].repeat(257), "resource_limit"));
    for (prefix, expected) in cases {
        let mut data = original.clone();
        data[first..first + length].copy_from_slice(&padded(prefix.clone(), length));
        assert_eq!(refusal(&data), expected, "packet prefix {prefix:?}");
    }
    // A failed later packet poisons the decoder; a successful first picture
    // never turns damaged subsequent content into an apparent end of file.
    let (last, _) = *packets("sdr-10-limited").last().unwrap();
    let mut data = original.clone();
    data[last] |= 0x80;
    assert_eq!(refusal(&data), "invalid_input");
    let mut decoder =
        SourceDecoder::open(copied(&data), DecodeLimits::default(), control()).unwrap();
    let failed = (0..=12).any(|_| decoder.next_metadata(control()).is_err());
    assert!(failed);
    assert!(
        decoder
            .next_metadata(control())
            .unwrap_err()
            .to_string()
            .contains("session_failed")
    );
}

#[test]
fn av1_uses_limits_before_its_private_allocator() {
    for limits in [
        DecodeLimits {
            max_pixels: 128 * 96 - 1,
            ..DecodeLimits::default()
        },
        DecodeLimits {
            max_dimension: 127,
            ..DecodeLimits::default()
        },
    ] {
        let error = SourceDecoder::open(
            File::open(fixture("sdr-10-limited", "mp4")).unwrap(),
            limits,
            control(),
        )
        .err()
        .unwrap();
        assert!(error.to_string().contains("resource_limit"), "{error}");
    }
    // The actual height fits 96, but the AV1 size-override field can represent
    // up to 128. A tight non-power-of-two dimension limit is conservative.
    let limits = DecodeLimits {
        max_dimension: 128,
        max_pixels: 128 * 96,
        ..DecodeLimits::default()
    };
    let mut source = SourceDecoder::open(
        File::open(fixture("superres", "mp4")).unwrap(),
        limits,
        control(),
    )
    .unwrap();
    let mut count = 0;
    while source.next_metadata(control()).unwrap().is_some() {
        count += 1;
    }
    assert_eq!(count, 12);
}

#[test]
fn an_empty_config_obu_list_bootstraps_from_the_first_guarded_packet() {
    let mut baseline = open("sdr-10-limited", "mp4", 1);
    let mut source = open("no-config-sequence", "mp4", 8);
    assert_eq!(source.info(), baseline.info());
    let mut reference = Vec::new();
    while let Some(expected) = baseline.next_rgba16(control()).unwrap() {
        assert_eq!(source.next_rgba16(control()).unwrap().unwrap(), expected);
        reference.push(expected);
    }
    assert!(source.next_rgba16(control()).unwrap().is_none());
    source.seek(0, control()).unwrap();
    for expected in reference {
        assert_eq!(source.next_rgba16(control()).unwrap().unwrap(), expected);
    }
    let mut data = std::fs::read(fixture("no-config-sequence", "mp4")).unwrap();
    let (first, _) = packets("no-config-sequence")[0];
    // Replace the first sequence OBU by same-size zero padding. Without a
    // preceding sequence even a plausible frame header must not reach dav1d.
    assert_eq!(&data[first..first + 2], &[0x0a, 14]);
    data[first..first + 16].copy_from_slice(&padded(Vec::new(), 16));
    assert_eq!(refusal(&data), "invalid_input");
}

#[test]
fn config_and_packet_hdr_metadata_are_retained_and_changes_are_rejected() {
    for name in ["pq-static-config", "pq-static-packet"] {
        let mut source = open(name, "mp4", 8);
        let color = source.info().color;
        assert_eq!(color.transfer, ColorTransfer::Pq);
        assert_eq!(
            color.content_light,
            Some(deadpan_source::ContentLight {
                max_cll: 1000,
                max_fall: 400
            })
        );
        assert_eq!(
            color.mastering,
            Some(deadpan_source::MasteringDisplay {
                primaries: [[34375, 15625], [12500, 34375], [6250, 3125]],
                white_point: [15625, 15625],
                max_luminance: 10_000_000,
                min_luminance: 10_000,
            })
        );
        assert!(color.ignored_static.is_empty());
        let mut baseline = open("pq", "mp4", 1);
        for _ in 0..12 {
            assert_eq!(
                source.next_rgba16(control()).unwrap().unwrap().rgba,
                baseline.next_rgba16(control()).unwrap().unwrap().rgba
            );
        }
        assert!(source.next_metadata(control()).unwrap().is_none());
        source.seek(0, control()).unwrap();
        assert!(source.next_metadata(control()).unwrap().is_some());
    }
    let original = std::fs::read(fixture("pq-static-config", "mp4")).unwrap();
    let cll = original
        .windows(8)
        .position(|b| b == [0x2a, 6, 1, 3, 232, 1, 144, 128])
        .unwrap();
    // Invalid static values remain explicitly reported as ignored.
    let mut invalid = original.clone();
    invalid[cll + 3..cll + 5].copy_from_slice(&100_u16.to_be_bytes());
    let source = SourceDecoder::open(copied(&invalid), DecodeLimits::default(), control()).unwrap();
    assert!(source.info().color.content_light.is_none());
    assert!(source.info().color.ignored_static.content_light);
    // The same valid declarations cannot disappear merely because the
    // sequence changes to SDR in both container and bitstream tags.
    let mut data = original.clone();
    let config = data.windows(4).position(|b| b == b"av1C").unwrap() + 4;
    let colr = data.windows(4).position(|b| b == b"nclx").unwrap() + 4;
    data[colr..colr + 6].copy_from_slice(&[0, 1, 0, 1, 0, 1]);
    let (first, _) = packets("pq-static-config")[0];
    for start in [config + 6, first + 2] {
        for at in [78, 86, 94] {
            set_bits(&mut data[start..start + 14], at, 8, 1);
        }
    }
    assert_eq!(refusal(&data), "unsupported_hdr");
    let (last, length) = *packets("pq-static-config").last().unwrap();
    let mut changed = original.clone();
    let mut metadata = original[cll..cll + 8].to_vec();
    metadata[4] ^= 1;
    changed[last..last + length].copy_from_slice(&padded(metadata, length));
    assert_eq!(refusal(&changed), "stream_changed");
}

fn pixel(frame: &deadpan_source::DecodedRgbaFrame, x: usize, y: usize) -> [u16; 4] {
    let at = y * frame.row_stride_bytes + x * 8;
    std::array::from_fn(|c| {
        u16::from_le_bytes([frame.rgba[at + c * 2], frame.rgba[at + c * 2 + 1]])
    })
}

#[test]
fn main_profiles_color_grain_superres_and_both_containers_preserve_exact_pictures() {
    for &name in CASES {
        let depth = if name.starts_with("sdr-8") { 8 } else { 10 };
        let full = name.ends_with("-full");
        let hdr = matches!(name, "pq" | "hlg");
        let raw = std::fs::read(fixture(
            name,
            if depth == 8 { "yuv420p" } else { "yuv420p10le" },
        ))
        .unwrap();
        let count = if matches!(name, "grain" | "multigop") {
            36
        } else {
            12
        };
        let ordinals: Vec<i64> = (0..count)
            .filter(|n| name != "vfr" || ![4, 7].contains(n))
            .collect();
        let step = if depth == 8 { 1 } else { 2 };
        assert_eq!(raw.len(), ordinals.len() * 128 * 96 * 3 / 2 * step);
        let scale = f64::from(1 << (depth - 8));
        let (offset, range, c_range) = if full {
            (
                0.0,
                f64::from((1 << depth) - 1),
                f64::from((1 << depth) - 1),
            )
        } else {
            (16.0 * scale, 219.0 * scale, 224.0 * scale)
        };
        let (kr, kb) = if hdr {
            (0.2627, 0.0593)
        } else {
            (0.2126, 0.0722)
        };
        let mut mp4_reference = Vec::new();
        for suffix in ["mp4", "webm"] {
            let mut decoder = open(name, suffix, 1);
            let info = decoder.info();
            assert_eq!(info.codec, "av1");
            assert_eq!(
                info.pixel_format,
                if depth == 8 { "yuv420p" } else { "yuv420p10le" }
            );
            assert_eq!((info.width, info.height), (128, 96));
            assert_eq!(
                info.color.range,
                if full {
                    ColorRange::Full
                } else {
                    ColorRange::Limited
                }
            );
            assert_eq!(
                info.color.transfer,
                match name {
                    "pq" => ColorTransfer::Pq,
                    "hlg" => ColorTransfer::Hlg,
                    _ => ColorTransfer::Bt709,
                }
            );
            assert_eq!(
                (info.sample_aspect_num, info.sample_aspect_den),
                if name == "anamorphic" { (3, 2) } else { (1, 1) }
            );
            assert!(!info.bwdif_fields);
            assert_eq!(
                info.nominal_frame_duration_ns,
                if suffix == "webm" {
                    Some(33_366_666)
                } else {
                    None
                }
            );
            let mut reference = Vec::new();
            for (index, &ordinal) in ordinals.iter().enumerate() {
                let frame = decoder.next_rgba16(control()).unwrap().unwrap();
                let pts = if suffix == "mp4" {
                    ordinal * 2002
                } else {
                    (ordinal * 1001 + 15) / 30
                };
                assert_eq!(frame.metadata.pts, pts, "{name}.{suffix} picture {index}");
                let delta = ordinals.get(index + 1).copied().unwrap_or(count) - ordinal;
                let duration = if suffix == "mp4" {
                    delta * 2002
                } else {
                    (delta * 1001 + 15) / 30
                };
                assert_eq!(
                    frame.metadata.reported_duration,
                    Some(duration),
                    "{name}.{suffix}"
                );
                assert_eq!(frame.sample_bits, 16);
                assert_eq!(
                    frame.metadata.keyframe,
                    ordinal % 16 == 0,
                    "{name}.{suffix} key {ordinal}"
                );
                let at = index * 128 * 96 * 3 / 2 * step;
                let code = |plane: usize, x: usize, y: usize| {
                    let n = if plane == 0 {
                        y * 128 + x
                    } else {
                        128 * 96 + (plane - 1) * 64 * 48 + y * 64 + x
                    };
                    if depth == 8 {
                        f64::from(raw[at + n])
                    } else {
                        f64::from(u16::from_le_bytes([raw[at + n * 2], raw[at + n * 2 + 1]]))
                    }
                };
                // Independent scalar 4:2:0 reconstruction at the declared
                // left siting, from retained planar decoder output.
                for y in 0..96 {
                    let cy = (y as f64 - if name == "topleft" { 0.0 } else { 0.5 }).max(0.0) / 2.0;
                    let y0 = (cy.floor() as usize).min(47);
                    let y1 = (y0 + 1).min(47);
                    let wy = cy - cy.floor();
                    for x in 0..128 {
                        let cx = x as f64 / 2.0;
                        let x0 = cx.floor() as usize;
                        let x1 = (x0 + 1).min(63);
                        let wx = cx - cx.floor();
                        let chroma = |p| {
                            let top = code(p, x0, y0) * (1.0 - wx) + code(p, x1, y0) * wx;
                            let bottom = code(p, x0, y1) * (1.0 - wx) + code(p, x1, y1) * wx;
                            (top * (1.0 - wy) + bottom * wy - 128.0 * scale) / c_range
                        };
                        let luma = (code(0, x, y) - offset) / range;
                        let red = luma + 2.0 * (1.0 - kr) * chroma(2);
                        let blue = luma + 2.0 * (1.0 - kb) * chroma(1);
                        let green = (luma - kr * red - kb * blue) / (1.0 - kr - kb);
                        let actual = pixel(&frame, x, y);
                        assert_eq!(actual[3], 65535);
                        for (c, expected) in [red, green, blue].into_iter().enumerate() {
                            let expected = (expected.clamp(0.0, 1.0) * 65535.0).round();
                            assert!(
                                (f64::from(actual[c]) - expected).abs() <= 1.0,
                                "{name}.{suffix} {ordinal} {x},{y} c{c}: {} vs {expected}",
                                actual[c]
                            );
                        }
                    }
                }
                if !matches!(name, "grain" | "superres") {
                    // Independent authored patch. The lossy multi-GOP encoder
                    // adds a measured small chroma bias after its second key.
                    let expected =
                        ((81.0 * scale + (ordinal % 4) as f64 - offset) / range * 65535.0).round();
                    assert!(
                        (f64::from(pixel(&frame, 40, 8)[0]) - expected).abs()
                            <= if name == "multigop" { 1101.0 } else { 300.0 },
                        "{name}.{suffix} authored picture {ordinal}"
                    );
                }
                if suffix == "webm" {
                    assert_eq!(frame.rgba, mp4_reference[index], "{name} container parity");
                }
                reference.push(frame);
            }
            assert!(decoder.next_rgba16(control()).unwrap().is_none());
            assert!(decoder.work().decoded_pictures >= ordinals.len() as u64);
            for threads in [8, 16] {
                let mut decoder = open(name, suffix, threads);
                for expected in &reference {
                    assert_eq!(&decoder.next_rgba16(control()).unwrap().unwrap(), expected);
                }
                for expected in reference.iter().rev() {
                    decoder.seek(expected.metadata.pts, control()).unwrap();
                    loop {
                        let actual = decoder.next_rgba16(control()).unwrap().unwrap();
                        if actual.metadata.pts == expected.metadata.pts {
                            assert_eq!(&actual, expected, "{name}.{suffix} threads {threads}");
                            break;
                        }
                        assert!(actual.metadata.pts < expected.metadata.pts);
                    }
                }
            }
            if suffix == "mp4" {
                mp4_reference = reference.into_iter().map(|f| f.rgba).collect();
            }
        }
    }
}
