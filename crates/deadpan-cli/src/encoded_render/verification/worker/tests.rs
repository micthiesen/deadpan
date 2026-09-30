use std::io::{Cursor, Seek, SeekFrom};
use std::sync::atomic::Ordering;

use deadpan_jobs::{AttemptId, RequestId, write_frame};

use super::*;

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
}

fn identity() -> RenderIdentity {
    RenderIdentity {
        request_id: RequestId::new("verify-request").unwrap(),
        attempt_id: AttemptId::new("verify-attempt").unwrap(),
    }
}

fn cancel_frame(version: u32, identity: RenderIdentity, token: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    // Raw framing deliberately permits invalid protocol versions for the
    // receiving worker to reject independently.
    write_frame(
        &mut bytes,
        &HostMessage::Cancel {
            protocol: version,
            identity,
            cancellation_token: CancellationToken::new(token).unwrap(),
        },
    )
    .unwrap();
    bytes
}

fn binary_payload() -> Vec<u8> {
    // Exercise two complete reads and a partial final read, including every
    // byte value and embedded NULs rather than a text-only hash fixture.
    (0_u8..=255).cycle().take(2 * 64 * 1024 + 19).collect()
}

fn expected_hash(bytes: &[u8]) -> Sha256 {
    let digest = Hasher::digest(bytes);
    Sha256::new(
        digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
    )
    .unwrap()
}

#[test]
fn cancellation_requires_exact_token_request_attempt_and_protocol() {
    let expected = identity();
    let token = CancellationToken::new("verify-token").unwrap();
    let valid = cancel_frame(protocol::VERSION, expected.clone(), "verify-token");
    assert_eq!(
        receive_control(&mut Cursor::new(valid), &expected, &token),
        Ok(ControlEnd::Cancelled)
    );

    let mut other_request = expected.clone();
    other_request.request_id = RequestId::new("other-request").unwrap();
    let mut other_attempt = expected.clone();
    other_attempt.attempt_id = AttemptId::new("other-attempt").unwrap();
    for (actual, actual_token) in [
        (expected.clone(), "other-token"),
        (other_request, "verify-token"),
        (other_attempt, "verify-token"),
    ] {
        let bytes = cancel_frame(protocol::VERSION, actual, actual_token);
        assert_eq!(
            receive_control(&mut Cursor::new(bytes), &expected, &token).unwrap_err(),
            "verification cancellation identity or token differs"
        );
    }
    let bytes = cancel_frame(protocol::VERSION + 1, expected.clone(), "verify-token");
    assert!(
        receive_control(&mut Cursor::new(bytes), &expected, &token)
            .unwrap_err()
            .contains("unsupported finished-file verification protocol")
    );
}

#[test]
fn eof_and_truncated_controls_never_acknowledge_cancellation() {
    let expected = identity();
    let token = CancellationToken::new("verify-token").unwrap();
    assert_eq!(
        receive_control(&mut Cursor::new(Vec::<u8>::new()), &expected, &token).unwrap_err(),
        "verification host closed its control stream"
    );
    let complete = cancel_frame(protocol::VERSION, expected.clone(), "verify-token");
    for length in [1, 2, 3, 4, 5, complete.len() - 1] {
        assert!(
            receive_control(&mut Cursor::new(&complete[..length]), &expected, &token).is_err(),
            "a {length}-byte prefix must not acknowledge cancellation"
        );
    }
}

#[test]
fn binary_hash_covers_all_chunks_without_moving_the_descriptor_position() {
    let bytes = binary_payload();
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(&bytes).unwrap();
    file.seek(SeekFrom::Start(4097)).unwrap();
    let actual = hash(
        &file,
        u64::try_from(bytes.len()).unwrap(),
        &AtomicBool::new(false),
        deadline(),
    )
    .unwrap();
    assert_eq!(actual, expected_hash(&bytes));
    assert_eq!(file.stream_position().unwrap(), 4097);
    let mut next = [0; 17];
    file.read_exact(&mut next).unwrap();
    assert_eq!(next, bytes[4097..4114]);
}

#[test]
fn hashing_keeps_the_open_file_identity_after_its_path_is_replaced() {
    let workspace = tempfile::tempdir().unwrap();
    let path = workspace.path().join("movie.mp4");
    let bytes = binary_payload();
    std::fs::write(&path, &bytes).unwrap();
    let file = File::open(&path).unwrap();
    std::fs::rename(&path, workspace.path().join("retained.mp4")).unwrap();
    let replacement = vec![0x55; bytes.len()];
    std::fs::write(&path, &replacement).unwrap();
    let actual = hash(
        &file,
        u64::try_from(bytes.len()).unwrap(),
        &AtomicBool::new(false),
        deadline(),
    )
    .unwrap();
    assert_eq!(actual, expected_hash(&bytes));
    assert_ne!(actual, expected_hash(&replacement));
}

#[test]
fn hashing_rejects_both_shrink_and_growth_against_the_captured_extent() {
    let bytes = binary_payload();
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(&bytes).unwrap();
    let captured = u64::try_from(bytes.len()).unwrap();
    let cancelled = AtomicBool::new(false);
    assert_eq!(
        hash(&file, captured, &cancelled, deadline()).unwrap(),
        expected_hash(&bytes)
    );
    for changed in [captured - 1, captured + 1] {
        file.set_len(changed).unwrap();
        assert_eq!(
            hash(&file, captured, &cancelled, deadline()).unwrap_err(),
            "verification input extent changed before hashing"
        );
    }
}

#[test]
fn hashing_honors_cancellation_and_deadline_even_for_empty_files() {
    let cancelled = AtomicBool::new(true);
    for bytes in [Vec::new(), binary_payload()] {
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(&bytes).unwrap();
        let length = u64::try_from(bytes.len()).unwrap();
        let position = file.stream_position().unwrap();
        cancelled.store(true, Ordering::Release);
        assert!(
            hash(&file, length, &cancelled, deadline())
                .unwrap_err()
                .contains("cancelled")
        );
        cancelled.store(false, Ordering::Release);
        assert!(
            hash(&file, length, &cancelled, Instant::now())
                .unwrap_err()
                .contains("deadline")
        );
        assert_eq!(file.stream_position().unwrap(), position);
        assert_eq!(
            hash(&file, length, &cancelled, deadline()).unwrap(),
            expected_hash(&bytes)
        );
    }
}
