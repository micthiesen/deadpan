#![cfg(any(target_os = "macos", target_os = "linux"))]
//! Progressive admission serves receipt-verified pictures while a background
//! decoder completes the fresh full index measurement.

use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_core::{AssetId, SourceFrameId, SourceFrameIndex};
use deadpan_media::source_index::{SourceContentIdentity, SourceIndexSnapshot};
use deadpan_media::source_session::{
    IndexMeasurement, SourceSession, SourceSessionError, SourceSessionLimits,
};
use deadpan_source::DecodeLimits;
use sha2::{Digest, Sha256};

const TIMEOUT: Duration = Duration::from_secs(10);

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../native/deadpan-source/tests/fixtures")
            .join(name),
    )
    .unwrap()
}

fn identity(bytes: &[u8]) -> SourceContentIdentity {
    SourceContentIdentity::new(Sha256::digest(bytes).into(), bytes.len() as u64).unwrap()
}

fn limits(threads: u32) -> SourceSessionLimits {
    SourceSessionLimits {
        decode: DecodeLimits {
            threads,
            ..DecodeLimits::default()
        },
        ..SourceSessionLimits::default()
    }
}

/// The complete-measurement session: the receipt's index and the reference
/// pictures come from one sequential single-threaded decoder.
fn complete(bytes: &[u8]) -> SourceSession {
    SourceSession::open_verified(
        &mut Cursor::new(bytes),
        identity(bytes),
        AssetId::new("source").unwrap(),
        limits(1),
        &AtomicBool::new(false),
    )
    .unwrap()
}

fn progressive(
    bytes: &[u8],
    expected: SourceIndexSnapshot,
    reference: &SourceSession,
) -> Result<SourceSession, SourceSessionError> {
    progressive_with(bytes, expected, reference, limits(8))
}

fn progressive_with(
    bytes: &[u8],
    expected: SourceIndexSnapshot,
    reference: &SourceSession,
    limits: SourceSessionLimits,
) -> Result<SourceSession, SourceSessionError> {
    SourceSession::open_admitted(
        &mut Cursor::new(bytes),
        Arc::new(expected),
        reference.info(),
        limits,
        &AtomicBool::new(false),
    )
}

