use deadpan_source::{
    ColorMatrix, ColorRange, ColorTransfer, DecodeControl, DecodeLimits, SourceDecodeError,
    SourceDecoder,
};
use std::{
    fs::File,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
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
fn rgb_fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../deadpan-media-worker/tests/fixtures")
        .join(name)
}
fn open(name: &str) -> SourceDecoder {
    SourceDecoder::open(
        File::open(fixture(name)).unwrap(),
        DecodeLimits::default(),
        control(),
    )
    .unwrap()
}
fn assert_code(error: SourceDecodeError, expected: &str) {
    match error {
        SourceDecodeError::Native { code, message } => assert_eq!(code, expected, "{message}"),
        SourceDecodeError::InvalidConfiguration(message) => {
            assert_eq!(expected, "invalid_configuration", "{message}");
        }
        other => panic!("{other}"),
    }
}

// Read authored digits from visible pixels independently of timestamps/hashes.
fn authored_identity(rgba: &[u8]) -> usize {
    const DIGITS: [[u8; 7]; 10] = [
        [14, 17, 19, 21, 25, 17, 14],
        [4, 12, 4, 4, 4, 4, 14],
        [14, 17, 1, 2, 4, 8, 31],
        [30, 1, 1, 14, 1, 1, 30],
        [2, 6, 10, 18, 31, 2, 2],
        [31, 16, 16, 30, 1, 1, 30],
        [14, 16, 16, 30, 17, 17, 14],
        [31, 1, 2, 4, 8, 8, 8],
        [14, 17, 17, 14, 17, 17, 14],
        [14, 17, 17, 15, 1, 1, 14],
    ];
    let mut number = 0;
    for place in 0..3 {
        let mut rows = [0_u8; 7];
        for (row, bits) in rows.iter_mut().enumerate() {
            for column in 0..5 {
                let x = 96 + place * 42 + column * 6 + 3;
                let y = 16 + row * 6 + 3;
                if rgba[(y * 320 + x) * 4] > 162 {
                    *bits |= 1 << (4 - column);
                }
            }
        }
        number = number * 10
            + DIGITS
                .iter()
                .position(|digit| *digit == rows)
                .expect("visible authored digit");
    }
    number
}

#[test]
fn original_bframe_pts_vfr_and_offset_survive_persistent_reverse_seeks() {
    for (name, variable, origin) in [
        ("cfr-bframes.mp4", false, 0),
        ("vfr.mp4", true, 0),
        ("offset-bframes.mp4", false, 60_060),
    ] {
        let mut decoder = open(name);
        assert_eq!(
            (decoder.info().time_base_num, decoder.info().time_base_den),
            (1, 30_000)
        );
        assert_eq!((decoder.info().width, decoder.info().height), (320, 180));
        assert_eq!(decoder.info().color.range, ColorRange::Limited);
        assert_eq!(decoder.info().color.matrix, ColorMatrix::Bt709);
        assert_eq!(decoder.info().color.transfer, ColorTransfer::Bt709);
        let mut expected_pts = origin;
        let mut frames = Vec::new();
        let mut retained = None;
        while let Some(meta) = decoder.next_metadata(control()).unwrap() {
            let ordinal = frames.len();
            assert_eq!(meta.pts, expected_pts, "{name} frame {ordinal}");
            expected_pts += 1001
                * if variable {
                    1 + (ordinal % 3) as i64
                } else {
                    1
                };
            let pixels = decoder.copy_current_rgba(control()).unwrap();
            assert_eq!(pixels.metadata, meta);
            assert_eq!(pixels.rgba.len(), 320 * 180 * 4);
            assert!(pixels.rgba.chunks_exact(4).all(|pixel| pixel[3] == 255));
            assert_eq!(authored_identity(&pixels.rgba), ordinal);
            frames.push((meta, blake3::hash(&pixels.rgba)));
            if ordinal == 0 {
                retained = Some(pixels);
            }
        }
        assert_eq!(frames.len(), 120, "{name}");
        assert!(frames.iter().any(|(frame, _)| frame.keyframe));
        assert!(
            frames
                .iter()
                .any(|(frame, _)| frame.decode_timestamp.is_some())
        );
        assert_eq!(frames.last().unwrap().0.reported_duration, Some(1001));
        assert!(decoder.next_metadata(control()).unwrap().is_none());
        for target in [119, 0, 60, 1, 30, 118, 16, 29, 61, 5, 119, 0]
            .into_iter()
            .chain((0..120).rev())
        {
            decoder.seek(frames[target].0.pts, control()).unwrap();
            let mut decoded = 0;
            loop {
                decoded += 1;
                assert!(decoded <= 120);
                let meta = decoder
                    .next_metadata(control())
                    .unwrap()
                    .expect("seek target exists");
                assert!(
                    meta.pts <= frames[target].0.pts,
                    "seek must not skip target"
                );
                if meta.pts == frames[target].0.pts {
                    assert_eq!(
                        blake3::hash(&decoder.copy_current_rgba(control()).unwrap().rgba),
                        frames[target].1
                    );
                    break;
                }
            }
        }
        drop(decoder);
        let retained = retained.unwrap();
        assert_eq!(blake3::hash(&retained.rgba), frames[0].1);
        assert_eq!(retained.metadata.pts, origin);
    }
}

