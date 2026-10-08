use super::*;
use deadpan_core::{
    AssetRecord, BeatNode, BoundaryQueryLimits, FrameDuration, HoldAudio, HoldRecipe, HoldVideo,
    NodeId, RevisionId,
};
use deadpan_plan::RenderPlan;

fn ratio(numerator: i64, denominator: i64) -> ExactRatio {
    ExactRatio::new(i128::from(numerator), i128::from(denominator)).unwrap()
}

// Observation intentionally accepts a descriptor, not an admission capability.
// Obtain its private canonical sample fields from a real plan, then vary the
// public provider facts to exercise malformed observations independently.
fn observed_span(
    document: &ProjectDocument,
    picture: Picture,
    clock: PictureClockSlope,
    distance: ExactRatio,
) -> DefinitionPictureSpan {
    let carrier = NodeId::new("carrier").unwrap();
    let mut wire = serde_json::to_value(document).unwrap();
    wire["nodes"]["root"]["kind"]["children"] = json!([carrier]);
    wire["nodes"]["carrier"] = serde_json::to_value(BeatNode::hold(
        "Carrier",
        HoldRecipe {
            duration: FrameDuration::new(100).unwrap(),
            video: HoldVideo::Background,
            picture_context: None,
            audio: HoldAudio::Silence,
        },
    ))
    .unwrap();
    let with_carrier = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = RenderPlan::compile(&with_carrier).unwrap();
    let mut start = plan
        .definition_picture(&carrier, ExactRatio::ZERO, BoundaryQueryLimits::default())
        .unwrap();
    start.picture = picture;
    DefinitionPictureSpan::observation(start, distance, clock)
}

fn original_document(connection: &Connection) -> (ProjectDocument, SourceQualificationId, usize) {
    let (qualification, bytes) = retain_receipt(connection);
    let receipt = crate::source_registration::read_receipt(connection, &qualification)
        .unwrap()
        .unwrap();
    let mut wire = serde_json::to_value(empty_document()).unwrap();
    wire["assets"] = json!({
        "authored-alias": receipt.asset_record("First label".into()).unwrap(),
        "other-alias": receipt.asset_record("Second label".into()).unwrap(),
    });
    (
        ProjectDocument::from_json(&wire.to_string()).unwrap(),
        qualification,
        bytes.len(),
    )
}

fn original_support(
    qualification: &SourceQualificationId,
    first: u64,
    last: u64,
) -> GenerationPictureSupport {
    let identity = |frame| GenerationPictureIdentity::Original {
        qualification: qualification.clone(),
        frame: SourceFrameId(frame),
    };
    GenerationPictureSupport {
        first: identity(first),
        last: identity(last),
    }
}

#[test]
fn span_support_reuses_measured_vfr_receipts_across_aliases_without_media() {
    let connection = connection();
    let (document, qualification, bytes) = original_document(&connection);
    // There is no media object on disk, decoder, sidecar, or source session.
    let pictures = QualifiedGenerationPictures::new(&connection);
    let forward = observed_span(
        &document,
        source(0, 24),
        PictureClockSlope::SourceTicks(ratio(1, 1)),
        ratio(11, 1),
    );
    let mut reverse_picture = source(20, 24);
    let Picture::Source { asset, .. } = &mut reverse_picture else {
        unreachable!()
    };
    *asset = AssetId::new("other-alias").unwrap();
    let reverse = observed_span(
        &document,
        reverse_picture,
        PictureClockSlope::SourceTicks(ratio(-1, 1)),
        ratio(9, 1),
    );
    let terminal = observed_span(
        &document,
        source(11, 24),
        PictureClockSlope::Constant,
        ExactRatio::ZERO,
    );
    for _ in 0..128 {
        assert_eq!(
            pictures.support(&document, &forward).unwrap(),
            original_support(&qualification, 0, 1)
        );
        assert_eq!(
            pictures.support(&document, &reverse).unwrap(),
            original_support(&qualification, 3, 2)
        );
        assert_eq!(
            pictures.support(&document, &terminal).unwrap(),
            original_support(&qualification, 2, 2)
        );
        assert_eq!(
            pictures.identity(&document, &source(9, 24)).unwrap(),
            original_support(&qualification, 1, 1).first
        );
    }
    let cache = pictures.receipts.borrow();
    assert_eq!(cache.entries.len(), 1);
    assert_eq!(cache.contract_builds, 1);
    assert_eq!(cache.bytes, bytes);
    assert_eq!(cache.frames, 4);
}

