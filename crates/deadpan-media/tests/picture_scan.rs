#![cfg(any(target_os = "macos", target_os = "linux"))]
//! Whole-source picture scans follow the qualified index exactly.

use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_core::AssetId;
use deadpan_media::picture_scan::{PictureScanError, scan_pictures, scan_pictures_from};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_media::source_input::VerifiedSourceInput;
use deadpan_media::source_qualification::DecodedSourceQualification;
use deadpan_media::source_session::{SourceSession, SourceSessionLimits};
use deadpan_source::DecodeLimits;
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
        AssetId::new("scan").unwrap(),
        SourceSessionLimits::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    DecodedSourceQualification::from_sessions(Some(&session), None).unwrap()
}

fn later() -> Instant {
    Instant::now() + Duration::from_secs(60)
}

#[test]
fn every_indexed_picture_is_visited_once_in_order_with_its_pts() {
    for name in ["cfr-bframes.mp4", "vfr.mp4", "offset-bframes.mp4"] {
        let input = input(name);
        let qualification = qualified(&input);
        let video = qualification.snapshot().video().unwrap();
        let frames = video.index().index().frames();
        let mut visited = Vec::new();
        let count = scan_pictures(
            &input,
            video,
            DecodeLimits::default(),
            later(),
            &AtomicBool::new(false),
            |ordinal, picture| {
                assert_eq!(picture.metadata.pts, frames[ordinal].pts);
                assert_eq!(
                    picture.rgba.len(),
                    picture.row_stride_bytes * picture.height as usize
                );
                visited.push(ordinal);
                Ok::<_, String>(())
            },
        )
        .unwrap();
        assert_eq!(count, frames.len(), "{name}");
        assert_eq!(visited, (0..frames.len()).collect::<Vec<_>>(), "{name}");
    }
}

#[test]
fn foreign_content_visitor_failures_and_cancellation_fail_explicitly() {
    let cfr = input("cfr-bframes.mp4");
    let qualification = qualified(&cfr);
    let video = qualification.snapshot().video().unwrap();
    let running = AtomicBool::new(false);
    // Bytes other than the qualified content are refused before decoding.
    let other = input("vfr.mp4");
    assert!(matches!(
        scan_pictures(
            &other,
            video,
            DecodeLimits::default(),
            later(),
            &running,
            |_, _| Ok::<_, String>(())
        ),
        Err(PictureScanError::Mismatch(_))
    ));
    assert!(matches!(
        scan_pictures(&cfr, video, DecodeLimits::default(), later(), &running, |ordinal, _| {
            if ordinal == 3 { Err("stop") } else { Ok(()) }
        }),
        Err(PictureScanError::Visit(message)) if message == "stop"
    ));
    assert!(matches!(
        scan_pictures(
            &cfr,
            video,
            DecodeLimits::default(),
            later(),
            &AtomicBool::new(true),
            |_, _| Ok::<_, String>(())
        ),
        Err(PictureScanError::Cancelled)
    ));
    assert!(matches!(
        scan_pictures(
            &cfr,
            video,
            DecodeLimits::default(),
            Instant::now(),
            &running,
            |_, _| Ok::<_, String>(())
        ),
        Err(PictureScanError::Deadline)
    ));
}

/// Every picture of a scan from the first picture, as (PTS, RGBA hash).
fn hashes(
    input: &VerifiedSourceInput,
    video: &deadpan_media::source_qualification::QualifiedVideoSnapshot,
    start: usize,
    threads: u32,
) -> Vec<(i64, [u8; 32])> {
    let mut seen = Vec::new();
    let limits = DecodeLimits {
        threads,
        ..DecodeLimits::default()
    };
    let count = scan_pictures_from(
        input,
        video,
        limits,
        later(),
        &AtomicBool::new(false),
        start,
        |ordinal, picture| {
            assert_eq!(ordinal, start + seen.len());
            seen.push((picture.metadata.pts, Sha256::digest(&picture.rgba).into()));
            Ok::<_, String>(())
        },
    )
    .unwrap();
    assert_eq!(count, seen.len());
    seen
}

#[test]
fn a_scan_from_any_picture_equals_the_tail_of_a_whole_scan_with_any_thread_count() {
    for name in ["cfr-bframes.mp4", "vfr.mp4", "offset-bframes.mp4"] {
        let input = input(name);
        let qualification = qualified(&input);
        let video = qualification.snapshot().video().unwrap();
        let pictures = video.index().index().frames().len();
        let whole = hashes(&input, video, 0, 1);
        assert_eq!(whole.len(), pictures);
        assert_eq!(hashes(&input, video, 0, 4), whole, "{name} threaded");
        for start in [1, 2, pictures / 3, pictures / 2 + 1, pictures - 1] {
            assert_eq!(
                hashes(&input, video, start, 1),
                whole[start..],
                "{name} from {start}"
            );
        }
        assert_eq!(
            hashes(&input, video, pictures / 2 + 1, 4),
            whole[pictures / 2 + 1..],
            "{name} threaded seek"
        );
        assert!(matches!(
            scan_pictures_from(
                &input,
                video,
                DecodeLimits::default(),
                later(),
                &AtomicBool::new(false),
                pictures,
                |_, _| Ok::<_, String>(())
            ),
            Err(PictureScanError::Mismatch(_))
        ));
    }
}