#[test]
fn full_range_rgb_is_exact_and_pixels_outlive_session_reuse() {
    let mut decoder = SourceDecoder::open(
        File::open(rgb_fixture("rgb30_30000_1001.mp4")).unwrap(),
        DecodeLimits::default(),
        control(),
    )
    .unwrap();
    assert_eq!(decoder.info().color.matrix, ColorMatrix::Rgb);
    assert_eq!(decoder.info().color.range, ColorRange::Full);
    assert_eq!(decoder.info().color.transfer, ColorTransfer::Srgb);
    let first = decoder.next_rgba(control()).unwrap().unwrap();
    for frame in 0..30_usize {
        let current = if frame == 0 {
            first.clone()
        } else {
            decoder.next_rgba(control()).unwrap().unwrap()
        };
        for y in 0..2_usize {
            for x in 0..4_usize {
                assert_eq!(
                    &current.rgba[(y * 4 + x) * 4..(y * 4 + x + 1) * 4],
                    &[
                        ((17 * frame + 31 * x + 7 * y + 3) % 256) as u8,
                        ((29 * frame + 5 * x + 47 * y + 11) % 256) as u8,
                        ((43 * frame + 13 * x + 19 * y + 23) % 256) as u8,
                        255
                    ]
                );
            }
        }
    }
    assert!(decoder.next_rgba(control()).unwrap().is_none());
    decoder.seek(first.metadata.pts, control()).unwrap();
    assert_eq!(decoder.next_rgba(control()).unwrap().unwrap(), first);
    drop(decoder);
    assert_eq!(first.rgba[0..4], [3, 11, 23, 255]);
}

#[test]
fn pixel_budget_rejects_sources_before_frame_decode() {
    for (max_pixels, expected) in [
        (0, "invalid_configuration"),
        (8192 * 8192 + 1, "invalid_configuration"),
        (1024, "resource_limit"),
    ] {
        let error = SourceDecoder::open(
            File::open(fixture("cfr-bframes.mp4")).unwrap(),
            DecodeLimits {
                max_pixels,
                ..DecodeLimits::default()
            },
            control(),
        )
        .err()
        .unwrap();
        assert_code(error, expected);
    }
    // The budget includes H.264's padded coded height, not only its visible crop.
    let mut decoder = SourceDecoder::open(
        File::open(fixture("cfr-bframes.mp4")).unwrap(),
        DecodeLimits {
            max_pixels: 320 * 192,
            ..DecodeLimits::default()
        },
        control(),
    )
    .unwrap();
    let frame = decoder.next_rgba(control()).unwrap().unwrap();
    assert_eq!((frame.width, frame.height), (320, 180));
    assert!(frame.rgba.len() <= 320 * 192 * 4);
}

#[test]
fn input_interpretation_and_resource_bounds_fail_explicitly() {
    let err = SourceDecoder::open(
        File::open(rgb_fixture("rgb1_24_no_tags.mp4")).unwrap(),
        DecodeLimits::default(),
        control(),
    )
    .err()
    .unwrap();
    assert_code(err, "unsupported_transfer");
    let err = SourceDecoder::open(
        File::open(rgb_fixture("rgb1_24.mp4")).unwrap(),
        DecodeLimits {
            max_input_bytes: 1,
            ..DecodeLimits::default()
        },
        control(),
    )
    .err()
    .unwrap();
    assert_code(err, "invalid_input");
    let err = SourceDecoder::open(
        File::open(rgb_fixture("rgb1_24.mp4")).unwrap(),
        DecodeLimits {
            max_dimension: 2,
            ..DecodeLimits::default()
        },
        control(),
    )
    .err()
    .unwrap();
    assert_code(err, "resource_limit");
    let mut decoder = SourceDecoder::open(
        File::open(rgb_fixture("rgb30_30000_1001.mp4")).unwrap(),
        DecodeLimits {
            max_frames: 2,
            ..DecodeLimits::default()
        },
        control(),
    )
    .unwrap();
    decoder.next_metadata(control()).unwrap();
    decoder.next_metadata(control()).unwrap();
    assert_code(
        decoder.next_metadata(control()).unwrap_err(),
        "resource_limit",
    );
    assert_code(decoder.seek(0, control()).unwrap_err(), "session_failed");
    let mut decoder = SourceDecoder::open(
        File::open(fixture("cfr-bframes.mp4")).unwrap(),
        DecodeLimits {
            max_packets_per_frame: 1,
            ..DecodeLimits::default()
        },
        control(),
    )
    .unwrap();
    assert_code(
        decoder.next_metadata(control()).unwrap_err(),
        "resource_limit",
    );
    let file = tempfile::tempfile().unwrap();
    assert_code(
        SourceDecoder::open(file, DecodeLimits::default(), control())
            .err()
            .unwrap(),
        "invalid_input",
    );
}

