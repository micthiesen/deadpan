//! Codec threading and non-reference preroll skipping change only latency:
//! every returned picture must equal a sequential single-threaded decode.

use deadpan_source::{DecodeControl, DecodeLimits, SourceDecodeError, SourceDecoder};
use std::{fs::File, path::PathBuf, sync::atomic::AtomicBool, time::Duration};

static CANCELLED: AtomicBool = AtomicBool::new(false);

fn control() -> DecodeControl<'static> {
    DecodeControl {
        timeout: Duration::from_secs(20),
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

fn open(name: &str, threads: u32) -> SourceDecoder {
    SourceDecoder::open(
        fixture(name),
        DecodeLimits {
            threads,
            ..DecodeLimits::default()
        },
        control(),
    )
    .unwrap()
}

#[derive(Clone, PartialEq, Eq, Debug)]
struct Picture {
    pts: i64,
    duration: Option<i64>,
    keyframe: bool,
    rgba: blake3::Hash,
}

/// One sequential pass with a single codec thread and no skipping.
fn reference(name: &str) -> Vec<Picture> {
    let mut decoder = open(name, 1);
    let mut pictures = Vec::new();
    while let Some(frame) = decoder.next_rgba(control()).unwrap() {
        pictures.push(Picture {
            pts: frame.metadata.pts,
            duration: frame.metadata.reported_duration,
            keyframe: frame.metadata.keyframe,
            rgba: blake3::hash(&frame.rgba),
        });
    }
    pictures
}

fn picture(frame: deadpan_source::DecodedRgbaFrame) -> Picture {
    Picture {
        pts: frame.metadata.pts,
        duration: frame.metadata.reported_duration,
        keyframe: frame.metadata.keyframe,
        rgba: blake3::hash(&frame.rgba),
    }
}

#[test]
fn threaded_sequential_decode_matches_single_threaded_pictures() {
    for name in [
        "cfr-bframes.mp4",
        "offset-bframes.mp4",
        "vfr.mp4",
        "pyramid-bframes.mp4",
        "fake-interlaced.mp4",
        "full709.mkv",
    ] {
        let expected = reference(name);
        for threads in [2, 8, 16] {
            let mut decoder = open(name, threads);
            let mut actual = Vec::new();
            while let Some(frame) = decoder.next_rgba(control()).unwrap() {
                actual.push(picture(frame));
            }
            assert_eq!(actual, expected, "{name} with {threads} threads");
        }
    }
}

#[test]
fn skipping_preroll_returns_the_exact_target_and_following_pictures() {
    // Declared reorder depth (pyramid, fake-interlaced) and estimated depth
    // (the VideoToolbox fixtures declare no bitstream_restriction).
    for name in [
        "cfr-bframes.mp4",
        "offset-bframes.mp4",
        "vfr.mp4",
        "pyramid-bframes.mp4",
        "fake-interlaced.mp4",
    ] {
        let expected = reference(name);
        for threads in [1, 8] {
            let mut decoder = open(name, threads);
            // Reverse order makes every request a real backward seek.
            for target in (0..expected.len()).rev() {
                let anchor = expected[..=target]
                    .iter()
                    .rposition(|picture| picture.keyframe)
                    .unwrap();
                decoder
                    .seek_to(expected[anchor].pts, expected[target].pts, control())
                    .unwrap();
                let mut decoded = Vec::new();
                loop {
                    let frame = decoder.next_metadata(control()).unwrap().unwrap();
                    decoded.push(frame.pts);
                    if frame.pts >= expected[target].pts {
                        break;
                    }
                }
                // Every returned preroll picture is a real indexed picture,
                // in presentation order, ending exactly at the target.
                assert!(decoded.windows(2).all(|pair| pair[0] < pair[1]));
                assert!(
                    decoded
                        .iter()
                        .all(|pts| expected.iter().any(|p| p.pts == *pts))
                );
                assert_eq!(*decoded.last().unwrap(), expected[target].pts);
                assert_eq!(
                    picture(decoder.copy_current_rgba(control()).unwrap()),
                    expected[target],
                    "{name} target {target} with {threads} threads"
                );
                // Pictures after the target were never skipped.
                for step in 1..=2 {
                    let Some(next) = expected.get(target + step) else {
                        break;
                    };
                    let frame = decoder.next_rgba(control()).unwrap().unwrap();
                    assert_eq!(picture(frame), *next, "{name} {target}+{step}");
                }
            }
        }
    }
}

/// Pictures the codec decodes (allocates) between a seek and the last
/// frame's output, single-threaded so the count is complete when read.
fn decoded_preroll(name: &str, skip: bool) -> u64 {
    let expected = reference(name);
    let target = expected.len() - 1;
    let mut anchor = expected
        .iter()
        .rposition(|picture| picture.keyframe)
        .unwrap();
    let mut decoder = open(name, 1);
    if decoder.info().bwdif_fields {
        anchor = expected[..anchor]
            .iter()
            .rposition(|picture| picture.keyframe)
            .unwrap_or(0);
    }
    let before = decoder.work().decoded_pictures;
    // Compare the same measured anchor. A target at that anchor leaves no
    // earlier packets to skip; generic seek also performs a temporal-context
    // probe for field streams, which is separate work from skipped preroll.
    decoder
        .seek_to(
            expected[anchor].pts,
            expected[if skip { target } else { anchor }].pts,
            control(),
        )
        .unwrap();
    while decoder.next_metadata(control()).unwrap().unwrap().pts < expected[target].pts {}
    decoder.work().decoded_pictures - before
}

#[test]
fn skipping_needs_a_declared_reorder_depth_and_frame_only_pictures() {
    // x264 B-pyramid declares bitstream_restriction with frame_mbs_only:
    // its non-reference B pictures before the target are not decoded.
    let ordinary = decoded_preroll("pyramid-bframes.mp4", false);
    let skipped = decoded_preroll("pyramid-bframes.mp4", true);
    assert!(
        skipped < ordinary,
        "non-reference pictures were not skipped: {skipped} of {ordinary}"
    );
    // Without bitstream_restriction FFmpeg estimates the reorder depth from
    // the pictures it sees, and with frame_mbs_only_flag 0 pictures may be
    // field pairs: both decode every preroll picture.
    for name in ["cfr-bframes.mp4", "vfr.mp4", "fake-interlaced.mp4"] {
        assert_eq!(
            decoded_preroll(name, true),
            decoded_preroll(name, false),
            "{name}"
        );
    }
}

#[test]
fn tiny_interlaced_planes_are_refused_at_every_thread_count() {
    for threads in [1, 8] {
        let error = SourceDecoder::open(
            fixture("interlaced.mkv"),
            DecodeLimits {
                threads,
                ..DecodeLimits::default()
            },
            control(),
        )
        .err()
        .unwrap();
        assert!(
            matches!(&error, SourceDecodeError::Native { code, .. } if code == "unsupported_interlace"),
            "{error}"
        );
    }
}

#[test]
fn thread_count_is_bounded_and_callback_failures_keep_their_codes() {
    for threads in [0, 17] {
        assert!(matches!(
            SourceDecoder::open(
                fixture("cfr-bframes.mp4"),
                DecodeLimits {
                    threads,
                    ..DecodeLimits::default()
                },
                control(),
            ),
            Err(SourceDecodeError::InvalidConfiguration(_))
        ));
    }
    // Format negotiation runs on a codec thread when threading is enabled;
    // its rejection still reaches the caller with its specific code.
    for threads in [1, 8] {
        let error = SourceDecoder::open(
            fixture("ten-bit.mkv"),
            DecodeLimits {
                threads,
                ..DecodeLimits::default()
            },
            control(),
        )
        .err()
        .unwrap();
        assert!(
            matches!(&error, SourceDecodeError::Native { code, .. } if code == "unsupported_depth"),
            "{error}"
        );
    }
}
