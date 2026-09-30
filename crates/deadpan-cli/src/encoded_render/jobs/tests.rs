//! Historical manifests are data. No test below constructs live encoder,
//! verified-media or publication authority from the retained measurements.
use super::*;
use deadpan_core::*;
use deadpan_jobs::render::{
    RenderAutomaticAlgorithm, RenderAutomaticPolicy, RenderAutomaticSelection, RenderSelection,
    admission::RenderProbeOutcome,
};
use std::{collections::BTreeMap, time::Duration};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

struct Fixture {
    _scratch: tempfile::TempDir,
    contract: ExportPictureContract,
    intent: RenderIntent,
    decision: RenderEncodingDecision,
    encoded: EncodedManifest,
}

fn fixture() -> Result<Fixture> {
    let mut decision = RenderEncodingDecision::from_json(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../deadpan-jobs/src/render/admission/tests/measured-decision-v1.json"
    )))?;
    let selected = decision.selected().ok_or("missing selected fixture")?;
    let mut encoded: EncodedManifest =
        serde_json::from_value(serde_json::to_value(&selected.manifest)?)?;
    let frames = i64::try_from(selected.manifest.contract.picture.frame_count)?;
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("manifest.deadpan");
    let empty = ProjectDocument::new(
        ProjectId::new("manifest-project")?,
        RevisionId::new("empty")?,
        PresentationBasis {
            width: 320,
            height: 180,
            frame_rate: FrameRate::new(30_000, 1001)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?;
    let transaction = apply(
        &empty,
        &CommandRequest {
            project_id: empty.project_id().clone(),
            expected_revision: empty.revision_id().clone(),
            new_revision: RevisionId::new("manifest-baseline")?,
            command: Command::Insert {
                parent: empty.root().clone(),
                index: 0,
                subtree: Subtree {
                    root: NodeId::new("background")?,
                    nodes: BTreeMap::from([(
                        NodeId::new("background")?,
                        BeatNode::hold(
                            "background",
                            HoldRecipe {
                                picture_context: None,
                                duration: FrameDuration::new(frames)?,
                                video: HoldVideo::Background,
                                audio: HoldAudio::Silence,
                            },
                        ),
                    )]),
                    overrides: BTreeMap::new(),
                    gap_overrides: BTreeMap::new(),
                },
            },
        },
    )?;
    let document = transaction.forward.apply(&empty)?;
    let store = ProjectStore::create(&package, &document)?;
    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(30);
    let pictures =
        ProjectPictureSession::open_revision(&package, document.revision_id(), None, &cancelled)?;
    let contract = ExportPictureContract::capture(&pictures)?;
    let intent = RenderIntent {
        schema_version: 2,
        job_id: decision.job_id.clone(),
        project_id: contract.project_id().clone(),
        revision_id: contract.revision_id().clone(),
        document_sha256: document_hash(&document, &cancelled, deadline)?,
        range: contract.range(),
        policy: RenderAutomaticPolicy {
            schema_version: 1,
            selection: RenderAutomaticSelection::Automatic,
            algorithm: RenderAutomaticAlgorithm::AutomaticSdrV1,
        }
        .into(),
    };
    decision.output = serde_json::from_value(serde_json::to_value(&contract)?)?;
    decision.document_sha256 = intent.document_sha256.clone();
    decision.validate_for(&intent, &decision.encoding_attempt_id)?;
    encoded.document_sha256 = intent.document_sha256.clone();
    encoded.contract = EncodedRenderContract::from_contract(&contract, encoded.contract.choice);
    encoded.validate().map_err(EncodedRenderError::Protocol)?;
    drop(store);
    Ok(Fixture {
        _scratch: scratch,
        contract,
        intent,
        decision,
        encoded,
    })
}

fn automatic(fixture: &Fixture) -> Result<RetainedRenderManifest> {
    Ok(RetainedRenderManifest::V2(Box::new(
        RetainedRenderManifestV2 {
            schema_version: 2,
            intent: fixture.intent.clone(),
            encoding_attempt_id: fixture.decision.encoding_attempt_id.clone(),
            encoded: fixture.encoded.clone(),
            encoding_decision: fixture.decision.clone(),
            encoding_binding: durable::binding_for_decision(
                &fixture.intent,
                &fixture.decision.encoding_attempt_id,
                &fixture.contract,
                &fixture.decision,
            )?,
        },
    )))
}

fn validate(
    manifest: &RetainedRenderManifest,
    fixture: &Fixture,
    decision: Option<&RenderEncodingDecision>,
) -> bool {
    manifest
        .validate(
            &fixture.intent,
            &fixture.decision.encoding_attempt_id,
            &fixture.contract,
            decision,
        )
        .is_ok()
}

#[test]
fn automatic_manifest_requires_exact_original_decision_and_completion_binding() -> Result {
    let fixture = fixture()?;
    let original = automatic(&fixture)?;
    assert!(validate(&original, &fixture, Some(&fixture.decision)));
    let decoded: RetainedRenderManifest = serde_json::from_slice(&manifest_bytes(&original)?)?;
    assert!(validate(&decoded, &fixture, Some(&fixture.decision)));
    assert!(!validate(&decoded, &fixture, None));

    // Same selected encoder/runtime/controls, different rejected-probe evidence.
    let mut different = fixture.decision.clone();
    let RenderProbeOutcome::Rejected { failure, .. } = &mut different.probes[0].result else {
        return Err("measured fixture lacks the hardware rejection".into());
    };
    failure.diagnostic = deadpan_jobs::Diagnostic::new("different recorded rejection")?;
    different.validate()?;
    assert!(!validate(&original, &fixture, Some(&different)));

    let mut changed_runtime = original.clone();
    let RetainedRenderManifest::V2(value) = &mut changed_runtime else {
        unreachable!()
    };
    value.encoding_binding.runtime.images[0].sha256 = deadpan_jobs::Sha256::new("9".repeat(64))?;
    assert!(!validate(
        &changed_runtime,
        &fixture,
        Some(&fixture.decision)
    ));

    let mut changed_settings = original.clone();
    let RetainedRenderManifest::V2(value) = &mut changed_settings else {
        unreachable!()
    };
    value.encoding_binding.settings.video_bitrate += 1;
    assert!(!validate(
        &changed_settings,
        &fixture,
        Some(&fixture.decision)
    ));

    let mut changed_owner = original;
    let RetainedRenderManifest::V2(value) = &mut changed_owner else {
        unreachable!()
    };
    value.encoding_attempt_id = AttemptId::new("verification-retry-is-not-encoding-owner")?;
    assert!(!validate(&changed_owner, &fixture, Some(&fixture.decision)));
    Ok(())
}

#[test]
fn automatic_manifest_never_accepts_missing_evidence_or_future_fields() -> Result {
    let fixture = fixture()?;
    let original = serde_json::to_value(automatic(&fixture)?)?;
    for key in ["encoding_decision", "encoding_binding"] {
        let mut missing = original.clone();
        missing
            .as_object_mut()
            .ok_or("manifest object")?
            .remove(key);
        assert!(serde_json::from_value::<RetainedRenderManifest>(missing).is_err());
        let mut null = original.clone();
        null[key] = serde_json::Value::Null;
        assert!(serde_json::from_value::<RetainedRenderManifest>(null).is_err());
    }
    let mut extra = original;
    extra["future_authority"] = true.into();
    assert!(serde_json::from_value::<RetainedRenderManifest>(extra).is_err());
    Ok(())
}

#[test]
fn legacy_manifest_bytes_keep_frozen_four_field_shape_and_reject_automatic_policy() -> Result {
    let mut fixture = fixture()?;
    let selected = fixture.decision.selected().ok_or("selected probe")?;
    fixture.intent.schema_version = 1;
    fixture.intent.policy = RenderEngineeringPolicy {
        schema_version: 1,
        selection: RenderSelection::ExplicitEngineering,
        encoder: selected.spec.choice.mode,
        b_frames: selected.spec.choice.b_frames,
    }
    .into();
    let old = RetainedRenderManifestV1 {
        schema_version: 1,
        intent: fixture.intent.clone(),
        encoding_attempt_id: fixture.decision.encoding_attempt_id.clone(),
        encoded: fixture.encoded.clone(),
    };
    let old_bytes = serde_json::to_vec(&old)?;
    let manifest = RetainedRenderManifest::V1(Box::new(old));
    assert_eq!(manifest_bytes(&manifest)?, old_bytes);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&old_bytes)?
            .as_object()
            .ok_or("object")?
            .len(),
        4
    );
    assert!(validate(&manifest, &fixture, None));
    assert!(!validate(&manifest, &fixture, Some(&fixture.decision)));
    let mut wire: serde_json::Value = serde_json::from_slice(&old_bytes)?;
    wire["intent"]["schema_version"] = 2.into();
    wire["intent"]["policy"] = serde_json::to_value(RenderAutomaticPolicy {
        schema_version: 1,
        selection: RenderAutomaticSelection::Automatic,
        algorithm: RenderAutomaticAlgorithm::AutomaticSdrV1,
    })?;
    assert!(serde_json::from_value::<RetainedRenderManifest>(wire).is_err());
    let mut extra: serde_json::Value = serde_json::from_slice(&old_bytes)?;
    extra["encoding_decision"] = serde_json::Value::Null;
    assert!(serde_json::from_value::<RetainedRenderManifest>(extra).is_err());
    Ok(())
}
