//! HDR (PQ/HLG) source admission and decode against the committed fixtures
//! described in `generate_hdr_fixtures.py`.

use deadpan_source::{
    ColorMatrix, ColorPrimaries, ColorRange, ColorTransfer, ContentLight, DecodeControl,
    DecodeLimits, MasteringDisplay, Mp4PacketReader, PictureType, SourceDecodeError, SourceDecoder,
    inspect_mp4,
};
use std::{fs::File, io::Write, path::PathBuf, sync::atomic::AtomicBool, time::Duration};

static CANCELLED: AtomicBool = AtomicBool::new(false);
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
fn open(name: &str) -> SourceDecoder {
    SourceDecoder::open(
        File::open(fixture(name)).unwrap(),
        DecodeLimits::default(),
        control(),
    )
    .unwrap_or_else(|error| panic!("{name}: {error}"))
}
fn code(error: SourceDecodeError) -> String {
    match error {
        SourceDecodeError::Native { code, .. } => code,
        other => panic!("unexpected error: {other}"),
    }
}
fn open_error(file: File) -> String {
    match SourceDecoder::open(file, DecodeLimits::default(), control()) {
        Err(error) => code(error),
        Ok(mut decoder) => loop {
            match decoder.next_metadata(control()) {
                Err(error) => break code(error),
                Ok(Some(_)) => {}
                Ok(None) => panic!("source decoded completely"),
            }
        },
    }
}

const MASTERING: MasteringDisplay = MasteringDisplay {
    primaries: [[34_000, 16_000], [13_250, 34_500], [7_500, 3_000]],
    white_point: [15_635, 16_450],
    max_luminance: 10_000_000,
    min_luminance: 1,
};
const LIGHT: ContentLight = ContentLight {
    max_cll: 1000,
    max_fall: 400,
};

/// The generator's 64x36 ten-bit planes for frame `f` with middle patch `patch`.
fn small_planes(patch: u16, f: usize) -> Vec<u16> {
    let (w, h) = (64, 36);
    let mut planes = Vec::new();
    for y in 0..h {
        for x in 0..w {
            planes.push(if y < 24 {
                [64, patch, 400, 940][x / 16]
            } else {
                64 + ((12 * x + 37 * f) % 877) as u16
            });
        }
    }
    for c in 0..2 {
        for y in 0..h / 2 {
            for x in 0..w / 2 {
                planes.push(if y < 12 {
                    [512, 512, [450, 600][c], 512][(2 * x) / 16]
                } else {
                    512
                });
            }
        }
    }
    planes
}

/// Independent BT.2020 NCL limited-range expectation, full-range 16-bit.
fn expected_rgb16(y: u16, cb: u16, cr: u16) -> [f64; 3] {
    let (kr, kb) = (0.2627, 0.0593);
    let luma = (f64::from(y) - 64.0) / 876.0;
    let blue_difference = (f64::from(cb) - 512.0) / 896.0;
    let red_difference = (f64::from(cr) - 512.0) / 896.0;
    let r = luma + 2.0 * (1.0 - kr) * red_difference;
    let b = luma + 2.0 * (1.0 - kb) * blue_difference;
    let g = (luma - kr * r - kb * b) / (1.0 - kr - kb);
    [r, g, b].map(|value| (value * 65535.0).clamp(0.0, 65535.0))
}

fn pixel16(frame: &deadpan_source::DecodedRgbaFrame, x: usize, y: usize) -> [u16; 4] {
    let at = y * frame.row_stride_bytes + x * 8;
    std::array::from_fn(|c| {
        u16::from_le_bytes([frame.rgba[at + 2 * c], frame.rgba[at + 2 * c + 1]])
    })
}

