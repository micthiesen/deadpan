#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use deadpan_core::{AssetId, SourceFrameId, TerminalProvenance};
use deadpan_media::ConversionError;
use deadpan_media::source_index::{SourceContentIdentity, SourceIndexSnapshot};
use deadpan_media::source_input::{SourceInputError, VerifiedSourceInput};
use deadpan_media::source_session::{SourceSession, SourceSessionError, SourceSessionLimits};
use sha2::{Digest, Sha256};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../native/deadpan-media-worker/tests/fixtures")
            .join(format!("{name}.mp4")),
    )
    .unwrap()
}

fn identity(bytes: &[u8]) -> SourceContentIdentity {
    SourceContentIdentity::new(Sha256::digest(bytes).into(), bytes.len() as u64).unwrap()
}

fn open(bytes: &[u8], limits: SourceSessionLimits) -> SourceSession {
    SourceSession::open_verified(
        &mut Cursor::new(bytes),
        identity(bytes),
        AssetId::new("source").unwrap(),
        limits,
        &AtomicBool::new(false),
    )
    .unwrap()
}

// Original authored fixture formula, independent of FFmpeg's output routines.
fn expected_rgba(frame: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    for y in 0..2 {
        for x in 0..4 {
            bytes.extend_from_slice(&[
                ((17 * frame + 31 * x + 7 * y + 3) % 256) as u8,
                ((29 * frame + 5 * x + 47 * y + 11) % 256) as u8,
                ((43 * frame + 13 * x + 19 * y + 23) % 256) as u8,
                255,
            ]);
        }
    }
    bytes
}

#[test]
fn indexed_original_frames_seek_exactly_and_own_pixels_after_source_and_decoder_loss() {
    for (name, count, rate_num, rate_den) in [
        ("rgb25_24", 25, 24, 1),
        ("rgb30_30000_1001", 30, 30_000, 1001),
    ] {
        let mut original = fixture(name);
        let mut session = open(&original, SourceSessionLimits::default());
        // Session input is a private checked copy, not a borrowed source buffer.
        original.fill(0);
        drop(original);
        let index = session.index();
        assert_eq!(index.index().frames().len(), count as usize);
        assert_eq!(
            index.index().terminal_provenance(),
            TerminalProvenance::DecodedFrameDuration
        );
        let saved = index.to_json().unwrap();
        assert_eq!(&SourceIndexSnapshot::from_json(&saved).unwrap(), index);
        let cancelled = AtomicBool::new(false);
        for id in (0..count).chain([count - 1, 0, 1, 17, 3, count - 1]) {
            let frame = session
                .frame(SourceFrameId(id), Duration::from_secs(2), &cancelled)
                .unwrap();
            assert_eq!(frame.rgba, expected_rgba(id), "{name} frame {id}");
            let clock = session.index().index().time_base();
            assert_eq!(
                i128::from(frame.metadata.pts) * i128::from(clock.numerator()) * rate_num,
                i128::from(id) * rate_den * i128::from(clock.denominator())
            );
        }
        let retained = session
            .frame(SourceFrameId(0), Duration::from_secs(2), &cancelled)
            .unwrap();
        session
            .frame(SourceFrameId(count - 1), Duration::from_secs(2), &cancelled)
            .unwrap();
        drop(session);
        assert_eq!(retained.rgba, expected_rgba(0));
    }
}

