use std::io::Cursor;
use std::time::Duration;

use super::*;

#[test]
fn destination_readback_requires_the_exact_extent_and_unmodified_bytes() {
    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(5);
    let bytes: Vec<_> = (0_u8..=255).cycle().take(2 * BUFFER_BYTES + 31).collect();
    assert_eq!(
        hash_reader(
            Cursor::new(&bytes),
            bytes.len() as u64,
            &cancelled,
            deadline
        )
        .unwrap(),
        digest(&bytes)
    );
    let error = hash_reader(
        Cursor::new(&bytes),
        bytes.len() as u64 - 1,
        &cancelled,
        deadline,
    )
    .unwrap_err();
    assert_eq!(error.code, "destination_length_mismatch");
    let error = hash_reader(
        Cursor::new(&bytes),
        bytes.len() as u64 + 1,
        &cancelled,
        deadline,
    )
    .unwrap_err();
    assert_eq!(error.code, "destination_readback_failed");
    let error = hash_reader(
        Cursor::new(&bytes),
        bytes.len() as u64,
        &AtomicBool::new(true),
        deadline,
    )
    .unwrap_err();
    assert_eq!(error.code, "cancelled");
    let error = hash_reader(
        Cursor::new(&bytes),
        bytes.len() as u64,
        &cancelled,
        Instant::now(),
    )
    .unwrap_err();
    assert_eq!(error.code, "deadline_exceeded");
}

#[test]
fn provenance_serialization_has_a_hard_byte_ceiling() {
    let ordinary = serde_json::json!({"schema_version": 1, "name": "render"});
    let bytes = serialize_report(&ordinary).unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
        ordinary
    );
    // Escaped JSON can exceed the limit even when the input string does not.
    let escaped = "\n".repeat(MAX_REPORT_BYTES / 2);
    let error = serialize_report(&escaped).unwrap_err();
    assert_eq!(error.code, "report_serialization_failed");
    assert!(error.message.contains("16 MiB"));
}