#[test]
fn pq_and_hlg_hevc_main10_are_admitted_with_exact_interpretation_and_metadata() {
    for (name, transfer, mastering, light) in [
        (
            "hevc-pq.mp4",
            ColorTransfer::Pq,
            Some(MASTERING),
            Some(LIGHT),
        ),
        ("hevc-hlg.mp4", ColorTransfer::Hlg, None, None),
        (
            "hdr-pq-av.mp4",
            ColorTransfer::Pq,
            Some(MASTERING),
            Some(LIGHT),
        ),
        ("hdr-hlg-av.mp4", ColorTransfer::Hlg, None, None),
    ] {
        let decoder = open(name);
        let info = decoder.info();
        assert_eq!(info.codec, "hevc", "{name}");
        assert_eq!(info.pixel_format, "yuv420p10le", "{name}");
        assert_eq!(info.color.range, ColorRange::Limited);
        assert_eq!(info.color.matrix, ColorMatrix::Bt2020NonConstant);
        assert_eq!(info.color.primaries, ColorPrimaries::Bt2020);
        assert_eq!(info.color.transfer, transfer);
        assert_eq!(info.color.mastering, mastering, "{name}");
        assert_eq!(info.color.content_light, light, "{name}");
    }
    let av = open("hdr-pq-av.mp4");
    assert_eq!((av.info().width, av.info().height), (320, 180));
    assert_eq!(av.info().audio_streams.len(), 1);
    assert_eq!(av.info().audio_streams[0].codec, "aac");
    let high10 = open("h264-high10-pq.mp4");
    assert_eq!(high10.info().codec, "h264");
    assert_eq!(high10.info().pixel_format, "yuv420p10le");
    assert_eq!(high10.info().color.transfer, ColorTransfer::Pq);
    assert_eq!(high10.info().color.mastering, None);
}

#[test]
fn lossless_hevc_ten_bit_planes_are_returned_exactly_with_export_metadata() {
    for (name, patch) in [("hevc-pq.mp4", 573), ("hevc-hlg.mp4", 721)] {
        let mut decoder = open(name);
        let mut types = Vec::new();
        for f in 0..8 {
            let frame = decoder.next_yuv420p10(control()).unwrap().unwrap();
            assert_eq!((frame.width, frame.height), (64, 36));
            assert_eq!(frame.samples, small_planes(patch, f), "{name} frame {f}");
            assert_eq!(frame.metadata.decoder_profile, 2, "HEVC Main10");
            assert_eq!(
                frame.metadata.chroma_location,
                deadpan_source::ChromaLocation::Left
            );
            assert_eq!(decoder.copy_current_yuv420p10(control()).unwrap(), frame);
            types.push(frame.metadata.picture_type);
        }
        assert!(decoder.next_yuv420p10(control()).unwrap().is_none());
        assert_eq!(
            types,
            [
                PictureType::I,
                PictureType::P,
                PictureType::P,
                PictureType::P
            ]
            .repeat(2)
        );
    }
}