#[test]
fn invalid_native_limits_fail_before_any_snapshot_read() {
    struct NoRead;
    impl std::io::Read for NoRead {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            panic!("invalid native limits must not start snapshot I/O");
        }
    }
    let defaults = deadpan_source::DecodeLimits::default();
    for decode in [
        deadpan_source::DecodeLimits {
            max_input_bytes: 64 * 1024 * 1024 * 1024 + 1,
            ..defaults
        },
        deadpan_source::DecodeLimits {
            max_frames: 0,
            ..defaults
        },
        deadpan_source::DecodeLimits {
            max_packets: 40_000_001,
            ..defaults
        },
        deadpan_source::DecodeLimits {
            max_io_bytes_per_call: 1024 * 1024 * 1024 + 1,
            ..defaults
        },
        deadpan_source::DecodeLimits {
            max_pixels: 8192 * 8192 + 1,
            ..defaults
        },
        deadpan_source::DecodeLimits {
            max_dimension: 8193,
            ..defaults
        },
        deadpan_source::DecodeLimits {
            max_packets_per_frame: 10_001,
            ..defaults
        },
    ] {
        assert!(matches!(
            SourceSession::open_verified(
                &mut NoRead,
                SourceContentIdentity::new([0; 32], 1).unwrap(),
                AssetId::new("source").unwrap(),
                SourceSessionLimits {
                    decode,
                    ..Default::default()
                },
                &AtomicBool::new(false),
            ),
            Err(SourceSessionError::Native(
                deadpan_source::SourceDecodeError::InvalidConfiguration(_)
            ))
        ));
    }
}

#[test]
fn shared_verified_bytes_survive_original_loss_and_independent_decoder_positions() {
    let mut bytes = fixture("rgb25_24");
    let cancelled = AtomicBool::new(false);
    let input = VerifiedSourceInput::copy_verified(
        &mut Cursor::new(&bytes),
        identity(&bytes),
        bytes.len() as u64,
        Duration::from_secs(2),
        &cancelled,
    )
    .unwrap();
    let content = input.identity();
    bytes.fill(0);
    let mut first = SourceSession::open_input(
        input.clone(),
        AssetId::new("first").unwrap(),
        SourceSessionLimits::default(),
        &cancelled,
    )
    .unwrap();
    let mut second = SourceSession::open_input(
        input,
        AssetId::new("second").unwrap(),
        SourceSessionLimits::default(),
        &cancelled,
    )
    .unwrap();
    assert_eq!(first.index().content(), content);
    assert_eq!(second.index().content(), content);
    for (left, right) in [(0, 24), (1, 0), (23, 12), (0, 13)] {
        assert_eq!(
            first
                .frame(SourceFrameId(left), Duration::from_secs(2), &cancelled)
                .unwrap()
                .rgba,
            expected_rgba(left)
        );
        assert_eq!(
            second
                .frame(SourceFrameId(right), Duration::from_secs(2), &cancelled)
                .unwrap()
                .rgba,
            expected_rgba(right)
        );
    }
    drop(first);
    assert_eq!(
        second
            .frame(SourceFrameId(14), Duration::from_secs(2), &cancelled)
            .unwrap()
            .rgba,
        expected_rgba(14)
    );
}

#[test]
fn shared_input_checks_limits_before_reading_and_identity_before_publication() {
    struct NoRead;
    impl std::io::Read for NoRead {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            panic!("preflight must reject without reading");
        }
    }
    let content = SourceContentIdentity::new([0; 32], 10).unwrap();
    for (maximum, timeout) in [
        (0, Duration::from_secs(1)),
        (9, Duration::from_secs(1)),
        (65 * 1024 * 1024 * 1024, Duration::from_secs(1)),
        (10, Duration::ZERO),
        (10, Duration::from_secs(86401)),
    ] {
        assert!(matches!(
            VerifiedSourceInput::copy_verified(
                &mut NoRead,
                content,
                maximum,
                timeout,
                &AtomicBool::new(false)
            ),
            Err(SourceInputError::Limits)
        ));
    }
    assert!(matches!(
        VerifiedSourceInput::copy_verified(
            &mut NoRead,
            content,
            10,
            Duration::from_secs(1),
            &AtomicBool::new(true)
        ),
        Err(SourceInputError::Snapshot(ConversionError::Cancelled))
    ));
    assert!(matches!(
        VerifiedSourceInput::copy_verified(
            &mut Cursor::new([0; 10]),
            content,
            10,
            Duration::from_secs(1),
            &AtomicBool::new(false)
        ),
        Err(SourceInputError::Snapshot(ConversionError::InputIdentity))
    ));
}

