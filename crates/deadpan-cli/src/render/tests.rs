use super::*;
use deadpan_core::{
    BeatNode, ColorPolicy, Command, CommandRequest, FrameDuration, FrameRate, HoldAudio,
    HoldRecipe, HoldVideo, NodeId, PresentationBasis, ProjectId, RevisionId, Subtree,
};
use deadpan_jobs::render::{
    RenderBFrames, RenderEncoder, RenderEngineeringPolicy, RenderIntent, RenderSelection,
};
use std::sync::atomic::AtomicBool;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn document() -> TestResult<ProjectDocument> {
    let empty = ProjectDocument::new(
        ProjectId::new("render-project")?,
        RevisionId::new("initial")?,
        PresentationBasis {
            width: 319,
            height: 181,
            frame_rate: FrameRate::new(30_000, 1001)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?;
    let edit = deadpan_core::apply(
        &empty,
        &CommandRequest {
            project_id: empty.project_id().clone(),
            expected_revision: empty.revision_id().clone(),
            new_revision: RevisionId::new("committed")?,
            command: Command::Insert {
                parent: empty.root().clone(),
                index: 0,
                subtree: Subtree {
                    root: NodeId::new("hold")?,
                    nodes: BTreeMap::from([(
                        NodeId::new("hold")?,
                        BeatNode::hold(
                            "Silent hold",
                            HoldRecipe {
                                picture_context: None,
                                duration: FrameDuration::new(128)?,
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
    Ok(edit.forward.apply(&empty)?)
}

#[test]
fn public_start_uses_automatic_policy_and_fresh_independent_identities() {
    let context = RenderContext {
        project_id: deadpan_core::ProjectId::new("project").unwrap(),
        revision_id: deadpan_core::RevisionId::new("revision").unwrap(),
    };
    let first = start_request(&context, "/tmp/first.mp4".into(), Instant::now()).unwrap();
    let second = start_request(&context, "/tmp/second.mp4".into(), Instant::now()).unwrap();
    assert_eq!(first.revision, context.revision_id);
    assert!(first.range.is_none());
    assert!(first.policy.is_automatic());
    assert_eq!(
        first.policy.automatic().unwrap().algorithm,
        RenderAutomaticAlgorithm::AutomaticSdrV1
    );
    assert_ne!(first.identity.job_id, second.identity.job_id);
    assert_ne!(first.identity.attempt_id, second.identity.attempt_id);
    assert_ne!(
        first.identity.cancellation_token,
        second.identity.cancellation_token
    );
    assert_ne!(
        first.publication.publication_id,
        second.publication.publication_id
    );
    assert_ne!(first.publication.operation_id, first.identity.attempt_id);
    assert_ne!(
        first.publication.cancellation_token,
        first.identity.cancellation_token
    );
    for path in [
        "relative.mp4",
        "/tmp/../output.mp4",
        "/tmp/output.mov",
        "/tmp/.deadpan-bad.mp4",
    ] {
        assert!(
            start_request(&context, path.into(), Instant::now()).is_err(),
            "{path}"
        );
    }
}

#[test]
fn public_summary_preserves_authored_geometry_and_origin_based_samples() -> TestResult {
    let document = document()?;
    let summary = output_summary(&document)?;
    assert_eq!(summary.canvas, [319, 181]);
    assert_eq!(summary.raster, [318, 180]);
    assert_eq!(summary.frame_count, 128);
    assert_eq!(summary.audio_samples, 205_005);
    assert_eq!(summary.frame_rate, document.presentation_basis().frame_rate);
    assert_eq!(document.presentation_basis().width, 319);
    let limits = default_limits()?;
    assert_eq!(
        limits.encode.encode.maximum_output_bytes,
        limits.verification.maximum_bytes
    );
    assert_eq!(limits.media.maximum_movie_bytes(), MAX_RENDER_MOVIE_BYTES);
    assert_eq!(
        limits.media.maximum_manifest_bytes(),
        MAX_RENDER_MANIFEST_BYTES
    );
    assert_eq!(
        limits.media.maximum_namespace_bytes(),
        RETAINED_NAMESPACE_BYTES
    );
    assert_eq!(limits.timeout, RENDER_TIMEOUT);
    Ok(())
}

#[test]
fn request_wire_rejects_policy_runtime_identity_overrides_and_unknown_versions() -> TestResult {
    let request = RenderRequest {
        schema_version: 1,
        request_id: RequestId::new("command")?,
        context: RenderContext::from_document(&document()?),
        operation: RenderOperation::Start {
            destination: "/tmp/result.mp4".into(),
        },
    };
    let wire = serde_json::to_value(&request)?;
    assert!(RenderRequest::from_json(&serde_json::to_vec(&wire)?).is_ok());
    for field in ["policy", "runtime", "job_id", "attempt_id", "range"] {
        let mut changed = wire.clone();
        changed["operation"][field] = serde_json::json!("foreign");
        assert!(
            RenderRequest::from_json(&serde_json::to_vec(&changed)?).is_err(),
            "{field}"
        );
    }
    let mut changed = wire.clone();
    changed["schema_version"] = serde_json::json!(2);
    assert_eq!(
        RenderRequest::from_json(&serde_json::to_vec(&changed)?)
            .unwrap_err()
            .code,
        "RenderProtocolUnsupported"
    );
    let mut changed = wire;
    changed.as_object_mut().unwrap().remove("request_id");
    assert!(RenderRequest::from_json(&serde_json::to_vec(&changed)?).is_err());
    assert!(RenderRequest::from_json(&vec![b' '; MAX_REQUEST_BYTES + 1]).is_err());
    Ok(())
}

#[test]
fn retry_builders_keep_original_job_policy_and_reject_foreign_context() -> TestResult {
    let root = tempfile::tempdir()?;
    let document = document()?;
    let mut store = ProjectStore::create(&root.path().join("retry.deadpan"), &document)?;
    let context = RenderContext::from_document(&document);
    let start = start_request(&context, root.path().join("output.mp4"), Instant::now())?;
    let intent = RenderIntent {
        schema_version: 2,
        job_id: start.identity.job_id.clone(),
        project_id: context.project_id.clone(),
        revision_id: context.revision_id.clone(),
        document_sha256: deadpan_jobs::render::document_sha256_for_validation(&document)?,
        range: deadpan_core::FrameRange::new(ProjectFrame(0), ProjectFrame(128))?,
        policy: start.policy,
    };
    store.create_render_job(
        intent.clone(),
        &AtomicBool::new(false),
        Instant::now() + Duration::from_secs(5),
    )?;
    let retry = retry_request(
        &store,
        &context,
        &intent.job_id,
        None,
        root.path().join("retry.mp4"),
        Instant::now(),
    )?;
    assert_eq!(retry.identity.job_id, intent.job_id);
    assert_ne!(retry.identity.attempt_id, start.identity.attempt_id);
    assert!(retry.checkpoint_attempt_id.is_none());
    assert!(
        retry_request(
            &store,
            &context,
            &intent.job_id,
            Some(&AttemptId::new("missing")?),
            root.path().join("retry.mp4"),
            Instant::now()
        )
        .is_err()
    );
    let foreign = RenderContext {
        project_id: ProjectId::new("foreign")?,
        ..context.clone()
    };
    assert_eq!(
        retry_request(
            &store,
            &foreign,
            &intent.job_id,
            None,
            root.path().join("retry.mp4"),
            Instant::now()
        )
        .unwrap_err()
        .code,
        "RenderProjectChanged"
    );
    let engineering = RenderIntent {
        schema_version: 1,
        job_id: RequestId::new("engineering")?,
        policy: RenderPolicy::Engineering(RenderEngineeringPolicy {
            schema_version: 1,
            selection: RenderSelection::ExplicitEngineering,
            encoder: RenderEncoder::Software,
            b_frames: RenderBFrames::None,
        }),
        ..intent
    };
    store.create_render_job(
        engineering.clone(),
        &AtomicBool::new(false),
        Instant::now() + Duration::from_secs(5),
    )?;
    assert_eq!(
        retry_request(
            &store,
            &context,
            &engineering.job_id,
            None,
            root.path().join("retry.mp4"),
            Instant::now()
        )
        .unwrap_err()
        .code,
        "RenderEngineeringJob"
    );
    assert_eq!(store.snapshot()?, document);
    assert!(store.render_attempts(&engineering.job_id, 0, 1)?.is_empty());
    Ok(())
}