#[test]
fn sixteen_bit_rgba_matches_an_independent_bt2020_expectation() {
    // Flat patch interiors avoid chroma interpolation. The sixteen-bit path
    // computes in double precision and rounds once, so lossless sources agree
    // within half a code; 1/65535 leaves margin for the last f64 bit. The H.264
    // qp-1 fixture may move a ten-bit code by one (75/65535 per code).
    let mut worst = 0.0_f64;
    for (name, patch, exact) in [
        ("hevc-pq.mp4", 573, true),
        ("hevc-hlg.mp4", 721, true),
        ("h264-high10-pq.mp4", 573, false),
    ] {
        let mut decoder = open(name);
        let frame = decoder.next_rgba16(control()).unwrap().unwrap();
        assert_eq!(frame.sample_bits, 16);
        assert_eq!(frame.row_stride_bytes, 64 * 8);
        assert_eq!(frame.rgba.len(), 64 * 36 * 8);
        let eight = decoder.copy_current_rgba(control()).unwrap();
        assert_eq!(eight.sample_bits, 8);
        assert_eq!(decoder.copy_current_rgba16(control()).unwrap(), frame);
        let planes = small_planes(patch, 0);
        for (column, (y, cb, cr)) in [
            (64, 512, 512),
            (patch, 512, 512),
            (400, 450, 600),
            (940, 512, 512),
        ]
        .into_iter()
        .enumerate()
        {
            let expected = expected_rgb16(y, cb, cr);
            for yy in 2..20 {
                for xx in column * 16 + 3..column * 16 + 13 {
                    let actual = pixel16(&frame, xx, yy);
                    assert_eq!(actual[3], 65535);
                    let tolerance = if exact { 1.0 } else { 2.0 * 65535.0 / 876.0 };
                    for c in 0..3 {
                        let error = (f64::from(actual[c]) - expected[c]).abs();
                        if exact {
                            worst = worst.max(error);
                        }
                        assert!(
                            error <= tolerance,
                            "{name} patch {column} ({xx},{yy}) channel {c}: {} vs {}",
                            actual[c],
                            expected[c]
                        );
                        let at = (yy * 64 + xx) * 4 + c;
                        assert!((i32::from(eight.rgba[at]) - i32::from(actual[c] >> 8)).abs() <= 1);
                    }
                }
            }
            // The ten-bit source planes agree with the decoded picture.
            if exact {
                assert_eq!(planes[2 * 64 + column * 16 + 3], y);
            }
        }
    }
    eprintln!("maximum sixteen-bit error against f64: {worst:.3}/65535");
}

#[test]
fn eight_bit_sources_expand_to_sixteen_bits_without_changing_rgba8() {
    let mut decoder = open("limited709.mkv");
    let eight = decoder.next_rgba(control()).unwrap().unwrap();
    let sixteen = decoder.copy_current_rgba16(control()).unwrap();
    assert_eq!((eight.sample_bits, sixteen.sample_bits), (8, 16));
    assert_eq!(&eight.rgba[0..8], &[0, 0, 0, 255, 255, 255, 255, 255]);
    for (index, value) in eight.rgba.iter().enumerate() {
        let wide = u16::from_le_bytes([sixteen.rgba[2 * index], sixteen.rgba[2 * index + 1]]);
        assert!((i32::from(*value) - i32::from(wide >> 8)).abs() <= 1);
    }
}

#[test]
fn hevc_fresh_keyframe_restart_requires_an_exact_irap() {
    let mut decoder = open("hevc-pq.mp4");
    let mut pts = Vec::new();
    while let Some(meta) = decoder.next_metadata(control()).unwrap() {
        pts.push(meta);
    }
    assert_eq!(pts.len(), 8);
    let key = pts[4];
    assert!(key.keyframe);
    decoder.restart_at_keyframe(key.pts, control()).unwrap();
    let frame = decoder.next_yuv420p10(control()).unwrap().unwrap();
    assert_eq!(frame.metadata.source.pts, key.pts);
    assert_eq!(frame.samples, small_planes(573, 4));
    let fresh = SourceDecoder::open_at_keyframe(
        File::open(fixture("hevc-pq.mp4")).unwrap(),
        DecodeLimits::default(),
        control(),
        key.pts,
    )
    .unwrap();
    assert_eq!(fresh.info().color.mastering, Some(MASTERING));
    let error = decoder
        .restart_at_keyframe(pts[5].pts, control())
        .unwrap_err();
    assert_eq!(code(error), "invalid_keyframe");
}

