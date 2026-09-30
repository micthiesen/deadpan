use super::*;
use std::io::Write;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

static CANCELLED: AtomicBool = AtomicBool::new(false);
const IDENTITY: [i32; 9] = [65_536, 0, 0, 0, 65_536, 0, 0, 0, 1 << 30];

fn control() -> DecodeControl<'static> {
    DecodeControl {
        timeout: Duration::from_secs(10),
        cancelled: &CANCELLED,
    }
}

fn file(bytes: &[u8]) -> File {
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(bytes).unwrap();
    file
}

fn word(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_be_bytes());
}

fn wide_word(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_be_bytes());
}

fn words(values: &[u32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_be_bytes())
        .collect()
}

fn atom(tag: &[u8; 4], body: Vec<u8>) -> Vec<u8> {
    [
        words(&[u32::try_from(body.len() + 8).unwrap()]),
        tag.to_vec(),
        body,
    ]
    .concat()
}

fn descriptor(tag: u8, bytes: Vec<u8>) -> Vec<u8> {
    [vec![tag, u8::try_from(bytes.len()).unwrap()], bytes].concat()
}

fn description(video: bool) -> Vec<u8> {
    let mut body = vec![0; if video { 78 } else { 28 }];
    body[7] = 1;
    if video {
        body[24..28].copy_from_slice(&[0, 2, 0, 2]);
        body.extend(atom(
            b"avcC",
            vec![1, 100, 0, 40, 0xff, 0xe1, 0, 1, 0x67, 1, 0, 1, 0x68],
        ));
        body.extend(atom(
            b"colr",
            [b"nclx".to_vec(), vec![0, 1, 0, 1, 0, 1, 0]].concat(),
        ));
        body.extend(atom(b"pasp", words(&[1, 1])));
    } else {
        body[16..20].copy_from_slice(&[0, 2, 0, 16]);
        word(&mut body, 24, 48_000 << 16);
        let config = [
            vec![0x40, 0x15],
            vec![0; 11],
            descriptor(5, vec![0x11, 0x90]),
        ]
        .concat();
        let es = [vec![0, 1, 0], descriptor(4, config), descriptor(6, vec![2])].concat();
        body.extend(atom(b"esds", [words(&[0]), descriptor(3, es)].concat()));
    }
    atom(if video { b"avc1" } else { b"mp4a" }, body)
}

fn edit(duration: u64, media_time: i64) -> Mp4Edit {
    Mp4Edit {
        segment_duration: duration,
        media_time,
        media_rate_integer: 1,
        media_rate_fraction: 0,
    }
}

