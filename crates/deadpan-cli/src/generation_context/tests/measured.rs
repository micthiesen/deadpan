use super::*;
use deadpan_plan::Picture;
use deadpan_store::generation_pictures::{GenerationPictureIdentity, GenerationPictures};

struct Measured(SourceFrameIndex);
impl GenerationPictures for Measured {
    fn identity(
        &self,
        document: &ProjectDocument,
        picture: &Picture,
    ) -> Result<GenerationPictureIdentity, deadpan_store::StoreError> {
        let frame = picture
            .select_source_frame(&self.0)
            .map_err(|error| deadpan_store::StoreError::GenerationPlan(error.to_string()))?;
        Ok(GenerationPictureIdentity::Original {
            qualification: SourceQualificationId::new("a".repeat(64)).unwrap(),
            frame: frame.identity,
            interpretation: match picture {
                Picture::Source { asset, .. } | Picture::Freeze { asset, .. } => {
                    document.source_interpretations().get(asset).copied()
                }
                _ => None,
            },
        })
    }
}
fn pictures() -> Measured {
    Measured(
        SourceFrameIndex::new(
            AssetId::new("video").unwrap(),
            clock(),
            [0, 14, 15, 16]
                .into_iter()
                .enumerate()
                .map(|(i, pts)| IndexedSourceFrame {
                    identity: SourceFrameId(i as u64),
                    pts: pts * 1001,
                    reported_duration: None,
                    keyframe: true,
                    seek_from: None,
                    decode_timestamp: None,
                })
                .collect(),
            300 * 1001,
            TerminalProvenance::Explicit,
        )
        .unwrap(),
    )
}
fn request(origin: &ProjectDocument) -> StoredGenerationRequest {
    use deadpan_jobs::*;
    let target = ScopedNodeTarget {
        node: id("h"),
        repeats: vec![],
    };
    let request_id = RequestId::new("measured-request").unwrap();
    StoredGenerationRequest {
        scope_id: deadpan_store::generation::GenerationScopeId::from_first_request(
            request_id.clone(),
        ),
        request_id,
        origin_revision: origin.revision_id().clone(),
        origin_target: target.clone(),
        target,
        binding: TargetBinding {
            project_id: origin.project_id().clone(),
            hold_id: id("h"),
            request_version: RequestVersion::new(1).unwrap(),
            context_sha256: deadpan_jobs::Sha256::new("a".repeat(64)).unwrap(),
        },
        constraints: HoldConstraints {
            video: VideoSpec::new(
                FrameDuration::new(6).unwrap(),
                origin.presentation_basis().frame_rate,
                512,
                320,
            )
            .unwrap(),
            conditioning: ConditioningMode::Bridge,
            motion: MotionAmount::Still,
            instructions: None,
            region_target: None,
        },
        provider: crate::generation::development_provider(1),
        plan: None,
        input_binding: None,
        relevance: Relevance::Current,
    }
}
fn with_right(right: BeatNode, revision: &str) -> ProjectDocument {
    let value = document(
        &["left", "h", "right"],
        vec![("left", source(10, 0)), ("h", hold(6)), ("right", right)],
    );
    let mut json = serde_json::to_value(value).unwrap();
    json["revision_id"] = serde_json::json!(revision);
    ProjectDocument::from_json(&json.to_string()).unwrap()
}

#[test]
fn measured_relevance_retains_identical_vfr_boundary_after_remote_span_change() {
    let origin = with_right(source(10, 10), "origin");
    let remote = with_right(source(20, 10), "remote");
    let changed = with_right(source(10, 15), "changed");
    let request = request(&origin);
    let resolver = BoundaryContextResolver::default();
    let pictures = pictures();
    let current = ContextObservation::Resolved(request.binding.context_sha256.clone());
    // The old descriptor comparison over-invalidates this edit. Both actual
    // first boundary samples fall before measured PTS 14 * 1001.
    assert_ne!(
        context_identity(&origin, &id("h")),
        context_identity(&remote, &id("h"))
    );
    assert_eq!(
        resolver.observe_with_pictures(&origin, &remote, &request, &pictures),
        current
    );
    let prepared = resolver.prepare_transition(&remote).unwrap();
    assert_eq!(
        prepared.observe_with_pictures(&origin, &remote, &request, &pictures),
        current
    );
    assert_eq!(
        resolver.observe_with_pictures(&origin, &changed, &request, &pictures),
        ContextObservation::Unresolved
    );
    assert_eq!(
        prepared.observe_with_pictures(&origin, &changed, &request, &pictures),
        ContextObservation::Unresolved
    );
}

#[test]
fn interpretation_changes_stale_captured_inputs_and_reset_restores_relevance() {
    use deadpan_store::generation_inputs::GenerationInputCapture;
    let mut wire = serde_json::to_value(with_right(source(10, 10), "origin")).unwrap();
    wire["nodes"]["h"] = serde_json::to_value(hold(12)).unwrap();
    wire["assets"]["video"]["source_qualification"] = serde_json::json!("a".repeat(64));
    wire["assets"]["unused"] = wire["assets"]["video"].clone();
    let origin = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let pictures = pictures();
    let resolver = BoundaryContextResolver::default();
    let mut request = request(&origin);
    request.constraints.video = deadpan_jobs::VideoSpec::new(
        FrameDuration::new(12).unwrap(),
        origin.presentation_basis().frame_rate,
        crate::generation::NATIVE_WIDTH,
        crate::generation::NATIVE_HEIGHT,
    )
    .unwrap();
    request.plan = Some(deadpan_jobs::GenerationPlan::Bridge(
        deadpan_jobs::BridgeGenerationPlan::new(
            FrameDuration::new(12).unwrap(),
            origin.presentation_basis().frame_rate,
            &crate::generation::development_capability(),
            crate::generation::native_dimensions(),
        )
        .unwrap(),
    ));
    request.input_binding =
        Some(GenerationInputCapture::capture(&origin, &request.target, &pictures).unwrap());
    let current = ContextObservation::Resolved(request.binding.context_sha256.clone());
    assert_eq!(
        resolver.observe_with_pictures(&origin, &origin, &request, &pictures),
        current
    );
    for choice in [
        serde_json::json!({"range":"full"}),
        serde_json::json!({"chroma_location":"center"}),
    ] {
        wire["source_interpretations"] = serde_json::json!({"video":choice});
        wire["revision_id"] = serde_json::json!("corrected");
        let corrected = ProjectDocument::from_json(&wire.to_string()).unwrap();
        assert_ne!(
            context_identity(&origin, &id("h")),
            context_identity(&corrected, &id("h"))
        );
        assert_eq!(
            resolver.observe_with_pictures(&origin, &corrected, &request, &pictures),
            ContextObservation::Unresolved
        );
        let prepared = resolver.prepare_transition(&corrected).unwrap();
        assert_eq!(
            prepared.observe_with_pictures(&origin, &corrected, &request, &pictures),
            ContextObservation::Unresolved
        );
    }
    wire["source_interpretations"] = serde_json::json!({"unused":{"range":"full"}});
    let unrelated = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert_eq!(
        context_identity(&origin, &id("h")),
        context_identity(&unrelated, &id("h"))
    );
    assert_eq!(
        resolver.observe_with_pictures(&origin, &unrelated, &request, &pictures),
        current
    );
    wire["source_interpretations"] = serde_json::json!({});
    let reset = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert_eq!(
        resolver.observe_with_pictures(&origin, &reset, &request, &pictures),
        current
    );
}