#[test]
fn av_fixtures_decode_every_picture_and_seek_through_b_frames() {
    for name in ["hdr-pq-av.mp4", "hdr-hlg-av.mp4"] {
        let mut decoder = open(name);
        let mut frames = Vec::new();
        while let Some(frame) = decoder.next_yuv420p10(control()).unwrap() {
            frames.push(frame);
        }
        assert_eq!(frames.len(), 60, "{name}");
        // The 1 s marker exists only in frame 30; lossy coding keeps it bright.
        let marker = |frame: &deadpan_source::DecodedYuv420p10Frame| frame.samples[164 * 320 + 304];
        assert!(marker(&frames[30]) > 900, "{name}");
        assert!(
            marker(&frames[29]) < 900 && marker(&frames[31]) < 900,
            "{name}"
        );
        decoder
            .seek(frames[45].metadata.source.pts, control())
            .unwrap();
        loop {
            let frame = decoder.next_yuv420p10(control()).unwrap().unwrap();
            if frame.metadata.source.pts == frames[45].metadata.source.pts {
                assert_eq!(frame.samples, frames[45].samples);
                break;
            }
        }
    }
}

#[test]
fn unqualified_hdr_and_depth_combinations_fail_with_explicit_codes() {
    for (name, expected) in [
        ("hevc-ten-bit-sdr.mp4", "unsupported_depth"),
        ("hevc-pq-bt709.mp4", "unsupported_primaries"),
        ("hevc-pq-mastering-change.mp4", "stream_changed"),
        ("hdr-pq.mkv", "unsupported_primaries"),
        ("ten-bit.mkv", "unsupported_depth"),
        ("sdr-with-stream-hdr.mkv", "unsupported_hdr"),
    ] {
        assert_eq!(
            open_error(File::open(fixture(name)).unwrap()),
            expected,
            "{name}"
        );
    }
    // The mastering change is detected at the second segment's first picture.
    let mut decoder = open("hevc-pq-mastering-change.mp4");
    assert_eq!(decoder.info().color.mastering, Some(MASTERING));
    decoder.next_metadata(control()).unwrap().unwrap();
    decoder.next_metadata(control()).unwrap().unwrap();
    assert_eq!(
        code(decoder.next_metadata(control()).unwrap_err()),
        "stream_changed"
    );
}

fn rewritten(name: &str, edit: impl FnOnce(&mut Vec<u8>)) -> File {
    let mut bytes = std::fs::read(fixture(name)).unwrap();
    edit(&mut bytes);
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(&bytes).unwrap();
    file
}
fn find(bytes: &[u8], tag: &[u8]) -> usize {
    bytes.windows(tag.len()).position(|w| w == tag).unwrap()
}

#[test]
fn hev1_in_band_parameter_sets_and_reserved_nal_units_are_refused() {
    let hev1 = rewritten("hevc-pq.mp4", |bytes| {
        let at = find(bytes, b"hvc1");
        bytes[at..at + 4].copy_from_slice(b"hev1");
    });
    assert_eq!(open_error(hev1), "unsupported_codec");
    // The first sample begins with a 9-byte content-light SEI NAL (type 39).
    // Relabel only its two-byte header; lengths and tables stay valid.
    for (header, expected) in [
        ([0x44, 0x01], "stream_changed"),    // PPS differing from hvcC
        ([0x40, 0x01], "stream_changed"),    // VPS differing from hvcC
        ([0x7c, 0x01], "unsupported_hdr"),   // type 62, Dolby Vision RPU
        ([0x52, 0x01], "unsupported_codec"), // type 41, reserved
        ([0x4e, 0x09], "unsupported_codec"), // nuh_layer_id 1
        ([0x4e, 0x00], "unsupported_codec"), // temporal id plus one 0
    ] {
        let changed = rewritten("hevc-pq.mp4", |bytes| {
            let first = find(bytes, b"mdat") + 4;
            assert_eq!(&bytes[first..first + 6], &[0, 0, 0, 9, 0x4e, 0x01]);
            bytes[first + 4..first + 6].copy_from_slice(&header);
        });
        assert_eq!(open_error(changed), expected, "{header:x?}");
    }
    // A byte-identical in-band PPS repetition is admitted: replace the 30-byte
    // mastering SEI by the hvcC PPS plus a filler-data NAL of the same extent.
    let identical = rewritten("hevc-pq.mp4", |bytes| {
        let pps = {
            let at = find(bytes, &[0xa2, 0, 1, 0, 6]) + 5;
            bytes[at..at + 6].to_vec()
        };
        let second = find(bytes, b"mdat") + 4 + 13;
        assert_eq!(&bytes[second..second + 6], &[0, 0, 0, 30, 0x4e, 0x01]);
        let mut replacement = vec![0, 0, 0, 6];
        replacement.extend(pps);
        replacement.extend([0, 0, 0, 20, 0x4c, 0x01]);
        replacement.extend([0xff; 17]);
        replacement.push(0x80);
        bytes[second..second + 34].copy_from_slice(&replacement);
    });
    let mut decoder = SourceDecoder::open(identical, DecodeLimits::default(), control()).unwrap();
    let mut frames = 0;
    while let Some(frame) = decoder.next_yuv420p10(control()).unwrap() {
        assert_eq!(frame.samples, small_planes(573, frames));
        frames += 1;
    }
    assert_eq!(frames, 8);
}