fn track(video: bool, wide: bool, offsets: &[u32], edits: &[Mp4Edit]) -> Vec<u8> {
    let mut header = vec![0; if wide { 96 } else { 84 }];
    word(&mut header, 0, (u32::from(wide) << 24) | 3);
    word(
        &mut header,
        if wide { 20 } else { 12 },
        if video { 7 } else { 19 },
    );
    let duration = if video { 24_024 } else { 8_005 };
    if wide {
        wide_word(&mut header, 28, duration);
    } else {
        word(&mut header, 20, duration as u32);
    }
    let matrix_at = if wide { 52 } else { 40 };
    for (index, value) in IDENTITY.into_iter().enumerate() {
        word(&mut header, matrix_at + index * 4, value as u32);
    }
    if video {
        word(&mut header, matrix_at + 36, 2 << 16);
        word(&mut header, matrix_at + 40, 2 << 16);
    }
    let mut mdhd = vec![0; if wide { 36 } else { 24 }];
    word(&mut mdhd, 0, u32::from(wide) << 24);
    word(
        &mut mdhd,
        if wide { 20 } else { 12 },
        if video { 30_000 } else { 48_000 },
    );
    let duration = if video { 3_003 } else { 2_625 };
    if wide {
        wide_word(&mut mdhd, 24, duration);
    } else {
        word(&mut mdhd, 16, duration as u32);
    }
    let mut hdlr = vec![0; 24];
    hdlr[8..12].copy_from_slice(if video { b"vide" } else { b"soun" });
    let mut chunks = words(&[0, u32::try_from(offsets.len()).unwrap()]);
    for offset in offsets {
        if wide {
            chunks.extend(u64::from(*offset).to_be_bytes());
        } else {
            chunks.extend(offset.to_be_bytes());
        }
    }
    let mut tables = vec![
        atom(b"stsd", [words(&[0, 1]), description(video)].concat()),
        atom(
            b"stts",
            if video {
                words(&[0, 1, 3, 1001])
            } else {
                words(&[0, 2, 2, 1024, 1, 577])
            },
        ),
        atom(
            b"stsc",
            if offsets.len() == 2 {
                words(&[0, 2, 1, 2, 1, 2, 1, 1])
            } else {
                words(&[0, 1, 1, 1, 1])
            },
        ),
        atom(b"stsz", words(&[0, if video { 6 } else { 2 }, 3])),
        atom(if wide { b"co64" } else { b"stco" }, chunks),
    ];
    if video {
        tables.push(atom(b"ctts", words(&[0, 3, 1, 1001, 1, 2002, 1, 0])));
        tables.push(atom(b"stss", words(&[0, 1, 1])));
    }
    let data = atom(
        b"dinf",
        atom(
            b"dref",
            [words(&[0, 1, 12]), b"url ".to_vec(), words(&[1])].concat(),
        ),
    );
    let minf = atom(
        b"minf",
        [
            atom(
                if video { b"vmhd" } else { b"smhd" },
                if video { words(&[1, 0, 0]) } else { vec![0; 8] },
            ),
            data,
            atom(b"stbl", tables.concat()),
        ]
        .concat(),
    );
    let mut parts = vec![atom(b"tkhd", header)];
    if !edits.is_empty() {
        let mut entries = words(&[u32::from(wide) << 24, u32::try_from(edits.len()).unwrap()]);
        for entry in edits {
            if wide {
                entries.extend(entry.segment_duration.to_be_bytes());
                entries.extend(entry.media_time.to_be_bytes());
            } else {
                entries.extend((entry.segment_duration as u32).to_be_bytes());
                entries.extend((entry.media_time as i32).to_be_bytes());
            }
            entries.extend(entry.media_rate_integer.to_be_bytes());
            entries.extend(entry.media_rate_fraction.to_be_bytes());
        }
        parts.push(atom(b"edts", atom(b"elst", entries)));
    }
    parts.push(atom(
        b"mdia",
        [atom(b"mdhd", mdhd), atom(b"hdlr", hdlr), minf].concat(),
    ));
    atom(b"trak", parts.concat())
}

// This models tables and AVC NAL headers only. These tiny payloads are not
// playable codec fixtures, and successful inspection must never claim decode.
fn fixture(wide: bool, faststart: bool, video_edits: &[Mp4Edit]) -> Vec<u8> {
    fixture_layout(wide, faststart, video_edits, false)
}

fn fixture_layout(wide: bool, faststart: bool, video_edits: &[Mp4Edit], grouped: bool) -> Vec<u8> {
    let ftyp = atom(
        b"ftyp",
        [b"isom".to_vec(), words(&[0]), b"mp41".to_vec()].concat(),
    );
    let packets = [
        vec![0, 0, 0, 2, 0x65, 0],
        vec![0, 0],
        vec![0, 0, 0, 2, 0x41, 0],
        vec![0, 0],
        vec![0, 0, 0, 2, 0x01, 0],
        vec![0, 0],
    ];
    let order = if grouped {
        [0, 2, 1, 3, 4, 5]
    } else {
        [0, 1, 2, 3, 4, 5]
    };
    let payload = order
        .into_iter()
        .flat_map(|index| packets[index].clone())
        .collect();
    let movie = |base: u32| {
        let mut header = vec![0; if wide { 112 } else { 100 }];
        word(&mut header, 0, u32::from(wide) << 24);
        word(&mut header, if wide { 20 } else { 12 }, 240_000);
        if wide {
            wide_word(&mut header, 24, 24_024);
        } else {
            word(&mut header, 16, 24_024);
        }
        for (index, value) in IDENTITY.into_iter().enumerate() {
            word(
                &mut header,
                if wide { 48 } else { 36 } + index * 4,
                value as u32,
            );
        }
        atom(
            b"moov",
            [
                atom(b"mvhd", header),
                track(
                    true,
                    wide,
                    &if grouped {
                        vec![base, base + 16]
                    } else {
                        vec![base, base + 8, base + 16]
                    },
                    video_edits,
                ),
                track(
                    false,
                    wide,
                    &[base + if grouped { 12 } else { 6 }, base + 14, base + 22],
                    &[edit(8005, 1024)],
                ),
            ]
            .concat(),
        )
    };
    let base = u32::try_from(ftyp.len() + 8).unwrap();
    let moov = movie(base);
    if faststart {
        [
            ftyp,
            movie(base + u32::try_from(moov.len()).unwrap()),
            atom(b"mdat", payload),
        ]
        .concat()
    } else {
        [ftyp, atom(b"mdat", payload), moov].concat()
    }
}

