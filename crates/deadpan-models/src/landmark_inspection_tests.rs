use super::*;
use std::io::Cursor;

#[test]
fn worker_input_copy_checks_both_identities_and_restores_publication_reader() {
    let bytes = b"retained input bytes";
    let object = GeneratedObjectRef::new(
        deadpan_core::GeneratedContentId::new(blake3::hash(bytes).to_hex().to_string()).unwrap(),
        bytes.len() as u64,
    )
    .unwrap();
    let hash: [u8; 32] = sha2::Sha256::digest(bytes).into();
    let cancelled = AtomicBool::new(false);
    let control = Control {
        deadline: Instant::now() + Duration::from_secs(10),
        cancelled: &cancelled,
    };
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("input")).unwrap();
    let mut reader = Cursor::new(bytes);
    reader.set_position(4);
    let copied = copy_input(
        &mut reader,
        &object,
        hash,
        directory.path(),
        "valid",
        1024,
        &control,
    )
    .unwrap();
    assert_eq!(reader.position(), 0);
    assert_eq!(copied.byte_length(), bytes.len() as u64);
    assert_eq!(
        std::fs::read(directory.path().join("input/valid")).unwrap(),
        bytes
    );
    reader.set_position(5);
    assert!(
        copy_input(
            &mut reader,
            &object,
            [0; 32],
            directory.path(),
            "wrong-sha",
            1024,
            &control
        )
        .is_err()
    );
    assert_eq!(reader.position(), 0);
    let wrong_object = GeneratedObjectRef::new(
        deadpan_core::GeneratedContentId::new("a".repeat(64)).unwrap(),
        bytes.len() as u64,
    )
    .unwrap();
    assert!(
        copy_input(
            &mut reader,
            &wrong_object,
            hash,
            directory.path(),
            "wrong-blake",
            1024,
            &control
        )
        .is_err()
    );
    assert_eq!(reader.position(), 0);
    let wrong_length =
        GeneratedObjectRef::new(object.content().clone(), bytes.len() as u64 - 1).unwrap();
    assert!(
        copy_input(
            &mut reader,
            &wrong_length,
            hash,
            directory.path(),
            "length",
            1024,
            &control
        )
        .is_err()
    );
    assert_eq!(reader.position(), 0);
    cancelled.store(true, Ordering::Release);
    assert!(matches!(
        copy_input(
            &mut reader,
            &object,
            hash,
            directory.path(),
            "cancelled",
            1024,
            &control
        ),
        Err(QualificationError::Cancelled)
    ));
    assert!(!directory.path().join("input/cancelled").exists());
}

#[test]
fn inspection_deadline_and_frame_budget_precede_worker_launch() {
    let cancelled = AtomicBool::new(false);
    let control = Control {
        deadline: Instant::now(),
        cancelled: &cancelled,
    };
    assert!(matches!(
        control.remaining(),
        Err(QualificationError::Deadline)
    ));
    let mut contract = VideoContract {
        width: 4,
        height: 2,
        frames: 3,
        rate_num: 24,
        rate_den: 1,
    };
    assert_eq!(picture_pts(contract).unwrap(), [0, 42, 83]);
    for frames in [0, 1, 1026, u32::MAX] {
        contract.frames = frames;
        assert!(picture_pts(contract).is_err());
    }
}