#[test]
fn span_support_respects_selected_clamps_and_refuses_changed_alias_contracts() {
    let connection = connection();
    let (document, qualification, _) = original_document(&connection);
    let pictures = QualifiedGenerationPictures::new(&connection);
    let mut selected = source(-10, 24);
    let Picture::Source { selection, .. } = &mut selected else {
        unreachable!()
    };
    *selection = span(3, 20).into();
    let mut observation = observed_span(
        &document,
        selected,
        PictureClockSlope::SourceTicks(ratio(50, 1)),
        ratio(1, 1),
    );
    assert_eq!(
        pictures.support(&document, &observation).unwrap(),
        original_support(&qualification, 1, 2)
    );
    let Picture::Source { endpoints, .. } = &mut observation.start.picture else {
        unreachable!()
    };
    *endpoints = EndpointPolicy::Reject;
    assert!(pictures.support(&document, &observation).is_err());
    let observation = observed_span(
        &document,
        source(0, 24),
        PictureClockSlope::SourceTicks(ratio(1, 1)),
        ratio(24, 1),
    );
    assert_eq!(
        pictures.support(&document, &observation).unwrap(),
        original_support(&qualification, 0, 3)
    );
    let mut wire = serde_json::to_value(&document).unwrap();
    wire["assets"]["authored-alias"]["content_hash"] = json!("b".repeat(64));
    let changed = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert!(
        pictures
            .support(&changed, &observation)
            .unwrap_err()
            .to_string()
            .contains("disagrees with its source qualification")
    );
    assert_eq!(pictures.receipts.borrow().contract_builds, 1);
}