fn locate(bytes: &[u8], tag: &[u8; 4], ordinal: usize) -> usize {
    bytes
        .windows(4)
        .enumerate()
        .filter_map(|(at, value)| (value == tag).then_some(at + 4))
        .nth(ordinal)
        .unwrap()
}

fn observe(bytes: &[u8]) -> Mp4Inspection {
    inspect_mp4(&file(bytes), DecodeLimits::default(), control()).unwrap()
}

fn packets(bytes: &[u8]) -> Vec<Mp4PacketObservation> {
    let mut reader =
        Mp4PacketReader::open(file(bytes), DecodeLimits::default(), control()).unwrap();
    let mut packets = Vec::new();
    while let Some(packet) = reader.next_packet(control()).unwrap() {
        packets.push(packet);
    }
    assert!(reader.next_packet(control()).unwrap().is_none());
    packets
}

#[test]
fn movie_track_and_edit_observations_preserve_both_clock_versions_and_order() {
    for wide in [false, true] {
        for faststart in [false, true] {
            let actual = observe(&fixture(wide, faststart, &[edit(24_024, 1001)]));
            assert_eq!(actual.movie_timescale, 240_000);
            assert_eq!(actual.movie_duration, Some(24_024));
            assert_eq!(actual.movie_matrix, IDENTITY);
            assert_eq!(actual.moov_before_mdat, faststart);
            assert_eq!(actual.tracks.len(), 2);
            let video = &actual.tracks[0];
            assert_eq!(
                (video.index, video.id, video.kind),
                (0, 7, Mp4TrackKind::Video)
            );
            assert_eq!(
                (video.duration, video.media_duration),
                (Some(24_024), Some(3003))
            );
            assert_eq!(
                (
                    video.media_timescale,
                    video.sample_count,
                    video.timing_duration
                ),
                (30_000, 3, 3003)
            );
            assert_eq!(video.edits, vec![edit(24_024, 1001)]);
            assert_eq!(video.matrix, IDENTITY);
            assert_eq!(video.rotation_quarter_turns, Some(0));
            assert_eq!(video.sample_dimensions, Some([2, 2]));
            assert_eq!(
                (video.display_width_16_16, video.display_height_16_16),
                (2 << 16, 2 << 16)
            );
            assert_eq!(
                video.avc,
                Some(Mp4AvcConfiguration {
                    profile: 100,
                    compatibility: 0,
                    level: 40,
                    nal_length_bytes: 4
                })
            );
            assert_eq!(
                video.color,
                Some(Mp4ColorDescription {
                    primaries: 1,
                    transfer: 1,
                    matrix: 1,
                    full_range: false,
                    range_byte: 0
                })
            );
            assert_eq!(video.pixel_aspect_ratio, Some([1, 1]));
            let audio = &actual.tracks[1];
            assert_eq!(
                (audio.index, audio.id, audio.kind),
                (1, 19, Mp4TrackKind::Audio)
            );
            assert_eq!(
                (audio.duration, audio.media_duration),
                (Some(8005), Some(2625))
            );
            assert_eq!(
                (audio.media_timescale, audio.timing_duration),
                (48_000, 2625)
            );
            assert_eq!(audio.edits, vec![edit(8005, 1024)]);
            assert!(audio.avc.is_none());
            assert_eq!(
                (audio.sample_audio_channels, audio.sample_audio_rate),
                (Some(2), Some(48_000))
            );
        }
    }
}

