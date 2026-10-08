use super::*;
use deadpan_core::{
    AssetId, AssetRecord, BeatNode, BridgeInterpolation, BridgeSamplingMap, ColorPolicy,
    FrameDuration, FrameRate, GeneratedArtifact, GeneratedContentId, HoldAudio, HoldRecipe,
    PresentationBasis, SourceSpan, SourceTimeBase, SourceTimestamp,
};

fn target(name: &str) -> ScopedNodeTarget {
    ScopedNodeTarget {
        node: NodeId::new(name).unwrap(),
        repeats: Vec::new(),
    }
}

fn object(digit: char) -> GeneratedObjectRef {
    GeneratedObjectRef::new(
        GeneratedContentId::new(digit.to_string().repeat(64)).unwrap(),
        100,
    )
    .unwrap()
}

fn fixture() -> (ProjectDocument, AcceptedGeneration) {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let document = ProjectDocument::new(
        ProjectId::new("final-provider").unwrap(),
        RevisionId::new("after").unwrap(),
        PresentationBasis {
            width: 640,
            height: 480,
            frame_rate: rate,
            color_policy: ColorPolicy::SdrRec709,
        },
        target("root").node,
    )
    .unwrap();
    let expected = AcceptedGeneration {
        artifact: GeneratedArtifact {
            sampled_asset: AssetId::new("sampled").unwrap(),
            sampled_object: object('a'),
            native_asset: AssetId::new("native").unwrap(),
            native_object: object('b'),
            provenance: object('c'),
            sampling: BridgeSamplingMap::new(
                rate,
                FrameRate::new(24, 1).unwrap(),
                FrameDuration::new(5).unwrap(),
                FrameDuration::new(4).unwrap(),
                BridgeInterpolation::EncodedSrgbRgb8LinearHalfUp,
            )
            .unwrap()
            .into(),
            content_aspect: Some([640, 480]),
        },
        fallback: HoldFallback::Background,
    };
    let mut wire = serde_json::to_value(document).unwrap();
    wire["nodes"]["root"] = serde_json::to_value(BeatNode::sequence(
        "Root",
        vec![target("original").node, target("mapped").node],
    ))
    .unwrap();
    for name in ["original", "mapped"] {
        wire["nodes"][name] = serde_json::to_value(BeatNode::hold(
            name,
            HoldRecipe {
                duration: FrameDuration::new(4).unwrap(),
                audio: HoldAudio::Silence,
                video: HoldVideo::Generated {
                    accepted: Box::new(expected.clone()),
                },
                picture_context: None,
            },
        ))
        .unwrap();
    }
    for (id, reference, frames) in [
        (
            &expected.artifact.sampled_asset,
            &expected.artifact.sampled_object,
            4,
        ),
        (
            &expected.artifact.native_asset,
            &expected.artifact.native_object,
            5,
        ),
    ] {
        let time_base = SourceTimeBase::new(1, 48_000).unwrap();
        wire["assets"][id.as_str()] = serde_json::to_value(AssetRecord {
            label: "generated".into(),
            content_hash: reference.content().to_string(),
            video: Some(
                SourceSpan::new(
                    SourceTimestamp {
                        ticks: 0,
                        time_base,
                    },
                    SourceTimestamp {
                        ticks: 48_000,
                        time_base,
                    },
                )
                .unwrap(),
            ),
            audio: None,
            still_image: false,
            frame_count: Some(FrameDuration::new(frames).unwrap()),
            source_qualification: None,
        })
        .unwrap();
    }
    (
        ProjectDocument::from_json(&wire.to_string()).unwrap(),
        expected,
    )
}

fn with_video(document: &ProjectDocument, name: &str, video: HoldVideo) -> ProjectDocument {
    let mut wire = serde_json::to_value(document).unwrap();
    wire["nodes"][name]["kind"]["recipe"]["video"] = serde_json::to_value(video).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

#[test]
fn final_mapped_hold_determines_feedback_even_when_original_still_has_ai_pictures() {
    let (document, expected) = fixture();
    let final_document = with_video(&document, "mapped", HoldVideo::Background);
    assert_eq!(
        FinalProvider::resolve(&final_document, &target("original"), &expected).unwrap(),
        FinalProvider::AiPictures
    );
    assert_eq!(
        FinalProvider::resolve(&final_document, &target("mapped"), &expected).unwrap(),
        FinalProvider::ReplacementFallback
    );
}

#[test]
fn accepted_identity_checks_every_object_sampling_and_canvas() {
    let (document, expected) = fixture();
    assert_eq!(
        FinalProvider::resolve(&document, &target("mapped"), &expected).unwrap(),
        FinalProvider::AiPictures
    );
    let mut wrong = expected.clone();
    wrong.artifact.provenance = object('d');
    assert!(FinalProvider::resolve(&document, &target("mapped"), &wrong).is_err());
    wrong = expected.clone();
    wrong.artifact.sampled_object = object('d');
    assert!(FinalProvider::resolve(&document, &target("mapped"), &wrong).is_err());
    wrong = expected.clone();
    wrong.artifact.native_asset = AssetId::new("other-alias").unwrap();
    assert!(FinalProvider::resolve(&document, &target("mapped"), &wrong).is_err());
    wrong = expected.clone();
    wrong.artifact.content_aspect = Some([480, 640]);
    assert!(FinalProvider::resolve(&document, &target("mapped"), &wrong).is_err());
    wrong = expected.clone();
    wrong.artifact.sampling = BridgeSamplingMap::new(
        FrameRate::new(30_000, 1001).unwrap(),
        FrameRate::new(24, 1).unwrap(),
        FrameDuration::new(5).unwrap(),
        FrameDuration::new(3).unwrap(),
        BridgeInterpolation::EncodedSrgbRgb8LinearHalfUp,
    )
    .unwrap()
    .into();
    assert!(FinalProvider::resolve(&document, &target("mapped"), &wrong).is_err());
}

#[test]
fn fallback_requires_exact_saved_asset_and_timestamp() {
    let (document, mut expected) = fixture();
    let asset = expected.artifact.sampled_asset.clone();
    let timestamp = SourceTimestamp {
        ticks: 0,
        time_base: SourceTimeBase::new(1, 48_000).unwrap(),
    };
    expected.fallback = HoldFallback::Freeze {
        asset: asset.clone(),
        timestamp,
    };
    let frozen = with_video(&document, "mapped", HoldVideo::Freeze { asset, timestamp });
    assert_eq!(
        FinalProvider::resolve(&frozen, &target("mapped"), &expected).unwrap(),
        FinalProvider::ReplacementFallback
    );
    let HoldFallback::Freeze { timestamp, .. } = &mut expected.fallback else {
        unreachable!()
    };
    timestamp.ticks = 1;
    assert!(FinalProvider::resolve(&frozen, &target("mapped"), &expected).is_err());
    expected.fallback = HoldFallback::Background;
    assert!(FinalProvider::resolve(&frozen, &target("mapped"), &expected).is_err());
}

#[test]
fn missing_non_hold_and_unrelated_provider_are_not_reported_as_fallback() {
    let (document, expected) = fixture();
    assert!(FinalProvider::resolve(&document, &target("missing"), &expected).is_err());
    assert!(FinalProvider::resolve(&document, &target("root"), &expected).is_err());
    let legacy = with_video(
        &document,
        "mapped",
        HoldVideo::Accepted {
            asset: expected.artifact.sampled_asset.clone(),
            frames: FrameRange::new(ProjectFrame(0), ProjectFrame(4)).unwrap(),
        },
    );
    assert!(FinalProvider::resolve(&legacy, &target("mapped"), &expected).is_err());
}