fn generated_document(picture: &Picture) -> ProjectDocument {
    let Picture::Accepted {
        asset,
        generated: Some(artifact),
        time_base,
        ..
    } = picture
    else {
        unreachable!()
    };
    let record = AssetRecord {
        label: "Sampled output".into(),
        content_hash: artifact.sampled_object.content().to_string(),
        video: Some(
            SourceSpan::new(
                SourceTimestamp {
                    ticks: 0,
                    time_base: *time_base,
                },
                SourceTimestamp {
                    ticks: artifact.sampling.output_frame_count().frames(),
                    time_base: *time_base,
                },
            )
            .unwrap(),
        ),
        audio: None,
        still_image: false,
        frame_count: Some(artifact.sampling.output_frame_count()),
        source_qualification: None,
    };
    let mut wire = serde_json::to_value(empty_document()).unwrap();
    wire["assets"][asset.as_str()] = serde_json::to_value(record).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

#[test]
fn generated_support_uses_sampled_extent_and_saved_crop_without_an_original_index() {
    // Even qualification tables are absent: accepted support needs no fabricated
    // Original receipt or measured native/context frame index.
    let connection = Connection::open_in_memory().unwrap();
    let pictures = QualifiedGenerationPictures::new(&connection);
    let picture = generated(Some([640, 480]), 0);
    let document = generated_document(&picture);
    let support = |aspect| {
        pictures.support(
            &document,
            &observed_span(
                &document,
                generated(aspect, 0),
                PictureClockSlope::AcceptedFrames(ratio(3, 2)),
                ratio(4, 1),
            ),
        )
    };
    let actual = support(Some([640, 480])).unwrap();
    assert_eq!(actual, support(Some([4, 3])).unwrap());
    assert_ne!(actual, support(Some([16, 9])).unwrap());
    assert_ne!(actual, support(None).unwrap());
    assert!(support(Some([0, 3])).is_err());
    assert!(matches!(
        actual.first,
        GenerationPictureIdentity::Generated {
            frame: SourceFrameId(0),
            content_aspect: Some([4, 3]),
            ..
        }
    ));
    assert!(matches!(
        actual.last,
        GenerationPictureIdentity::Generated {
            frame: SourceFrameId(5),
            content_aspect: Some([4, 3]),
            ..
        }
    ));
    let terminal = observed_span(
        &document,
        generated(Some([4, 3]), 5),
        PictureClockSlope::Constant,
        ExactRatio::ZERO,
    );
    let terminal = pictures.support(&document, &terminal).unwrap();
    assert_eq!(terminal.first, terminal.last);
    assert_eq!(terminal.last, actual.last);
    assert!(pictures.receipts.borrow().entries.is_empty());
}

#[test]
fn generated_support_refuses_bad_count_object_clock_ordinal_and_sampled_bounds() {
    let connection = Connection::open_in_memory().unwrap();
    let pictures = QualifiedGenerationPictures::new(&connection);
    let picture = generated(Some([4, 3]), 0);
    let document = generated_document(&picture);
    let observation = observed_span(
        &document,
        picture,
        PictureClockSlope::AcceptedFrames(ratio(1, 1)),
        ratio(6, 1),
    );
    for (field, value) in [
        ("frame_count", json!(5)),
        ("content_hash", json!("b".repeat(64))),
    ] {
        let mut wire = serde_json::to_value(&document).unwrap();
        wire["assets"]["sampled"][field] = value;
        let changed = ProjectDocument::from_json(&wire.to_string()).unwrap();
        assert!(pictures.support(&changed, &observation).is_err(), "{field}");
    }
    let mut wrong_clock = observation.clone();
    let Picture::Accepted { time_base, .. } = &mut wrong_clock.start.picture else {
        unreachable!()
    };
    *time_base = clock();
    assert!(pictures.support(&document, &wrong_clock).is_err());
    let mut wrong_asset = observation.clone();
    let Picture::Accepted { asset, .. } = &mut wrong_asset.start.picture else {
        unreachable!()
    };
    *asset = AssetId::new("native").unwrap();
    assert!(pictures.support(&document, &wrong_asset).is_err());
    let mut wrong_ordinal = observation.clone();
    let Picture::Accepted { frame, .. } = &mut wrong_ordinal.start.picture else {
        unreachable!()
    };
    *frame = SourceFrameId(1);
    assert!(pictures.support(&document, &wrong_ordinal).is_err());
    let mut outside = observation.clone();
    outside.end_exclusive = ratio(6001, 1000);
    assert!(pictures.support(&document, &outside).is_err());
    let mut backwards = observation.clone();
    backwards.clock = PictureClockSlope::AcceptedFrames(ratio(-1, 1));
    assert!(pictures.support(&document, &backwards).is_err());
    let mut missing = observation;
    let Picture::Accepted { generated, .. } = &mut missing.start.picture else {
        unreachable!()
    };
    *generated = None;
    assert!(pictures.support(&document, &missing).is_err());
}

#[test]
fn support_is_revision_bound_strict_and_explicitly_unavailable_to_endpoint_only_providers() {
    struct EndpointsOnly;
    impl GenerationPictures for EndpointsOnly {
        fn identity(
            &self,
            _: &ProjectDocument,
            _: &Picture,
        ) -> Result<GenerationPictureIdentity, StoreError> {
            Ok(GenerationPictureIdentity::AuthoredBlack)
        }
    }
    let connection = Connection::open_in_memory().unwrap();
    let pictures = QualifiedGenerationPictures::new(&connection);
    let document = empty_document();
    let mut observation = observed_span(
        &document,
        Picture::Background,
        PictureClockSlope::Constant,
        ratio(4, 1),
    );
    let black = GenerationPictureSupport {
        first: GenerationPictureIdentity::AuthoredBlack,
        last: GenerationPictureIdentity::AuthoredBlack,
    };
    assert_eq!(pictures.support(&document, &observation).unwrap(), black);
    assert!(
        EndpointsOnly
            .support(&document, &observation)
            .unwrap_err()
            .to_string()
            .contains("support is unavailable")
    );
    let wire = serde_json::to_value(&black).unwrap();
    assert_eq!(
        serde_json::from_value::<GenerationPictureSupport>(wire.clone()).unwrap(),
        black
    );
    let mut unknown = wire;
    unknown["invented"] = json!(true);
    assert!(serde_json::from_value::<GenerationPictureSupport>(unknown).is_err());
    observation.start.revision_id = RevisionId::new("later").unwrap();
    assert!(pictures.support(&document, &observation).is_err());
    observation.start.revision_id = document.revision_id().clone();
    observation.end_exclusive = ratio(-1, 1);
    assert!(pictures.support(&document, &observation).is_err());
    observation.end_exclusive = ratio(4, 1);
    observation.clock = PictureClockSlope::SourceTicks(ratio(1, 1));
    assert!(pictures.support(&document, &observation).is_err());
}