#[test]
fn sample_tables_yield_exact_reordered_and_primed_clocks_without_trimming() {
    for wide in [false, true] {
        let bytes = fixture(wide, true, &[edit(24_024, 1001)]);
        let actual = packets(&bytes);
        assert_eq!(actual.len(), 6);
        for (index, packet) in actual[..3].iter().enumerate() {
            assert_eq!(
                (packet.track_index, packet.track_id, packet.sample_index),
                (0, 7, index as u32)
            );
            assert_eq!(packet.length, 6);
            assert_eq!(packet.dts, index as i64 * 1001);
            assert_eq!(packet.pts, [1001, 3003, 2002][index]);
            assert_eq!(packet.duration, 1001);
            assert_eq!(
                packet.presentation,
                Some(Mp4PresentationTime {
                    dts: [-1001, 0, 1001][index],
                    pts: [0, 2002, 1001][index]
                })
            );
            assert_eq!(packet.table_sync, index == 0);
            let nal = packet.h264.unwrap();
            assert_eq!(nal.nal_count, 1);
            assert_eq!(nal.idr_nal_count, u32::from(index == 0));
            assert_eq!(nal.non_idr_vcl_nal_count, u32::from(index != 0));
            assert_eq!(nal.nal_types, if index == 0 { 1 << 5 } else { 1 << 1 });
            assert_eq!(actual[index + 3].offset, packet.offset + 6);
        }
        for (index, packet) in actual[3..].iter().enumerate() {
            assert_eq!(
                (packet.track_index, packet.track_id, packet.sample_index),
                (1, 19, index as u32)
            );
            assert_eq!(packet.duration, [1024, 1024, 577][index]);
            assert_eq!(packet.presentation.unwrap().pts, [-1024, 0, 1024][index]);
            assert!(packet.h264.is_none());
            assert!(packet.table_sync);
        }
    }
}

#[test]
fn signed_composition_offsets_and_source_empty_edits_are_never_rounded() {
    let mut bytes = fixture(true, false, &[edit(24_024, 1001)]);
    let at = locate(&bytes, b"ctts", 0);
    word(&mut bytes, at, 1 << 24);
    word(&mut bytes, at + 20, (-1001_i32) as u32);
    assert_eq!(packets(&bytes)[1].pts, 0);
    assert_eq!(packets(&bytes)[1].presentation.unwrap().pts, -1001);

    let edits = [edit(7, -1), edit(24_024, 1001)];
    let bytes = fixture(false, false, &edits);
    assert_eq!(observe(&bytes).tracks[0].edits, edits);
    assert!(
        packets(&bytes)[..3]
            .iter()
            .all(|packet| packet.presentation.is_none())
    );
    // The original source admission still permits its existing empty-edit grammar.
    super::super::validate(
        &file(&bytes),
        Selection::Video,
        DecodeLimits::default().into(),
        control(),
    )
    .unwrap();
    let no_edit = packets(&fixture(false, false, &[]));
    assert_eq!(no_edit[0].presentation.unwrap().pts, no_edit[0].pts);
}

#[test]
fn varying_chunk_runs_and_compact_sample_sizes_preserve_packet_offsets() {
    for wide in [false, true] {
        let bytes = fixture_layout(wide, true, &[edit(24_024, 1001)], true);
        let actual = packets(&bytes);
        assert_eq!(actual[1].offset, actual[0].offset + 6);
        assert_eq!(actual[2].offset, actual[0].offset + 16);
        assert_eq!(actual[3].offset, actual[0].offset + 12);
        assert_eq!(actual[4].offset, actual[0].offset + 14);
        assert_eq!(actual[5].offset, actual[0].offset + 22);
        assert_eq!(actual[1].presentation.unwrap().pts, 2002);
    }
    // Existing source fixtures exercise actual stsz sizes. Compact tables use
    // the same admitted sample_size accessor rather than an unchecked decoder.
    let input = file(&[0x67, 0x80]);
    let mut read = reader(&input, DecodeLimits::default(), control()).unwrap();
    let sizes = Sizes {
        count: 3,
        constant: 0,
        bits: 4,
        data: Span { start: 0, end: 2 },
    };
    assert_eq!(
        [
            read.sample_size(sizes, 0).unwrap(),
            read.sample_size(sizes, 1).unwrap(),
            read.sample_size(sizes, 2).unwrap()
        ],
        [6, 7, 8]
    );
}

#[test]
fn contradictory_color_and_aspect_headers_are_retained_for_export_rejection() {
    let mut bytes = fixture(false, true, &[edit(24_024, 1001)]);
    let at = locate(&bytes, b"colr", 0);
    bytes[at + 4..at + 10].copy_from_slice(&[0, 9, 0, 16, 0, 9]);
    bytes[at + 10] = 0x81;
    let aspect = locate(&bytes, b"pasp", 0);
    word(&mut bytes, aspect, 4);
    word(&mut bytes, aspect + 4, 3);
    let actual = observe(&bytes);
    assert_eq!(
        actual.tracks[0].color,
        Some(Mp4ColorDescription {
            primaries: 9,
            transfer: 16,
            matrix: 9,
            full_range: true,
            range_byte: 0x81
        })
    );
    assert_eq!(actual.tracks[0].pixel_aspect_ratio, Some([4, 3]));
    // These are observations. Decoder/product policy must not replace them
    // with intended Rec.709 tags merely because the structural grammar admits.
}