#[test]
fn hevc_container_inspection_reports_configuration_static_metadata_and_irap_packets() {
    let limits = DecodeLimits::default();
    let inspection = inspect_mp4(
        &File::open(fixture("hevc-pq.mp4")).unwrap(),
        limits,
        control(),
    )
    .unwrap();
    let track = &inspection.tracks[0];
    assert_eq!(track.avc, None);
    let hevc = track.hevc.unwrap();
    assert_eq!(
        (
            hevc.profile_space,
            hevc.profile_idc,
            hevc.chroma_format_idc,
            hevc.bit_depth_luma,
            hevc.bit_depth_chroma,
            hevc.nal_length_bytes,
        ),
        (0, 2, 1, 10, 10, 4)
    );
    assert_eq!((hevc.vps_count, hevc.sps_count, hevc.pps_count), (1, 1, 1));
    assert_eq!(track.mastering, Some(MASTERING));
    assert_eq!(track.content_light, Some(LIGHT));
    let color = track.color.unwrap();
    assert_eq!((color.primaries, color.transfer, color.matrix), (9, 16, 9));
    let hlg = inspect_mp4(
        &File::open(fixture("hevc-hlg.mp4")).unwrap(),
        limits,
        control(),
    )
    .unwrap();
    assert_eq!(hlg.tracks[0].mastering, None);
    assert_eq!(hlg.tracks[0].color.unwrap().transfer, 18);

    let mut reader = Mp4PacketReader::open(
        File::open(fixture("hevc-pq.mp4")).unwrap(),
        limits,
        control(),
    )
    .unwrap();
    let mut packets = Vec::new();
    while let Some(packet) = reader.next_packet(control()).unwrap() {
        assert_eq!(packet.h264, None);
        packets.push(packet.hevc.unwrap());
    }
    assert_eq!(packets.len(), 8);
    for (index, packet) in packets.iter().enumerate() {
        let key = index % 4 == 0;
        assert_eq!(packet.irap_nal_count > 0, key, "packet {index}");
        assert_eq!(packet.idr_nal_count > 0, key, "packet {index}");
        assert_eq!(packet.non_irap_vcl_nal_count > 0, !key, "packet {index}");
        assert_eq!(packet.parameter_set_nal_count, 0);
    }
}

