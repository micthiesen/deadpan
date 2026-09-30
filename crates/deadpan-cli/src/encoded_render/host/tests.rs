use super::*;

#[test]
fn snapshot_interruption_retains_the_public_cancellation_and_deadline_outcomes() {
    assert!(matches!(
        artifact_failure(ArtifactError::Interrupted(SnapshotInterruption::Cancelled)),
        EncodedRenderError::Cancelled
    ));
    assert!(matches!(
        artifact_failure(ArtifactError::Interrupted(SnapshotInterruption::Deadline)),
        EncodedRenderError::Deadline
    ));
    assert!(matches!(
        artifact_failure(ArtifactError::SourceMutated),
        EncodedRenderError::Artifact(ArtifactError::SourceMutated)
    ));
}

#[test]
fn artifact_admission_can_retain_the_native_maximum_without_the_raw_picture_spool_limit() {
    let limits = EncodedWorkerLimits::default();
    assert!(limits.encode.maximum_output_bytes > crate::render_worker::protocol::MAX_PICTURE_BYTES);
    assert_eq!(
        ArtifactLimits::new(limits.encode.maximum_output_bytes)
            .unwrap()
            .maximum_bytes(),
        deadpan_encode::MAX_OUTPUT_BYTES
    );
}
