use std::fs;

use deadpan_jobs::WorkspaceArtifact;
use sha2::{Digest, Sha256 as Sha256Hasher};

use super::*;
use crate::render_worker::protocol::RenderPixelPolicy;

#[test]
fn validation_copy_interruption_keeps_the_same_host_cancellation_and_deadline_outcome() {
    assert!(matches!(
        artifact_failure(ArtifactError::Interrupted(SnapshotInterruption::Cancelled)),
        RenderWorkerError::Cancelled
    ));
    assert!(matches!(
        artifact_failure(ArtifactError::Interrupted(SnapshotInterruption::Deadline)),
        RenderWorkerError::Deadline
    ));
    assert!(matches!(
        artifact_failure(ArtifactError::SourceMutated),
        RenderWorkerError::Artifact(ArtifactError::SourceMutated)
    ));
}

fn snapshot(
    directory: &std::path::Path,
    bytes: &[u8],
) -> (HashedArtifactSnapshot, WorkspaceArtifact) {
    fs::create_dir(directory.join("output")).unwrap();
    fs::write(directory.join(protocol::PICTURE_REF), bytes).unwrap();
    let hex: String = Sha256Hasher::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let declaration = WorkspaceArtifact::new(
        WorkspaceRef::new(protocol::PICTURE_REF).unwrap(),
        Sha256::new(hex).unwrap(),
        u64::try_from(bytes.len()).unwrap(),
    )
    .unwrap();
    let snapshot = ArtifactWorkspace::open(directory)
        .unwrap()
        .snapshot(
            &WorkspaceRef::new(protocol::OUTPUT_SCOPE).unwrap(),
            &declaration,
            ArtifactLimits::new(100).unwrap(),
        )
        .unwrap();
    (snapshot, declaration)
}

#[test]
fn every_plane_and_frame_is_checked_before_an_owned_range_can_be_read() {
    let project = tempfile::tempdir().unwrap();
    let pictures =
        crate::render_worker::tests::pictures(&project.path().join("pictures.deadpan"), "pixels");
    let contract = ExportPictureContract::capture(&pictures).unwrap();
    let legal = [16, 235, 16, 235, 128, 128, 16, 16, 235, 235, 16, 240];
    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(5);
    for (index, code, expected_frame, expected_plane) in [
        (0, 0, 0, "Y"),
        (6, 236, 1, "Y"),
        (10, 0, 1, "Cb"),
        (11, 241, 1, "Cr"),
    ] {
        let scratch = tempfile::tempdir().unwrap();
        let mut bytes = legal;
        bytes[index] = code;
        let (mut snapshot, _) = snapshot(scratch.path(), &bytes);
        assert!(
            matches!(validate_planes(&mut snapshot, &contract, &cancelled, deadline), Err(RenderWorkerError::InvalidPixels {frame,plane}) if frame == expected_frame && plane == expected_plane)
        );
    }
    let scratch = tempfile::tempdir().unwrap();
    let (mut snapshot, declaration) = snapshot(scratch.path(), &legal);
    validate_planes(&mut snapshot, &contract, &cancelled, deadline).unwrap();
    let hash = document_sha256(&pictures, &cancelled, deadline).unwrap();
    let manifest = RenderManifest {
        contract: RenderContract::from_contract(&contract),
        document_sha256: hash.clone(),
        planes: declaration,
        pixel_policy: RenderPixelPolicy::I420Rec709LimitedLeft,
    };
    let mut prepared = PreparedPictureRange {
        contract,
        document_sha256: hash,
        manifest,
        snapshot,
        frame_bytes: 6,
    };
    fs::write(scratch.path().join(protocol::PICTURE_REF), [255; 12]).unwrap();
    drop(scratch);
    drop(pictures);
    drop(project);
    let mut bytes = [0; 6];
    let timing = prepared
        .read_frame(OutputFrameOrdinal(1), &mut bytes)
        .unwrap();
    assert_eq!(bytes, legal[6..]);
    assert_eq!(timing.pts(), 1001);
    assert!(
        prepared
            .read_frame(OutputFrameOrdinal(2), &mut bytes)
            .is_err()
    );
    assert!(
        prepared
            .read_frame(OutputFrameOrdinal(0), &mut [0; 5])
            .is_err()
    );
}