/// Replace one entry's reported duration, keeping a valid index.
fn tampered(index: &SourceIndexSnapshot, frame: usize) -> SourceIndexSnapshot {
    let mut frames = index.index().frames().to_vec();
    frames[frame].reported_duration = frames[frame].reported_duration.map(|value| value + 1);
    SourceIndexSnapshot::new(
        index.content(),
        index.stream_index(),
        SourceFrameIndex::new(
            index.index().asset().clone(),
            index.index().time_base(),
            frames,
            index.index().terminal_end(),
            index.index().terminal_provenance(),
        )
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn progressive_pictures_equal_complete_admission_and_measurement_verifies() {
    let cancelled = AtomicBool::new(false);
    for name in ["cfr-bframes.mp4", "offset-bframes.mp4", "vfr.mp4"] {
        let bytes = fixture(name);
        let mut reference = complete(&bytes);
        let mut session = progressive(&bytes, reference.index().clone(), &reference).unwrap();
        assert_eq!(session.index(), reference.index());
        let count = reference.index().index().frames().len() as u64;
        // Backward seeks, forward steps and repeats.
        let order: Vec<u64> = (0..count)
            .rev()
            .step_by(7)
            .chain([0, 1, 2, count - 1, count / 2, count / 2 + 1])
            .collect();
        for id in order {
            let expected = reference
                .frame(SourceFrameId(id), TIMEOUT, &cancelled)
                .unwrap();
            let actual = session
                .frame(SourceFrameId(id), TIMEOUT, &cancelled)
                .unwrap();
            assert_eq!(actual.metadata.pts, expected.metadata.pts, "{name} {id}");
            assert_eq!(actual.rgba, expected.rgba, "{name} {id}");
        }
        assert_eq!(
            session.wait_measured(Duration::from_secs(30)),
            IndexMeasurement::Verified
        );
        assert_eq!(reference.measurement(), IndexMeasurement::Verified);
    }
}

#[test]
fn a_receipt_entry_that_differs_from_its_decoded_picture_is_never_served() {
    let cancelled = AtomicBool::new(false);
    let bytes = fixture("cfr-bframes.mp4");
    let reference = complete(&bytes);
    let last = reference.index().index().frames().len() - 1;
    let expected = tampered(reference.index(), last);
    let mut session = progressive(&bytes, expected, &reference).unwrap();
    // Either its own check or a finished verdict refuses the tampered entry.
    assert!(matches!(
        session.frame(SourceFrameId(last as u64), TIMEOUT, &cancelled),
        Err(SourceSessionError::IndexMismatch | SourceSessionError::MeasurementMismatch(_))
    ));
    let measured = session.wait_measured(Duration::from_secs(30));
    assert!(
        matches!(measured, IndexMeasurement::Mismatch(_)),
        "{measured:?}"
    );
    // Once the fresh measurement disagrees, every later request fails, even
    // for entries that match, and the failure is not an interruption.
    for id in [0, 1, 2] {
        let error = session
            .frame(SourceFrameId(id), TIMEOUT, &cancelled)
            .unwrap_err();
        assert!(
            matches!(error, SourceSessionError::MeasurementMismatch(_)),
            "{error}"
        );
        assert!(!error.is_interruption());
    }
}

#[test]
fn an_exhausted_background_deadline_is_an_interruption_not_a_verdict() {
    let cancelled = AtomicBool::new(false);
    let bytes = fixture("cfr-bframes.mp4");
    let reference = complete(&bytes);
    let mut session = progressive_with(
        &bytes,
        reference.index().clone(),
        &reference,
        SourceSessionLimits {
            measurement_timeout: Duration::from_nanos(1),
            ..limits(8)
        },
    )
    .unwrap();
    let measured = session.wait_measured(Duration::from_secs(30));
    assert!(
        matches!(measured, IndexMeasurement::Interrupted(_)),
        "{measured:?}"
    );
    let error = session
        .frame(SourceFrameId(0), TIMEOUT, &cancelled)
        .unwrap_err();
    assert!(
        matches!(error, SourceSessionError::MeasurementInterrupted(_)),
        "{error}"
    );
    assert!(error.is_interruption());
    // A new admission measures again and verifies.
    let mut retried = progressive(&bytes, reference.index().clone(), &reference).unwrap();
    assert_eq!(
        retried.wait_measured(Duration::from_secs(30)),
        IndexMeasurement::Verified
    );
    retried
        .frame(SourceFrameId(0), TIMEOUT, &cancelled)
        .unwrap();
}

#[test]
fn measured_indexes_do_not_depend_on_the_serving_thread_count() {
    for name in [
        "cfr-bframes.mp4",
        "pyramid-bframes.mp4",
        "fake-interlaced.mp4",
        "full709.mkv",
    ] {
        let bytes = fixture(name);
        let single = complete(&bytes);
        for threads in [2, 16] {
            let threaded = SourceSession::open_verified(
                &mut Cursor::new(&bytes),
                identity(&bytes),
                AssetId::new("source").unwrap(),
                limits(threads),
                &AtomicBool::new(false),
            )
            .unwrap();
            assert_eq!(threaded.index(), single.index(), "{name} {threads}");
        }
    }
}

#[test]
fn receipt_stream_metadata_and_content_identity_must_match_before_serving() {
    let bytes = fixture("cfr-bframes.mp4");
    let reference = complete(&bytes);
    let mut info = reference.info().clone();
    info.width += 2;
    assert!(matches!(
        SourceSession::open_admitted(
            &mut Cursor::new(&bytes),
            Arc::new(reference.index().clone()),
            &info,
            limits(8),
            &AtomicBool::new(false),
        ),
        Err(SourceSessionError::IndexMismatch)
    ));
    // The receipt index names other bytes: the verified copy refuses them.
    let other = fixture("offset-bframes.mp4");
    assert!(matches!(
        progressive(&other, reference.index().clone(), &reference),
        Err(SourceSessionError::Snapshot(_))
    ));
}

#[test]
fn dropping_a_measuring_session_cancels_and_joins_its_thread() {
    let bytes = fixture("cfr-bframes.mp4");
    let reference = complete(&bytes);
    for _ in 0..8 {
        let session = progressive(&bytes, reference.index().clone(), &reference).unwrap();
        let started = Instant::now();
        drop(session);
        // Cancellation is observed between single-threaded codec calls.
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