#[test]
fn unknown_durations_and_noncanonical_matrices_remain_explicit_observations() {
    for wide in [false, true] {
        let mut bytes = fixture(wide, false, &[edit(24_024, 1001)]);
        for (tag, small, large) in [(b"mvhd", 16, 24), (b"tkhd", 20, 28), (b"mdhd", 16, 24)] {
            let at = locate(&bytes, tag, 0);
            if wide {
                wide_word(&mut bytes, at + large, u64::MAX);
            } else {
                word(&mut bytes, at + small, u32::MAX);
            }
        }
        let at = locate(&bytes, b"tkhd", 0) + if wide { 52 } else { 40 };
        word(&mut bytes, at, 131_072);
        let actual = observe(&bytes);
        assert_eq!(actual.movie_duration, None);
        assert_eq!(actual.tracks[0].duration, None);
        assert_eq!(actual.tracks[0].media_duration, None);
        assert_eq!(actual.tracks[0].matrix[0], 131_072);
        assert_eq!(actual.tracks[0].rotation_quarter_turns, None);
    }
    for (values, expected) in [
        ([0, 65_536, -65_536, 0], 1),
        ([-65_536, 0, 0, -65_536], 2),
        ([0, -65_536, 65_536, 0], 3),
    ] {
        let mut matrix = IDENTITY;
        for (index, value) in [0, 1, 3, 4].into_iter().zip(values) {
            matrix[index] = value;
        }
        assert_eq!(rotation(matrix), Some(expected));
        matrix[2] = 1;
        assert_eq!(rotation(matrix), None);
    }
}

#[test]
fn malformed_edits_clocks_matrices_and_tables_fail_before_packet_traversal() {
    for edit in [
        edit(0, 0),
        edit(1, -1),
        Mp4Edit {
            media_rate_integer: 2,
            ..edit(1, 0)
        },
        Mp4Edit {
            media_rate_fraction: 1,
            ..edit(1, 0)
        },
    ] {
        assert!(
            inspect_mp4(
                &file(&fixture(false, true, &[edit])),
                DecodeLimits::default(),
                control()
            )
            .is_err()
        );
    }
    assert!(
        inspect_mp4(
            &file(&fixture(true, true, &[edit(1, -1), edit(2, 0), edit(3, 0)])),
            DecodeLimits::default(),
            control()
        )
        .is_err()
    );
    let base = fixture(false, true, &[edit(24_024, 1001)]);
    for (tag, offset, value) in [
        (b"mvhd", 12, 0),
        (b"mdhd", 12, u32::MAX),
        (b"elst", 4, 0),
        (b"elst", 4, 3),
        (b"elst", 0, 2 << 24),
        (b"stts", 8, 4),
        (b"stts", 12, 0),
        (b"ctts", 8, 0),
        (b"ctts", 8, 2),
        (b"stss", 8, 0),
        (b"stss", 8, 4),
        (b"stsc", 8, 2),
        (b"stsc", 12, 0),
        (b"stsc", 16, 2),
        (b"stco", 8, u32::MAX),
        (b"stsz", 8, 1_000_001),
    ] {
        let mut bytes = base.clone();
        let at = locate(&bytes, tag, 0);
        word(&mut bytes, at + offset, value);
        assert!(
            inspect_mp4(&file(&bytes), DecodeLimits::default(), control()).is_err(),
            "{tag:?}+{offset}={value}"
        );
    }
    let mut bytes = base.clone();
    let at = locate(&bytes, b"tkhd", 0);
    word(&mut bytes, at - 8, 84); // Truncates the declared matrix/dimension tail.
    assert!(inspect_mp4(&file(&bytes), DecodeLimits::default(), control()).is_err());
    for prefix in [0, 7, base.len() - 1] {
        assert!(inspect_mp4(&file(&base[..prefix]), DecodeLimits::default(), control()).is_err());
    }
}

