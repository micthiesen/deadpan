//! A whole-source picture scan of FFV1/Matroska returns the same pictures
//! with one and with several codec threads, as shot detection assumes.
//! The FFV1 file is made by the real conversion worker, since the source
//! fixtures hold only single-picture FFV1 files.

use std::io::{Cursor, Read};
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_core::AssetId;
use deadpan_media::InputIdentity;
use deadpan_media::canonicalize;
use deadpan_media::picture_scan::scan_pictures;
use deadpan_media::protocol::{ConversionLimits, ConversionRequest, VideoContract};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_media::source_input::VerifiedSourceInput;
use deadpan_media::source_qualification::DecodedSourceQualification;
use deadpan_media::source_session::{SourceSession, SourceSessionLimits};
use deadpan_source::DecodeLimits;
use sha2::{Digest, Sha256};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

/// The 30-picture RGB fixture converted to FFV1/Matroska.
fn ffv1() -> Result<Vec<u8>> {
    let source = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rgb30_30000_1001.mp4"),
    )?;
    let video = VideoContract {
        width: 4,
        height: 2,
        frames: 30,
        rate_num: 30_000,
        rate_den: 1001,
    };
    let request = ConversionRequest {
        protocol: 1,
        video,
        input_byte_length: source.len() as u64,
        limits: ConversionLimits {
            max_input_bytes: source.len() as u64 + 1,
            max_output_bytes: 4 * 1024 * 1024,
            max_scratch_bytes: video.scratch_bytes().unwrap(),
            timeout_ms: 30_000,
        },
    };
    let mut media = canonicalize(
        Path::new(env!("CARGO_BIN_EXE_deadpan-media-worker")),
        &mut Cursor::new(source.clone()),
        InputIdentity {
            sha256: Sha256::digest(&source).into(),
        },
        &request,
        &AtomicBool::new(false),
    )?;
    assert_eq!(media.report().ffv1_version, 3);
    let mut bytes = Vec::new();
    media.read_to_end(&mut bytes)?;
    Ok(bytes)
}

#[test]
fn ffv1_matroska_scans_are_identical_with_one_and_four_decode_threads() -> Result {
    let bytes = ffv1()?;
    let identity = SourceContentIdentity::new(Sha256::digest(&bytes).into(), bytes.len() as u64)?;
    let running = AtomicBool::new(false);
    let input = VerifiedSourceInput::copy_verified(
        &mut Cursor::new(bytes),
        identity,
        identity.byte_length(),
        Duration::from_secs(10),
        &running,
    )?;
    let session = SourceSession::open_input(
        input.clone(),
        AssetId::new("ffv1")?,
        SourceSessionLimits::default(),
        &running,
    )?;
    let qualification = DecodedSourceQualification::from_sessions(Some(&session), None)?;
    let video = qualification.snapshot().video().unwrap();
    let pictures = video.index().index().frames().len();
    assert_eq!(pictures, 30);
    let scan = |threads: u32| -> Result<Vec<(i64, [u8; 32])>> {
        let mut seen = Vec::new();
        let limits = DecodeLimits {
            threads,
            ..DecodeLimits::default()
        };
        let count = scan_pictures(
            &input,
            video,
            limits,
            Instant::now() + Duration::from_secs(60),
            &running,
            |ordinal, picture| {
                assert_eq!(ordinal, seen.len());
                seen.push((picture.metadata.pts, Sha256::digest(&picture.rgba).into()));
                Ok::<_, String>(())
            },
        )?;
        assert_eq!(count, seen.len());
        Ok(seen)
    };
    let single = scan(1)?;
    assert_eq!(single.len(), pictures);
    // The pattern changes every picture, so equal hashes are not vacuous.
    assert!(single.windows(2).all(|pair| pair[0].1 != pair[1].1));
    assert_eq!(scan(4)?, single);
    Ok(())
}