#[test]
fn cancellation_before_call_preserves_session_and_invalid_timeouts_are_rejected() {
    let mut decoder = open("cfr-bframes.mp4");
    let cancelled = AtomicBool::new(true);
    let ctl = DecodeControl {
        timeout: Duration::from_secs(1),
        cancelled: &cancelled,
    };
    assert_code(decoder.next_metadata(ctl).unwrap_err(), "cancelled");
    assert_code(decoder.next_rgba(ctl).unwrap_err(), "cancelled");
    assert_code(decoder.copy_current_rgba(ctl).unwrap_err(), "cancelled");
    cancelled.store(false, Ordering::Relaxed);
    assert_eq!(decoder.next_metadata(ctl).unwrap().unwrap().pts, 0);
    cancelled.store(true, Ordering::Relaxed);
    assert_code(decoder.copy_current_rgba(ctl).unwrap_err(), "cancelled");
    cancelled.store(false, Ordering::Relaxed);
    assert_eq!(decoder.copy_current_rgba(ctl).unwrap().metadata.pts, 0);
    assert!(matches!(
        decoder.next_metadata(DecodeControl {
            timeout: Duration::ZERO,
            cancelled: &cancelled
        }),
        Err(SourceDecodeError::InvalidConfiguration(_))
    ));
    assert_eq!(decoder.next_metadata(ctl).unwrap().unwrap().pts, 1001);
}

#[test]
fn yuv_matrix_and_range_are_applied_without_relabeling_transfer() {
    let mut limited = open("limited709.mkv");
    let mut full = open("full709.mkv");
    let limited = limited.next_rgba(control()).unwrap().unwrap();
    let full = full.next_rgba(control()).unwrap().unwrap();
    assert_eq!(&limited.rgba[0..8], &[0, 0, 0, 255, 255, 255, 255, 255]);
    assert_eq!(&full.rgba[0..8], &[16, 16, 16, 255, 235, 235, 235, 255]);
    // BT.709 conversion of the exact [Y=81,U=90,V=240] limited-range sample.
    // A BT.601 relabel/conversion would incorrectly produce green ~= 0.
    assert_eq!(limited.rgba[8], 255);
    assert!((20..=28).contains(&limited.rgba[9]));
    assert_eq!(limited.rgba[10], 0);
    assert_ne!(&limited.rgba[8..12], &full.rgba[8..12]);
}

#[test]
fn stream_hdr_metadata_is_rejected_during_open_despite_sdr_transfer_tags() {
    // This encoded FFV1 fixture retains limited709.mkv's BT.709 transfer tags
    // and adds only container-level content-light metadata. Reject it during
    // open, before a caller can request metadata or RGBA from the decoder.
    let error = SourceDecoder::open(
        File::open(fixture("sdr-with-stream-hdr.mkv")).unwrap(),
        DecodeLimits::default(),
        control(),
    )
    .err()
    .expect("stream HDR metadata must fail admission before frame decode");
    assert_code(error, "unsupported_hdr");
}

#[test]
fn source_orientation_is_retained_and_unqualified_hdr_and_depth_fail() {
    let decoder = open("rotated90.mp4");
    assert_eq!(decoder.info().rotation_quarter_turns, 3);
    assert_eq!((decoder.info().width, decoder.info().height), (4, 2));
    assert_code(
        SourceDecoder::open(
            File::open(fixture("hdr-pq.mkv")).unwrap(),
            DecodeLimits::default(),
            control(),
        )
        .err()
        .unwrap(),
        "unsupported_transfer",
    );
    assert_code(
        SourceDecoder::open(
            File::open(fixture("ten-bit.mkv")).unwrap(),
            DecodeLimits::default(),
            control(),
        )
        .err()
        .unwrap(),
        "unsupported_depth",
    );
}

#[test]
fn anamorphic_samples_are_retained_and_interlace_is_rejected() {
    let mut decoder = open("anamorphic.mkv");
    assert_eq!(
        (
            decoder.info().sample_aspect_num,
            decoder.info().sample_aspect_den
        ),
        (2, 1)
    );
    decoder.next_rgba(control()).unwrap().unwrap();
    let result = SourceDecoder::open(
        File::open(fixture("interlaced.mkv")).unwrap(),
        DecodeLimits::default(),
        control(),
    );
    let error = match result {
        Err(error) => error,
        Ok(mut decoder) => decoder.next_metadata(control()).unwrap_err(),
    };
    assert_code(error, "unsupported_interlace");
}

#[test]
fn descriptor_byte_budget_and_external_playlist_fail_closed() {
    use std::io::Write;
    let error = SourceDecoder::open(
        File::open(fixture("cfr-bframes.mp4")).unwrap(),
        DecodeLimits {
            max_io_bytes_per_call: 1,
            ..DecodeLimits::default()
        },
        control(),
    )
    .err()
    .unwrap();
    assert_code(error, "resource_limit");
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(b"#EXTM3U\n#EXT-X-TARGETDURATION:10\n#EXTINF:10,\nhttps://127.0.0.1:1/external.ts\n#EXT-X-ENDLIST\n").unwrap();
    let error = SourceDecoder::open(file, DecodeLimits::default(), control())
        .err()
        .unwrap();
    assert_code(error, "ffmpeg_failure");
}
