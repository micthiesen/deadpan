use deadpan_core::*;

#[test]
fn detached_source_view_keeps_revision_and_asset_but_does_not_edit_the_capture() {
    let root = NodeId::new("root").unwrap();
    let asset = AssetId::new("original").unwrap();
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: -100,
            time_base: SourceTimeBase::new(1, 1000).unwrap(),
        },
        SourceTimestamp {
            ticks: 900,
            time_base: SourceTimeBase::new(1, 1000).unwrap(),
        },
    )
    .unwrap();
    let mut captured = serde_json::to_value(
        ProjectDocument::new(
            ProjectId::new("project").unwrap(),
            RevisionId::new("captured").unwrap(),
            PresentationBasis {
                width: 16,
                height: 16,
                frame_rate: FrameRate::new(24, 1).unwrap(),
                color_policy: ColorPolicy::SdrRec709,
            },
            root.clone(),
        )
        .unwrap(),
    )
    .unwrap();
    captured["assets"]["original"] = serde_json::to_value(AssetRecord {
        label: "Original".into(),
        content_hash: "a".repeat(64),
        video: None,
        audio: Some(span),
        still_image: false,
        frame_count: None,
        source_qualification: None,
    })
    .unwrap();
    let captured = ProjectDocument::from_json(&captured.to_string()).unwrap();
    let before = captured.clone();
    let source = SourceNode {
        duration: FrameDuration::new(24).unwrap(),
        video: SourceVideo::Blank,
        video_mapping: SourceVideoMapping::FitBeat,
        audio: Some(SourceAudio {
            asset: asset.clone(),
            span,
        }),
        audio_mapping: SourceAudioMapping::natural_rate(
            span,
            captured.presentation_basis().frame_rate,
        )
        .unwrap(),
        audio_offset: AudioSample(0),
        link: LinkRelation::Independent,
    };
    let view = captured
        .source_view(
            &asset,
            source.clone(),
            NodeId::new("view-root").unwrap(),
            NodeId::new("view-source").unwrap(),
        )
        .unwrap();
    assert_eq!(view.project_id(), captured.project_id());
    assert_eq!(view.revision_id(), captured.revision_id());
    assert_eq!(view.presentation_basis(), captured.presentation_basis());
    assert_eq!(view.assets(), captured.assets());
    assert_eq!(view.duration().unwrap().frames(), 24);
    assert_eq!(captured, before);
    assert_eq!(captured.duration().unwrap().frames(), 0);
    assert!(
        captured
            .source_view(&asset, source.clone(), root.clone(), root.clone())
            .is_err()
    );
    let mut invalid = source;
    invalid.audio.as_mut().unwrap().asset = AssetId::new("foreign").unwrap();
    assert_eq!(
        captured
            .source_view(&asset, invalid, root, NodeId::new("source").unwrap())
            .unwrap_err()
            .code,
        DocumentErrorCode::MissingAsset
    );
}
