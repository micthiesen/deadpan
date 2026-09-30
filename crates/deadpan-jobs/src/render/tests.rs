use super::*;
use deadpan_core::{ColorPolicy, FrameRate, NodeId, PresentationBasis, ProjectFrame};
use std::time::Duration;

fn document() -> ProjectDocument {
    ProjectDocument::new(
        ProjectId::new("project").unwrap(),
        RevisionId::new("initial").unwrap(),
        PresentationBasis {
            width: 640,
            height: 360,
            frame_rate: FrameRate::new(30, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root").unwrap(),
    )
    .unwrap()
}
fn intent() -> RenderIntent {
    RenderIntent {
        schema_version: 1,
        job_id: RequestId::new("job").unwrap(),
        project_id: ProjectId::new("project").unwrap(),
        revision_id: RevisionId::new("initial").unwrap(),
        document_sha256: Sha256::new("a".repeat(64)).unwrap(),
        range: FrameRange::new(ProjectFrame(0), ProjectFrame(1)).unwrap(),
        policy: RenderEngineeringPolicy {
            schema_version: 1,
            selection: RenderSelection::ExplicitEngineering,
            encoder: RenderEncoder::Software,
            b_frames: RenderBFrames::None,
        },
    }
}
#[test]
fn document_identity_preserves_exact_serialization_and_controls() {
    let document = document();
    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(5);
    let expected = Hasher::digest(serde_json::to_vec(&document).unwrap());
    assert_eq!(
        document_sha256(&document, &cancelled, deadline)
            .unwrap()
            .as_str(),
        expected
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    assert_eq!(
        document_sha256_for_validation(&document).unwrap(),
        document_sha256(&document, &cancelled, deadline).unwrap()
    );
    assert!(matches!(
        document_sha256(&document, &cancelled, Instant::now()),
        Err(RenderError::Deadline)
    ));
    cancelled.store(true, Ordering::Release);
    assert!(matches!(
        document_sha256(&document, &cancelled, deadline),
        Err(RenderError::Cancelled)
    ));
}
#[test]
fn strict_intent_rejects_new_policy_and_foreign_fields() {
    let value = serde_json::to_value(intent()).unwrap();
    for path in ["schema_version", "policy"] {
        let mut invalid = value.clone();
        if path == "policy" {
            invalid[path]["schema_version"] = 2.into();
        } else {
            invalid[path] = 2.into();
        }
        assert!(serde_json::from_value::<RenderIntent>(invalid).is_err());
    }
    let mut invalid = value;
    invalid["destination"] = "not-authorized.mp4".into();
    assert!(serde_json::from_value::<RenderIntent>(invalid).is_err());
    let mut invalid = intent();
    invalid.range = FrameRange::new(ProjectFrame(-1), ProjectFrame(1)).unwrap();
    assert!(invalid.validate().is_err());
}
#[test]
fn lifecycle_requires_checkpoint_and_confirmed_teardown() {
    use RenderAttemptState::*;
    for (from, to, checkpoint) in [
        (Queued, Encoding, false),
        (Encoding, EncodedRetained, true),
        (EncodedRetained, Verifying, true),
        (Verifying, Verified, true),
        (Queued, Verifying, true),
        (Encoding, Cancelling, false),
        (Cancelling, Cancelled, false),
        (Cancelling, Interrupted, true),
    ] {
        validate_transition(from, to, checkpoint).unwrap();
    }
    for (from, to, checkpoint) in [
        (Queued, Verified, true),
        (Queued, Encoding, true),
        (Queued, Verifying, false),
        (Encoding, EncodedRetained, false),
        (Encoding, Cancelled, false),
        (Cancelling, Verified, true),
        (Cancelled, Queued, false),
        (Verified, Verifying, true),
        (Interrupted, Encoding, false),
    ] {
        assert!(
            validate_transition(from, to, checkpoint).is_err(),
            "{from:?} -> {to:?}"
        );
    }
}
#[test]
fn verification_evidence_is_bounded_and_versioned() {
    let mut observation = RenderVerificationObservation {
        schema_version: 1,
        validator_id: "validator".into(),
        validator_version: "v1".into(),
        movie_sha256: Sha256::new("b".repeat(64)).unwrap(),
        movie_byte_length: 1,
        report: serde_json::json!({"observed":true}),
    };
    observation.validate().unwrap();
    observation.report = serde_json::json!({"oversized":"x".repeat(MAX_RENDER_REPORT_BYTES)});
    assert!(observation.validate().is_err());
    observation.report = serde_json::json!([]);
    assert!(observation.validate().is_err());
    observation.report = serde_json::json!({});
    observation.schema_version = 2;
    assert!(observation.validate().is_err());
}
