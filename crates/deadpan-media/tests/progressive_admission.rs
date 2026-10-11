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
#[cfg(target_os = "macos")]
fn hardware_serving_preserves_software_receipts_and_rejects_bad_indexes() {
    use deadpan_source::DecodeAcceleration;
    let cancelled = AtomicBool::new(false);
    for name in [
        "cfr-bframes.mp4",
        "hevc-sdr-10-full.mp4",
        "vp9-pq-static.webm",
        "fields-tff.mp4",
    ] {
        let bytes = fixture(name);
        let mut reference = complete(&bytes);
        for threads in [1, 16] {
            let mut limits = SourceSessionLimits::interactive();
            limits.decode.acceleration = DecodeAcceleration::PreferVideoToolbox;
            limits.decode.threads = threads;
            let mut serving =
                progressive_with(&bytes, reference.index().clone(), &reference, limits).unwrap();
            let hardware = name != "fields-tff.mp4";
            assert_eq!(serving.decode_work().hardware_frames > 0, hardware);
            let complete_serving = SourceSession::open_verified(
                &mut Cursor::new(&bytes),
                identity(&bytes),
                AssetId::new("source").unwrap(),
                limits,
                &cancelled,
            )
            .unwrap();
            assert_eq!(complete_serving.index(), reference.index());
            assert_eq!(
                complete_serving.decode_work().hardware_frames > 0,
                hardware,
                "complete admission must not reuse its software measuring decoder for hardware serving"
            );
            assert_eq!(serving.info(), reference.info());
            assert_eq!(
                serving.wait_measured(Duration::from_secs(30)),
                IndexMeasurement::Verified
            );
            for ordinal in [0, 5, 2, 1] {
                let expected = reference
                    .frame(SourceFrameId(ordinal), TIMEOUT, &cancelled)
                    .unwrap();
                let actual = serving
                    .frame(SourceFrameId(ordinal), TIMEOUT, &cancelled)
                    .unwrap();
                assert_eq!(
                    actual, expected,
                    "{name}, threads={threads}, frame={ordinal}"
                );
            }
            // A valid-but-different receipt is rejected by the independent
            // software measurement even while hardware could serve pictures.
            let mut wrong =
                progressive_with(&bytes, tampered(reference.index(), 3), &reference, limits)
                    .unwrap();
            assert!(matches!(
                wrong.wait_measured(Duration::from_secs(30)),
                IndexMeasurement::Mismatch(_)
            ));
            assert!(wrong.frame(SourceFrameId(0), TIMEOUT, &cancelled).is_err());
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

/// A store-verified private snapshot serves through progressive admission
/// without a second copy: the same pictures and verdict as copying, and an
/// identity that names other bytes is refused before any decoder opens.
#[test]
fn an_adopted_private_snapshot_serves_like_a_copied_one() {
    use deadpan_media::source_input::VerifiedSourceInput;
    use std::io::Write;
    let cancelled = AtomicBool::new(false);
    let bytes = fixture("cfr-bframes.mp4");
    let mut reference = complete(&bytes);
    let snapshot = |bytes: &[u8]| {
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(bytes).unwrap();
        file
    };
    let input =
        VerifiedSourceInput::from_store_verified_snapshot(snapshot(&bytes), identity(&bytes))
            .unwrap();
    let mut session = SourceSession::open_admitted_input(
        input,
        Arc::new(reference.index().clone()),
        reference.info(),
        limits(8),
        &cancelled,
    )
    .unwrap();
    let count = reference.index().index().frames().len() as u64;
    for id in [count - 1, 0, count / 2, count / 2 + 1] {
        let expected = reference
            .frame(SourceFrameId(id), TIMEOUT, &cancelled)
            .unwrap();
        let actual = session
            .frame(SourceFrameId(id), TIMEOUT, &cancelled)
            .unwrap();
        assert_eq!(actual.rgba, expected.rgba, "{id}");
    }
    assert_eq!(
        session.wait_measured(Duration::from_secs(30)),
        IndexMeasurement::Verified
    );
    // A snapshot whose length differs from its claimed identity is refused,
    // and so is an identity that is not the receipt's content.
    assert!(
        VerifiedSourceInput::from_store_verified_snapshot(snapshot(&bytes[1..]), identity(&bytes))
            .is_err()
    );
    let other = fixture("vfr.mp4");
    let input =
        VerifiedSourceInput::from_store_verified_snapshot(snapshot(&other), identity(&other))
            .unwrap();
    assert!(matches!(
        SourceSession::open_admitted_input(
            input,
            Arc::new(reference.index().clone()),
            reference.info(),
            limits(8),
            &cancelled,
        ),
        Err(SourceSessionError::IndexMismatch)
    ));
}
