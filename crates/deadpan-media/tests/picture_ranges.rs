#![cfg(any(target_os = "macos", target_os = "linux"))]
//! Bounded picture windows preserve exact qualified PTS, durations and pixels.

use std::io::Cursor;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_core::AssetId;
use deadpan_media::picture_scan::{
    PictureScanError, PictureScanOptions, scan_pictures_from, scan_pictures_range,
};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_media::source_input::VerifiedSourceInput;
use deadpan_media::source_qualification::{DecodedSourceQualification, QualifiedVideoSnapshot};
use deadpan_media::source_session::{SourceSession, SourceSessionLimits};
use deadpan_source::{DecodeLimits, VideoInterpretation};
use sha2::{Digest, Sha256};

fn input(name: &str) -> VerifiedSourceInput {
    let bytes = std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../native/deadpan-source/tests/fixtures")
            .join(name),
    )
    .unwrap();
    let identity =
        SourceContentIdentity::new(Sha256::digest(&bytes).into(), bytes.len() as u64).unwrap();
    VerifiedSourceInput::copy_verified(
        &mut Cursor::new(bytes),
        identity,
        identity.byte_length(),
        Duration::from_secs(10),
        &AtomicBool::new(false),
    )
    .unwrap()
}

fn qualified(input: &VerifiedSourceInput) -> DecodedSourceQualification {
    let session = SourceSession::open_input(
        input.clone(),
        AssetId::new("range").unwrap(),
        SourceSessionLimits::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    DecodedSourceQualification::from_sessions(Some(&session), None).unwrap()
}

type Picture = (usize, i64, Option<i64>, [u8; 32]);

fn window(
    input: &VerifiedSourceInput,
    video: &QualifiedVideoSnapshot,
    range: Range<usize>,
    options: PictureScanOptions,
) -> Vec<Picture> {
    let mut seen = Vec::new();
    let count = scan_pictures_range(
        input,
        video,
        options,
        Instant::now() + Duration::from_secs(60),
        &AtomicBool::new(false),
        range.clone(),
        |ordinal, picture| {
            seen.push((
                ordinal,
                picture.metadata.pts,
                picture.metadata.reported_duration,
                Sha256::digest(&picture.rgba).into(),
            ));
            Ok::<_, String>(())
        },
    )
    .unwrap_or_else(|error| panic!("picture range {range:?} failed: {error}"));
    assert_eq!(count, range.end - range.start);
    assert_eq!(count, seen.len());
    seen
}

#[test]
fn bounded_windows_equal_whole_decode_on_cfr_vfr_offsets_rotation_and_interpretation() {
    for (name, expected_total) in [
        ("cfr-bframes.mp4", 120),
        ("vfr.mp4", 120),
        ("offset-bframes.mp4", 120),
        ("rotated90.mp4", 1),
        ("hevc-sdr-10-limited.mp4", 12),
    ] {
        let input = input(name);
        let qualification = qualified(&input);
        let video = qualification.snapshot().video().unwrap();
        let total = video.index().index().frames().len();
        assert_eq!(total, expected_total, "{name}: qualified picture count");
        let options = PictureScanOptions {
            decode: DecodeLimits {
                threads: 4,
                ..DecodeLimits::default()
            },
            interpretation: VideoInterpretation::default(),
        };
        let mut whole: Vec<Picture> = Vec::new();
        scan_pictures_from(
            &input,
            video,
            options,
            Instant::now() + Duration::from_secs(60),
            &AtomicBool::new(false),
            0,
            |ordinal, picture| {
                whole.push((
                    ordinal,
                    picture.metadata.pts,
                    picture.metadata.reported_duration,
                    Sha256::digest(&picture.rgba).into(),
                ));
                Ok::<_, String>(())
            },
        )
        .unwrap();
        assert_eq!(whole.len(), total, "{name}: whole scan picture count");
        // The rotation fixture is a single real picture. Its only valid
        // window still verifies rotation and terminal EOF; multi-picture
        // fixtures additionally exercise the second-picture seek.
        let second = usize::from(total > 1);
        for range in [
            0..1,
            second..second + 1,
            total / 3..(total / 3 + 3).min(total),
            total - 1..total,
            0..total,
        ] {
            assert_eq!(
                window(&input, video, range.clone(), options),
                whole[range.clone()],
                "{name}: {range:?}"
            );
        }
        if name.starts_with("hevc") {
            let corrected = PictureScanOptions {
                interpretation: VideoInterpretation {
                    range: Some(deadpan_source::ColorRange::Full),
                    chroma_location: Some(deadpan_source::ChromaLocation::Center),
                },
                ..options
            };
            let all = window(&input, video, 0..total, corrected);
            assert_ne!(all, whole);
            assert_eq!(
                window(&input, video, total / 2..total, corrected),
                all[total / 2..]
            );
        }
    }
}

#[test]
fn range_admission_cancellation_deadline_and_visitor_errors_stay_explicit() {
    let input = input("cfr-bframes.mp4");
    let qualification = qualified(&input);
    let video = qualification.snapshot().video().unwrap();
    let total = video.index().index().frames().len();
    for range in [
        0..0,
        Range { start: 2, end: 1 },
        total..total + 1,
        0..total + 1,
    ] {
        assert!(matches!(
            scan_pictures_range(
                &input,
                video,
                PictureScanOptions::default(),
                Instant::now() + Duration::from_secs(60),
                &AtomicBool::new(false),
                range,
                |_, _| Ok::<_, String>(())
            ),
            Err(PictureScanError::Mismatch(_))
        ));
    }
    assert!(matches!(
        scan_pictures_range(
            &input,
            video,
            PictureScanOptions::default(),
            Instant::now() + Duration::from_secs(60),
            &AtomicBool::new(true),
            1..3,
            |_, _| Ok::<_, String>(())
        ),
        Err(PictureScanError::Cancelled)
    ));
    assert!(matches!(
        scan_pictures_range(
            &input,
            video,
            PictureScanOptions::default(),
            Instant::now(),
            &AtomicBool::new(false),
            1..3,
            |_, _| Ok::<_, String>(())
        ),
        Err(PictureScanError::Deadline)
    ));
    let cancelled = AtomicBool::new(false);
    let mut visited = Vec::new();
    assert!(matches!(
        scan_pictures_range(
            &input,
            video,
            PictureScanOptions::default(),
            Instant::now() + Duration::from_secs(60),
            &cancelled,
            2..7,
            |ordinal, _| {
                visited.push(ordinal);
                cancelled.store(true, Ordering::Release);
                Ok::<_, String>(())
            }
        ),
        Err(PictureScanError::Cancelled)
    ));
    assert_eq!(visited, [2]);
    assert!(
        matches!(scan_pictures_range(&input, video, PictureScanOptions::default(),
        Instant::now() + Duration::from_secs(60), &AtomicBool::new(false), 2..7,
        |_, _| Err::<(), _>("visitor refusal")), Err(PictureScanError::Visit(error)) if error == "visitor refusal")
    );
}
