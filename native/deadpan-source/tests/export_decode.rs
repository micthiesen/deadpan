use deadpan_source::{
    ChromaLocation, DecodeControl, DecodeLimits, PictureType, SourceDecodeError, SourceDecoder,
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

fn fixture(name: &str) -> File {
    File::open(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
    .unwrap()
}

fn open(name: &str) -> SourceDecoder {
    SourceDecoder::open(fixture(name), DecodeLimits::default(), control()).unwrap()
}

fn assert_code(error: SourceDecodeError, expected: &str) {
    match error {
        SourceDecodeError::Native { code, message } => assert_eq!(code, expected, "{message}"),
        other => panic!("expected {expected}, received {other}"),
    }
}

#[test]
fn i420_observes_the_current_picture_without_converting_or_advancing() {
    let mut decoder = open("cfr-bframes.mp4");
    let runtime = decoder.runtime_info();
    assert_eq!(runtime.avcodec, (62 << 16) | (11 << 8) | 103);
    assert_eq!(runtime.avformat, (62 << 16) | (3 << 8) | 103);
    assert_eq!(runtime.avutil, (60 << 16) | (8 << 8) | 103);
    assert_eq!(runtime.swscale, (9 << 16) | (1 << 8) | 103);
    assert_eq!(runtime.avfilter, (11 << 16) | (4 << 8) | 103);
    assert_eq!(
        decoder.work().frames,
        1,
        "opening retains one observed picture"
    );
    let source = decoder.next_metadata(control()).unwrap().unwrap();
    let before_copy = decoder.work();
    let frame = decoder.copy_current_i420(control()).unwrap();
    assert_eq!(frame.metadata.source, source);
    assert_eq!((frame.width, frame.height), (320, 180));
    assert_eq!(frame.i420.len(), 320 * 180 * 3 / 2);
    assert_eq!(frame.metadata.picture_type, PictureType::I);
    assert_eq!(frame.metadata.chroma_location, ChromaLocation::Left);
    assert_eq!(frame.metadata.best_effort_pts, Some(source.pts));
    assert!(frame.metadata.decoder_profile > 0);
    assert!(!frame.metadata.interlaced);
    assert!(!frame.metadata.corrupt);
    assert_eq!(frame.metadata.decode_error_flags, 0);
    for aspect in [
        frame.metadata.stream_sample_aspect_ratio,
        frame.metadata.codec_sample_aspect_ratio,
        frame.metadata.frame_sample_aspect_ratio,
    ] {
        assert!(aspect.numerator == 0 || aspect.numerator == aspect.denominator);
    }
    assert_eq!(decoder.copy_current_i420(control()).unwrap(), frame);
    assert_eq!(
        decoder.copy_current_rgba(control()).unwrap().metadata,
        source
    );
    assert_eq!(decoder.copy_current_i420(control()).unwrap(), frame);
    assert_eq!(decoder.work(), before_copy);
    let next = decoder.next_i420(control()).unwrap().unwrap();
    assert_eq!(next.metadata.source.pts, source.pts + 1001);
    assert_ne!(next.i420, frame.i420);
    assert_eq!(decoder.work().frames, before_copy.frames + 1);
}

#[test]
fn fresh_idr_decoders_match_linear_planes_and_keep_cumulative_work() {
    for name in ["cfr-bframes.mp4", "offset-bframes.mp4"] {
        let mut linear = open(name);
        let mut reference = Vec::new();
        while let Some(frame) = linear.next_i420(control()).unwrap() {
            reference.push((frame.metadata, blake3::hash(&frame.i420)));
        }
        assert_eq!(reference.len(), 120);
        assert!(
            reference
                .iter()
                .any(|(frame, _)| frame.picture_type == PictureType::B)
        );
        let keys: Vec<_> = reference
            .iter()
            .enumerate()
            .filter(|(_, (frame, _))| frame.source.keyframe)
            .map(|(index, _)| index)
            .collect();
        assert!(keys.len() > 1);
        let target = reference[keys[1]].0.source.pts;
        // A one-frame limit also counts opening work. This succeeds only when
        // opening does not decode an earlier picture before the requested GOP.
        let mut fresh = SourceDecoder::open_at_keyframe(
            fixture(name),
            DecodeLimits {
                max_frames: 1,
                ..DecodeLimits::default()
            },
            control(),
            target,
        )
        .unwrap();
        let first = fresh.next_i420(control()).unwrap().unwrap();
        assert_eq!(first.metadata.source.pts, target);
        assert_eq!(fresh.work().frames, 1);
        assert_eq!(blake3::hash(&first.i420), reference[keys[1]].1);
        assert_code(fresh.next_i420(control()).unwrap_err(), "resource_limit");

        let mut previous_work = linear.work();
        for (key_index, &start) in keys.iter().enumerate() {
            let end = keys.get(key_index + 1).copied().unwrap_or(reference.len());
            linear
                .restart_at_keyframe(reference[start].0.source.pts, control())
                .unwrap();
            assert_eq!(linear.work().frames, previous_work.frames + 1);
            assert!(linear.work().packets > previous_work.packets);
            assert!(linear.work().io_bytes >= previous_work.io_bytes);
            for (expected, digest) in &reference[start..end] {
                let actual = linear.next_i420(control()).unwrap().unwrap();
                assert_eq!(actual.metadata.source.pts, expected.source.pts);
                assert_eq!(
                    actual.metadata.source.reported_duration,
                    expected.source.reported_duration
                );
                assert_eq!(actual.metadata.picture_type, expected.picture_type);
                assert_eq!(actual.metadata.source.keyframe, expected.source.keyframe);
                assert_eq!(blake3::hash(&actual.i420), *digest);
            }
            previous_work = linear.work();
        }
        // Ordinary seeks also preserve the session-wide work observations.
        linear.seek(reference[0].0.source.pts, control()).unwrap();
        assert_eq!(linear.work().frames, previous_work.frames);
        assert_eq!(linear.work().packets, previous_work.packets);
    }
}

#[test]
fn fresh_gop_rejects_a_non_key_target_and_poisoned_sessions() {
    let mut decoder = open("cfr-bframes.mp4");
    assert_code(
        decoder.restart_at_keyframe(1001, control()).unwrap_err(),
        "invalid_keyframe",
    );
    assert_code(decoder.next_i420(control()).unwrap_err(), "session_failed");
    assert_code(
        SourceDecoder::open_at_keyframe(
            fixture("cfr-bframes.mp4"),
            DecodeLimits::default(),
            control(),
            1001,
        )
        .err()
        .unwrap(),
        "invalid_keyframe",
    );
}

#[test]
fn i420_rejects_an_unqualified_pixel_format_without_an_rgb_round_trip() {
    let mut decoder = open("full709.mkv");
    assert_code(
        decoder.next_i420(control()).unwrap_err(),
        "unsupported_export_format",
    );
    assert_code(
        decoder.next_metadata(control()).unwrap_err(),
        "session_failed",
    );
}

#[test]
fn cancellation_preserves_current_picture_and_does_not_restart_the_decoder() {
    let mut decoder = open("cfr-bframes.mp4");
    let first = decoder.next_i420(control()).unwrap().unwrap();
    let work = decoder.work();
    let cancelled = AtomicBool::new(true);
    let stopped = DecodeControl {
        cancelled: &cancelled,
        ..control()
    };
    assert_code(decoder.next_i420(stopped).unwrap_err(), "cancelled");
    assert_code(
        decoder.restart_at_keyframe(0, stopped).unwrap_err(),
        "cancelled",
    );
    assert_eq!(decoder.work(), work);
    cancelled.store(false, Ordering::Relaxed);
    assert_eq!(decoder.copy_current_i420(stopped).unwrap(), first);
    decoder.restart_at_keyframe(0, stopped).unwrap();
    assert_eq!(
        decoder.next_i420(stopped).unwrap().unwrap().i420,
        first.i420
    );
}