/// Open GOP: a fresh decoder at a CRA must drop its RASL pictures (they
/// reference the previous GOP), so a fresh start there cannot reproduce the
/// continuous decode. Restart and fresh opening refuse it explicitly; the IDR
/// and continuous decoding (including ordinary seeks) remain exact.
#[test]
fn hevc_fresh_restart_refuses_a_cra_with_rasl_pictures() {
    let name = "hevc-pq-open-gop.mp4";
    let mut decoder = open(name);
    let mut frames = Vec::new();
    while let Some(frame) = decoder.next_yuv420p10(control()).unwrap() {
        frames.push(frame);
    }
    assert_eq!(frames.len(), 24);
    for (f, frame) in frames.iter().enumerate() {
        assert_eq!(frame.samples, small_planes(573, f), "frame {f}");
    }
    let keys: Vec<_> = frames
        .iter()
        .enumerate()
        .filter(|(_, frame)| frame.metadata.source.keyframe)
        .map(|(f, frame)| (f, frame.metadata.source.pts))
        .collect();
    assert_eq!(keys.iter().map(|key| key.0).collect::<Vec<_>>(), [0, 8, 16]);

    // Drive a fresh decoder until it fails; it must fail before reaching the
    // end, never silently return a GOP without its leading pictures.
    let refuse = |decoder: Result<SourceDecoder, SourceDecodeError>| match decoder {
        Err(error) => code(error),
        Ok(mut decoder) => loop {
            match decoder.next_yuv420p10(control()) {
                Err(error) => break code(error),
                Ok(Some(_)) => {}
                Ok(None) => panic!("fresh CRA decode completed without its RASL pictures"),
            }
        },
    };
    for &(_, pts) in &keys[1..] {
        let mut restarted = open(name);
        let result = restarted
            .restart_at_keyframe(pts, control())
            .map(|()| restarted);
        assert_eq!(refuse(result), "invalid_keyframe");
        let fresh = SourceDecoder::open_at_keyframe(
            File::open(fixture(name)).unwrap(),
            DecodeLimits::default(),
            control(),
            pts,
        );
        assert_eq!(refuse(fresh), "invalid_keyframe");
    }

    // The IDR start is a closed GOP: fresh decoding is exact.
    let mut fresh = SourceDecoder::open_at_keyframe(
        File::open(fixture(name)).unwrap(),
        DecodeLimits::default(),
        control(),
        keys[0].1,
    )
    .unwrap();
    let mut count = 0;
    while let Some(frame) = fresh.next_yuv420p10(control()).unwrap() {
        assert_eq!(frame.samples, frames[count].samples);
        count += 1;
    }
    assert_eq!(count, 24);

    // An ordinary seek to the CRA keeps decoding from the continuous stream.
    let mut sought = open(name);
    sought.seek(keys[1].1, control()).unwrap();
    let frame = sought.next_yuv420p10(control()).unwrap().unwrap();
    assert_eq!(frame.metadata.source.pts, keys[1].1);
    assert_eq!(frame.samples, frames[8].samples);
}

/// Static metadata that fail the shared `deadpan_core` rule set are treated
/// as absent with a recorded note; the stream itself stays admissible.
#[test]
fn invalid_static_metadata_is_ignored_with_a_note_instead_of_refusing() {
    let mut decoder = open("hevc-pq-invalid-static.mp4");
    let color = decoder.info().color;
    assert_eq!(color.transfer, ColorTransfer::Pq);
    assert_eq!((color.mastering, color.content_light), (None, None));
    assert_eq!(
        color.ignored_static,
        deadpan_source::IgnoredStaticMetadata {
            mastering: true,
            content_light: true,
        }
    );
    let mut count = 0;
    while let Some(frame) = decoder.next_yuv420p10(control()).unwrap() {
        assert_eq!(frame.samples, small_planes(573, count));
        count += 1;
    }
    assert_eq!(count, 2);
    // Valid declarations carry no note, and the wrappers apply the core rules.
    assert!(open("hevc-pq.mp4").info().color.ignored_static.is_empty());
    for (shared, expected) in deadpan_core::mastering_rule_cases() {
        let local = MasteringDisplay {
            primaries: shared.primaries,
            white_point: shared.white_point,
            max_luminance: shared.max_luminance,
            min_luminance: shared.min_luminance,
        };
        assert_eq!(local.is_valid(), expected.is_ok());
    }
    for (shared, expected) in deadpan_core::content_light_rule_cases() {
        let local = ContentLight {
            max_cll: shared.max_cll,
            max_fall: shared.max_fall,
        };
        assert_eq!(local.is_valid(), expected);
    }
}
