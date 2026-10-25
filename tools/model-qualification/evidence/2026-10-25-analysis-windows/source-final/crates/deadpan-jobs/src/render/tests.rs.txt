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
        policy: RenderPolicy::Engineering(RenderEngineeringPolicy {
            schema_version: 1,
            selection: RenderSelection::ExplicitEngineering,
            encoder: RenderEncoder::Software,
            b_frames: RenderBFrames::None,
        }),
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
fn engineering_wire_is_unchanged_and_frozen_parser_never_admits_automatic() {
    let original = intent();
    let bytes = serde_json::to_vec(&original).unwrap();
    let value = serde_json::to_value(&original).unwrap();
    assert_eq!(
        value["policy"],
        serde_json::json!({
            "schema_version": 1,
            "selection": "explicit_engineering",
            "encoder": "software",
            "b_frames": "none"
        })
    );
    assert_eq!(parse_render_intent_v1(&bytes).unwrap(), original);
    assert_eq!(
        serde_json::to_vec(&parse_render_intent_v1(&bytes).unwrap()).unwrap(),
        bytes
    );

    let mut automatic = original;
    automatic.schema_version = 2;
    automatic.policy = RenderPolicy::Automatic(RenderAutomaticPolicy {
        schema_version: 1,
        selection: RenderAutomaticSelection::Automatic,
        algorithm: RenderAutomaticAlgorithm::AutomaticSdrV1,
    });
    automatic.validate().unwrap();
    let bytes = serde_json::to_vec(&automatic).unwrap();
    assert_eq!(
        serde_json::from_slice::<RenderIntent>(&bytes).unwrap(),
        automatic
    );
    assert!(parse_render_intent_v1(&bytes).is_err());
    assert!(automatic.policy.engineering().is_none());
    assert!(automatic.policy.is_automatic());
}

#[test]
fn automatic_grammar_rejects_mixed_versions_loose_controls_and_null_fields() {
    let mut value = serde_json::to_value(intent()).unwrap();
    value["schema_version"] = 2.into();
    value["policy"] = serde_json::json!({
        "schema_version": 1,
        "selection": "automatic",
        "algorithm": "automatic_sdr_v1"
    });
    serde_json::from_value::<RenderIntent>(value.clone()).unwrap();
    for (pointer, replacement) in [
        ("/schema_version", serde_json::json!(1)),
        ("/schema_version", serde_json::json!(3)),
        ("/policy/schema_version", serde_json::json!(2)),
        (
            "/policy/selection",
            serde_json::json!("explicit_engineering"),
        ),
        ("/policy/algorithm", serde_json::json!("automatic_sdr_v2")),
        ("/policy/algorithm", serde_json::Value::Null),
    ] {
        let mut changed = value.clone();
        *changed.pointer_mut(pointer).unwrap() = replacement;
        assert!(
            serde_json::from_value::<RenderIntent>(changed).is_err(),
            "{pointer}"
        );
    }
    for field in ["encoder", "b_frames", "decision", "runtime", "bitrate"] {
        let mut changed = value.clone();
        changed["policy"][field] = serde_json::Value::Null;
        assert!(
            serde_json::from_value::<RenderIntent>(changed).is_err(),
            "{field}"
        );
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct LegacyEnvelope {
        #[serde(deserialize_with = "deserialize_render_intent_v1")]
        intent: RenderIntent,
    }
    assert!(serde_json::from_value::<LegacyEnvelope>(serde_json::json!({"intent":value})).is_err());
    let legacy: LegacyEnvelope =
        serde_json::from_value(serde_json::json!({"intent":intent()})).unwrap();
    assert_eq!(legacy.intent, intent());
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