#[test]
fn hash_length_cancellation_and_index_budgets_fail_without_inventing_an_index() {
    let bytes = fixture("rgb25_24");
    let correct = identity(&bytes);
    for wrong in [
        SourceContentIdentity::new([0; 32], correct.byte_length()).unwrap(),
        SourceContentIdentity::new(correct.sha256(), correct.byte_length() - 1).unwrap(),
        SourceContentIdentity::new(correct.sha256(), correct.byte_length() + 1).unwrap(),
    ] {
        assert!(matches!(
            SourceSession::open_verified(
                &mut Cursor::new(&bytes),
                wrong,
                AssetId::new("source").unwrap(),
                SourceSessionLimits::default(),
                &AtomicBool::new(false)
            ),
            Err(SourceSessionError::Snapshot(ConversionError::InputIdentity))
        ));
    }
    let cancelled = AtomicBool::new(true);
    let mut reader = Cursor::new(&bytes);
    assert!(matches!(
        SourceSession::open_verified(
            &mut reader,
            correct,
            AssetId::new("source").unwrap(),
            SourceSessionLimits::default(),
            &cancelled
        ),
        Err(SourceSessionError::Snapshot(ConversionError::Cancelled))
    ));
    assert_eq!(reader.position(), 0);
    for limits in [
        SourceSessionLimits {
            maximum_index_frames: 1,
            ..Default::default()
        },
        SourceSessionLimits {
            maximum_index_bytes: 1,
            ..Default::default()
        },
    ] {
        assert!(matches!(
            SourceSession::open_verified(
                &mut Cursor::new(&bytes),
                correct,
                AssetId::new("source").unwrap(),
                limits,
                &AtomicBool::new(false)
            ),
            Err(SourceSessionError::Limits(_))
        ));
    }
}

#[test]
fn cancelled_or_budgeted_seek_reopens_only_the_decoder_and_preserves_the_index() {
    let bytes = fixture("rgb25_24");
    let mut session = open(
        &bytes,
        SourceSessionLimits {
            maximum_seek_frames: 1,
            ..Default::default()
        },
    );
    let before = session.index().to_json().unwrap();
    let cancelled = AtomicBool::new(true);
    assert!(
        session
            .frame(SourceFrameId(0), Duration::from_secs(2), &cancelled)
            .is_err()
    );
    cancelled.store(false, Ordering::Release);
    assert_eq!(
        session
            .frame(SourceFrameId(0), Duration::from_secs(2), &cancelled)
            .unwrap()
            .rgba,
        expected_rgba(0)
    );
    assert!(matches!(
        session.frame(SourceFrameId(24), Duration::from_secs(2), &cancelled),
        Err(SourceSessionError::SeekLimit)
    ));
    assert_eq!(
        session
            .frame(SourceFrameId(0), Duration::from_secs(2), &cancelled)
            .unwrap()
            .rgba,
        expected_rgba(0)
    );
    assert_eq!(session.index().to_json().unwrap(), before);
    assert!(matches!(
        session.frame(SourceFrameId(25), Duration::from_secs(2), &cancelled),
        Err(SourceSessionError::MissingFrame(_))
    ));
}

#[test]
fn unqualified_tags_and_corrupt_frames_cannot_create_a_source_session() {
    for name in ["rgb1_24_no_tags", "rgb1_24_corrupt"] {
        let bytes = fixture(name);
        assert!(
            SourceSession::open_verified(
                &mut Cursor::new(&bytes),
                identity(&bytes),
                AssetId::new("source").unwrap(),
                SourceSessionLimits::default(),
                &AtomicBool::new(false),
            )
            .is_err(),
            "{name}"
        );
    }
}