#[test]
fn nal_bounds_fail_closed_and_poison_further_traversal() {
    let base = fixture(false, true, &[edit(24_024, 1001)]);
    for packet in [
        [0, 0, 0, 0, 0x65, 0],
        [0, 0, 0, 3, 0x65, 0],
        [0, 0, 0, 1, 0x65, 0],
        [0, 0, 0, 2, 0xe5, 0],
        [0, 0, 0, 2, 0, 0],
    ] {
        let mut bytes = base.clone();
        let at = locate(&bytes, b"mdat", 0);
        bytes[at..at + 6].copy_from_slice(&packet);
        let mut reader =
            Mp4PacketReader::open(file(&bytes), DecodeLimits::default(), control()).unwrap();
        assert!(reader.next_packet(control()).is_err());
        assert!(
            reader
                .next_packet(control())
                .unwrap_err()
                .to_string()
                .contains("poisoned")
        );
    }
}

#[test]
fn limits_and_control_apply_to_admission_and_each_packet_without_renewing_state() {
    let bytes = fixture(false, true, &[edit(24_024, 1001)]);
    for limits in [
        DecodeLimits {
            max_packets: 5,
            ..DecodeLimits::default()
        },
        DecodeLimits {
            max_packet_bytes: 5,
            ..DecodeLimits::default()
        },
        DecodeLimits {
            max_packet_bytes: 32 * 1024 * 1024,
            ..DecodeLimits::default()
        },
        DecodeLimits {
            max_input_bytes: bytes.len() as u64 - 1,
            ..DecodeLimits::default()
        },
        DecodeLimits {
            max_io_bytes_per_call: 1,
            ..DecodeLimits::default()
        },
    ] {
        assert!(inspect_mp4(&file(&bytes), limits, control()).is_err());
    }
    let cancelled = AtomicBool::new(true);
    let stopped = DecodeControl {
        cancelled: &cancelled,
        ..control()
    };
    assert!(inspect_mp4(&file(&bytes), DecodeLimits::default(), stopped).is_err());
    let mut reader =
        Mp4PacketReader::open(file(&bytes), DecodeLimits::default(), control()).unwrap();
    assert!(reader.next_packet(stopped).is_err());
    assert_eq!(
        reader.next_packet(control()).unwrap().unwrap().sample_index,
        0
    );
    let expired = DecodeControl {
        timeout: Duration::from_nanos(1),
        ..control()
    };
    assert!(reader.next_packet(expired).is_err());
    assert_eq!(
        reader.next_packet(control()).unwrap().unwrap().sample_index,
        1
    );
    // Tables remain in the bounded page cache. Five positional bytes suffice
    // for this six-byte packet's NAL length and header, without its payload.
    reader.limits.max_io_bytes_per_call = 5;
    assert_eq!(
        reader.next_packet(control()).unwrap().unwrap().sample_index,
        2
    );
    let mut reader =
        Mp4PacketReader::open(file(&bytes), DecodeLimits::default(), control()).unwrap();
    reader.limits.max_io_bytes_per_call = 4;
    assert!(reader.next_packet(control()).is_err());
}

#[test]
fn actual_registered_fixture_tables_are_observed_without_changing_import_admission() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cfr-bframes.mp4");
    let file = File::open(path).unwrap();
    let expected = inspect_mp4(&file, DecodeLimits::default(), control()).unwrap();
    super::super::validate(
        &file,
        Selection::Video,
        DecodeLimits::default().into(),
        control(),
    )
    .unwrap();
    let mut reader = Mp4PacketReader::open(file, DecodeLimits::default(), control()).unwrap();
    assert_eq!(reader.inspection(), &expected);
    let mut counts = vec![0; expected.tracks.len()];
    let mut durations = vec![0; expected.tracks.len()];
    let mut idr = 0;
    while let Some(packet) = reader.next_packet(control()).unwrap() {
        let index = packet.track_index as usize;
        assert_eq!(packet.sample_index, counts[index]);
        assert_eq!(packet.dts as u64, durations[index]);
        counts[index] += 1;
        durations[index] += u64::from(packet.duration);
        idr += packet.h264.map_or(0, |value| value.idr_nal_count);
    }
    assert!(idr > 0);
    for (index, track) in expected.tracks.iter().enumerate() {
        assert_eq!(counts[index], track.sample_count);
        assert_eq!(durations[index], track.timing_duration);
    }
}
